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
use nk_proc::{BitcoindProcess, OrdProcess};
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
    #[error("bitcoind is already running for {chain:?}")]
    BitcoindAlreadyRunning { chain: Chain },
    #[error("bitcoind is not running for {chain:?}")]
    BitcoindNotRunning { chain: Chain },
    #[error("ord is already running for {chain:?}")]
    OrdAlreadyRunning { chain: Chain },
    #[error("ord is not running for {chain:?}")]
    OrdNotRunning { chain: Chain },
    #[error("bitcoind error: {0}")]
    Bitcoind(#[from] nk_proc::BitcoindError),
    #[error("ord process error: {0}")]
    OrdProcess(#[from] nk_proc::OrdProcessError),
    #[error("ord api error: {0}")]
    OrdApi(#[from] nk_ord::OrdApiError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("rpc error: {0}")]
    Rpc(#[from] nk_rpc::RpcError),
}

impl NodeManagerError {
    pub fn code(&self) -> Option<AppErrorCode> {
        match self {
            Self::Bitcoind(e) => e.code(),
            Self::OrdProcess(e) => e.code(),
            Self::BitcoindAlreadyRunning { .. }
            | Self::BitcoindNotRunning { .. }
            | Self::OrdAlreadyRunning { .. }
            | Self::OrdNotRunning { .. }
            | Self::OrdApi(_)
            | Self::Io(_)
            | Self::Rpc(_) => None,
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

/// docs/SPEC.md item 2's ord dashboard section: "index height vs node
/// height, indexing / caught-up status, and which index options are
/// enabled." `index_sats`/`index_runes`/`index_addresses` reflect
/// ord's own report of its *active* flags (`/status`'s `*_index`
/// booleans), not just what Nodekeeper configured -- a second,
/// independent confirmation (DECISIONS.md Phase 4 VERIFY), same
/// reasoning as everywhere else this project prefers checking the real
/// running state over trusting its own configuration.
#[derive(Debug, Clone, Serialize, TS)]
pub struct OrdStatus {
    #[ts(type = "number")]
    pub index_height: u64,
    #[ts(type = "number")]
    pub node_height: u64,
    pub caught_up: bool,
    pub index_sats: bool,
    pub index_runes: bool,
    pub index_addresses: bool,
    #[ts(type = "number")]
    pub uptime_seconds: u64,
}

struct RunningNode {
    process: BitcoindProcess,
    rpc: RpcClient,
    environment: Environment,
}

struct RunningOrd {
    process: OrdProcess,
    client: nk_ord::OrdClient,
    environment: Environment,
}

/// App-wide (one per running Nodekeeper instance) tracker of started
/// bitcoind and ord processes, keyed by chain -- there is no support yet
/// for more than one running environment per chain (docs/SPEC.md's
/// "several environments... side by side" is multiple *chains* at once,
/// not multiple instances of the same chain). bitcoind and ord are
/// tracked separately (two maps, not one) because docs/SPEC.md item 2
/// treats them as independently startable/stoppable services ("Start /
/// stop / restart per service through the process manager"), not a
/// single bundled unit.
#[derive(Default)]
pub struct NodeManager {
    running: Mutex<HashMap<Chain, RunningNode>>,
    running_ord: Mutex<HashMap<Chain, RunningOrd>>,
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
                return Err(NodeManagerError::BitcoindAlreadyRunning { chain });
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
                .ok_or(NodeManagerError::BitcoindNotRunning { chain })?
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
                .ok_or(NodeManagerError::BitcoindNotRunning { chain })?;
            (
                node.rpc.clone(),
                node.environment.data_root.clone(),
                node.process.started_at,
            )
        }; // lock released before the .await calls below

        // background: true -- this is the Dashboard's periodic status
        // poll (docs/SPEC.md item 7's "background polling"), called
        // every few seconds for as long as a screen is open.
        let blockchain_info = rpc.get_blockchain_info(true).await?;
        let network_info = rpc.get_network_info(true).await?;
        let mempool_info = rpc.get_mempool_info(true).await?;

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

    /// Starts `chain`'s ord server, pointed at this manager's own
    /// already-running bitcoind for that chain (its cookie file and
    /// data directory) -- refuses if bitcoind isn't running yet
    /// (docs/SPEC.md item 1: ord always connects to Nodekeeper's own
    /// bitcoind, never a default/external one) or if ord already is.
    pub async fn start_ord(
        &self,
        chain: Chain,
        binary_path: &Path,
        executor: Executor,
        environment: Environment,
    ) -> Result<(), NodeManagerError> {
        {
            let running_ord = self
                .running_ord
                .lock()
                .expect("mutex should not be poisoned");
            if running_ord.contains_key(&chain) {
                return Err(NodeManagerError::OrdAlreadyRunning { chain });
            }
        }
        {
            let running = self.running.lock().expect("mutex should not be poisoned");
            if !running.contains_key(&chain) {
                return Err(NodeManagerError::BitcoindNotRunning { chain });
            }
        } // both locks released before the .await below

        let cookie_path = environment.bitcoin_cookie_path();
        let bitcoin_datadir = environment.bitcoin_datadir_arg();

        let (process, client) = OrdProcess::start_and_wait_ready(
            binary_path,
            &environment,
            &cookie_path,
            &bitcoin_datadir,
            format!("http://127.0.0.1:{}", environment.ord_port),
            executor,
            environment.name.clone(),
            // Same 60s startup budget as bitcoind's own start_and_wait_
            // ready, same real-machine-load reasoning.
            Duration::from_secs(60),
        )
        .await?;

        let mut running_ord = self
            .running_ord
            .lock()
            .expect("mutex should not be poisoned");
        running_ord.insert(
            chain,
            RunningOrd {
                process,
                client,
                environment,
            },
        );
        Ok(())
    }

    /// Graceful stop for ord (a signal, not an RPC call -- ord has
    /// none), same "remove from tracking either way" contract as
    /// bitcoind's own `stop`.
    pub async fn stop_ord(&self, chain: Chain, timeout: Duration) -> Result<(), NodeManagerError> {
        let ord = {
            let mut running_ord = self
                .running_ord
                .lock()
                .expect("mutex should not be poisoned");
            running_ord
                .remove(&chain)
                .ok_or(NodeManagerError::OrdNotRunning { chain })?
        };
        ord.process.stop(&ord.environment, timeout).await?;
        Ok(())
    }

    /// Live status for the dashboard's ord section (docs/SPEC.md item
    /// 2). Reads bitcoind's current height in the same call rather than
    /// relying on the frontend's separately-polled `status()` result,
    /// so `caught_up` never compares against a stale node height.
    pub async fn ord_status(&self, chain: Chain) -> Result<OrdStatus, NodeManagerError> {
        let bitcoin_rpc = {
            let running = self.running.lock().expect("mutex should not be poisoned");
            running
                .get(&chain)
                .ok_or(NodeManagerError::BitcoindNotRunning { chain })?
                .rpc
                .clone()
        };
        let (client, started_at) = {
            let running_ord = self
                .running_ord
                .lock()
                .expect("mutex should not be poisoned");
            let ord = running_ord
                .get(&chain)
                .ok_or(NodeManagerError::OrdNotRunning { chain })?;
            (ord.client.clone(), ord.process.started_at)
        }; // both locks released before the .await calls below

        // background: true -- same periodic dashboard poll reasoning as
        // bitcoind's own status().
        let blockchain_info = bitcoin_rpc.get_blockchain_info(true).await?;
        let node_height = blockchain_info
            .get("blocks")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let status = client.status(true).await?;
        let index_height = status.get("height").and_then(|v| v.as_u64()).unwrap_or(0);

        Ok(OrdStatus {
            index_height,
            node_height,
            caught_up: index_height >= node_height,
            index_sats: status
                .get("sat_index")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            index_runes: status
                .get("rune_index")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            index_addresses: status
                .get("address_index")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            uptime_seconds: started_at.elapsed().as_secs(),
        })
    }

    /// Same "lets the frontend distinguish not-running from an error"
    /// reasoning as `is_running`, for ord.
    pub fn is_ord_running(&self, chain: Chain) -> bool {
        self.running_ord
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
            Err(NodeManagerError::BitcoindAlreadyRunning {
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
            Err(NodeManagerError::BitcoindNotRunning {
                chain: Chain::Regtest
            })
        ));
    }

    /// Real end-to-end coverage for ord, mirroring the bitcoind test
    /// above: refuses to start ord before bitcoind is running, starts
    /// it against a real running bitcoind, reports real status (height
    /// vs node height, active index options), refuses a second
    /// instance, and stops gracefully.
    #[tokio::test]
    #[serial_test::serial(real_bitcoind)]
    async fn starts_ord_reports_status_and_stops_it() {
        let (Some(bitcoind_path), Some(ord_path)) = (
            std::env::var_os("NK_TEST_BITCOIND"),
            std::env::var_os("NK_TEST_ORD"),
        ) else {
            eprintln!("skipping: NK_TEST_BITCOIND and/or NK_TEST_ORD not set");
            return;
        };
        let bitcoind_path = std::path::PathBuf::from(bitcoind_path);
        let ord_path = std::path::PathBuf::from(ord_path);

        let dir = tempfile::tempdir().unwrap();
        let mut environment = Environment::new_default(Chain::Regtest, dir.path());
        environment.rpc_port = random_free_port();
        environment.p2p_port = random_free_port();

        let manager = NodeManager::new();

        // ord can't start before its chain's bitcoind is running --
        // checked before ord_port is even picked below, so this can't
        // be affected by the port-reservation race the next comment
        // describes.
        let too_early = manager
            .start_ord(
                Chain::Regtest,
                &ord_path,
                Executor::new(),
                environment.clone(),
            )
            .await;
        assert!(matches!(
            too_early,
            Err(NodeManagerError::BitcoindNotRunning {
                chain: Chain::Regtest
            })
        ));

        manager
            .start(
                Chain::Regtest,
                &bitcoind_path,
                Executor::new(),
                environment.clone(),
            )
            .await
            .expect("bitcoind should start");

        // Picked here, immediately before use -- not at the top of the
        // test alongside rpc_port/p2p_port. Reserving it that much
        // earlier left enough of a gap (bitcoind's own startup above)
        // for something else on the machine to grab the same ephemeral
        // port, reproducing as a real `PortInUse` failure (same root
        // cause fixed in `nk-testkit`'s `RegtestFixture::start_ord`).
        environment.ord_port = random_free_port();

        manager
            .start_ord(
                Chain::Regtest,
                &ord_path,
                Executor::new(),
                environment.clone(),
            )
            .await
            .expect("ord should start");
        assert!(manager.is_ord_running(Chain::Regtest));

        let second_start = manager
            .start_ord(Chain::Regtest, &ord_path, Executor::new(), environment)
            .await;
        assert!(matches!(
            second_start,
            Err(NodeManagerError::OrdAlreadyRunning {
                chain: Chain::Regtest
            })
        ));

        let status = manager.ord_status(Chain::Regtest).await.unwrap();
        assert_eq!(status.index_height, 0);
        assert_eq!(status.node_height, 0);
        assert!(status.caught_up);
        // Regtest defaults to all three index options on
        // (Chain::default_index_options).
        assert!(status.index_sats);
        assert!(status.index_runes);
        assert!(status.index_addresses);

        manager
            .stop_ord(Chain::Regtest, Duration::from_secs(15))
            .await
            .expect("ord should stop cleanly");
        assert!(!manager.is_ord_running(Chain::Regtest));

        assert!(matches!(
            manager.ord_status(Chain::Regtest).await,
            Err(NodeManagerError::OrdNotRunning {
                chain: Chain::Regtest
            })
        ));

        manager
            .stop(Chain::Regtest, Duration::from_secs(30))
            .await
            .expect("bitcoind should stop cleanly");
    }

    #[test]
    fn already_running_and_not_running_map_to_no_shared_error_code() {
        assert_eq!(
            NodeManagerError::BitcoindAlreadyRunning {
                chain: Chain::Regtest
            }
            .code(),
            None
        );
        assert_eq!(
            NodeManagerError::BitcoindNotRunning {
                chain: Chain::Regtest
            }
            .code(),
            None
        );
        assert_eq!(
            NodeManagerError::OrdAlreadyRunning {
                chain: Chain::Regtest
            }
            .code(),
            None
        );
        assert_eq!(
            NodeManagerError::OrdNotRunning {
                chain: Chain::Regtest
            }
            .code(),
            None
        );
    }
}
