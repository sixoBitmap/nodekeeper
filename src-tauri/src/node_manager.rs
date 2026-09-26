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
use std::sync::atomic::{AtomicUsize, Ordering};
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
    /// Found alive (pid file plus liveness) after everything had been
    /// asked to stop -- see `NodeManager::stop_everything`. Deliberately
    /// worded without "after being asked to stop": it is also what an
    /// orphan from a crashed session, or a node started outside
    /// Nodekeeper, looks like, and a retry can never stop those.
    #[error("still running (process id {pid})")]
    StillRunning { pid: u32 },
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
            | Self::Rpc(_)
            | Self::StillRunning { .. } => None,
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
    /// Held for the whole of a `stop_everything` call, so two of them
    /// (a double-clicked close, Quit during Safe Eject) run one after the
    /// other instead of racing. Async because it is held across awaits.
    stop_gate: tokio::sync::Mutex<()>,
    /// How many `stop_everything` calls are running or queued.
    /// `any_running()` counts them: a stop removes each process from the
    /// tracking maps as soon as it *starts*, so without this everything
    /// would read as "stopped" while a slow bitcoind is still flushing.
    stops_in_flight: AtomicUsize,
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

    /// Whether *any* environment's bitcoind or ord is currently tracked
    /// as running, across every chain -- used to refuse changing the
    /// environment data root (docs/SPEC.md item 1's data-directory
    /// picker) while something could still be reading/writing under the
    /// old path; changing it under a running node's feet risks pointing
    /// a later command (e.g. a stop or status check) at the wrong
    /// on-disk location than the process actually running.
    pub fn any_running(&self) -> bool {
        !self
            .running
            .lock()
            .expect("mutex should not be poisoned")
            .is_empty()
            || !self
                .running_ord
                .lock()
                .expect("mutex should not be poisoned")
                .is_empty()
            || self.stops_in_flight.load(Ordering::SeqCst) > 0
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

    /// Gracefully stops every running ord and bitcoind, across all
    /// chains -- the shared core of tray Quit, Safe Eject, and the
    /// portable-mode close warning.
    ///
    /// - **Keeps going past a failure** and reports every one at the end:
    ///   one ord that won't stop must not leave its own environment's
    ///   bitcoind, or every later environment, running. (The first
    ///   version aborted at the first error, which on a Windows release
    ///   build -- where ord's stop failed -- would have orphaned
    ///   everything; DECISIONS.md, "Windows release builds: console-less
    ///   process handling".)
    /// - **Verifies, rather than trusting its own bookkeeping.** `stop`
    ///   and `stop_ord` stop *tracking* a process the moment a stop is
    ///   asked for, so after a failed or timed-out stop the process can
    ///   still be alive yet invisible here -- and a second Safe Eject
    ///   would find nothing tracked and say "safe to unplug". So once the
    ///   stops are done, each environment's `bitcoind.pid` / `ord.pid` is
    ///   checked too (`environments`: one per chain, where those files
    ///   live), and anything still alive is reported -- which also catches
    ///   an orphan left by an earlier crashed session.
    /// - **One at a time.** A second call (a double-clicked window close,
    ///   Quit while Safe Eject runs) waits for the first instead of racing
    ///   it -- racing would let it see "nothing tracked" while the first is
    ///   still waiting on a slow bitcoind and report success -- and
    ///   `any_running()` stays true for as long as any call is in flight.
    pub async fn stop_everything(&self, environments: &[Environment]) -> Vec<StopFailure> {
        self.stop_everything_via(
            &ManagerStopControl {
                manager: self,
                environments,
            },
            StopBudget::REAL,
            VERIFY_SETTLE,
        )
        .await
    }

    /// `stop_everything` over any `StopControl` (the tests use a fake),
    /// with the single-flight gate and the in-flight count.
    pub(crate) async fn stop_everything_via<C: StopControl>(
        &self,
        control: &C,
        budget: StopBudget,
        settle: Duration,
    ) -> Vec<StopFailure> {
        // Counted *before* waiting for the gate, so a caller queued
        // behind a slow stop already makes `any_running()` true.
        let _in_flight = InFlight::enter(&self.stops_in_flight);
        let _one_at_a_time = self.stop_gate.lock().await;
        stop_everything_with(control, budget, settle).await
    }
}

/// Counts a `stop_everything` call as in flight until dropped (so it is
/// undone even if the call's future is cancelled).
struct InFlight<'a>(&'a AtomicUsize);

impl<'a> InFlight<'a> {
    fn enter(counter: &'a AtomicUsize) -> Self {
        counter.fetch_add(1, Ordering::SeqCst);
        Self(counter)
    }
}

impl Drop for InFlight<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Graceful-stop budget per service when stopping everything (docs/
/// SPEC.md Foundation C: bitcoind's default is 120s; ord only has to
/// flush its index, so 30s).
pub const ORD_STOP_TIMEOUT: Duration = Duration::from_secs(30);
pub const BITCOIND_STOP_TIMEOUT: Duration = Duration::from_secs(120);

/// How long each service's stop may take before `stop_everything` gives
/// up on it and records a failure, so **nothing under the single-flight
/// gate can hang forever** -- a hung stop would otherwise hold the gate
/// (and `any_running()`) for good, and a second Quit or window close,
/// which used to be an escape hatch, would just queue behind it.
///
/// The per-service timeouts (`ord`, `bitcoind`) are what the stop itself
/// is given; the outer bound adds what the stop can legitimately spend on
/// top: bitcoind's stop bounds its RPC call *and* its wait by the timeout
/// (so up to twice it), ord's only the wait; plus `slack`.
#[derive(Clone, Copy)]
pub(crate) struct StopBudget {
    ord: Duration,
    bitcoind: Duration,
    slack: Duration,
}

impl StopBudget {
    pub(crate) const REAL: Self = Self {
        ord: ORD_STOP_TIMEOUT,
        bitcoind: BITCOIND_STOP_TIMEOUT,
        slack: Duration::from_secs(5),
    };

    fn ord_outer_bound(self) -> Duration {
        self.ord + self.slack
    }

    fn bitcoind_outer_bound(self) -> Duration {
        self.bitcoind * 2 + self.slack
    }
}

/// After the stops, how long a process that is still alive gets to finish
/// disappearing before it is reported as still running (a process that
/// has just been waited on can linger in the process table for a moment),
/// and how often it is re-checked meanwhile.
const VERIFY_SETTLE: Duration = Duration::from_secs(3);
const VERIFY_POLL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Service {
    Ord,
    Bitcoind,
}

impl std::fmt::Display for Service {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Ord => "ord",
            Self::Bitcoind => "bitcoind",
        })
    }
}

/// One service that failed to stop during `stop_everything`.
#[derive(Debug)]
pub struct StopFailure {
    pub chain: Chain,
    pub service: Service,
    pub error: NodeManagerError,
}

impl std::fmt::Display for StopFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({:?}): {}", self.service, self.chain, self.error)
    }
}

/// What `stop_everything_with` needs from a node manager. A trait only
/// so the "keep going past a failure" and "verify afterwards" logic can
/// be tested with a fake -- real processes can't be made to fail on
/// demand. The `+ Send` on each future is what lets the Tauri commands
/// that await this stay `Send`.
pub(crate) trait StopControl {
    fn is_ord_running(&self, chain: Chain) -> bool;
    fn is_running(&self, chain: Chain) -> bool;
    fn stop_ord(
        &self,
        chain: Chain,
        timeout: Duration,
    ) -> impl std::future::Future<Output = Result<(), NodeManagerError>> + Send;
    fn stop(
        &self,
        chain: Chain,
        timeout: Duration,
    ) -> impl std::future::Future<Output = Result<(), NodeManagerError>> + Send;
    /// Services of `chain` whose process is actually alive right now
    /// (pid file plus liveness), whether or not this manager is tracking
    /// them, with their pids.
    fn live_services(&self, chain: Chain) -> Vec<(Service, u32)>;
}

/// The real `StopControl`: the manager's own tracked processes, plus each
/// chain's pid files for the verification pass.
struct ManagerStopControl<'a> {
    manager: &'a NodeManager,
    environments: &'a [Environment],
}

impl StopControl for ManagerStopControl<'_> {
    fn is_ord_running(&self, chain: Chain) -> bool {
        self.manager.is_ord_running(chain)
    }
    fn is_running(&self, chain: Chain) -> bool {
        self.manager.is_running(chain)
    }
    async fn stop_ord(&self, chain: Chain, timeout: Duration) -> Result<(), NodeManagerError> {
        self.manager.stop_ord(chain, timeout).await
    }
    async fn stop(&self, chain: Chain, timeout: Duration) -> Result<(), NodeManagerError> {
        self.manager.stop(chain, timeout).await
    }
    fn live_services(&self, chain: Chain) -> Vec<(Service, u32)> {
        let Some(environment) = self.environments.iter().find(|e| e.chain == chain) else {
            return Vec::new();
        };
        let mut live = Vec::new();
        if let Some(pid) = nk_proc::detect_running_ord(environment) {
            live.push((Service::Ord, pid));
        }
        if let Some(pid) = nk_proc::detect_running_bitcoind(environment) {
            live.push((Service::Bitcoind, pid));
        }
        live
    }
}

/// Per chain, ord first (it depends on bitcoind being reachable while
/// it shuts down), then bitcoind. A failure is recorded and the loop
/// carries on; nothing here returns early. Afterwards every chain is
/// verified against reality (`live_services`), giving anything still
/// alive up to `settle` to finish going away: "untracked" is not
/// "stopped".
pub(crate) async fn stop_everything_with<C: StopControl>(
    control: &C,
    budget: StopBudget,
    settle: Duration,
) -> Vec<StopFailure> {
    let mut failures = Vec::new();
    for &chain in Chain::ALL.iter() {
        if control.is_ord_running(chain) {
            let stopped = tokio::time::timeout(
                budget.ord_outer_bound(),
                control.stop_ord(chain, budget.ord),
            )
            .await;
            // Timing out here means the stop hung past even its own
            // internal bounds: give up on it like any other failure.
            let error = match stopped {
                Ok(Ok(())) => None,
                Ok(Err(error)) => Some(error),
                Err(_elapsed) => Some(NodeManagerError::OrdProcess(
                    nk_proc::OrdProcessError::StopTimeout,
                )),
            };
            if let Some(error) = error {
                failures.push(StopFailure {
                    chain,
                    service: Service::Ord,
                    error,
                });
            }
        }
        if control.is_running(chain) {
            let stopped = tokio::time::timeout(
                budget.bitcoind_outer_bound(),
                control.stop(chain, budget.bitcoind),
            )
            .await;
            let error = match stopped {
                Ok(Ok(())) => None,
                Ok(Err(error)) => Some(error),
                Err(_elapsed) => Some(NodeManagerError::Bitcoind(
                    nk_proc::BitcoindError::StopTimeout,
                )),
            };
            if let Some(error) = error {
                failures.push(StopFailure {
                    chain,
                    service: Service::Bitcoind,
                    error,
                });
            }
        }
    }

    let deadline = tokio::time::Instant::now() + settle;
    for &chain in Chain::ALL.iter() {
        let mut live = control.live_services(chain);
        while !live.is_empty() && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(VERIFY_POLL).await;
            live = control.live_services(chain);
        }
        for (service, pid) in live {
            // A stop that already failed is reported once, not twice.
            if !failures
                .iter()
                .any(|f| f.chain == chain && f.service == service)
            {
                failures.push(StopFailure {
                    chain,
                    service,
                    error: NodeManagerError::StillRunning { pid },
                });
            }
        }
    }
    failures
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
        assert!(!manager.any_running());

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
        assert!(manager.any_running());

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
        assert!(!manager.any_running());

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

    #[test]
    fn any_running_is_false_on_a_fresh_manager() {
        let manager = NodeManager::new();
        assert!(!manager.any_running());
    }

    /// A `StopControl` with scripted state, modelling the real manager's
    /// two views of a process separately: whether it is *tracked* (a stop
    /// removes it the moment it is asked for, working or not) and whether
    /// it is actually *alive* (only a stop that succeeds ends it).
    #[derive(Default)]
    struct FakeControl {
        ord_tracked: Mutex<Vec<Chain>>,
        bitcoind_tracked: Mutex<Vec<Chain>>,
        alive: Mutex<Vec<(Chain, Service, u32)>>,
        failing: Vec<(Chain, Service)>,
        /// If set, every alive process disappears once `live_services`
        /// has been called more than this many times (a process that
        /// takes a moment to finish going away).
        vanish_after_polls: Option<usize>,
        polls: AtomicUsize,
        /// Sleep inside every stop, to keep one "in flight".
        stop_delay: Duration,
        attempts: Mutex<Vec<(Chain, Service)>>,
    }

    impl FakeControl {
        /// Tracked and alive: `ord_on` / `bitcoind_on` are the chains where
        /// each service was started.
        fn running(ord_on: &[Chain], bitcoind_on: &[Chain]) -> Self {
            let mut alive = Vec::new();
            for (n, &chain) in ord_on.iter().enumerate() {
                alive.push((chain, Service::Ord, 1000 + n as u32));
            }
            for (n, &chain) in bitcoind_on.iter().enumerate() {
                alive.push((chain, Service::Bitcoind, 2000 + n as u32));
            }
            Self {
                ord_tracked: Mutex::new(ord_on.to_vec()),
                bitcoind_tracked: Mutex::new(bitcoind_on.to_vec()),
                alive: Mutex::new(alive),
                ..Default::default()
            }
        }

        fn failing_to_stop(mut self, chain: Chain, service: Service) -> Self {
            self.failing.push((chain, service));
            self
        }

        /// Alive but not tracked -- e.g. left over from a crashed session.
        fn with_orphan(self, chain: Chain, service: Service, pid: u32) -> Self {
            self.alive.lock().unwrap().push((chain, service, pid));
            self
        }

        /// A stop begins: like the real manager, the process is untracked
        /// *immediately* (whether or not the stop then works), while the
        /// stop itself takes `stop_delay` to finish.
        async fn attempt(&self, chain: Chain, service: Service) -> Result<(), NodeManagerError> {
            self.attempts.lock().unwrap().push((chain, service));
            let tracked = match service {
                Service::Ord => &self.ord_tracked,
                Service::Bitcoind => &self.bitcoind_tracked,
            };
            tracked.lock().unwrap().retain(|&c| c != chain);
            tokio::time::sleep(self.stop_delay).await;
            if self.failing.contains(&(chain, service)) {
                return Err(NodeManagerError::Io(std::io::Error::other(format!(
                    "scripted {service} stop failure"
                ))));
            }
            self.alive
                .lock()
                .unwrap()
                .retain(|&(c, s, _)| !(c == chain && s == service));
            Ok(())
        }

        fn attempts(&self) -> Vec<(Chain, Service)> {
            self.attempts.lock().unwrap().clone()
        }
    }

    impl StopControl for FakeControl {
        fn is_ord_running(&self, chain: Chain) -> bool {
            self.ord_tracked.lock().unwrap().contains(&chain)
        }
        fn is_running(&self, chain: Chain) -> bool {
            self.bitcoind_tracked.lock().unwrap().contains(&chain)
        }
        async fn stop_ord(&self, chain: Chain, _timeout: Duration) -> Result<(), NodeManagerError> {
            self.attempt(chain, Service::Ord).await
        }
        async fn stop(&self, chain: Chain, _timeout: Duration) -> Result<(), NodeManagerError> {
            self.attempt(chain, Service::Bitcoind).await
        }
        fn live_services(&self, chain: Chain) -> Vec<(Service, u32)> {
            let polls = self.polls.fetch_add(1, Ordering::SeqCst) + 1;
            if self.vanish_after_polls.is_some_and(|n| polls > n) {
                return Vec::new();
            }
            self.alive
                .lock()
                .unwrap()
                .iter()
                .filter(|&&(c, _, _)| c == chain)
                .map(|&(_, service, pid)| (service, pid))
                .collect()
        }
    }

    const NO_SETTLE: Duration = Duration::ZERO;

    /// Generous next to the fake's own delays (at most a few hundred ms),
    /// so ordinary tests never trip the outer bound.
    const TEST_BUDGET: StopBudget = StopBudget {
        ord: Duration::from_secs(5),
        bitcoind: Duration::from_secs(5),
        slack: Duration::from_secs(1),
    };

    #[tokio::test]
    async fn stopping_everything_with_nothing_running_attempts_nothing() {
        let control = FakeControl::default();
        let failures = stop_everything_with(&control, TEST_BUDGET, NO_SETTLE).await;
        assert!(failures.is_empty());
        assert!(control.attempts().is_empty());
    }

    #[tokio::test]
    async fn stopping_everything_stops_ord_before_bitcoind_on_each_chain_in_order() {
        let control = FakeControl::running(
            &[Chain::Mainnet, Chain::Regtest],
            &[Chain::Mainnet, Chain::Regtest, Chain::Signet],
        );
        let failures = stop_everything_with(&control, TEST_BUDGET, NO_SETTLE).await;
        assert!(failures.is_empty(), "{failures:?}");
        assert_eq!(
            control.attempts(),
            vec![
                (Chain::Mainnet, Service::Ord),
                (Chain::Mainnet, Service::Bitcoind),
                (Chain::Regtest, Service::Ord),
                (Chain::Regtest, Service::Bitcoind),
                (Chain::Signet, Service::Bitcoind),
            ]
        );
    }

    /// The Windows release-build failure mode this exists for: ord
    /// won't stop. Everything else must still be asked to stop -- its
    /// own environment's bitcoind and every later environment -- and
    /// the failure must still be reported (once, not once for the failed
    /// stop and again for the process being alive).
    #[tokio::test]
    async fn an_ord_that_will_not_stop_does_not_leave_anything_else_running() {
        let control = FakeControl::running(
            &[Chain::Mainnet, Chain::Regtest],
            &[Chain::Mainnet, Chain::Regtest],
        )
        .failing_to_stop(Chain::Mainnet, Service::Ord);
        let failures = stop_everything_with(&control, TEST_BUDGET, NO_SETTLE).await;
        assert_eq!(control.attempts().len(), 4, "every service was attempted");
        assert!(control
            .attempts()
            .contains(&(Chain::Mainnet, Service::Bitcoind)));
        assert!(control.attempts().contains(&(Chain::Regtest, Service::Ord)));
        assert!(control
            .attempts()
            .contains(&(Chain::Regtest, Service::Bitcoind)));
        assert_eq!(failures.len(), 1, "{failures:?}");
        assert_eq!(failures[0].chain, Chain::Mainnet);
        assert_eq!(failures[0].service, Service::Ord);
        assert!(matches!(failures[0].error, NodeManagerError::Io(_)));
    }

    #[tokio::test]
    async fn every_failed_stop_is_reported_not_just_the_first() {
        let control = FakeControl::running(&[Chain::Mainnet], &[Chain::Mainnet, Chain::Regtest])
            .failing_to_stop(Chain::Mainnet, Service::Ord)
            .failing_to_stop(Chain::Regtest, Service::Bitcoind);
        let failures = stop_everything_with(&control, TEST_BUDGET, NO_SETTLE).await;
        let reported: Vec<_> = failures.iter().map(|f| (f.chain, f.service)).collect();
        assert_eq!(
            reported,
            vec![
                (Chain::Mainnet, Service::Ord),
                (Chain::Regtest, Service::Bitcoind)
            ]
        );
    }

    /// The Safe Eject retry hole: a failed (or timed-out) stop leaves the
    /// process alive but no longer *tracked*, so a second attempt sees
    /// "nothing running" -- and used to report success ("safe to unplug")
    /// with the process still holding the drive. It must keep reporting
    /// the process until it is really gone.
    #[tokio::test]
    async fn a_retry_after_a_failed_stop_does_not_report_success() {
        let control = FakeControl::running(&[Chain::Mainnet], &[])
            .failing_to_stop(Chain::Mainnet, Service::Ord);

        let first = stop_everything_with(&control, TEST_BUDGET, NO_SETTLE).await;
        assert_eq!(first.len(), 1);
        assert!(!control.is_ord_running(Chain::Mainnet), "no longer tracked");

        let second = stop_everything_with(&control, TEST_BUDGET, NO_SETTLE).await;
        assert_eq!(
            second.len(),
            1,
            "still alive, so still not safe: {second:?}"
        );
        assert_eq!(second[0].chain, Chain::Mainnet);
        assert_eq!(second[0].service, Service::Ord);
        assert!(
            matches!(
                second[0].error,
                NodeManagerError::StillRunning { pid: 1000 }
            ),
            "{:?}",
            second[0].error
        );
        // And the retry did not pretend to stop something it no longer tracks.
        assert_eq!(control.attempts().len(), 1);
    }

    /// Something alive that this manager never tracked -- an orphan from
    /// a crashed earlier session, or a node started outside Nodekeeper --
    /// makes "safe to unplug" untrue just the same.
    #[tokio::test]
    async fn a_live_process_nobody_is_tracking_is_reported_too() {
        let control = FakeControl::default().with_orphan(Chain::Regtest, Service::Bitcoind, 99);
        let failures = stop_everything_with(&control, TEST_BUDGET, NO_SETTLE).await;
        assert_eq!(failures.len(), 1, "{failures:?}");
        assert_eq!(failures[0].chain, Chain::Regtest);
        assert_eq!(failures[0].service, Service::Bitcoind);
        assert!(matches!(
            failures[0].error,
            NodeManagerError::StillRunning { pid: 99 }
        ));
        assert!(control.attempts().is_empty(), "nothing was tracked to stop");
    }

    /// A process that has just exited can linger in the process table for
    /// a moment; it gets a short window to finish going away before being
    /// reported (and reported at once if that window is zero).
    #[tokio::test]
    async fn a_process_that_is_still_going_away_gets_a_moment_before_being_reported() {
        let lingering = || {
            let mut control = FakeControl::default().with_orphan(Chain::Signet, Service::Ord, 5);
            control.vanish_after_polls = Some(3);
            control
        };

        let patient = stop_everything_with(&lingering(), TEST_BUDGET, Duration::from_secs(5)).await;
        assert!(patient.is_empty(), "{patient:?}");

        let impatient = stop_everything_with(&lingering(), TEST_BUDGET, NO_SETTLE).await;
        assert_eq!(impatient.len(), 1);
    }

    /// Two overlapping calls (a double-clicked window close, Quit during
    /// Safe Eject) must not race. The second waits for the first, then
    /// finds everything really gone; racing it would have seen "nothing
    /// tracked" while the first was still stopping, and reported a
    /// failure (still alive) or -- worse -- exited early. And
    /// `any_running()` must stay true the whole time, because the
    /// tracking maps empty out as soon as a stop *starts*.
    #[tokio::test]
    async fn overlapping_stop_calls_run_one_after_the_other() {
        let manager = NodeManager::new();
        let control = FakeControl {
            stop_delay: Duration::from_millis(200),
            ..FakeControl::running(&[Chain::Mainnet], &[])
        };

        let started = std::time::Instant::now();
        let first = async {
            let failures = manager
                .stop_everything_via(&control, TEST_BUDGET, NO_SETTLE)
                .await;
            (failures, started.elapsed())
        };
        let second = async {
            let failures = manager
                .stop_everything_via(&control, TEST_BUDGET, NO_SETTLE)
                .await;
            (failures, started.elapsed())
        };
        let watcher = async {
            tokio::time::sleep(Duration::from_millis(80)).await;
            // The fake already untracked the ord, yet a stop is in flight.
            assert!(!control.is_ord_running(Chain::Mainnet));
            manager.any_running()
        };
        let ((first_failures, first_done), (second_failures, second_done), any_running_midway) =
            tokio::join!(first, second, watcher);

        assert!(first_failures.is_empty(), "{first_failures:?}");
        assert!(
            second_failures.is_empty(),
            "the second call must wait, not race: {second_failures:?}"
        );
        assert!(second_done >= first_done, "second finished before first");
        assert!(any_running_midway, "a stop in flight counts as running");
        assert!(!manager.any_running(), "and stops counting once done");
        assert_eq!(control.attempts().len(), 1, "stopped exactly once");
    }

    /// A stop that hangs past even its own internal bounds -- here the
    /// fake sleeps a full minute -- must be given up on and recorded as a
    /// failure, not held forever: it runs under the single-flight gate, so
    /// a hang would wedge `any_running()` and every later Quit / window
    /// close (which used to be an escape hatch) for good.
    #[tokio::test]
    async fn a_stop_that_hangs_is_given_up_on_and_does_not_hold_the_gate() {
        let manager = NodeManager::new();
        let control = FakeControl {
            stop_delay: Duration::from_secs(60),
            ..FakeControl::running(&[Chain::Mainnet], &[Chain::Mainnet])
        };
        let tiny = StopBudget {
            ord: Duration::from_millis(10),
            bitcoind: Duration::from_millis(10),
            slack: Duration::from_millis(10),
        };

        let started = std::time::Instant::now();
        let failures = manager.stop_everything_via(&control, tiny, NO_SETTLE).await;
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "gave up promptly, not after the fake's minute: {:?}",
            started.elapsed()
        );
        assert_eq!(failures.len(), 2, "{failures:?}");
        assert!(matches!(
            failures[0].error,
            NodeManagerError::OrdProcess(nk_proc::OrdProcessError::StopTimeout)
        ));
        assert!(matches!(
            failures[1].error,
            NodeManagerError::Bitcoind(nk_proc::BitcoindError::StopTimeout)
        ));

        // The gate was released, so a second call is not stuck behind it
        // -- and, both processes still being alive, does not say "safe".
        let again = tokio::time::timeout(
            Duration::from_secs(5),
            manager.stop_everything_via(&control, tiny, NO_SETTLE),
        )
        .await
        .expect("the gate must be free again");
        assert_eq!(again.len(), 2, "{again:?}");
        assert!(again
            .iter()
            .all(|f| matches!(f.error, NodeManagerError::StillRunning { .. })));
        assert!(
            !manager.any_running(),
            "and nothing is left counted in flight"
        );
    }

    #[test]
    fn a_stop_failure_names_the_service_the_chain_and_the_cause() {
        let failure = StopFailure {
            chain: Chain::Mainnet,
            service: Service::Ord,
            error: NodeManagerError::OrdProcess(nk_proc::OrdProcessError::StopTimeout),
        };
        let text = failure.to_string();
        assert!(text.contains("ord"), "{text}");
        assert!(text.contains("Mainnet"), "{text}");
        assert!(text.contains("did not exit within the timeout"), "{text}");
    }

    #[test]
    fn a_still_running_failure_says_so_and_names_the_process() {
        let failure = StopFailure {
            chain: Chain::Regtest,
            service: Service::Bitcoind,
            error: NodeManagerError::StillRunning { pid: 4242 },
        };
        let text = failure.to_string();
        assert!(text.contains("bitcoind (Regtest)"), "{text}");
        assert!(text.contains("still running"), "{text}");
        assert!(text.contains("4242"), "{text}");
    }
}
