//! Verification helper for a release-build concern (PROGRESS.md Phase 10,
//! step 0): the shipped Nodekeeper exe is a Windows *GUI* subsystem
//! program (`src-tauri/src/main.rs`) -- it has no console -- but `ord` is
//! stopped with a console signal, which only works between processes
//! sharing a console. Every earlier check (Phase 0 spike, `cargo test`,
//! `tauri dev`) ran under a parent that *had* a console, so none of them
//! exercised the release-build situation.
//!
//! This program reproduces it without a full Tauri release build: it is
//! itself a windows-subsystem exe, it clears its own standard handles to
//! NULL (as a GUI exe launched from Explorer has), and it drives the same
//! `nk-proc`/`nk-exec` code paths the app uses:
//!
//! 1. start bitcoind, start ord, run executor children (a `ping`, and
//!    `console_probe_child`, which reports whether it got a console
//!    window);
//! 2. graceful-stop ord (the fix under test);
//! 3. **spawn again after that stop**: restart ord and start a second
//!    bitcoind -- the stop leaves this process's standard handles in a
//!    state that used to make every later spawn fail with "The handle is
//!    invalid" (found by review; the first version of this probe never
//!    spawned after a stop, so it missed it);
//! 4. stop everything again.
//!
//! Because it has no console it cannot print, so every step is appended
//! to a result file, which is what both consumers read:
//!
//! - `scripts/console_less_probe.ps1` (crate root): launches it, samples
//!   the process tree and visible windows while everything is alive (are
//!   there stray console windows?), and prints the log.
//! - `tests/console_less.rs`: launches it and asserts on the log. Both
//!   programs are `[[bin]]`s (not examples) precisely so Cargo builds them
//!   before that integration test runs and hands the test the exact path
//!   -- `cargo test` does not rebuild examples, and a regression test that
//!   can silently run a stale binary is worse than none.
//!
//!   cargo build -p nk-testkit --bins
//!   ./crates/nk-testkit/scripts/console_less_probe.ps1
//!
//! Usage: `console_less_probe <bitcoind> <ord> <result-file> [hold-secs]`.
//! `hold-secs` (default 8) is how long everything is left running for the
//! driver to sample; 0 skips the wait (the test doesn't need it).
//!
//! Results and the decision behind the fix: DECISIONS.md, "Windows
//! release builds: console-less process handling (Phase 10 step 0)".

#![cfg_attr(windows, windows_subsystem = "windows")]

use nk_exec::{CommandSource, CommandSpec, Executor, Sensitivity};
use nk_testkit::RegtestFixture;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// The process's three standard handles, read and cleared. Windows-only
/// in effect; elsewhere they are reported as 0 and clearing does nothing.
#[cfg(windows)]
mod std_handles {
    extern "system" {
        fn GetStdHandle(std_handle: u32) -> isize;
        fn SetStdHandle(std_handle: u32, handle: isize) -> i32;
    }
    /// `STD_INPUT_HANDLE`, `STD_OUTPUT_HANDLE`, `STD_ERROR_HANDLE`.
    const WHICH: [u32; 3] = [0xFFFF_FFF6, 0xFFFF_FFF5, 0xFFFF_FFF4];

    pub fn clear() {
        for which in WHICH {
            // Safety: plain integers; failure is ignored (best effort).
            unsafe { SetStdHandle(which, 0) };
        }
    }

    pub fn read() -> [isize; 3] {
        // Safety: plain integer argument, no pointers.
        WHICH.map(|which| unsafe { GetStdHandle(which) })
    }
}
#[cfg(not(windows))]
mod std_handles {
    pub fn clear() {}
    pub fn read() -> [isize; 3] {
        [0; 3]
    }
}

/// Which processes own a visible top-level window right now. A console
/// child of a GUI parent that was *not* spawned with `CREATE_NO_WINDOW`
/// shows up here (its console window -- under Windows Terminal, the
/// child's own pseudo-console window -- is visible and owned by its pid),
/// so this is what lets the test assert that the long-lived bitcoind and
/// ord have no visible window. Caveat: with the classic console host the
/// visible window is owned by the host process, not the child, so this
/// check can miss there; `scripts/console_less_probe.ps1` samples all
/// visible windows and is the backstop.
#[cfg(windows)]
mod visible_windows {
    extern "system" {
        fn EnumWindows(
            callback: unsafe extern "system" fn(isize, isize) -> i32,
            lparam: isize,
        ) -> i32;
        fn IsWindowVisible(hwnd: isize) -> i32;
        fn GetWindowThreadProcessId(hwnd: isize, pid: *mut u32) -> u32;
    }

    unsafe extern "system" fn collect(hwnd: isize, lparam: isize) -> i32 {
        // Safety: `lparam` is the address of the `Vec` `owning_pids` below
        // passes, alive for the whole (synchronous) `EnumWindows` call.
        let owners = &mut *(lparam as *mut Vec<u32>);
        if IsWindowVisible(hwnd) != 0 {
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            owners.push(pid);
        }
        1 // keep enumerating
    }

    pub fn owning_pids() -> Vec<u32> {
        let mut owners: Vec<u32> = Vec::new();
        // Safety: the callback only touches `owners`, via the pointer.
        unsafe { EnumWindows(collect, &mut owners as *mut Vec<u32> as isize) };
        owners
    }
}
#[cfg(not(windows))]
mod visible_windows {
    pub fn owning_pids() -> Vec<u32> {
        Vec::new()
    }
}

struct Log {
    path: PathBuf,
    started: Instant,
}

impl Log {
    fn line(&self, message: impl AsRef<str>) {
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        {
            let _ = writeln!(
                file,
                "[{:>6}ms] {}",
                self.started.elapsed().as_millis(),
                message.as_ref()
            );
        }
    }
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (Some(bitcoind), Some(ord), Some(result)) = (args.get(1), args.get(2), args.get(3)) else {
        return;
    };
    let hold_secs: u64 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(8);
    let log = Log {
        path: PathBuf::from(result),
        started: Instant::now(),
    };
    run(&log, Path::new(bitcoind), Path::new(ord), hold_secs).await;
    log.line("DONE");
}

fn command(program: String, args: Vec<String>) -> CommandSpec {
    CommandSpec {
        program,
        args,
        stdin: None,
        environment: "regtest".into(),
        source: CommandSource::OrdCli,
        triggering_action: "console-less probe".into(),
        sensitivity: Sensitivity::Normal,
        redact: vec![],
        background: false,
        env_vars: vec![],
    }
}

async fn run(log: &Log, bitcoind: &Path, ord: &Path, hold_secs: u64) {
    log.line(format!("probe pid {}", std::process::id()));
    // Recorded so a consumer can tell this really ran without a console
    // (the whole point) rather than passing vacuously under one.
    log.line(format!("has console: {}", nk_proc::process_has_console()));
    // A GUI exe launched normally has NULL standard handles; the test
    // harness launches this one with pipes, so normalise to the same
    // starting point.
    std_handles::clear();
    log.line(format!("std handles at start: {:?}", std_handles::read()));

    let mut fixture = match RegtestFixture::start(bitcoind).await {
        Ok(fixture) => fixture,
        Err(e) => return log.line(format!("bitcoind start FAILED: {e}")),
    };
    let bitcoind_pid = nk_proc::detect_running_bitcoind(&fixture.environment);
    log.line(format!("bitcoind started, pid {bitcoind_pid:?}"));

    if let Err(e) = fixture.start_ord(ord).await {
        return log.line(format!("ord start FAILED: {e}"));
    }
    let ord_pid = nk_proc::detect_running_ord(&fixture.environment);
    log.line(format!("ord started, pid {ord_pid:?}"));

    // The long-lived children must not own a visible window (a console
    // window can take a moment to appear, so give it one).
    tokio::time::sleep(Duration::from_millis(600)).await;
    let owners = visible_windows::owning_pids();
    let offenders: Vec<u32> = [bitcoind_pid, ord_pid]
        .into_iter()
        .flatten()
        .filter(|pid| owners.contains(pid))
        .collect();
    log.line(format!(
        "visible windows owned by bitcoind/ord: {offenders:?}"
    ));

    // Console-subsystem children spawned through the executor, the way
    // every `ord wallet ...` / `bitcoin-cli` command is. The ping stays
    // alive (about `pings - 1` seconds) so the driver can look for a
    // console window; `console_probe_child` reports whether *it* got one.
    let pings = if hold_secs == 0 { 2 } else { 6 };
    let executor = Executor::new();
    let ping_task = {
        let executor = executor.clone();
        tokio::spawn(async move {
            executor
                .execute(command(
                    "ping".to_string(),
                    vec!["-n".into(), pings.to_string(), "127.0.0.1".into()],
                ))
                .await
                .map(|o| o.exit_code)
        })
    };
    log.line("executor child (ping) started");

    let child_probe = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("console_probe_child.exe")));
    match child_probe {
        Some(path) => match executor
            .execute(command(path.to_string_lossy().into_owned(), vec![]))
            .await
        {
            Ok(outcome) => log.line(format!(
                "executor child console: {}",
                String::from_utf8_lossy(&outcome.stdout).trim()
            )),
            Err(e) => log.line(format!("executor child console probe FAILED: {e}")),
        },
        None => log.line("executor child console probe FAILED: no exe path"),
    }

    // Window for the driver to inspect consoles / windows.
    tokio::time::sleep(Duration::from_secs(hold_secs)).await;
    log.line(format!("executor child finished: {:?}", ping_task.await));
    log.line("READY_FOR_STOP (driver sample point passed)");

    // --- The fix under test: graceful-stop ord with no console. ---
    let t = Instant::now();
    match fixture.stop_ord_with_status().await {
        Ok(status) => log.line(format!(
            "stop_ord OK in {}ms; ord exit code: {:?}; ord still detected running: {:?}",
            t.elapsed().as_millis(),
            status.and_then(|s| s.code()),
            nk_proc::detect_running_ord(&fixture.environment)
        )),
        Err(e) => log.line(format!(
            "stop_ord FAILED after {}ms: {e:?}; ord still detected running: {:?}",
            t.elapsed().as_millis(),
            nk_proc::detect_running_ord(&fixture.environment)
        )),
    }
    // The stop must leave this process exactly as it found it.
    log.line(format!(
        "after stop_ord: has console: {}; std handles: {:?}",
        nk_proc::process_has_console(),
        std_handles::read()
    ));

    // --- Spawning again after that stop (the stale-handle regression). ---
    match fixture.start_ord(ord).await {
        Ok(()) => {
            log.line(format!(
                "ord restarted, pid {:?}",
                nk_proc::detect_running_ord(&fixture.environment)
            ));
            match fixture.stop_ord_with_status().await {
                Ok(status) => log.line(format!(
                    "second stop_ord OK; ord exit code: {:?}",
                    status.and_then(|s| s.code())
                )),
                Err(e) => log.line(format!("second stop_ord FAILED: {e:?}")),
            }
        }
        Err(e) => log.line(format!("ord restart FAILED: {e}")),
    }
    match RegtestFixture::start(bitcoind).await {
        Ok(second) => {
            log.line("second bitcoind started");
            match second.stop().await {
                Ok(()) => log.line("second bitcoind stop OK"),
                Err(e) => log.line(format!("second bitcoind stop FAILED: {e:?}")),
            }
        }
        Err(e) => log.line(format!("second bitcoind start FAILED: {e}")),
    }

    let env = fixture.environment.clone();
    let t = Instant::now();
    match fixture.stop().await {
        Ok(()) => log.line(format!("bitcoind stop OK in {}ms", t.elapsed().as_millis())),
        Err(e) => log.line(format!(
            "bitcoind stop FAILED after {}ms: {e:?}",
            t.elapsed().as_millis()
        )),
    }
    log.line(format!(
        "bitcoind still detected running: {:?}",
        nk_proc::detect_running_bitcoind(&env)
    ));
}
