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
//!   live; this is its first real Rust use).

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
        command
            .args(&args)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        // New process group so a later CTRL_BREAK_EVENT targets only ord
        // (and nothing it spawns), not Nodekeeper's own process group —
        // Windows-only; Unix's SIGINT is sent straight to ord's own pid.
        #[cfg(windows)]
        {
            // `tokio::process::Command` exposes `creation_flags` as an
            // inherent method on Windows (no `CommandExt` import
            // needed, unlike `std::process::Command`).
            const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
            command.creation_flags(CREATE_NEW_PROCESS_GROUP);
        }

        let child = command.spawn().map_err(OrdProcessError::Spawn)?;
        let pid = child.id().expect("a just-spawned child process has a pid");

        std::fs::write(environment.ord_pid_path(), pid.to_string())?;

        Ok(Self {
            child,
            pid,
            started_at: std::time::Instant::now(),
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
        let client = nk_ord::OrdClient::new(base_url, executor, environment_label);

        let deadline = tokio::time::Instant::now() + ready_timeout;
        loop {
            // background: true -- this readiness poll can repeat many
            // times during a slow startup and isn't itself a meaningful
            // user-facing check (docs/SPEC.md item 7), same reasoning as
            // bitcoind's own RPC-ready poll.
            if client.status(true).await.is_ok() {
                return Ok((process, client));
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
    pub async fn stop(
        mut self,
        environment: &Environment,
        timeout: Duration,
    ) -> Result<(), OrdProcessError> {
        send_graceful_stop(self.pid)?;
        match tokio::time::timeout(timeout, self.child.wait()).await {
            Ok(Ok(_status)) => {
                let _ = std::fs::remove_file(environment.ord_pid_path());
                Ok(())
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

fn check_port_available(port: u16) -> Result<(), OrdProcessError> {
    TcpListener::bind(("127.0.0.1", port))
        .map(|_listener| ())
        .map_err(|_| OrdProcessError::PortInUse { port })
}

#[cfg(unix)]
fn send_graceful_stop(pid: u32) -> Result<(), OrdProcessError> {
    let result = unsafe { libc::kill(pid as libc::pid_t, libc::SIGINT) };
    if result == 0 {
        Ok(())
    } else {
        Err(OrdProcessError::Io(std::io::Error::last_os_error()))
    }
}

#[cfg(windows)]
#[allow(non_snake_case)]
fn send_graceful_stop(pid: u32) -> Result<(), OrdProcessError> {
    const CTRL_BREAK_EVENT: u32 = 1;

    extern "system" {
        fn GenerateConsoleCtrlEvent(dwCtrlEvent: u32, dwProcessGroupId: u32) -> i32;
    }

    // Safety: `pid` is a plain u32 process/group id (no pointers), and
    // this Win32 call has no failure mode worse than returning 0 -- the
    // exact FFI signature Phase 0's spike proved live
    // (spikes/test-createprocess-v2.ps1).
    let ok = unsafe { GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, pid) };
    if ok != 0 {
        Ok(())
    } else {
        Err(OrdProcessError::Io(std::io::Error::last_os_error()))
    }
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
