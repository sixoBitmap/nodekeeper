//! Windows release-build regression (PROGRESS.md Phase 10 step 0): the
//! shipped exe is a GUI-subsystem program with no console, and there
//! ord's graceful stop used to fail (`GenerateConsoleCtrlEvent` -> "The
//! handle is invalid") and leave ord running. Every other test in the
//! workspace runs under a parent that *has* a console, so none of them
//! could catch it.
//!
//! This one launches the `console_less_probe` binary -- itself a
//! console-less exe with NULL standard handles -- and asserts on the log
//! it writes. What it pins down:
//!
//! - the probe really had no console (so nothing below passes vacuously);
//! - ord stopped gracefully: exit code 0 means its own shutdown handler
//!   ran (an unhandled CTRL_BREAK gives 0xC000013A), and it is gone;
//! - the stop leaves the process exactly as it found it (still no
//!   console, standard handles still NULL) and **spawning still works
//!   afterwards** -- ord restarts and stops again, a second bitcoind
//!   starts and stops (the first version of the fix left stale standard
//!   handles behind, which made every later spawn fail; review found it);
//! - a child spawned through the executor got no console window, and the
//!   long-lived bitcoind and ord own no visible window.
//!
//! Each fix was verified to make this test fail when removed (DECISIONS.md,
//! "Windows release builds: console-less process handling"); the
//! exit-code-0 assertion on its own was not separately shown to fail
//! (nothing here can make ord die by an unhandled CTRL_BREAK on demand).
//! The visible-window
//! check is by owning pid, which is reliable under Windows Terminal (this
//! machine's setup) but can miss with the classic console host, where the
//! host process owns the window; `scripts/console_less_probe.ps1` samples
//! every visible window and is the backstop.
//!
//! Gated like every other real-binary test: skipped unless
//! `NK_TEST_BITCOIND` and `NK_TEST_ORD` point at real binaries.
//!
//! Failure hygiene matters more than usual here, because the bug this
//! guards against *is* an orphaned ord: an orphan inherits the probe's
//! output pipes, so the executor would never see end-of-output and the
//! test would hang until someone killed it by hand. So the test waits on
//! the probe's own `DONE` log line (not on pipe EOF) and then kills any
//! bitcoind/ord that is a child of the probe -- found by parent pid, not
//! by the log, so a process whose startup *failed* (and so never logged a
//! pid) is still cleaned up -- on every path, before waiting for the
//! probe's output to close.
#![cfg(windows)]

use nk_exec::{CommandSource, CommandSpec, Executor, Sensitivity};
use serial_test::serial;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The probe's own pid, from its first log line.
fn probe_pid(log: &str) -> Option<u32> {
    log.lines()
        .find_map(|line| line.split("probe pid ").nth(1)?.trim().parse().ok())
}

/// Kills every bitcoind/ord whose parent is the probe. Matches on parent
/// pid *and* process name, so an unrelated process that happens to reuse
/// a pid is never touched.
fn kill_stragglers(log_path: &Path) {
    let Ok(log) = std::fs::read_to_string(log_path) else {
        return;
    };
    let Some(probe) = probe_pid(&log) else {
        return;
    };
    let mut system = sysinfo::System::new();
    system.refresh_all();
    for process in system.processes().values() {
        let name = process.name().to_string_lossy().to_lowercase();
        let is_ours = process.parent() == Some(sysinfo::Pid::from_u32(probe));
        if is_ours && (name.starts_with("bitcoind") || name.starts_with("ord")) {
            process.kill();
        }
    }
}

/// Runs `kill_stragglers` when dropped, so a panic anywhere in the test
/// (including the assertions below) still cleans up.
struct KillStragglersOnDrop(PathBuf);

impl Drop for KillStragglersOnDrop {
    fn drop(&mut self) {
        kill_stragglers(&self.0);
    }
}

/// Polls `log_path` until it contains `DONE`, or `timeout` passes.
/// Returns whatever the log holds at that point.
async fn wait_for_done(log_path: &Path, timeout: Duration) -> String {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let log = std::fs::read_to_string(log_path).unwrap_or_default();
        if log.lines().any(|line| line.ends_with("DONE")) || tokio::time::Instant::now() >= deadline
        {
            return log;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

#[tokio::test]
#[serial(real_bitcoind)]
async fn a_console_less_parent_gracefully_stops_ord_and_can_still_spawn_afterwards() {
    let (Some(bitcoind_path), Some(ord_path)) = (
        nk_core::live_tests::live_binary("NK_TEST_BITCOIND"),
        nk_core::live_tests::live_binary("NK_TEST_ORD"),
    ) else {
        eprintln!("skipping: NK_TEST_BITCOIND and/or NK_TEST_ORD not set");
        return;
    };

    let dir = tempfile::tempdir().unwrap();
    let result_file = dir.path().join("probe-result.txt");
    let _cleanup = KillStragglersOnDrop(result_file.clone());

    let spec = CommandSpec {
        program: env!("CARGO_BIN_EXE_console_less_probe").to_string(),
        args: vec![
            bitcoind_path.to_string_lossy().into_owned(),
            ord_path.to_string_lossy().into_owned(),
            result_file.to_string_lossy().into_owned(),
            "0".to_string(), // no need to hold for window sampling
        ],
        stdin: None,
        environment: "regtest".to_string(),
        source: CommandSource::Script,
        triggering_action: "console-less release-build regression test".to_string(),
        sensitivity: Sensitivity::Normal,
        redact: vec![],
        background: false,
        env_vars: vec![],
    };
    let executor = Executor::new();
    let mut probe = tokio::spawn(async move { executor.execute(spec).await });

    // Generous: a healthy run takes ~20s, but bitcoind startup alone has
    // taken >30s under CI load (see RegtestFixture::start), and the probe
    // now starts three bitcoind/ord processes. Also returns early if the
    // probe itself dies, so a crash fails fast instead of after 180s.
    let mut finished_early = None;
    let log = tokio::select! {
        log = wait_for_done(&result_file, Duration::from_secs(180)) => log,
        outcome = &mut probe => {
            finished_early = Some(outcome);
            std::fs::read_to_string(&result_file).unwrap_or_default()
        }
    };
    // Kill anything the probe left behind *before* waiting on its
    // output, or an orphaned ord (the very failure under test) would keep
    // the pipes open and this wait would never end.
    kill_stragglers(&result_file);
    let outcome = match finished_early {
        Some(outcome) => outcome,
        None => tokio::time::timeout(Duration::from_secs(30), probe)
            .await
            .unwrap_or_else(|_| {
                panic!(
                    "the probe's output should close once its children are gone; probe log:\n{log}"
                )
            }),
    }
    .expect("the probe task should not panic")
    .unwrap_or_else(|e| panic!("the probe should launch: {e}; probe log:\n{log}"));
    assert_eq!(outcome.exit_code, Some(0), "probe log:\n{log}");

    let expect_line = |needle: &str| {
        assert!(
            log.contains(needle),
            "expected {needle:?} in probe log:\n{log}"
        );
    };
    // It must really have run without a console and with NULL standard
    // handles, or nothing below proves anything about the release build.
    expect_line("has console: false");
    expect_line("std handles at start: [0, 0, 0]");

    // Both processes came up (and a pid was captured, not just "None").
    expect_line("bitcoind started, pid Some(");
    expect_line("ord started, pid Some(");

    // No console window for an executor child, and none owned by the
    // long-lived bitcoind / ord either.
    expect_line("executor child console: console_window=none");
    expect_line("visible windows owned by bitcoind/ord: []");

    // The fix: a graceful, complete stop of ord. Asserted on the *first*
    // stop's own log line: the second stop logs similar words ("second
    // stop_ord OK; ord exit code: ..."), and separate substring checks
    // would be satisfied by that one even if the first stop had ended
    // badly.
    assert!(
        log.lines().any(|line| {
            line.contains("] stop_ord OK in ")
                && line.contains("ord exit code: Some(0); ord still detected running: None")
        }),
        "expected one line showing the first stop_ord succeeded gracefully, in probe log:\n{log}"
    );

    // ...that leaves the process exactly as it found it...
    expect_line("after stop_ord: has console: false; std handles: [0, 0, 0]");

    // ...so spawning still works afterwards.
    expect_line("ord restarted, pid Some(");
    expect_line("second stop_ord OK; ord exit code: Some(0)");
    expect_line("second bitcoind started");
    expect_line("second bitcoind stop OK");

    expect_line("bitcoind stop OK in");
    expect_line("bitcoind still detected running: None");
    assert!(
        !log.contains("FAILED"),
        "probe log reports a failure:\n{log}"
    );
    assert!(
        log.trim_end().ends_with("DONE"),
        "probe did not finish:\n{log}"
    );
}
