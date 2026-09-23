//! Spawns and gracefully stops bitcoind per environment (docs/SPEC.md
//! Foundation C). Long-lived daemon lifecycle is nk-proc's own direct
//! responsibility (hence the `disallowed_methods` exemption on this
//! crate) — distinct from nk-exec, which handles short-lived one-shot
//! commands and RPC calls *against* an already-running node.

use crate::process_check::process_is_alive;
use nk_core::{AppErrorCode, Environment};
use std::net::TcpListener;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BitcoindError {
    #[error("bitcoind is already running on this data directory (pid {pid})")]
    AlreadyRunning { pid: u32 },
    /// Detected *before* spawning (docs/SPEC.md item 8's "port in use"
    /// friendly error): a pre-flight bind check on the RPC/P2P port,
    /// not a parse of bitcoind's own startup failure message, which
    /// would mean spawning it first and racing its stderr against a
    /// string pattern. Checking first is deterministic and testable
    /// without a real bitcoind binary at all.
    #[error("port {port} is already in use")]
    PortInUse { port: u16 },
    #[error("failed to spawn bitcoind: {0}")]
    Spawn(std::io::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("bitcoind did not exit within the timeout after being asked to stop")]
    StopTimeout,
}

impl BitcoindError {
    /// The shared plain-language error code this failure maps to, if
    /// any (docs/SPEC.md item 8) — `None` for failures with no good
    /// user-facing code, which stay backend-only technical detail.
    pub fn code(&self) -> Option<AppErrorCode> {
        match self {
            Self::PortInUse { .. } => Some(AppErrorCode::PortInUse),
            Self::AlreadyRunning { .. } | Self::Spawn(_) | Self::Io(_) | Self::StopTimeout => None,
        }
    }
}

pub struct BitcoindProcess {
    child: tokio::process::Child,
    pub pid: u32,
}

impl BitcoindProcess {
    /// Starts bitcoind for `environment`. Refuses if one is already
    /// running on the same data directory (docs/SPEC.md Foundation C:
    /// "Never start a second bitcoind on the same data directory").
    ///
    /// Passes `-datadir` and the chain flag only — everything else
    /// (`txindex`, `prune`, RPC binding, `dbcache`, ...) comes from the
    /// generated `bitcoin.conf` already written into that data
    /// directory (`nk_core::bitcoin_conf`), so it isn't duplicated here.
    pub async fn start(
        binary_path: &Path,
        environment: &Environment,
    ) -> Result<Self, BitcoindError> {
        if let Some(pid) = detect_running_bitcoind(environment) {
            return Err(BitcoindError::AlreadyRunning { pid });
        }
        check_port_available(environment.rpc_port)?;
        check_port_available(environment.p2p_port)?;

        let datadir = environment.bitcoin_datadir_arg();
        std::fs::create_dir_all(&datadir)?;

        let mut args = vec![format!("-datadir={}", datadir.display())];
        if let Some(flag) = environment.chain.bitcoin_cli_flag() {
            args.push(flag.to_string());
        }

        let child = tokio::process::Command::new(binary_path)
            .args(&args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(BitcoindError::Spawn)?;
        let pid = child.id().expect("a just-spawned child process has a pid");
        // Windows builds of bitcoind have no `-daemon` flag (confirmed in
        // the Phase 0 spike, DECISIONS.md) — nk-proc always runs it as a
        // tracked foreground child on every OS, which is exactly this.
        Ok(Self { child, pid })
    }

    /// Graceful stop (docs/SPEC.md Foundation C): the `stop` RPC, then
    /// wait for the process to actually exit, up to `timeout` (spec
    /// default: 120s, configurable — the caller decides the duration).
    /// The RPC call is allowed to fail (e.g. the node already went away
    /// on its own) — we still wait for the process to exit either way.
    pub async fn stop(
        mut self,
        rpc: &nk_rpc::RpcClient,
        timeout: Duration,
    ) -> Result<(), BitcoindError> {
        let _ = rpc.stop().await;
        match tokio::time::timeout(timeout, self.child.wait()).await {
            Ok(Ok(_status)) => Ok(()),
            Ok(Err(e)) => Err(BitcoindError::Io(e)),
            Err(_elapsed) => Err(BitcoindError::StopTimeout),
        }
    }

    /// Immediately force-kills the process with no graceful RPC stop.
    /// Only for cleanup paths where graceful shutdown isn't possible
    /// (e.g. a test fixture tearing down after a panic) — prefer
    /// `stop()` for the real, spec-mandated graceful-stop path.
    pub fn kill_sync(&mut self) {
        let _ = self.child.start_kill();
    }
}

/// Detects an already-running bitcoind on this environment's data
/// directory via its own `bitcoind.pid` file (confirmed live in the
/// Phase 0 spike: plain numeric PID, written to `<chain-dir>/
/// bitcoind.pid`) plus a liveness check — the same pattern the
/// single-instance lock (`lock.rs`) uses for stale-lock detection.
pub fn detect_running_bitcoind(environment: &Environment) -> Option<u32> {
    let pid_path = environment.bitcoin_chain_dir().join("bitcoind.pid");
    let pid: u32 = std::fs::read_to_string(pid_path)
        .ok()?
        .trim()
        .parse()
        .ok()?;
    process_is_alive(pid).then_some(pid)
}

/// A pre-flight-only check: binding and immediately dropping a listener
/// tells us the port was free *at that instant*, not that it will still
/// be free by the time bitcoind itself tries to bind it (there's an
/// unavoidable TOCTOU gap either way) — good enough for the spec's
/// "produces the friendly error" bar, not a hard guarantee.
fn check_port_available(port: u16) -> Result<(), BitcoindError> {
    TcpListener::bind(("127.0.0.1", port))
        .map(|_listener| ())
        .map_err(|_| BitcoindError::PortInUse { port })
}

#[cfg(test)]
mod tests {
    use super::*;
    use nk_core::Chain;

    #[test]
    fn no_pid_file_means_nothing_detected() {
        let dir = tempfile::tempdir().unwrap();
        let env = Environment::new_default(Chain::Regtest, dir.path());
        assert_eq!(detect_running_bitcoind(&env), None);
    }

    #[test]
    fn a_pid_file_for_a_dead_process_is_not_detected_as_running() {
        let dir = tempfile::tempdir().unwrap();
        let env = Environment::new_default(Chain::Regtest, dir.path());
        let chain_dir = env.bitcoin_chain_dir();
        std::fs::create_dir_all(&chain_dir).unwrap();
        std::fs::write(
            chain_dir.join("bitcoind.pid"),
            definitely_dead_pid().to_string(),
        )
        .unwrap();
        assert_eq!(detect_running_bitcoind(&env), None);
    }

    #[test]
    fn port_in_use_maps_to_the_shared_error_code() {
        let err = BitcoindError::PortInUse { port: 8332 };
        assert_eq!(err.code(), Some(AppErrorCode::PortInUse));
    }

    #[test]
    fn other_errors_have_no_shared_code() {
        assert_eq!(BitcoindError::AlreadyRunning { pid: 1 }.code(), None);
        assert_eq!(BitcoindError::StopTimeout.code(), None);
    }

    /// The Phase 3 [CI] acceptance criterion ("a busy port produces the
    /// friendly error"): occupy the RPC port first, then attempt to
    /// start bitcoind -- the pre-flight check must catch this *before*
    /// ever trying to spawn a process, so a nonexistent binary path
    /// still produces `PortInUse`, not a spawn failure.
    #[tokio::test]
    async fn starting_with_a_busy_rpc_port_fails_with_the_friendly_error() {
        let dir = tempfile::tempdir().unwrap();
        let mut env = Environment::new_default(Chain::Regtest, dir.path());
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        env.rpc_port = listener.local_addr().unwrap().port();

        let result = BitcoindProcess::start(Path::new("this-binary-does-not-exist"), &env).await;

        assert!(matches!(
            result,
            Err(BitcoindError::PortInUse { port }) if port == env.rpc_port
        ));
        drop(listener);
    }

    /// A PID that's real enough to have existed a moment ago but is
    /// guaranteed not to be running anymore -- see the identical helper
    /// (and its rationale) in `lock.rs`'s tests.
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
