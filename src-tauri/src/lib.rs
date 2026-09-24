mod node_manager;
mod wallet_session;

use nk_core::system_check::{run_system_check, SystemCheck};
use nk_core::{AppErrorCode, Chain, Environment};
use nk_exec::Executor;
use nk_store::Store;
use node_manager::{NodeManager, NodeManagerError};
use serde::Serialize;
use std::sync::{Arc, Mutex};
use tauri::Emitter;
use ts_rs::TS;
use wallet_session::WalletSession;

/// Runs the setup-wizard system check (OS/CPU/RAM/disk) for the given data
/// directory. Doubles as the Phase 1 typed-IPC scaffold: a real command
/// whose return type is generated into `ui/src/bindings/` by ts-rs (see
/// the `export_bindings` test below), proving the Rust-type -> TS-type ->
/// UI-call pipeline end to end.
#[tauri::command]
fn system_check(data_dir: String) -> SystemCheck {
    run_system_check(std::path::Path::new(&data_dir))
}

/// Every environment with its defaults. Phase 1 has no setup wizard or
/// persisted environment configuration yet (that's Phase 2+), so this
/// always returns one default `Environment` per `Chain`, rooted at the
/// relative `data/` folder (docs/SPEC.md Foundation A's portable-mode-
/// friendly layout) — enough for the environment switcher to render
/// against a real backend type instead of a frontend-only placeholder.
#[tauri::command]
fn list_default_environments() -> Vec<Environment> {
    Chain::ALL
        .iter()
        .map(|&chain| Environment::new_default(chain, data_root()))
        .collect()
}

#[tauri::command]
fn get_setting(
    store: tauri::State<Arc<Mutex<Store>>>,
    key: String,
) -> Result<Option<String>, String> {
    store
        .lock()
        .unwrap()
        .get_setting(&key)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn set_setting(
    store: tauri::State<Arc<Mutex<Store>>>,
    key: String,
    value: String,
) -> Result<(), String> {
    store
        .lock()
        .unwrap()
        .set_setting(&key, &value)
        .map_err(|e| e.to_string())
}

/// Everything a Tauri command needs to send a typed, friendly-mappable
/// error across IPC (docs/SPEC.md item 8) instead of a bare string —
/// `code` lets the frontend look up its plain-language message and
/// "what to do" action; `message` is always available as the technical
/// detail behind the toggle, including for errors with no good code.
#[derive(Debug, Clone, Serialize, TS)]
pub struct TypedError {
    pub code: Option<AppErrorCode>,
    pub message: String,
}

impl From<NodeManagerError> for TypedError {
    fn from(e: NodeManagerError) -> Self {
        TypedError {
            code: e.code(),
            message: e.to_string(),
        }
    }
}

impl From<nk_ord::wallet::WalletError> for TypedError {
    fn from(e: nk_ord::wallet::WalletError) -> Self {
        TypedError {
            code: e.code(),
            message: e.to_string(),
        }
    }
}

impl From<String> for TypedError {
    fn from(message: String) -> Self {
        TypedError {
            code: None,
            message,
        }
    }
}

impl From<std::io::Error> for TypedError {
    fn from(e: std::io::Error) -> Self {
        TypedError {
            code: None,
            message: e.to_string(),
        }
    }
}

/// The settings key an (eventual) setup wizard writes once it downloads
/// and verifies a Bitcoin Core binary into a real install location —
/// see `node_manager`'s doc comment for why start/stop reads this
/// instead of locating a binary itself.
const BITCOIND_PATH_SETTING: &str = "bitcoind_path";
/// Same scoping note as `BITCOIND_PATH_SETTING` -- no setup-wizard flow
/// downloads/verifies an ord binary yet either.
const ORD_PATH_SETTING: &str = "ord_path";

fn configured_bitcoind_path(store: &tauri::State<Arc<Mutex<Store>>>) -> Result<String, TypedError> {
    let path = store
        .lock()
        .unwrap()
        .get_setting(BITCOIND_PATH_SETTING)
        .map_err(|e| TypedError::from(e.to_string()))?;
    path.ok_or_else(|| TypedError {
        code: Some(AppErrorCode::BinaryNotVerified),
        message: "No verified Bitcoin Core binary is configured yet.".to_string(),
    })
}

fn configured_ord_path(store: &tauri::State<Arc<Mutex<Store>>>) -> Result<String, TypedError> {
    let path = store
        .lock()
        .unwrap()
        .get_setting(ORD_PATH_SETTING)
        .map_err(|e| TypedError::from(e.to_string()))?;
    path.ok_or_else(|| TypedError {
        code: Some(AppErrorCode::BinaryNotVerified),
        message: "No verified ord binary is configured yet.".to_string(),
    })
}

/// Only the default wallet Nodekeeper creates on first use of a chain's
/// wallet screen -- "Multiple named wallets" (docs/SPEC.md item 3) is a
/// later task; every wallet command hardcodes this name for now, same
/// scoping shape as the missing setup wizard elsewhere in this file.
const DEFAULT_WALLET_NAME: &str = "ord";

/// Everything an `ord wallet` command needs, resolved from `chain`'s
/// environment and settings, with both bitcoind and ord confirmed
/// running first -- ord wallet commands need a real running ord server
/// to talk to (`--server-url`), and ord itself needs bitcoind
/// (confirmed live, DECISIONS.md Phase 5 VERIFY: ord's own sync-status
/// gate refuses wallet commands otherwise anyway, but this fails with a
/// clearer message before ever shelling out).
struct WalletContext {
    ord_binary_path: String,
    environment: Environment,
    cookie_path: std::path::PathBuf,
    bitcoin_datadir: std::path::PathBuf,
    server_url: String,
    /// For `walletpassphrase`/`walletlock` around a real signing
    /// action -- a fresh client built straight from the cookie file
    /// (readable by anyone with filesystem access, same as
    /// `NodeManager`'s own internal one), not reused from `NodeManager`
    /// (which doesn't expose its tracked client publicly).
    rpc: nk_rpc::RpcClient,
}

impl WalletContext {
    fn target(&self) -> nk_ord::wallet::WalletTarget<'_> {
        nk_ord::wallet::WalletTarget {
            binary_path: std::path::Path::new(&self.ord_binary_path),
            environment: &self.environment,
            cookie_path: &self.cookie_path,
            bitcoin_datadir: &self.bitcoin_datadir,
            server_url: &self.server_url,
            wallet_name: DEFAULT_WALLET_NAME,
        }
    }
}

fn wallet_context(
    chain: Chain,
    node_manager: &tauri::State<'_, NodeManager>,
    store: &tauri::State<'_, Arc<Mutex<Store>>>,
    executor: &tauri::State<'_, Executor>,
) -> Result<WalletContext, TypedError> {
    if !node_manager.is_running(chain) {
        return Err(TypedError::from(format!(
            "{chain:?}'s node must be running before using its wallet"
        )));
    }
    if !node_manager.is_ord_running(chain) {
        return Err(TypedError::from(format!(
            "{chain:?}'s ord server must be running before using its wallet"
        )));
    }
    let ord_binary_path = configured_ord_path(store)?;
    let environment = Environment::new_default(chain, data_root());
    let cookie_path = environment.bitcoin_cookie_path();
    let bitcoin_datadir = environment.bitcoin_datadir_arg();
    let server_url = format!("http://127.0.0.1:{}", environment.ord_port);
    let rpc = nk_rpc::RpcClient::from_cookie_file(
        format!("http://127.0.0.1:{}", environment.rpc_port),
        &cookie_path,
        executor.inner().clone(),
        environment.name.clone(),
        chain,
    )
    .map_err(|e| TypedError::from(e.to_string()))?;
    Ok(WalletContext {
        ord_binary_path,
        environment,
        cookie_path,
        bitcoin_datadir,
        server_url,
        rpc,
    })
}

/// The mnemonic, returned directly in the IPC response and nowhere
/// else (docs/SPEC.md Foundation B's sensitive channel) -- a dedicated
/// type, not reused for anything that might tempt a caller into
/// logging/storing it alongside other wallet data.
#[derive(Debug, Clone, Serialize, TS)]
pub struct CreateWalletResult {
    pub mnemonic: String,
}

#[tauri::command]
async fn create_wallet(
    chain: Chain,
    node_manager: tauri::State<'_, NodeManager>,
    store: tauri::State<'_, Arc<Mutex<Store>>>,
    executor: tauri::State<'_, Executor>,
) -> Result<CreateWalletResult, TypedError> {
    let ctx = wallet_context(chain, &node_manager, &store, &executor)?;
    let response = nk_ord::wallet::create_wallet(&executor, &ctx.target())
        .await
        .map_err(TypedError::from)?;
    let mnemonic = response
        .get("mnemonic")
        .and_then(|v| v.as_str())
        .ok_or_else(|| TypedError::from("ord did not return a mnemonic".to_string()))?
        .to_string();
    Ok(CreateWalletResult { mnemonic })
}

/// `timestamp`: `"now"` to skip scanning (a brand-new restore with
/// nothing to find yet), a unix timestamp, or `"0"` for a full rescan
/// -- the frontend decides which, based on what it asks the user (see
/// `nk_ord::wallet::restore_wallet`'s doc comment).
#[tauri::command]
async fn restore_wallet(
    chain: Chain,
    mnemonic: String,
    timestamp: String,
    node_manager: tauri::State<'_, NodeManager>,
    store: tauri::State<'_, Arc<Mutex<Store>>>,
    executor: tauri::State<'_, Executor>,
) -> Result<(), TypedError> {
    let ctx = wallet_context(chain, &node_manager, &store, &executor)?;
    nk_ord::wallet::restore_wallet(&executor, &ctx.target(), &mnemonic, &timestamp)
        .await
        .map_err(TypedError::from)
}

/// Whether `chain` already has a wallet, so the Wallet screen knows
/// whether to show create/restore or the wallet itself. `ord wallet`
/// commands transparently reload an existing on-disk wallet on every
/// call (confirmed live, DECISIONS.md Phase 5 VERIFY) -- Nodekeeper
/// never needs to explicitly (re)load one, on startup or otherwise.
#[tauri::command]
async fn wallet_exists(
    chain: Chain,
    node_manager: tauri::State<'_, NodeManager>,
    store: tauri::State<'_, Arc<Mutex<Store>>>,
    executor: tauri::State<'_, Executor>,
) -> Result<bool, TypedError> {
    let ctx = wallet_context(chain, &node_manager, &store, &executor)?;
    nk_ord::wallet::wallet_exists(&executor, &ctx.target())
        .await
        .map_err(TypedError::from)
}

/// docs/SPEC.md item 3: "Balance: cardinal vs inscribed sats."
#[derive(Debug, Clone, Serialize, TS)]
pub struct WalletBalance {
    #[ts(type = "number")]
    pub cardinal: u64,
    #[ts(type = "number")]
    pub ordinal: u64,
    #[ts(type = "number")]
    pub total: u64,
}

#[tauri::command]
async fn wallet_balance(
    chain: Chain,
    node_manager: tauri::State<'_, NodeManager>,
    store: tauri::State<'_, Arc<Mutex<Store>>>,
    executor: tauri::State<'_, Executor>,
) -> Result<WalletBalance, TypedError> {
    let ctx = wallet_context(chain, &node_manager, &store, &executor)?;
    let response = nk_ord::wallet::wallet_balance(&executor, &ctx.target())
        .await
        .map_err(TypedError::from)?;
    let field = |key: &str| {
        response.get(key).and_then(|v| v.as_u64()).ok_or_else(|| {
            TypedError::from(format!(
                "ord did not return a numeric \"{key}\" balance field"
            ))
        })
    };
    Ok(WalletBalance {
        cardinal: field("cardinal")?,
        ordinal: field("ordinal")?,
        total: field("total")?,
    })
}

/// docs/SPEC.md item 3: "Receive: address with a QR code" -- just the
/// single next address; `nk_ord::wallet::wallet_receive` supports
/// requesting several at once, not needed by this screen yet.
#[tauri::command]
async fn wallet_receive_address(
    chain: Chain,
    node_manager: tauri::State<'_, NodeManager>,
    store: tauri::State<'_, Arc<Mutex<Store>>>,
    executor: tauri::State<'_, Executor>,
) -> Result<String, TypedError> {
    let ctx = wallet_context(chain, &node_manager, &store, &executor)?;
    let response = nk_ord::wallet::wallet_receive(&executor, &ctx.target(), None)
        .await
        .map_err(TypedError::from)?;
    response
        .get("addresses")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| TypedError::from("ord did not return a receive address".to_string()))
}

/// A signing RPC's unlock timeout at the bitcoind level (docs/SPEC.md
/// item 3: "a short timeout") -- distinct from `WalletSession`'s much
/// longer app-level "remember" timeout. This only needs to outlive one
/// send; `wallet_lock` runs immediately after regardless.
const WALLET_UNLOCK_TIMEOUT_SECS: u32 = 60;

/// `ord wallet send`'s `{"txid", "fee"}` (sats) -- `psbt`/`asset` are
/// dropped rather than exposed, since nothing in the UI uses them yet.
#[derive(Debug, Clone, Serialize, TS)]
pub struct WalletSendResult {
    pub txid: String,
    #[ts(type = "number")]
    pub fee: u64,
}

fn parse_wallet_send_result(response: serde_json::Value) -> Result<WalletSendResult, TypedError> {
    let txid = response
        .get("txid")
        .and_then(|v| v.as_str())
        .ok_or_else(|| TypedError::from("ord did not return a txid".to_string()))?
        .to_string();
    let fee = response
        .get("fee")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| TypedError::from("ord did not return a fee".to_string()))?;
    Ok(WalletSendResult { txid, fee })
}

/// Preview only -- confirmed live (DECISIONS.md Phase 5 VERIFY) that
/// `--dry-run` needs no wallet unlock at all, even against a locked
/// encrypted wallet, so this never touches `WalletSession` or calls
/// `walletpassphrase`.
#[tauri::command]
async fn wallet_send_dry_run(
    chain: Chain,
    address: String,
    asset: String,
    fee_rate: f64,
    node_manager: tauri::State<'_, NodeManager>,
    store: tauri::State<'_, Arc<Mutex<Store>>>,
    executor: tauri::State<'_, Executor>,
) -> Result<WalletSendResult, TypedError> {
    let ctx = wallet_context(chain, &node_manager, &store, &executor)?;
    let response =
        nk_ord::wallet::wallet_send(&executor, &ctx.target(), &address, &asset, fee_rate, true)
            .await
            .map_err(TypedError::from)?;
    parse_wallet_send_result(response)
}

/// The real, signing send (docs/SPEC.md item 3: unlock with
/// `walletpassphrase` for a short timeout, run the action, lock again
/// afterward regardless of the outcome). The frontend's expected flow:
/// try with `passphrase: None` first (relying on anything
/// `WalletSession` already has remembered); if that fails with
/// `AppErrorCode::WalletLocked`, prompt the user and retry with
/// `passphrase: Some(...)`, which always takes priority over whatever
/// (if anything, possibly now-expired) is remembered.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
async fn wallet_send(
    chain: Chain,
    address: String,
    asset: String,
    fee_rate: f64,
    passphrase: Option<String>,
    remember: bool,
    node_manager: tauri::State<'_, NodeManager>,
    store: tauri::State<'_, Arc<Mutex<Store>>>,
    executor: tauri::State<'_, Executor>,
    wallet_session: tauri::State<'_, WalletSession>,
) -> Result<WalletSendResult, TypedError> {
    let ctx = wallet_context(chain, &node_manager, &store, &executor)?;

    let passphrase = match passphrase.or_else(|| wallet_session.get(chain)) {
        Some(p) => p,
        None => {
            return Err(TypedError {
                code: Some(AppErrorCode::WalletLocked),
                message: "This wallet is locked; enter its passphrase to continue.".to_string(),
            })
        }
    };

    ctx.rpc
        .wallet_passphrase(DEFAULT_WALLET_NAME, &passphrase, WALLET_UNLOCK_TIMEOUT_SECS)
        .await
        .map_err(|e| TypedError::from(e.to_string()))?;
    if remember {
        wallet_session.remember(chain, passphrase);
    }

    let response =
        nk_ord::wallet::wallet_send(&executor, &ctx.target(), &address, &asset, fee_rate, false)
            .await;
    // Best-effort: a lock failure here shouldn't hide the send's own
    // result (success or failure) from the caller.
    let _ = ctx.rpc.wallet_lock(DEFAULT_WALLET_NAME).await;

    parse_wallet_send_result(response.map_err(TypedError::from)?)
}

#[tauri::command]
async fn start_node(
    chain: Chain,
    node_manager: tauri::State<'_, NodeManager>,
    store: tauri::State<'_, Arc<Mutex<Store>>>,
    executor: tauri::State<'_, Executor>,
) -> Result<(), TypedError> {
    let binary_path = configured_bitcoind_path(&store)?;
    let environment = Environment::new_default(chain, data_root());
    node_manager
        .start(
            chain,
            std::path::Path::new(&binary_path),
            executor.inner().clone(),
            environment,
        )
        .await
        .map_err(TypedError::from)
}

#[tauri::command]
async fn stop_node(
    chain: Chain,
    node_manager: tauri::State<'_, NodeManager>,
) -> Result<(), TypedError> {
    node_manager
        .stop(chain, std::time::Duration::from_secs(120))
        .await
        .map_err(TypedError::from)
}

#[tauri::command]
async fn restart_node(
    chain: Chain,
    node_manager: tauri::State<'_, NodeManager>,
    store: tauri::State<'_, Arc<Mutex<Store>>>,
    executor: tauri::State<'_, Executor>,
) -> Result<(), TypedError> {
    node_manager
        .stop(chain, std::time::Duration::from_secs(120))
        .await
        .map_err(TypedError::from)?;
    let binary_path = configured_bitcoind_path(&store)?;
    let environment = Environment::new_default(chain, data_root());
    node_manager
        .start(
            chain,
            std::path::Path::new(&binary_path),
            executor.inner().clone(),
            environment,
        )
        .await
        .map_err(TypedError::from)
}

#[tauri::command]
async fn node_status(
    chain: Chain,
    node_manager: tauri::State<'_, NodeManager>,
) -> Result<node_manager::NodeStatus, TypedError> {
    node_manager.status(chain).await.map_err(TypedError::from)
}

/// Whether `chain`'s node is currently running, cheaper than
/// `node_status` and without erroring on "not running" -- lets the
/// dashboard decide whether to show a Start button or poll `node_status`
/// without treating "not running yet" as a failure.
#[tauri::command]
fn is_node_running(chain: Chain, node_manager: tauri::State<'_, NodeManager>) -> bool {
    node_manager.is_running(chain)
}

#[tauri::command]
async fn start_ord(
    chain: Chain,
    node_manager: tauri::State<'_, NodeManager>,
    store: tauri::State<'_, Arc<Mutex<Store>>>,
    executor: tauri::State<'_, Executor>,
) -> Result<(), TypedError> {
    let binary_path = configured_ord_path(&store)?;
    let environment = Environment::new_default(chain, data_root());
    node_manager
        .start_ord(
            chain,
            std::path::Path::new(&binary_path),
            executor.inner().clone(),
            environment,
        )
        .await
        .map_err(TypedError::from)
}

#[tauri::command]
async fn stop_ord(
    chain: Chain,
    node_manager: tauri::State<'_, NodeManager>,
) -> Result<(), TypedError> {
    node_manager
        .stop_ord(chain, std::time::Duration::from_secs(30))
        .await
        .map_err(TypedError::from)
}

#[tauri::command]
async fn restart_ord(
    chain: Chain,
    node_manager: tauri::State<'_, NodeManager>,
    store: tauri::State<'_, Arc<Mutex<Store>>>,
    executor: tauri::State<'_, Executor>,
) -> Result<(), TypedError> {
    node_manager
        .stop_ord(chain, std::time::Duration::from_secs(30))
        .await
        .map_err(TypedError::from)?;
    let binary_path = configured_ord_path(&store)?;
    let environment = Environment::new_default(chain, data_root());
    node_manager
        .start_ord(
            chain,
            std::path::Path::new(&binary_path),
            executor.inner().clone(),
            environment,
        )
        .await
        .map_err(TypedError::from)
}

#[tauri::command]
async fn ord_status(
    chain: Chain,
    node_manager: tauri::State<'_, NodeManager>,
) -> Result<node_manager::OrdStatus, TypedError> {
    node_manager
        .ord_status(chain)
        .await
        .map_err(TypedError::from)
}

/// Same "cheaper than status, no error on not-running" reasoning as
/// `is_node_running`, for ord.
#[tauri::command]
fn is_ord_running(chain: Chain, node_manager: tauri::State<'_, NodeManager>) -> bool {
    node_manager.is_ord_running(chain)
}

/// Default tail/page window: generous enough to show a useful amount of
/// recent log context without ever reading more of a (possibly
/// multi-GB) file than this (docs/SPEC.md item 2: "never load a whole
/// file").
const DEFAULT_LOG_WINDOW_BYTES: u64 = 256 * 1024;

#[tauri::command]
fn tail_debug_log(chain: Chain) -> Result<nk_core::log_tail::LogWindow, TypedError> {
    let environment = Environment::new_default(chain, data_root());
    nk_core::log_tail::tail(
        &environment.bitcoin_debug_log_path(),
        DEFAULT_LOG_WINDOW_BYTES,
    )
    .map_err(TypedError::from)
}

/// Scrolls further back ("load older") from a previous `tail_debug_log`
/// or `page_debug_log_before` call's `start_offset`.
#[tauri::command]
fn page_debug_log_before(
    chain: Chain,
    end_offset: u64,
) -> Result<nk_core::log_tail::LogWindow, TypedError> {
    let environment = Environment::new_default(chain, data_root());
    nk_core::log_tail::page_before(
        &environment.bitcoin_debug_log_path(),
        end_offset,
        DEFAULT_LOG_WINDOW_BYTES,
    )
    .map_err(TypedError::from)
}

const MAX_LOG_SEARCH_MATCHES: usize = 500;

#[tauri::command]
fn search_debug_log(chain: Chain, query: String) -> Result<Vec<String>, TypedError> {
    let environment = Environment::new_default(chain, data_root());
    nk_core::log_tail::search(
        &environment.bitcoin_debug_log_path(),
        &query,
        MAX_LOG_SEARCH_MATCHES,
    )
    .map_err(TypedError::from)
}

const DEFAULT_HISTORY_LIMIT: u32 = 500;

/// Rolling command history for the Live Command Monitor (docs/SPEC.md
/// item 7). `environment` filters to one environment's commands, or
/// `None` for every environment.
#[tauri::command]
fn list_command_history(
    store: tauri::State<Arc<Mutex<Store>>>,
    environment: Option<String>,
) -> Result<Vec<nk_store::CommandHistoryEntry>, TypedError> {
    store
        .lock()
        .unwrap()
        .list_command_history(environment.as_deref(), DEFAULT_HISTORY_LIMIT)
        .map_err(|e| TypedError::from(e.to_string()))
}

/// Placeholder (see `list_default_environments`'s doc comment for the
/// same caveat): proper OS-specific app-data-dir resolution, and
/// portable-vs-installed mode, are a later-phase concern.
fn data_root() -> &'static std::path::Path {
    std::path::Path::new("data")
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    std::fs::create_dir_all(data_root()).expect("failed to create data directory");
    let db_path = data_root().join("nodekeeper.sqlite3");
    let store = Store::open(&db_path).expect("failed to open the settings database");
    let store = Arc::new(Mutex::new(store));

    let executor = Executor::new();
    let node_manager = NodeManager::new();
    let wallet_session = WalletSession::new();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(store.clone())
        .manage(executor.clone())
        .manage(node_manager)
        .manage(wallet_session)
        .setup(move |app| {
            // Feeds every command the Live Command Monitor will show
            // (Phase 3) into the rolling history table (docs/SPEC.md
            // item 7) — started once, for the app's lifetime, alongside
            // the single shared Executor instance every command goes
            // through.
            tauri::async_runtime::spawn(nk_store::persist_exec_events(
                store.clone(),
                executor.subscribe(),
            ));

            // Live feed for the Live Command Monitor UI itself: every
            // ExecEvent re-emitted as a Tauri event, for the frontend to
            // `listen("exec-event", ...)`. Already redacted/placeholder'd
            // by nk-exec (Phase 2) before it ever reaches this stream --
            // nothing more to withhold at this layer.
            let app_handle = app.handle().clone();
            let mut frontend_events = executor.subscribe();
            tauri::async_runtime::spawn(async move {
                loop {
                    match frontend_events.recv().await {
                        Ok(event) => {
                            let _ = app_handle.emit("exec-event", event);
                        }
                        // A slow/absent listener missed some events --
                        // keep forwarding what arrives next rather than
                        // giving up on the stream entirely.
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            system_check,
            list_default_environments,
            get_setting,
            set_setting,
            start_node,
            stop_node,
            restart_node,
            node_status,
            is_node_running,
            start_ord,
            stop_ord,
            restart_ord,
            ord_status,
            is_ord_running,
            create_wallet,
            restore_wallet,
            wallet_exists,
            wallet_balance,
            wallet_receive_address,
            wallet_send_dry_run,
            wallet_send,
            tail_debug_log,
            page_debug_log_before,
            search_debug_log,
            list_command_history,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use ts_rs::{Config, TS};

    /// Regenerates `ui/src/bindings/*.ts` from the Rust IPC types. Run as
    /// part of `just check` (`cargo test`); commit the generated files so
    /// the frontend never hand-writes a duplicate type.
    #[test]
    fn export_bindings() {
        let config = Config::new().with_out_dir(PathBuf::from("../ui/src/bindings/"));
        SystemCheck::export_all(&config).unwrap();
        Environment::export_all(&config).unwrap();
        Chain::export_all(&config).unwrap();
        AppErrorCode::export_all(&config).unwrap();
        TypedError::export_all(&config).unwrap();
        node_manager::NodeStatus::export_all(&config).unwrap();
        node_manager::OrdStatus::export_all(&config).unwrap();
        CreateWalletResult::export_all(&config).unwrap();
        WalletBalance::export_all(&config).unwrap();
        WalletSendResult::export_all(&config).unwrap();
        nk_core::log_tail::LogWindow::export_all(&config).unwrap();
        nk_store::CommandHistoryEntry::export_all(&config).unwrap();
        nk_exec::ExecEvent::export_all(&config).unwrap();
    }
}
