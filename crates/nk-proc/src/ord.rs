//! Spawns and gracefully stops `ord server` per environment (docs/SPEC.md
//! Foundation F). Mirrors `bitcoind.rs`'s shape closely, with two real
//! differences forced by how ord itself behaves (both confirmed live,
//! DECISIONS.md Phase 4):
//!
//! - ord writes no PID file of its own, so this module writes and reads
//!   one itself (`nk_core::Environment::ord_pid_path`), unlike
//!   `detect_running_bitcoind`, which just reads bitcoind's own file.
//! - ord has no RPC-based graceful stop (no `bitcoin-cli stop`
//!   equivalent) — graceful shutdown is always a signal: SIGINT on
//!   macOS/Linux, `CTRL_BREAK_EVENT` on Windows (Phase 0's spike,
//!   `spikes/test-createprocess-v2.ps1`, proved the Windows mechanism
//!   live; this is its first real Rust use). On Windows the delivery
//!   route depends on whether Nodekeeper itself has a console -- see
//!   `console.rs`.

use crate::console::ConsoleMode;
use crate::process_check::process_is_alive;
use nk_core::{AppErrorCode, Environment};
use std::net::TcpListener;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OrdProcessError {
    #[error("ord is already running on this data directory (pid {pid})")]
    AlreadyRunning { pid: u32 },
    /// Same pre-flight-check reasoning as `BitcoindError::PortInUse`.
    #[error("port {port} is already in use")]
    PortInUse { port: u16 },
    #[error("failed to spawn ord: {0}")]
    Spawn(std::io::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("ord did not exit within the timeout after being asked to stop")]
    StopTimeout,
    #[error("ord did not become ready (HTTP /status responding) within the timeout")]
    StartupTimeout,
    /// Distinct from `StartupTimeout`: ord's HTTP server was already
    /// responding, it just hadn't finished indexing up to the node's
    /// current height yet.
    #[error("ord did not catch up with the node's block height within the timeout")]
    SyncTimeout,
}

impl OrdProcessError {
    pub fn code(&self) -> Option<AppErrorCode> {
        match self {
            Self::PortInUse { .. } => Some(AppErrorCode::PortInUse),
            Self::SyncTimeout => Some(AppErrorCode::OrdNotSynced),
            Self::AlreadyRunning { .. }
            | Self::Spawn(_)
            | Self::Io(_)
            | Self::StopTimeout
            | Self::StartupTimeout => None,
        }
    }
}

pub struct OrdProcess {
    child: tokio::process::Child,
    pub pid: u32,
    pub started_at: std::time::Instant,
    /// Decided once at spawn and reused at stop: the route the graceful
    /// stop signal must take depends on how ord's console was set up
    /// then, not on whatever this process looks like later.
    console_mode: ConsoleMode,
}

impl OrdProcess {
    /// Starts `ord server` for `environment`. Refuses if one is already
    /// running on the same data directory (same single-instance rule as
    /// bitcoind, Foundation C).
    ///
    /// Pre-creates `ord_index_dir()` (not just the top-level `--data-dir`
    /// argument) *before* spawning, unlike bitcoind's `start()` which
    /// only creates the top-level dir and lets bitcoind itself create
    /// its chain subfolder: here, Nodekeeper needs to write `ord.pid`
    /// into that chain subfolder immediately after spawn, and can't rely
    /// on ord's own (asynchronous, racy-to-wait-for) directory creation
    /// to have happened yet.
    pub async fn start(
        binary_path: &Path,
        environment: &Environment,
        cookie_path: &Path,
        bitcoin_datadir: &Path,
    ) -> Result<Self, OrdProcessError> {
        if let Some(pid) = detect_running_ord(environment) {
            return Err(OrdProcessError::AlreadyRunning { pid });
        }
        check_port_available(environment.ord_port)?;

        std::fs::create_dir_all(environment.ord_index_dir())?;

        let mut args = nk_core::ord_conf::ord_base_args(environment, cookie_path, bitcoin_datadir);
        args.extend(nk_core::ord_conf::ord_server_args(environment));

        let mut command = tokio::process::Command::new(binary_path);
        // stdin is null explicitly, not left to default to "inherit": an
        // inherited stdin duplicates whatever this process's standard
        // handle currently is, which can be a stale value in the release
        // exe after an ord stop (see `console.rs`).
        command
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        // Windows-only: always a new process group, so a later
        // CTRL_BREAK_EVENT targets only ord (and nothing it spawns), not
        // Nodekeeper's own process group -- and, when Nodekeeper has no
        // console (the shipped GUI exe), a hidden console of ord's own
        // instead of a visible window. Unix's SIGINT is sent straight to
        // ord's own pid and needs neither.
        let console_mode = ConsoleMode::current();
        #[cfg(windows)]
        {
            // `tokio::process::Command` exposes `creation_flags` as an
            // inherent method on Windows (no `CommandExt` import
            // needed, unlike `std::process::Command`).
            command.creation_flags(crate::console::ord_creation_flags(console_mode));
        }

        let child = command.spawn().map_err(OrdProcessError::Spawn)?;
        let pid = child.id().expect("a just-spawned child process has a pid");

        std::fs::write(environment.ord_pid_path(), pid.to_string())?;

        Ok(Self {
            child,
            pid,
            started_at: std::time::Instant::now(),
            console_mode,
        })
    }

    /// `start()` plus waiting for ord's HTTP server to actually answer
    /// `/status` — the ord equivalent of `BitcoindProcess::
    /// start_and_wait_ready`'s RPC-ready phase. Unlike that method,
    /// there's no separate "cookie appears" phase to wait through first:
    /// ord reads bitcoind's cookie file itself at startup and there's
    /// nothing analogous for Nodekeeper to poll for beforehand.
    ///
    /// This is a *startup* readiness wait, not a "caught up with the
    /// chain tip" wait — ord can answer `/status` (with a low `height`)
    /// long before its index catches up. Comparing `height` against
    /// bitcoind's block count is a separate, open-ended concern (the
    /// Dashboard's ongoing sync-progress display), not something a
    /// bounded startup wait should block on.
    /// **Never returns an orphan.** Same reasoning and contract as
    /// `BitcoindProcess::start_and_wait_ready` (PROGRESS.md, Phase 10M/
    /// 10R): if ord never answers `/status` in time, the process this
    /// call just spawned is killed and `ord.pid` removed before the
    /// error comes back, so a later `start()` doesn't refuse with
    /// `AlreadyRunning` against a process nothing can stop.
    #[allow(clippy::too_many_arguments)]
    pub async fn start_and_wait_ready(
        binary_path: &Path,
        environment: &Environment,
        cookie_path: &Path,
        bitcoin_datadir: &Path,
        base_url: String,
        executor: nk_exec::Executor,
        environment_label: String,
        ready_timeout: Duration,
    ) -> Result<(Self, nk_ord::OrdClient), OrdProcessError> {
        let process = Self::start(binary_path, environment, cookie_path, bitcoin_datadir).await?;
        Self::wait_ready_or_clean_up(
            process,
            environment,
            base_url,
            executor,
            environment_label,
            ready_timeout,
        )
        .await
    }

    /// `wait_ready`, plus the orphan-cleanup contract documented on
    /// `start_and_wait_ready`. Takes an already-spawned `process` rather
    /// than spawning one itself, so a test can substitute a process it
    /// controls -- same reason `BitcoindProcess` splits this the same way.
    async fn wait_ready_or_clean_up(
        mut process: Self,
        environment: &Environment,
        base_url: String,
        executor: nk_exec::Executor,
        environment_label: String,
        ready_timeout: Duration,
    ) -> Result<(Self, nk_ord::OrdClient), OrdProcessError> {
        match Self::wait_ready(base_url, executor, environment_label, ready_timeout).await {
            Ok(client) => Ok((process, client)),
            Err(e) => {
                process.kill_sync();
                let _ = tokio::time::timeout(Duration::from_secs(5), process.child.wait()).await;
                let _ = std::fs::remove_file(environment.ord_pid_path());
                Err(e)
            }
        }
    }

    async fn wait_ready(
        base_url: String,
        executor: nk_exec::Executor,
        environment_label: String,
        ready_timeout: Duration,
    ) -> Result<nk_ord::OrdClient, OrdProcessError> {
        let client = nk_ord::OrdClient::new(base_url, executor, environment_label);

        let deadline = tokio::time::Instant::now() + ready_timeout;
        loop {
            // background: true -- this readiness poll can repeat many
            // times during a slow startup and isn't itself a meaningful
            // user-facing check (docs/SPEC.md item 7), same reasoning as
            // bitcoind's own RPC-ready poll.
            //
            // Wrapped in its own timeout for the same reason bitcoind's
            // poll is: `OrdClient`'s HTTP client has no request timeout
            // of its own, so an ord that accepts the connection but never
            // answers would otherwise hang this whole wait past
            // `deadline`, which is only ever checked *between* calls.
            if tokio::time::timeout(ready_timeout, client.status(true))
                .await
                .is_ok_and(|result| result.is_ok())
            {
                return Ok(client);
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(OrdProcessError::StartupTimeout);
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    /// Graceful stop (Foundation C, extended to ord): a signal, not an
    /// RPC call — ord has no `stop` RPC equivalent (DECISIONS.md Phase
    /// 4). Removes the PID file Nodekeeper wrote in `start()` once ord
    /// has actually exited, so a stale file never causes a later
    /// `detect_running_ord` false positive.
    ///
    /// Returns ord's exit status so a caller (or test) can tell a
    /// graceful shutdown (ord's own handler, exit code 0 on Windows)
    /// from being killed by an unhandled signal; the stop itself only
    /// fails if the signal couldn't be sent or ord didn't exit in time.
    ///
    /// An ord that has *already* exited (crashed, killed from Task
    /// Manager, ...) is a successful stop, not an error: there is nothing
    /// left to signal, and on the Windows hidden-console route the signal
    /// itself cannot even be attempted (`AttachConsole` fails for a
    /// process that no longer exists), so this is checked before the
    /// signal and again if the signal fails.
    pub async fn stop(
        mut self,
        environment: &Environment,
        timeout: Duration,
    ) -> Result<std::process::ExitStatus, OrdProcessError> {
        if let Ok(Some(status)) = self.child.try_wait() {
            let _ = std::fs::remove_file(environment.ord_pid_path());
            return Ok(status);
        }
        if let Err(signal_error) = send_graceful_stop(self.pid, self.console_mode) {
            // It may have exited between the check above and the signal.
            if let Ok(Some(status)) = self.child.try_wait() {
                let _ = std::fs::remove_file(environment.ord_pid_path());
                return Ok(status);
            }
            return Err(signal_error);
        }
        match tokio::time::timeout(timeout, self.child.wait()).await {
            Ok(Ok(status)) => {
                let _ = std::fs::remove_file(environment.ord_pid_path());
                Ok(status)
            }
            Ok(Err(e)) => Err(OrdProcessError::Io(e)),
            Err(_elapsed) => Err(OrdProcessError::StopTimeout),
        }
    }

    /// Same "no graceful attempt, cleanup-path only" contract as
    /// `BitcoindProcess::kill_sync`.
    pub fn kill_sync(&mut self) {
        let _ = self.child.start_kill();
    }
}

/// Polls ord's `/status` and the node's `getblockchaininfo` until ord's
/// own indexed `height` reaches the node's `blocks` (docs/SPEC.md's
/// wait-for-sync logic, DECISIONS.md Phase 4: ord never does this
/// comparison itself). Deliberately a free function, not an
/// `OrdProcess` method -- it only needs the two already-running
/// clients, not the process handle, and its caller decides how long an
/// "open-ended" wait is acceptable (a bounded regtest test vs. a real
/// mainnet chain catching up are very different timescales, unlike
/// `start_and_wait_ready`'s short, always-bounded startup check).
///
/// Any error from either poll is treated the same as "not caught up
/// yet" and retried -- same convention `BitcoindProcess::
/// start_and_wait_ready`'s RPC-ready loop and `OrdProcess::
/// start_and_wait_ready` above both already use: a real, persistent
/// failure surfaces as `SyncTimeout` once the deadline passes, rather
/// than needing this loop to distinguish "transiently unavailable"
/// from "genuinely broken".
pub async fn wait_until_caught_up(
    ord: &nk_ord::OrdClient,
    bitcoin_rpc: &nk_rpc::RpcClient,
    timeout: Duration,
) -> Result<(), OrdProcessError> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let node_height = bitcoin_rpc
            .get_blockchain_info(true)
            .await
            .ok()
            .and_then(|v| v.get("blocks").and_then(|b| b.as_u64()));
        let ord_height = ord
            .status(true)
            .await
            .ok()
            .and_then(|v| v.get("height").and_then(|h| h.as_u64()));

        if let (Some(node_height), Some(ord_height)) = (node_height, ord_height) {
            if ord_height >= node_height {
                return Ok(());
            }
        }

        if tokio::time::Instant::now() >= deadline {
            return Err(OrdProcessError::SyncTimeout);
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// Same pattern as `detect_running_bitcoind`, reading Nodekeeper's own
/// PID file (`ord_pid_path`) instead of one ord wrote itself.
pub fn detect_running_ord(environment: &Environment) -> Option<u32> {
    let pid: u32 = std::fs::read_to_string(environment.ord_pid_path())
        .ok()?
        .trim()
        .parse()
        .ok()?;
    process_is_alive(pid).then_some(pid)
}

/// docs/SPEC.md item 12's unclean-shutdown signal, for ord -- same
/// reasoning as `bitcoind_had_unclean_shutdown`, but the guarantee
/// doesn't need a live VERIFY here: `ord.pid` is Nodekeeper's own file
/// (ord writes none of its own), and `OrdProcess::stop` above removes
/// it explicitly on a clean exit (`std::fs::remove_file`), right in
/// this same file -- so its stale presence reliably means the
/// *previous* run ended uncleanly, not just "never started here."
pub fn ord_had_unclean_shutdown(environment: &Environment) -> bool {
    let Ok(contents) = std::fs::read_to_string(environment.ord_pid_path()) else {
        return false;
    };
    let Ok(pid) = contents.trim().parse::<u32>() else {
        return false;
    };
    !process_is_alive(pid)
}

fn check_port_available(port: u16) -> Result<(), OrdProcessError> {
    TcpListener::bind(("127.0.0.1", port))
        .map(|_listener| ())
        .map_err(|_| OrdProcessError::PortInUse { port })
}

#[cfg(unix)]
fn send_graceful_stop(pid: u32, _console_mode: ConsoleMode) -> Result<(), OrdProcessError> {
    let result = unsafe { libc::kill(pid as libc::pid_t, libc::SIGINT) };
    if result == 0 {
        Ok(())
    } else {
        Err(OrdProcessError::Io(std::io::Error::last_os_error()))
    }
}

#[cfg(windows)]
fn send_graceful_stop(pid: u32, console_mode: ConsoleMode) -> Result<(), OrdProcessError> {
    // The FFI signature `GenerateConsoleCtrlEvent` uses is the one
    // Phase 0's spike proved live (spikes/test-createprocess-v2.ps1);
    // `console.rs` explains the extra hidden-console route.
    crate::console::send_ctrl_break(pid, console_mode).map_err(OrdProcessError::Io)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nk_core::Chain;

    #[test]
    fn no_pid_file_means_nothing_detected() {
        let dir = tempfile::tempdir().unwrap();
        let env = Environment::new_default(Chain::Regtest, dir.path());
        assert_eq!(detect_running_ord(&env), None);
    }

    #[test]
    fn a_pid_file_for_a_dead_process_is_not_detected_as_running() {
        let dir = tempfile::tempdir().unwrap();
        let env = Environment::new_default(Chain::Regtest, dir.path());
        std::fs::create_dir_all(env.ord_index_dir()).unwrap();
        std::fs::write(env.ord_pid_path(), definitely_dead_pid().to_string()).unwrap();
        assert_eq!(detect_running_ord(&env), None);
    }

    #[test]
    fn no_pid_file_is_not_an_unclean_shutdown() {
        let dir = tempfile::tempdir().unwrap();
        let env = Environment::new_default(Chain::Regtest, dir.path());
        assert!(!ord_had_unclean_shutdown(&env));
    }

    #[test]
    fn a_pid_file_for_a_live_process_is_not_an_unclean_shutdown() {
        let dir = tempfile::tempdir().unwrap();
        let env = Environment::new_default(Chain::Regtest, dir.path());
        std::fs::create_dir_all(env.ord_index_dir()).unwrap();
        std::fs::write(env.ord_pid_path(), std::process::id().to_string()).unwrap();
        assert!(!ord_had_unclean_shutdown(&env));
    }

    #[test]
    fn a_pid_file_for_a_dead_process_is_an_unclean_shutdown() {
        let dir = tempfile::tempdir().unwrap();
        let env = Environment::new_default(Chain::Regtest, dir.path());
        std::fs::create_dir_all(env.ord_index_dir()).unwrap();
        std::fs::write(env.ord_pid_path(), definitely_dead_pid().to_string()).unwrap();
        assert!(ord_had_unclean_shutdown(&env));
    }

    #[test]
    fn port_in_use_maps_to_the_shared_error_code() {
        let err = OrdProcessError::PortInUse { port: 8081 };
        assert_eq!(err.code(), Some(AppErrorCode::PortInUse));
    }

    #[test]
    fn other_errors_have_no_shared_code() {
        assert_eq!(OrdProcessError::AlreadyRunning { pid: 1 }.code(), None);
        assert_eq!(OrdProcessError::StopTimeout.code(), None);
        assert_eq!(OrdProcessError::StartupTimeout.code(), None);
    }

    /// Same acceptance criterion as bitcoind's: a busy port is caught by
    /// the pre-flight check before ever trying to spawn, so a
    /// nonexistent binary path still produces `PortInUse`.
    #[tokio::test]
    async fn starting_with_a_busy_ord_port_fails_with_the_friendly_error() {
        let dir = tempfile::tempdir().unwrap();
        let mut env = Environment::new_default(Chain::Regtest, dir.path());
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        env.ord_port = listener.local_addr().unwrap().port();

        let result = OrdProcess::start(
            Path::new("this-binary-does-not-exist"),
            &env,
            Path::new("/cookie"),
            Path::new("/bitcoin"),
        )
        .await;

        assert!(matches!(
            result,
            Err(OrdProcessError::PortInUse { port }) if port == env.ord_port
        ));
        drop(listener);
    }

    /// Same helper as `bitcoind.rs`'s identical one.
    fn random_free_port() -> u16 {
        TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }

    /// A long-lived dummy standing in for an ord that is never going to
    /// become ready -- deliberately *not* `kill_on_drop`, so only
    /// `wait_ready_or_clean_up`'s own cleanup can end it. Same rationale
    /// as `bitcoind.rs`'s identical helper.
    fn spawn_long_lived_dummy() -> tokio::process::Child {
        let mut command =
            tokio::process::Command::new(if cfg!(windows) { "ping" } else { "sleep" });
        if cfg!(windows) {
            command.args(["-n", "30", "127.0.0.1"]);
        } else {
            command.arg("30");
        }
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        command
            .spawn()
            .expect("failed to spawn a long-lived process")
    }

    /// The same orphan bug bitcoind's own equivalent test catches
    /// (PROGRESS.md, Phase 10M/10R): an ord that never answers `/status`
    /// used to be dropped, still running, the moment the wait gave up.
    #[tokio::test]
    async fn a_process_that_never_becomes_ready_is_killed_not_orphaned() {
        let child = spawn_long_lived_dummy();
        let pid = child.id().expect("a just-spawned child has a pid");
        let process = OrdProcess {
            child,
            pid,
            started_at: std::time::Instant::now(),
            console_mode: ConsoleMode::current(),
        };

        let dir = tempfile::tempdir().unwrap();
        let environment = Environment::new_default(Chain::Regtest, dir.path());
        std::fs::create_dir_all(environment.ord_index_dir()).unwrap();
        std::fs::write(environment.ord_pid_path(), pid.to_string()).unwrap();

        assert!(
            process_is_alive(pid),
            "the dummy should be alive to start with"
        );

        let unused_port = random_free_port();
        let result = OrdProcess::wait_ready_or_clean_up(
            process,
            &environment,
            format!("http://127.0.0.1:{unused_port}"),
            nk_exec::Executor::new(),
            "test".to_string(),
            Duration::from_millis(300),
        )
        .await;

        assert!(
            matches!(result, Err(OrdProcessError::StartupTimeout)),
            "{:?}",
            result.err()
        );
        assert!(
            !process_is_alive(pid),
            "a process that never became ready must be killed, not left running"
        );
        assert!(
            !environment.ord_pid_path().exists(),
            "the pid file must not survive pointing at a killed process"
        );
    }

    /// `OrdClient` has no request timeout of its own (same fact
    /// bitcoind's equivalent test exercises), so a connection accepted
    /// but never answered must not be able to hang the ready wait past
    /// its deadline.
    #[tokio::test]
    async fn a_connection_that_never_answers_cannot_hang_the_ready_wait_forever() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let held = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let held_by_thread = held.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                held_by_thread.lock().unwrap().push(stream);
            }
        });

        let started = std::time::Instant::now();
        let result = tokio::time::timeout(
            Duration::from_secs(10),
            OrdProcess::wait_ready(
                format!("http://127.0.0.1:{port}"),
                nk_exec::Executor::new(),
                "test".to_string(),
                Duration::from_millis(200),
            ),
        )
        .await
        .expect("a stuck connection must not hang the wait past the test's own outer timeout");
        assert!(
            matches!(result, Err(OrdProcessError::StartupTimeout)),
            "{:?}",
            result.err()
        );
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "took {:?}",
            started.elapsed()
        );
        drop(held);
    }

    /// An ord that already exited on its own (crashed, killed from Task
    /// Manager) is a *successful* stop on every console route -- most
    /// importantly the Windows hidden-console one, where the signal
    /// cannot even be attempted for a process that no longer exists
    /// (`AttachConsole` fails), which used to make Stop / Safe Eject
    /// report a failure for something that wasn't running. Also cleans up
    /// the pid file, so it doesn't later read as an unclean shutdown.
    #[tokio::test]
    async fn stopping_an_ord_that_already_exited_succeeds_and_removes_its_pid_file() {
        for console_mode in [ConsoleMode::Shared, ConsoleMode::Hidden] {
            let dir = tempfile::tempdir().unwrap();
            let env = Environment::new_default(Chain::Regtest, dir.path());
            std::fs::create_dir_all(env.ord_index_dir()).unwrap();

            let mut command =
                tokio::process::Command::new(if cfg!(windows) { "cmd" } else { "true" });
            if cfg!(windows) {
                command.args(["/C", "exit", "0"]);
            }
            let mut child = command
                .spawn()
                .expect("failed to spawn a throwaway process");
            let pid = child.id().expect("a just-spawned child has a pid");
            child.wait().await.expect("throwaway process should exit");
            std::fs::write(env.ord_pid_path(), pid.to_string()).unwrap();

            let process = OrdProcess {
                child,
                pid,
                started_at: std::time::Instant::now(),
                console_mode,
            };
            let status = process
                .stop(&env, Duration::from_secs(5))
                .await
                .unwrap_or_else(|e| {
                    panic!("stopping an already-exited ord ({console_mode:?}): {e}")
                });

            assert!(status.success());
            assert!(
                !env.ord_pid_path().exists(),
                "pid file should be removed ({console_mode:?})"
            );
        }
    }

    /// Same helper (and rationale) as `bitcoind.rs`'s and `lock.rs`'s
    /// tests: a PID that definitely doesn't belong to a live process.
    fn definitely_dead_pid() -> u32 {
        let mut child = std::process::Command::new(if cfg!(windows) { "cmd" } else { "true" })
            .args(if cfg!(windows) {
                &["/C", "exit"][..]
            } else {
                &[][..]
            })
            .spawn()
            .expect("failed to spawn a throwaway process");
        let pid = child.id();
        child.wait().expect("failed to wait for throwaway process");
        pid
    }
}
