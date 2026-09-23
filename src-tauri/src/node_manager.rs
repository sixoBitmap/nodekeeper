//! Per-environment bitcoind lifecycle for the dashboard (docs/SPEC.md
//! item 2): tracks running processes, and composes `nk-core`/`nk-proc`/
//! `nk-rpc` into start/stop/status operations. Deliberately not a
//! `#[tauri::command]` itself (CLAUDE.md: "business logic lives in
//! crates, not in Tauri command handlers, so it is testable without the
//! GUI") — `lib.rs`'s commands are thin wrappers around this.
//!
//! **Scoping note**: `start()` takes an already-known `binary_path`
//! rather than locating or downloading one itself. There is no
//! "download Bitcoin Core into a real, persistent install location"
//! flow yet (`nk-verify`'s download+verify path is currently only
//! exercised by the CI/dev helper `fetch_bitcoin_core.rs` and
//! `nk-testkit`'s ephemeral fixtures) -- that's the setup wizard's job
//! (docs/SPEC.md item 1), not yet built as a UI flow. For now, the
//! caller (a Tauri command) reads the path from the existing settings
//! table (`bitcoind_path`, via `get_setting`/`set_setting` from Phase 1)
//! and returns `AppErrorCode::BinaryNotVerified` if it's unset. This
//! makes the dashboard's start/stop controls genuinely functional
//! end-to-end against a real, once-manually-configured binary without
//! block-and-ask on building the full wizard UI first.

use nk_core::disk::{disk_usage_for, DiskUsage};
use nk_core::{bitcoin_conf::generate_bitcoin_conf, system_check::run_system_check};
use nk_core::{AppErrorCode, Chain, Environment};
use nk_exec::Executor;
use nk_proc::BitcoindProcess;
use nk_rpc::RpcClient;
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;
use thiserror::Error;
use ts_rs::TS;

#[derive(Debug, Error)]
pub enum NodeManagerError {
    #[error("{chain:?} is already running")]
    AlreadyRunning { chain: Chain },
    #[error("{chain:?} is not running")]
    NotRunning { chain: Chain },
    #[error("bitcoind error: {0}")]
    Bitcoind(#[from] nk_proc::BitcoindError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("rpc error: {0}")]
    Rpc(#[from] nk_rpc::RpcError),
}

impl NodeManagerError {
    pub fn code(&self) -> Option<AppErrorCode> {
        match self {
            Self::Bitcoind(e) => e.code(),
            Self::AlreadyRunning { .. } | Self::NotRunning { .. } | Self::Io(_) | Self::Rpc(_) => {
                None
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct NodeStatus {
    #[ts(type = "number")]
    pub blocks: u64,
    #[ts(type = "number")]
    pub headers: u64,
    pub verification_progress: f64,
    pub initial_block_download: bool,
    #[ts(type = "number")]
    pub peers: u64,
    #[ts(type = "number")]
    pub mempool_transactions: u64,
    #[ts(type = "number")]
    pub mempool_bytes: u64,
    pub disk: DiskUsage,
    #[ts(type = "number")]
    pub uptime_seconds: u64,
}

struct RunningNode {
    process: BitcoindProcess,
    rpc: RpcClient,
    environment: Environment,
}

/// App-wide (one per running Nodekeeper instance) tracker of started
/// bitcoind processes, keyed by chain -- there is no support yet for
/// more than one running environment per chain (docs/SPEC.md's "several
/// environments... side by side" is multiple *chains* at once, not
/// multiple instances of the same chain).
#[derive(Default)]
pub struct NodeManager {
    running: Mutex<HashMap<Chain, RunningNode>>,
}

impl NodeManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts `chain`'s bitcoind: generates and writes `bitcoin.conf`,
    /// spawns the process, and waits for it to actually become ready
    /// (cookie file + responsive RPC) before returning. Refuses if this
    /// `NodeManager` is already tracking a running process for `chain`
    /// -- `BitcoindProcess::start`'s own pid-file-based detection still
    /// separately catches a bitcoind started outside this app instance.
    pub async fn start(
        &self,
        chain: Chain,
        binary_path: &Path,
        executor: Executor,
        environment: Environment,
    ) -> Result<(), NodeManagerError> {
        {
            let running = self.running.lock().expect("mutex should not be poisoned");
            if running.contains_key(&chain) {
                return Err(NodeManagerError::AlreadyRunning { chain });
            }
        } // lock released before the .await below

        let datadir = environment.bitcoin_datadir_arg();
        std::fs::create_dir_all(&datadir)?;
        let system = run_system_check(&environment.data_root);
        let conf = generate_bitcoin_conf(
            chain,
            environment.rpc_port,
            environment.p2p_port,
            system.available_memory_bytes,
            // Other environments' dbcache reservations aren't accounted
            // for yet -- a real gap once multiple environments run
            // concurrently (Phase 8's Test Lab is the first place that
            // actually needs it), same simplification nk-testkit's
            // fixture already makes.
            0,
        );
        std::fs::write(datadir.join("bitcoin.conf"), conf)?;

        let (process, rpc) = BitcoindProcess::start_and_wait_ready(
            binary_path,
            &environment,
            format!("http://127.0.0.1:{}", environment.rpc_port),
            executor,
            environment.name.clone(),
            // 60s: real users' machines can be under just as much load
            // (antivirus scanning a freshly-written binary, a slow
            // spinning disk, other environments starting concurrently)
            // as CI's runners, where this margin was shown necessary —
            // see DECISIONS.md.
            Duration::from_secs(60),
        )
        .await?;

        let mut running = self.running.lock().expect("mutex should not be poisoned");
        running.insert(
            chain,
            RunningNode {
                process,
                rpc,
                environment,
            },
        );
        Ok(())
    }

    /// Graceful stop (docs/SPEC.md Foundation C), removing `chain` from
    /// the tracked set either way -- once `stop` has been asked for,
    /// the process is no longer this manager's to track even if the
    /// graceful RPC stop itself times out (the caller can still see the
    /// error and decide whether to force-kill separately).
    pub async fn stop(&self, chain: Chain, timeout: Duration) -> Result<(), NodeManagerError> {
        let node = {
            let mut running = self.running.lock().expect("mutex should not be poisoned");
            running
                .remove(&chain)
                .ok_or(NodeManagerError::NotRunning { chain })?
        };
        node.process.stop(&node.rpc, timeout).await?;
        Ok(())
    }

    /// Live status for the dashboard (docs/SPEC.md item 2): sync
    /// progress, peers, mempool, disk usage, uptime.
    pub async fn status(&self, chain: Chain) -> Result<NodeStatus, NodeManagerError> {
        let (rpc, data_root, started_at) = {
            let running = self.running.lock().expect("mutex should not be poisoned");
            let node = running
                .get(&chain)
                .ok_or(NodeManagerError::NotRunning { chain })?;
            (
                node.rpc.clone(),
                node.environment.data_root.clone(),
                node.process.started_at,
            )
        }; // lock released before the .await calls below

        let blockchain_info = rpc.get_blockchain_info().await?;
        let network_info = rpc.get_network_info().await?;
        let mempool_info = rpc.get_mempool_info().await?;

        Ok(NodeStatus {
            blocks: blockchain_info
                .get("blocks")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            headers: blockchain_info
                .get("headers")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            verification_progress: blockchain_info
                .get("verificationprogress")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0),
            initial_block_download: blockchain_info
                .get("initialblockdownload")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            peers: network_info
                .get("connections")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            mempool_transactions: mempool_info
                .get("size")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            mempool_bytes: mempool_info
                .get("bytes")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            disk: disk_usage_for(&data_root),
            uptime_seconds: started_at.elapsed().as_secs(),
        })
    }

    /// Whether this manager is currently tracking a running process for
    /// `chain` -- lets the frontend distinguish "not running" from an
    /// actual status-fetch error without needing to call `status` first.
    pub fn is_running(&self, chain: Chain) -> bool {
        self.running
            .lock()
            .expect("mutex should not be poisoned")
            .contains_key(&chain)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    fn random_free_port() -> u16 {
        TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }

    /// Real end-to-end coverage against a real bitcoind, not mocked --
    /// gated the same way as every other real-binary test this project
    /// uses, so `cargo test` doesn't need a pre-staged binary on every
    /// dev machine.
    ///
    /// `#[serial(real_bitcoind)]`, same lock name `nk-testkit`'s
    /// real-bitcoind tests use (cross-process via serial_test's
    /// `file_locks` feature) -- several full bitcoind processes running
    /// at once starved CI's windows-latest runners of enough time to
    /// become ready even at a 60s timeout (DECISIONS.md).
    #[tokio::test]
    #[serial_test::serial(real_bitcoind)]
    async fn starts_reports_status_and_stops_a_real_node() {
        let Some(binary_path) = std::env::var_os("NK_TEST_BITCOIND") else {
            eprintln!("skipping: NK_TEST_BITCOIND not set");
            return;
        };
        let binary_path = std::path::PathBuf::from(binary_path);

        let dir = tempfile::tempdir().unwrap();
        let mut environment = Environment::new_default(Chain::Regtest, dir.path());
        environment.rpc_port = random_free_port();
        environment.p2p_port = random_free_port();

        let manager = NodeManager::new();
        assert!(!manager.is_running(Chain::Regtest));

        manager
            .start(
                Chain::Regtest,
                &binary_path,
                Executor::new(),
                environment.clone(),
            )
            .await
            .expect("node should start");
        assert!(manager.is_running(Chain::Regtest));

        // Starting the same chain again while already running must be
        // refused, not silently spawn a second process.
        let second_start = manager
            .start(Chain::Regtest, &binary_path, Executor::new(), environment)
            .await;
        assert!(matches!(
            second_start,
            Err(NodeManagerError::AlreadyRunning {
                chain: Chain::Regtest
            })
        ));

        let status = manager.status(Chain::Regtest).await.unwrap();
        assert_eq!(status.blocks, 0);
        assert_eq!(status.peers, 0);

        manager
            .stop(Chain::Regtest, Duration::from_secs(30))
            .await
            .expect("node should stop cleanly");
        assert!(!manager.is_running(Chain::Regtest));

        // Once stopped, status must report NotRunning, not stale data.
        assert!(matches!(
            manager.status(Chain::Regtest).await,
            Err(NodeManagerError::NotRunning {
                chain: Chain::Regtest
            })
        ));
    }

    #[test]
    fn already_running_and_not_running_map_to_no_shared_error_code() {
        assert_eq!(
            NodeManagerError::AlreadyRunning {
                chain: Chain::Regtest
            }
            .code(),
            None
        );
        assert_eq!(
            NodeManagerError::NotRunning {
                chain: Chain::Regtest
            }
            .code(),
            None
        );
    }
}
