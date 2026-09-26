//! Manual verification helper for a release-build concern (PROGRESS.md
//! Phase 10, step 0): the shipped Nodekeeper exe is a Windows *GUI*
//! subsystem program (`src-tauri/src/main.rs`) -- it has no console --
//! but `nk-proc` stops `ord` with `GenerateConsoleCtrlEvent`, which per
//! the Win32 docs only reaches processes sharing the caller's console.
//! Every earlier check (Phase 0 spike, `cargo test`, `tauri dev`) ran
//! under a parent that *had* a console, so none of them exercised the
//! release-build situation.
//!
//! This example reproduces it without a full Tauri release build: it is
//! itself a windows-subsystem exe, and drives the same `nk-proc`/
//! `nk-exec` code paths the app uses (start bitcoind, start ord, run an
//! executor child, graceful-stop ord, graceful-stop bitcoind). Because
//! it has no console it cannot print, so every step is appended to a
//! result file for the driver script to read.
//!
//! Not meant to be run by hand: `console_less_probe.ps1` (same folder)
//! launches it, samples processes and windows while everything is
//! alive, and prints this file's log. Windows-only in practice; on other
//! platforms it still runs but proves nothing about consoles.
//!
//!   cargo build -p nk-testkit --example console_less_probe
//!   ./crates/nk-testkit/examples/console_less_probe.ps1
//!
//! Results and the decision they led to: DECISIONS.md, "Windows release
//! builds: console-less process handling (Phase 10 step 0)".

#![cfg_attr(windows, windows_subsystem = "windows")]

use nk_exec::{CommandSource, CommandSpec, Executor, Sensitivity};
use nk_testkit::RegtestFixture;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

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
    let [_, bitcoind, ord, result] = args.as_slice() else {
        return;
    };
    let log = Log {
        path: PathBuf::from(result),
        started: Instant::now(),
    };
    run(&log, Path::new(bitcoind), Path::new(ord)).await;
    log.line("DONE");
}

async fn run(log: &Log, bitcoind: &Path, ord: &Path) {
    log.line(format!("probe pid {}", std::process::id()));

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

    // A console-subsystem child spawned through the executor, the way
    // every `ord wallet ...` / `bitcoin-cli` command is: 6 pings take
    // ~5s, long enough for the driver to look for a console window.
    let executor = Executor::new();
    let exec_task = tokio::spawn(async move {
        executor
            .execute(CommandSpec {
                program: "ping".to_string(),
                args: vec!["-n".into(), "6".into(), "127.0.0.1".into()],
                stdin: None,
                environment: "regtest".into(),
                source: CommandSource::OrdCli,
                triggering_action: "console-less probe".into(),
                sensitivity: Sensitivity::Normal,
                redact: vec![],
                background: false,
                env_vars: vec![],
            })
            .await
            .map(|o| o.exit_code)
    });
    log.line("executor child (ping) started");

    // Window for the driver to inspect consoles / windows.
    tokio::time::sleep(Duration::from_secs(8)).await;
    log.line(format!("executor child finished: {:?}", exec_task.await));
    log.line("READY_FOR_STOP (driver sample point passed)");

    let t = Instant::now();
    match fixture.stop_ord().await {
        Ok(()) => log.line(format!(
            "stop_ord OK in {}ms; ord still detected running: {:?}",
            t.elapsed().as_millis(),
            nk_proc::detect_running_ord(&fixture.environment)
        )),
        Err(e) => log.line(format!(
            "stop_ord FAILED after {}ms: {e:?}; ord still detected running: {:?}",
            t.elapsed().as_millis(),
            nk_proc::detect_running_ord(&fixture.environment)
        )),
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
