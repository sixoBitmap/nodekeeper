mod instance_lock;
mod node_manager;
mod wallet_session;

use nk_core::system_check::{run_system_check, SystemCheck};
use nk_core::{AppErrorCode, Chain, Environment, IndexOptions};
use nk_exec::Executor;
use nk_store::Store;
use node_manager::{NodeManager, NodeManagerError};
use serde::Serialize;
use std::sync::{Arc, Mutex};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager};
use tauri_plugin_dialog::DialogExt;
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

/// Every environment with its defaults, index options included. No
/// other per-environment customization exists yet (e.g. renaming) --
/// this always returns one `Environment` per `Chain`, rooted at
/// `environment_data_root` (the data-directory picker, docs/SPEC.md
/// item 1) -- enough for the environment switcher to render against a
/// real backend type instead of a frontend-only placeholder.
#[tauri::command]
fn list_default_environments(store: tauri::State<'_, Arc<Mutex<Store>>>) -> Vec<Environment> {
    let root = environment_data_root(&store);
    Chain::ALL
        .iter()
        .map(|&chain| {
            let mut env = Environment::new_default(chain, &root);
            env.index_options = effective_index_options(chain, &store);
            env
        })
        .collect()
}

/// Settings key an index-options override for `chain` is stored under
/// (docs/SPEC.md Foundation F/item 1) -- one JSON-encoded `IndexOptions`
/// per chain, since each environment records its own.
fn index_options_setting_key(chain: Chain) -> String {
    format!("index_options_{}", chain.dir_name())
}

/// `chain`'s effective index options: the wizard-chosen override if one
/// was ever saved, else the chain's built-in default (`Chain::
/// default_index_options` -- e.g. regtest enables everything, since it
/// "costs almost nothing there," per Foundation F).
fn effective_index_options(
    chain: Chain,
    store: &tauri::State<'_, Arc<Mutex<Store>>>,
) -> IndexOptions {
    store
        .lock()
        .unwrap()
        .get_setting(&index_options_setting_key(chain))
        .ok()
        .flatten()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_else(|| chain.default_index_options())
}

/// Setup wizard step (docs/SPEC.md item 1 / Foundation F): saves the
/// user's chosen index options for `chain`. These are "effectively
/// permanent" once ord has indexed with an option disabled (enabling it
/// later means a full reindex), so this refuses while that chain's ord
/// is running rather than let a change silently apply to nothing until
/// the next restart.
#[tauri::command]
fn set_index_options(
    chain: Chain,
    index_sats: bool,
    index_runes: bool,
    index_addresses: bool,
    node_manager: tauri::State<'_, NodeManager>,
    store: tauri::State<'_, Arc<Mutex<Store>>>,
) -> Result<(), TypedError> {
    if node_manager.is_ord_running(chain) {
        return Err(TypedError::from(
            "Stop ord for this environment before changing its index options.".to_string(),
        ));
    }
    let options = IndexOptions {
        index_sats,
        index_runes,
        index_addresses,
    };
    let json = serde_json::to_string(&options).map_err(|e| TypedError::from(e.to_string()))?;
    store
        .lock()
        .unwrap()
        .set_setting(&index_options_setting_key(chain), &json)
        .map_err(|e| TypedError::from(e.to_string()))?;
    Ok(())
}

/// The data-directory picker's current effective value (docs/SPEC.md
/// item 1) -- always resolvable, since `environment_data_root` already
/// falls back to a default.
#[tauri::command]
fn get_environment_data_root(store: tauri::State<'_, Arc<Mutex<Store>>>) -> String {
    environment_data_root(&store).display().to_string()
}

/// Changes where every environment's data lives from now on (docs/
/// SPEC.md item 1: "including external drives"). Refuses while
/// anything is running (`NodeManager::any_running`) -- a bitcoind/ord
/// process already using the *old* path shouldn't have a later command
/// (start/stop/status/logs) suddenly resolve a *different* one out
/// from under it. Fails closed if the chosen folder can't actually be
/// created or written to, rather than saving a path that would only
/// break the next time something tries to use it.
#[tauri::command]
fn set_environment_data_root(
    path: String,
    node_manager: tauri::State<'_, NodeManager>,
    store: tauri::State<'_, Arc<Mutex<Store>>>,
) -> Result<(), TypedError> {
    if node_manager.any_running() {
        return Err(TypedError::from(
            "Stop every running environment before changing the data directory.".to_string(),
        ));
    }
    let candidate = std::path::PathBuf::from(&path);
    std::fs::create_dir_all(&candidate)
        .map_err(|e| TypedError::from(format!("Can't use this folder: {e}")))?;
    // `create_dir_all` above only proves the folder (or its parent, if
    // it already existed) was creatable -- an existing folder could
    // still be read-only, so confirm actual write access directly.
    let probe = candidate.join(".nodekeeper-write-test");
    std::fs::write(&probe, b"")
        .map_err(|e| TypedError::from(format!("This folder isn't writable: {e}")))?;
    let _ = std::fs::remove_file(&probe);
    store
        .lock()
        .unwrap()
        .set_setting(ENVIRONMENT_DATA_ROOT_SETTING, &path)
        .map_err(|e| TypedError::from(e.to_string()))?;
    Ok(())
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
/// Where every environment's own data (bitcoind, ord, wallets, logs)
/// lives -- docs/SPEC.md item 1: "Let the user choose the data
/// directory (including external drives)." Deliberately a *different*
/// setting from `data_root()`'s location (where Nodekeeper's own tiny
/// settings database lives): that location has to be resolved *before*
/// the settings database can even be opened, so it can't itself be
/// settings-driven without a chicken-and-egg problem -- see
/// `data_root()`/`default_environment_data_root()` (Phase 9) for how
/// each is resolved instead. This setting only changes where
/// *environment* data (which can legitimately be huge and belongs on a
/// chosen/external drive) is written, once the user has explicitly
/// picked somewhere via the data-directory picker.
const ENVIRONMENT_DATA_ROOT_SETTING: &str = "environment_data_root";

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

/// Where downloaded-and-verified binaries are extracted to. Deliberately
/// under `data_root()` (Nodekeeper's own fixed app-data location), not
/// `environment_data_root()` -- a verified Bitcoin Core/ord binary isn't
/// tied to any one environment's (possibly external-drive, possibly
/// later-relocated) data directory, so moving that directory shouldn't
/// force a redownload.
fn binaries_root() -> std::path::PathBuf {
    data_root().join("bin")
}

/// Setup wizard progress event (docs/SPEC.md item 1: "show progress")
/// emitted on `"setup-download-progress"` while `download_and_verify_*`
/// runs. `binary` is `"bitcoin_core"` or `"ord"` so one frontend listener
/// can drive both progress bars.
#[derive(Debug, Clone, Serialize, TS)]
struct DownloadProgress {
    binary: String,
    #[ts(type = "number")]
    downloaded_bytes: u64,
    #[ts(type = "number | null")]
    total_bytes: Option<u64>,
}

/// Setup wizard step (docs/SPEC.md item 1, Phase 2): downloads and
/// verifies the pinned Bitcoin Core release for this platform (SHA-256 +
/// >= 3 pinned-key signatures, `nk_verify::bitcoin_core`), extracts it,
/// and -- only on success -- persists its path as the binary the rest of
/// the app launches. Fails closed: any error leaves `bitcoind_path`
/// unset, same as if this had never run.
#[tauri::command]
async fn download_and_verify_bitcoin_core(
    app: tauri::AppHandle,
    store: tauri::State<'_, Arc<Mutex<Store>>>,
) -> Result<String, TypedError> {
    let dest_dir =
        binaries_root().join(format!("bitcoin-core-{}", nk_verify::bitcoin_core::VERSION));
    let installed = nk_verify::bitcoin_core::download_verify_and_install_bitcoin_core(
        &dest_dir,
        move |downloaded_bytes, total_bytes| {
            let _ = app.emit(
                "setup-download-progress",
                DownloadProgress {
                    binary: "bitcoin_core".to_string(),
                    downloaded_bytes,
                    total_bytes,
                },
            );
        },
    )
    .await
    .map_err(|e| TypedError::from(format!("Bitcoin Core download/verification failed: {e}")))?;

    let path = installed.binary_path.display().to_string();
    store
        .lock()
        .unwrap()
        .set_setting(BITCOIND_PATH_SETTING, &path)
        .map_err(|e| TypedError::from(e.to_string()))?;
    Ok(path)
}

/// Same as `download_and_verify_bitcoin_core`, for ord (docs/SPEC.md
/// item 1, Phase 4) -- verified against Nodekeeper's pinned SHA-256
/// (`nk_verify::ord`; ord publishes no maintainer-signed checksums file
/// to check against, so the pinned hash *is* the verification here).
#[tauri::command]
async fn download_and_verify_ord(
    app: tauri::AppHandle,
    store: tauri::State<'_, Arc<Mutex<Store>>>,
) -> Result<String, TypedError> {
    let dest_dir = binaries_root().join(format!("ord-{}", nk_verify::ord::VERSION));
    let installed = nk_verify::ord::download_verify_and_install_ord(
        &dest_dir,
        move |downloaded_bytes, total_bytes| {
            let _ = app.emit(
                "setup-download-progress",
                DownloadProgress {
                    binary: "ord".to_string(),
                    downloaded_bytes,
                    total_bytes,
                },
            );
        },
    )
    .await
    .map_err(|e| TypedError::from(format!("ord download/verification failed: {e}")))?;

    let path = installed.binary_path.display().to_string();
    store
        .lock()
        .unwrap()
        .set_setting(ORD_PATH_SETTING, &path)
        .map_err(|e| TypedError::from(e.to_string()))?;
    Ok(path)
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

/// Just the bitcoind-level pieces (RPC client + environment), for
/// commands that need the node but not ord/a wallet -- fee estimation,
/// for instance, is a plain bitcoind RPC with no wallet involved at
/// all, so requiring ord to be running for it would be an unmotivated
/// extra constraint. `wallet_context` below builds on top of this.
fn bitcoin_rpc_context(
    chain: Chain,
    node_manager: &tauri::State<'_, NodeManager>,
    store: &tauri::State<'_, Arc<Mutex<Store>>>,
    executor: &tauri::State<'_, Executor>,
) -> Result<(Environment, nk_rpc::RpcClient), TypedError> {
    if !node_manager.is_running(chain) {
        return Err(TypedError::from(format!(
            "{chain:?}'s node must be running first"
        )));
    }
    let environment = Environment::new_default(chain, &environment_data_root(store));
    let cookie_path = environment.bitcoin_cookie_path();
    let rpc = nk_rpc::RpcClient::from_cookie_file(
        format!("http://127.0.0.1:{}", environment.rpc_port),
        &cookie_path,
        executor.inner().clone(),
        environment.name.clone(),
        chain,
    )
    .map_err(|e| TypedError::from(e.to_string()))?;
    Ok((environment, rpc))
}

fn wallet_context(
    chain: Chain,
    node_manager: &tauri::State<'_, NodeManager>,
    store: &tauri::State<'_, Arc<Mutex<Store>>>,
    executor: &tauri::State<'_, Executor>,
) -> Result<WalletContext, TypedError> {
    let (environment, rpc) = bitcoin_rpc_context(chain, node_manager, store, executor)?;
    if !node_manager.is_ord_running(chain) {
        return Err(TypedError::from(format!(
            "{chain:?}'s ord server must be running before using its wallet"
        )));
    }
    let ord_binary_path = configured_ord_path(store)?;
    let cookie_path = environment.bitcoin_cookie_path();
    let bitcoin_datadir = environment.bitcoin_datadir_arg();
    let server_url = format!("http://127.0.0.1:{}", environment.ord_port);
    Ok(WalletContext {
        ord_binary_path,
        environment,
        cookie_path,
        bitcoin_datadir,
        server_url,
        rpc,
    })
}

/// Just enough for a read-only `ord server` HTTP API call (sat/
/// inscription lookups for reinscribe mode, docs/SPEC.md item 4) --
/// needs ord running, but no wallet at all. A fresh client each call,
/// not reused from `NodeManager`'s own internally tracked one (which
/// doesn't expose it publicly) -- same "construct fresh from public
/// state" precedent as `bitcoin_rpc_context`'s `RpcClient`.
fn ord_client(
    chain: Chain,
    node_manager: &tauri::State<'_, NodeManager>,
    store: &tauri::State<'_, Arc<Mutex<Store>>>,
    executor: &tauri::State<'_, Executor>,
) -> Result<nk_ord::OrdClient, TypedError> {
    if !node_manager.is_ord_running(chain) {
        return Err(TypedError::from(format!(
            "{chain:?}'s ord server must be running first"
        )));
    }
    let environment = Environment::new_default(chain, &environment_data_root(store));
    Ok(nk_ord::OrdClient::new(
        format!("http://127.0.0.1:{}", environment.ord_port),
        executor.inner().clone(),
        environment.name,
    ))
}

/// The mnemonic, returned directly in the IPC response and nowhere
/// else (docs/SPEC.md Foundation B's sensitive channel) -- a dedicated
/// type, not reused for anything that might tempt a caller into
/// logging/storing it alongside other wallet data.
#[derive(Debug, Clone, Serialize, TS)]
pub struct CreateWalletResult {
    pub mnemonic: String,
}

/// docs/SPEC.md SECURITY RULES: "All mainnet wallets are encrypted."
/// Enforced here, not left to the frontend showing/hiding a field --
/// the Phase 5 security self-review (DECISIONS.md) found this rule
/// wasn't actually enforced anywhere and flagged it as a real gap.
fn require_encryption_passphrase_on_mainnet(
    chain: Chain,
    passphrase: &Option<zeroize::Zeroizing<String>>,
) -> Result<(), TypedError> {
    let is_empty = match passphrase {
        Some(p) => p.is_empty(),
        None => true,
    };
    if chain == Chain::Mainnet && is_empty {
        return Err(TypedError::from(
            "Mainnet wallets must be encrypted -- enter a passphrase.".to_string(),
        ));
    }
    Ok(())
}

/// `passphrase`: required on mainnet (see
/// `require_encryption_passphrase_on_mainnet`), optional elsewhere --
/// when given, the freshly created wallet is encrypted with it
/// immediately, before the mnemonic is returned to the frontend at
/// all. VERIFIED live (DECISIONS.md Phase 5) that encrypting an
/// `ord`-created wallet right after creation leaves the mnemonic ord
/// already returned as a correct, complete backup -- Core's own
/// "a new HD seed was generated" message on `encryptwallet` is
/// misleading boilerplate for descriptor wallets, not a real reseed.
#[tauri::command]
async fn create_wallet(
    chain: Chain,
    passphrase: Option<String>,
    node_manager: tauri::State<'_, NodeManager>,
    store: tauri::State<'_, Arc<Mutex<Store>>>,
    executor: tauri::State<'_, Executor>,
) -> Result<CreateWalletResult, TypedError> {
    let passphrase = passphrase.map(zeroize::Zeroizing::new);
    require_encryption_passphrase_on_mainnet(chain, &passphrase)?;
    let ctx = wallet_context(chain, &node_manager, &store, &executor)?;
    let response = nk_ord::wallet::create_wallet(&executor, &ctx.target())
        .await
        .map_err(TypedError::from)?;
    let mnemonic = response
        .get("mnemonic")
        .and_then(|v| v.as_str())
        .ok_or_else(|| TypedError::from("ord did not return a mnemonic".to_string()))?
        .to_string();
    if let Some(passphrase) = passphrase.filter(|p| !p.is_empty()) {
        ctx.rpc
            .encrypt_wallet(DEFAULT_WALLET_NAME, &passphrase)
            .await
            .map_err(|e| TypedError::from(e.to_string()))?;
    }
    Ok(CreateWalletResult { mnemonic })
}

/// `timestamp`: `"now"` to skip scanning (a brand-new restore with
/// nothing to find yet), a unix timestamp, or `"0"` for a full rescan
/// -- the frontend decides which, based on what it asks the user (see
/// `nk_ord::wallet::restore_wallet`'s doc comment). `passphrase`: same
/// mainnet-required rule and immediately-after-creation encryption as
/// `create_wallet` -- a restore also creates a fresh, initially
/// unencrypted local Core wallet, so it needs the same treatment.
#[tauri::command]
async fn restore_wallet(
    chain: Chain,
    mnemonic: String,
    timestamp: String,
    passphrase: Option<String>,
    node_manager: tauri::State<'_, NodeManager>,
    store: tauri::State<'_, Arc<Mutex<Store>>>,
    executor: tauri::State<'_, Executor>,
) -> Result<(), TypedError> {
    let passphrase = passphrase.map(zeroize::Zeroizing::new);
    require_encryption_passphrase_on_mainnet(chain, &passphrase)?;
    let ctx = wallet_context(chain, &node_manager, &store, &executor)?;
    nk_ord::wallet::restore_wallet(&executor, &ctx.target(), &mnemonic, &timestamp)
        .await
        .map_err(TypedError::from)?;
    if let Some(passphrase) = passphrase.filter(|p| !p.is_empty()) {
        ctx.rpc
            .encrypt_wallet(DEFAULT_WALLET_NAME, &passphrase)
            .await
            .map_err(|e| TypedError::from(e.to_string()))?;
    }
    Ok(())
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

/// docs/SPEC.md item 3: "rune balances only when the runes index is
/// enabled" (Foundation F). Forwarded as opaque JSON text rather than a
/// typed amount/symbol/divisibility struct -- DECISIONS.md Phase 5
/// VERIFY found `"runes": {}"` (name -> per-rune object) appears only
/// when the running ord server's runes index is on, but couldn't
/// confirm the non-empty per-rune value shape live.
#[derive(Debug, Clone, Serialize, TS)]
pub struct WalletRuneBalance {
    pub name: String,
    pub raw: String,
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
    /// `None` when ord's runes index is disabled for this environment
    /// -- absent, not empty, in that case (DECISIONS.md Phase 5
    /// VERIFY: the field is genuinely missing from ord's JSON, not
    /// zero). `Some(vec![])` means the index is on but the wallet owns
    /// no runes yet.
    pub runes: Option<Vec<WalletRuneBalance>>,
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
    let runes = response
        .get("runes")
        .and_then(|v| v.as_object())
        .map(|obj| {
            obj.iter()
                .map(|(name, value)| WalletRuneBalance {
                    name: name.clone(),
                    raw: value.to_string(),
                })
                .collect()
        });
    Ok(WalletBalance {
        cardinal: field("cardinal")?,
        ordinal: field("ordinal")?,
        total: field("total")?,
        runes,
    })
}

/// docs/SPEC.md item 3: "Inscriptions gallery: static previews... loaded
/// from the ord server" -- just enough per entry (id + postage) for the
/// gallery to render a sandboxed `<iframe src="<ord-origin>/preview/
/// <id>">` per Foundation D; DECISIONS.md Phase 5 VERIFY confirmed
/// `/preview/<id>` (not `/content/<id>`) is the right embed target.
#[derive(Debug, Clone, Serialize, TS)]
pub struct WalletInscriptionEntry {
    pub id: String,
    #[ts(type = "number")]
    pub postage: u64,
}

#[tauri::command]
async fn wallet_inscriptions(
    chain: Chain,
    node_manager: tauri::State<'_, NodeManager>,
    store: tauri::State<'_, Arc<Mutex<Store>>>,
    executor: tauri::State<'_, Executor>,
) -> Result<Vec<WalletInscriptionEntry>, TypedError> {
    let ctx = wallet_context(chain, &node_manager, &store, &executor)?;
    let response = nk_ord::wallet::wallet_inscriptions(&executor, &ctx.target())
        .await
        .map_err(TypedError::from)?;
    let entries = response.as_array().ok_or_else(|| {
        TypedError::from("ord did not return a JSON array of inscriptions".to_string())
    })?;
    entries
        .iter()
        .map(|entry| {
            let id = entry
                .get("inscription")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    TypedError::from(
                        "ord inscription entry is missing an \"inscription\" id".to_string(),
                    )
                })?;
            let postage = entry
                .get("postage")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| {
                    TypedError::from(
                        "ord inscription entry is missing a numeric \"postage\"".to_string(),
                    )
                })?;
            Ok(WalletInscriptionEntry {
                id: id.to_string(),
                postage,
            })
        })
        .collect()
}

/// How many of the wallet's most recent transactions to show -- same
/// "last N" cap philosophy as `command_history`'s per-environment cap,
/// not configurable yet.
const TRANSACTION_HISTORY_LIMIT: u32 = 50;

/// docs/SPEC.md item 3: "Transaction history." `ord wallet
/// transactions` only reports `{transaction, confirmations}` per entry
/// (DECISIONS.md Phase 5 VERIFY) -- no amount, direction, or time -- so
/// this joins each txid against bitcoind's own wallet-scoped
/// `gettransaction` for the fields a history list actually needs.
#[derive(Debug, Clone, Serialize, TS)]
pub struct WalletTransactionEntry {
    pub txid: String,
    /// Net effect on the wallet's balance: negative for a send,
    /// positive for a receive (bitcoind's `gettransaction.amount`,
    /// already netted across every output -- no manual summing of
    /// `details[]` needed).
    #[ts(type = "number")]
    pub amount_sats: i64,
    /// Bitcoin Core's `gettransaction.confirmations` can go negative
    /// for a conflicted/abandoned transaction, hence `i64` not `u64`.
    #[ts(type = "number")]
    pub confirmations: i64,
    #[ts(type = "number")]
    pub time: u64,
    pub generated: bool,
}

#[tauri::command]
async fn wallet_transaction_history(
    chain: Chain,
    node_manager: tauri::State<'_, NodeManager>,
    store: tauri::State<'_, Arc<Mutex<Store>>>,
    executor: tauri::State<'_, Executor>,
) -> Result<Vec<WalletTransactionEntry>, TypedError> {
    let ctx = wallet_context(chain, &node_manager, &store, &executor)?;
    let response = nk_ord::wallet::wallet_transactions(
        &executor,
        &ctx.target(),
        Some(TRANSACTION_HISTORY_LIMIT),
    )
    .await
    .map_err(TypedError::from)?;
    let entries = response.as_array().ok_or_else(|| {
        TypedError::from("ord did not return a JSON array of transactions".to_string())
    })?;

    let mut history = Vec::with_capacity(entries.len());
    for entry in entries {
        let txid = entry
            .get("transaction")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                TypedError::from(
                    "ord transaction entry is missing a \"transaction\" id".to_string(),
                )
            })?;
        let detail = ctx
            .rpc
            .wallet_get_transaction(DEFAULT_WALLET_NAME, txid)
            .await
            .map_err(|e| TypedError::from(e.to_string()))?;
        let field_f64 = |key: &str| {
            detail.get(key).and_then(|v| v.as_f64()).ok_or_else(|| {
                TypedError::from(format!(
                    "bitcoind did not return a numeric \"{key}\" transaction field"
                ))
            })
        };
        history.push(WalletTransactionEntry {
            txid: txid.to_string(),
            amount_sats: (field_f64("amount")? * 100_000_000.0).round() as i64,
            confirmations: detail
                .get("confirmations")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| {
                    TypedError::from(
                        "bitcoind did not return a numeric \"confirmations\" field".to_string(),
                    )
                })?,
            time: detail.get("time").and_then(|v| v.as_u64()).ok_or_else(|| {
                TypedError::from("bitcoind did not return a numeric \"time\" field".to_string())
            })?,
            generated: detail
                .get("generated")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        });
    }
    Ok(history)
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

/// What the app tells the user for each way the unlock before a signing
/// action can be refused.
fn unlock_error_to_typed(error: nk_rpc::UnlockError) -> TypedError {
    match error {
        // The frontend prompts for the passphrase on this code.
        nk_rpc::UnlockError::PassphraseRequired => TypedError {
            code: Some(AppErrorCode::WalletLocked),
            message: error.to_string(),
        },
        // A message and a "what to do" the user can act on (the Console's
        // `encryptwallet`); a passphrase prompt cannot fix it.
        nk_rpc::UnlockError::MainnetWalletNotEncrypted => TypedError {
            code: Some(AppErrorCode::WalletNotEncrypted),
            message: error.to_string(),
        },
        other => TypedError::from(other.to_string()),
    }
}

/// Whether Bitcoin Core rejected a passphrase (`-14`, "The wallet passphrase
/// entered was incorrect").
fn is_wrong_passphrase(error: &nk_rpc::UnlockError) -> bool {
    matches!(
        error,
        nk_rpc::UnlockError::Rpc(nk_rpc::RpcError::Rpc { code: -14, .. })
    )
}

/// Runs `action` -- an `ord` command that signs -- with the wallet ready to
/// sign, and puts it back the way it was (docs/SPEC.md item 3, Foundation D).
///
/// The one place that decides *how* a wallet is unlocked, shared by
/// `wallet_send` and both inscribe commands (each used to carry its own copy,
/// which asked for a passphrase on a wallet that has none):
///
/// - an **encrypted** wallet is unlocked with `walletpassphrase` for a short
///   timeout (the passphrase comes from the caller, or from what the user chose
///   to have remembered), and **re-locked afterwards whatever the action's
///   outcome**;
/// - an **unencrypted wallet on a test chain** (regtest, signet, testnet4) needs
///   no unlock: nothing is unlocked, so nothing is re-locked;
/// - an **unencrypted wallet on mainnet is refused** -- every mainnet wallet is
///   encrypted (CLAUDE.md, Mainnet safety), and one that is not is not
///   something to sign with.
///
/// Whether a wallet is encrypted is asked of Bitcoin Core (`getwalletinfo`),
/// never assumed -- see `nk_rpc::RpcClient::unlock_for_signing`. A dry-run
/// needs none of this (confirmed live, DECISIONS.md Phase 5 VERIFY) and does
/// not come through here.
///
/// A **remembered** passphrase that Core now rejects (the wallet's passphrase
/// was changed, or the data folder now holds another wallet) is forgotten and
/// reported as `WALLET_LOCKED`, so the frontend asks for the passphrase again
/// instead of retrying the stale one for the rest of the remember window.
async fn with_wallet_unlocked<T, Fut>(
    chain: Chain,
    rpc: &nk_rpc::RpcClient,
    wallet_session: &WalletSession,
    passphrase: Option<String>,
    remember: bool,
    action: impl FnOnce() -> Fut,
) -> Result<T, TypedError>
where
    Fut: std::future::Future<Output = Result<T, TypedError>>,
{
    // Wrapped in `Zeroizing` immediately -- Phase 5 security self-review
    // (DECISIONS.md): neither this plain-text `String` from IPC nor
    // `WalletSession`'s own copy should linger unzeroized past its use.
    let explicit = passphrase.map(zeroize::Zeroizing::new);
    let came_from_the_session = explicit.is_none();
    let passphrase: Option<zeroize::Zeroizing<String>> =
        explicit.or_else(|| wallet_session.get(chain));

    let unlock = match rpc
        .unlock_for_signing(
            DEFAULT_WALLET_NAME,
            passphrase.as_ref().map(|p| p.as_str()),
            WALLET_UNLOCK_TIMEOUT_SECS,
        )
        .await
    {
        Ok(unlock) => unlock,
        Err(error) if came_from_the_session && is_wrong_passphrase(&error) => {
            wallet_session.forget(chain);
            return Err(TypedError {
                code: Some(AppErrorCode::WalletLocked),
                message: "The passphrase Nodekeeper remembered no longer unlocks this wallet -- \
                          enter the passphrase again."
                    .to_string(),
            });
        }
        Err(error) => return Err(unlock_error_to_typed(error)),
    };
    if unlock == nk_rpc::SigningUnlock::Unlocked && remember {
        if let Some(passphrase) = passphrase {
            wallet_session.remember(chain, passphrase);
        }
    }

    let result = action().await;
    if unlock == nk_rpc::SigningUnlock::Unlocked {
        // Best-effort: a lock failure here shouldn't hide the action's own
        // result (success or failure) from the caller.
        let _ = rpc.wallet_lock(DEFAULT_WALLET_NAME).await;
    }
    result
}

/// The mainnet rule for the **console's** `ord wallet ...` commands, which sign
/// on their own (the console has no unlock step; the user unlocks by hand): on
/// mainnet, a command that can sign and broadcast is refused unless the wallet
/// is encrypted -- the same rule `with_wallet_unlocked` applies to the guided
/// screens. Read-only commands and a dry-run (which signs nothing) are not
/// gated, and other chains are not either. Also refuses `--name`: the app has
/// one wallet per environment, and a different name would sign with a wallet
/// this check never looked at. Fails closed: if Core cannot say whether the
/// wallet is encrypted, the command does not run.
async fn require_encrypted_wallet_for_console_signing(
    chain: Chain,
    rpc: &nk_rpc::RpcClient,
    args: &[String],
    class: nk_core::console_safety::OrdCommandClass,
    dry_run: bool,
) -> Result<(), TypedError> {
    use nk_core::console_safety::OrdCommandClass;
    if chain != Chain::Mainnet || class == OrdCommandClass::ReadOnly {
        return Ok(());
    }
    if dry_run && class == OrdCommandClass::StateChangingWithDryRun {
        return Ok(());
    }
    if args
        .iter()
        .any(|arg| arg == "--name" || arg.starts_with("--name="))
    {
        return Err(TypedError::from(
            "Nodekeeper uses one wallet per environment, so a mainnet command that signs cannot \
             pick another with --name -- remove it."
                .to_string(),
        ));
    }
    match rpc.wallet_protection(DEFAULT_WALLET_NAME).await {
        Ok(nk_rpc::WalletProtection::Encrypted) => Ok(()),
        Ok(nk_rpc::WalletProtection::Unencrypted) => Err(unlock_error_to_typed(
            nk_rpc::UnlockError::MainnetWalletNotEncrypted,
        )),
        Err(error) => Err(TypedError::from(error.to_string())),
    }
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

    let response = with_wallet_unlocked(
        chain,
        &ctx.rpc,
        &wallet_session,
        passphrase,
        remember,
        || async {
            nk_ord::wallet::wallet_send(&executor, &ctx.target(), &address, &asset, fee_rate, false)
                .await
                .map_err(TypedError::from)
        },
    )
    .await?;

    parse_wallet_send_result(response)
}

/// The confirm dialog / "Learn mode" (docs/SPEC.md item 6) view of what
/// a raw console command line would do, before anything runs.
#[derive(Debug, Clone, Serialize, TS)]
pub struct ConsoleCommandPreview {
    /// Which half of the console this is -- `BitcoinCli` or `OrdCli`
    /// only; reuses `nk_exec::CommandSource` rather than a new type
    /// since it already carries exactly this distinction.
    pub source: nk_exec::CommandSource,
    /// The exact command that will run, with any secret argument
    /// (a passphrase, a private key) already redacted -- always safe
    /// to show directly in the confirm dialog.
    pub display: String,
    pub read_only: bool,
    /// True when a `--dry-run` preview should be fetched and shown
    /// before the real confirmation (ord commands that support it).
    pub supports_dry_run: bool,
    /// `Some(reason)` when this must be refused outright and
    /// `console_run` will error rather than execute -- a fund-moving
    /// bitcoin-cli command against the wallet ord uses, an ord
    /// `create`/`restore` that could print a recovery phrase, or a command
    /// that can print private keys (`listdescriptors true`, `gethdkeys`
    /// with `private`, `ord wallet dump`).
    pub blocked_reason: Option<String>,
}

/// The arguments of a bitcoin-cli console call that are secret, hidden **by
/// position** (see `nk_core::console_safety::bitcoin_rpc_secret_arg_mask`:
/// everything from a secret method's first secret argument on, and every
/// argument of a method Bitcoin Core does not have): the mask itself, the
/// secret values (a second net for text substitution -- the output and the
/// recorded command), and the command line as it may be shown or stored, with
/// each run of secret arguments replaced by ONE `[redacted]` (how many there
/// are would tell how many words a passphrase has).
struct ConsoleSecrets {
    mask: Vec<bool>,
    values: Vec<String>,
    display: String,
}

fn console_secrets(method: &str, args: &[String]) -> ConsoleSecrets {
    let mask = nk_core::console_safety::bitcoin_rpc_secret_arg_mask(method, args);
    let values: Vec<String> = args
        .iter()
        .zip(&mask)
        .filter(|(_, hidden)| **hidden)
        .map(|(arg, _)| arg.clone())
        .collect();
    let mut parts = vec![method.to_string()];
    let mut previous_hidden = false;
    for (arg, hidden) in args.iter().zip(&mask) {
        if *hidden {
            if !previous_hidden {
                parts.push("[redacted]".to_string());
            }
        } else {
            parts.push(arg.clone());
        }
        previous_hidden = *hidden;
    }
    // An extended private key or a WIF key inside a descriptor that is not at
    // a secret position (`deriveaddresses wpkh(...)`, `getdescriptorinfo`) is
    // removed from the display too -- the same backstop the executor applies.
    let display = nk_exec::redact::scrub_private_keys(&parts.join(" ")).into_owned();
    ConsoleSecrets {
        mask,
        values,
        display,
    }
}

/// What the console hands back to the screen is scrubbed of private keys the
/// way the executor scrubs what it records: an error message can quote what
/// was typed (`key '<WIF>' is not valid`).
fn scrub_for_screen(text: String) -> String {
    nk_exec::redact::scrub_private_keys(&text).into_owned()
}

fn scrub_value_for_screen(value: serde_json::Value) -> serde_json::Value {
    match nk_exec::redact::scrub_private_keys(&value.to_string()) {
        std::borrow::Cow::Borrowed(_) => value,
        std::borrow::Cow::Owned(scrubbed) => {
            serde_json::from_str(&scrubbed).unwrap_or(serde_json::Value::String(scrubbed))
        }
    }
}

fn scrub_error_for_screen(mut error: TypedError) -> TypedError {
    error.message = scrub_for_screen(error.message);
    error
}

/// Refuses a console line whose first word is not a plain command name -- a
/// pasted `bitcoin-cli ...`, `bitcoin-cli.exe`, a path, an option. Whatever
/// follows would sit at positions nothing knows to hide. The error text never
/// echoes the pasted line.
fn refuse_pasted_command_prefix(method: &str) -> Result<(), TypedError> {
    match nk_core::console_safety::pasted_command_prefix_problem(method) {
        Some(message) => Err(TypedError::from(message.to_string())),
        None => Ok(()),
    }
}

/// Classifies a raw console command line (docs/SPEC.md item 6) without
/// running anything -- the frontend calls this first, shows a confirm
/// dialog (or refuses outright) based on the result, and only then
/// calls `console_run`. Takes no `chain`/state: classification is the
/// same regardless of which environment a tab is locked to.
#[tauri::command]
fn console_classify(command_line: String) -> Result<ConsoleCommandPreview, TypedError> {
    let parsed = nk_core::console_parse::parse_command_line(&command_line)
        .map_err(|e| TypedError::from(e.to_string()))?;

    if parsed.command == "ord" {
        let sub_args: Vec<&str> = parsed.args.iter().map(String::as_str).collect();
        let class = nk_core::console_safety::classify_ord_wallet_subcommand(&sub_args);
        let display = scrub_for_screen(nk_core::console_safety::ord_console_display(
            &sub_args, class,
        ));
        use nk_core::console_safety::OrdCommandClass;
        let (read_only, supports_dry_run, blocked_reason) = match class {
            OrdCommandClass::ReadOnly => (true, false, None),
            OrdCommandClass::StateChangingWithDryRun => (false, true, None),
            OrdCommandClass::StateChangingNoDryRun => (false, false, None),
            OrdCommandClass::BlockedUseWalletScreen => (
                false,
                false,
                Some(
                    "This can print a recovery phrase -- use the Wallet screen's create/restore \
                     flow instead."
                        .to_string(),
                ),
            ),
            OrdCommandClass::BlockedPrivateKeys => (
                false,
                false,
                Some(nk_core::console_safety::PRIVATE_KEYS_BLOCKED_MESSAGE.to_string()),
            ),
        };
        return Ok(ConsoleCommandPreview {
            source: nk_exec::CommandSource::OrdCli,
            display,
            read_only,
            supports_dry_run,
            blocked_reason,
        });
    }

    refuse_pasted_command_prefix(&parsed.command)?;
    // Per *call* (method and arguments): `listdescriptors true` is not the
    // same command as `listdescriptors`.
    let class = nk_core::console_safety::classify_bitcoin_rpc_call(&parsed.command, &parsed.args);
    let display = console_secrets(&parsed.command, &parsed.args).display;
    use nk_core::console_safety::RpcCommandClass;
    let (read_only, blocked_reason) = match class {
        RpcCommandClass::ReadOnly => (true, None),
        RpcCommandClass::StateChanging => (false, None),
        RpcCommandClass::FundMoving => (
            false,
            Some(
                "This can move funds and is blocked for the wallet ord uses -- use the Send \
                 screen instead."
                    .to_string(),
            ),
        ),
        RpcCommandClass::BlockedPrivateKeys => (
            false,
            Some(nk_core::console_safety::PRIVATE_KEYS_BLOCKED_MESSAGE.to_string()),
        ),
    };
    Ok(ConsoleCommandPreview {
        source: nk_exec::CommandSource::BitcoinCli,
        display,
        read_only,
        supports_dry_run: false,
        blocked_reason,
    })
}

/// Actually runs a console command line (docs/SPEC.md item 6), after
/// the frontend has already called `console_classify` and (for
/// anything not `read_only`) shown the user a confirmation. Refuses a
/// `blocked_reason` command outright regardless of what the frontend
/// did -- the backend is the real enforcement point, not the dialog.
/// `dry_run` only has an effect for an ord command that
/// `console_classify` reported `supports_dry_run: true`; it's silently
/// ignored otherwise (bitcoin-cli has no dry-run concept -- raw
/// Core-wallet spend previews are still a separate tracked task).
#[tauri::command]
async fn console_run(
    chain: Chain,
    command_line: String,
    dry_run: bool,
    node_manager: tauri::State<'_, NodeManager>,
    store: tauri::State<'_, Arc<Mutex<Store>>>,
    executor: tauri::State<'_, Executor>,
) -> Result<serde_json::Value, TypedError> {
    let parsed = nk_core::console_parse::parse_command_line(&command_line)
        .map_err(|e| TypedError::from(e.to_string()))?;

    if parsed.command == "ord" {
        let sub_args: Vec<&str> = parsed.args.iter().map(String::as_str).collect();
        let class = nk_core::console_safety::classify_ord_wallet_subcommand(&sub_args);
        match class {
            nk_core::console_safety::OrdCommandClass::BlockedUseWalletScreen => {
                return Err(TypedError::from(
                    "This can print a recovery phrase -- use the Wallet screen's create/restore \
                     flow instead."
                        .to_string(),
                ));
            }
            nk_core::console_safety::OrdCommandClass::BlockedPrivateKeys => {
                return Err(TypedError::from(
                    nk_core::console_safety::PRIVATE_KEYS_BLOCKED_MESSAGE.to_string(),
                ));
            }
            _ => {}
        }
        let ctx = wallet_context(chain, &node_manager, &store, &executor)?;
        require_encrypted_wallet_for_console_signing(chain, &ctx.rpc, &parsed.args, class, dry_run)
            .await?;
        let mut args = parsed.args;
        if dry_run
            && matches!(
                class,
                nk_core::console_safety::OrdCommandClass::StateChangingWithDryRun
            )
        {
            args.push("--dry-run".to_string());
        }
        return nk_ord::wallet::run_console_subcommand(&executor, &ctx.target(), args, "console")
            .await
            .map(scrub_value_for_screen)
            .map_err(|e| scrub_error_for_screen(TypedError::from(e)));
    }

    refuse_pasted_command_prefix(&parsed.command)?;
    let class = nk_core::console_safety::classify_bitcoin_rpc_call(&parsed.command, &parsed.args);
    match class {
        nk_core::console_safety::RpcCommandClass::FundMoving => {
            return Err(TypedError::from(
                "This can move funds and is blocked for the wallet ord uses -- use the Send \
                 screen instead."
                    .to_string(),
            ));
        }
        nk_core::console_safety::RpcCommandClass::BlockedPrivateKeys => {
            return Err(TypedError::from(
                nk_core::console_safety::PRIVATE_KEYS_BLOCKED_MESSAGE.to_string(),
            ));
        }
        _ => {}
    }
    let (_, rpc) = bitcoin_rpc_context(chain, &node_manager, &store, &executor)?;
    let secrets = console_secrets(&parsed.command, &parsed.args);
    let json_args = nk_core::console_parse::coerce_json_args(&parsed.args);
    rpc.call_masked(
        &parsed.command,
        json_args,
        "console",
        secrets.values,
        &secrets.mask,
        false,
    )
    .await
    .map(scrub_value_for_screen)
    .map_err(|e| scrub_error_for_screen(TypedError::from(e.to_string())))
}

/// One script offered by the script runner (docs/SPEC.md item 6).
/// Every field here is Nodekeeper's own, never read from the script
/// file's own content -- `regtest_only` in particular: "the 'regtest
/// only' restriction is enforced by the runner, not the script," since
/// a script file could be edited to lie about it.
#[derive(Debug, Clone, Serialize, TS)]
pub struct ScriptInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Matches `nk_scripts::Interpreter`'s variant names, lowercased --
    /// only "python" exists today (all 3 built-ins), but the shape
    /// supports node/bash scripts later without changing it.
    pub language: String,
    pub regtest_only: bool,
}

/// The 3 built-in example scripts (docs/SPEC.md item 6), embedded at
/// compile time so the app never depends on a separate install step or
/// bundled-resource path resolution -- `run_script` writes the current
/// build's copy to disk fresh on every run (cheap: these are a few KB
/// of text), so there's no risk of a stale on-disk copy surviving an
/// app update. None are regtest-only: all three only ever read state
/// (HTTP GETs to ord, a read-only RPC call, local disk stats), nothing
/// they do is unsafe on mainnet.
fn built_in_scripts() -> Vec<(ScriptInfo, &'static str)> {
    vec![
        (
            ScriptInfo {
                id: "export_inscriptions_csv".to_string(),
                name: "Export inscriptions to CSV".to_string(),
                description: "Exports every inscription held by an address to a CSV file. \
                    Argument: the address to export."
                    .to_string(),
                language: "python".to_string(),
                regtest_only: false,
            },
            include_str!("../scripts/export_inscriptions_csv.py"),
        ),
        (
            ScriptInfo {
                id: "alert_node_behind".to_string(),
                name: "Alert when the node falls behind".to_string(),
                description: "Exits non-zero (suitable for a scheduled task) if Bitcoin Core \
                    hasn't caught up to its own peers' tip. Optional argument: max blocks \
                    behind before alerting (default 2)."
                    .to_string(),
                language: "python".to_string(),
                regtest_only: false,
            },
            include_str!("../scripts/alert_node_behind.py"),
        ),
        (
            ScriptInfo {
                id: "disk_usage_report".to_string(),
                name: "Daily disk-usage report".to_string(),
                description: "Reports how much space this environment is using and how much \
                    is free on the volume, warning if free space is low."
                    .to_string(),
                language: "python".to_string(),
                regtest_only: false,
            },
            include_str!("../scripts/disk_usage_report.py"),
        ),
    ]
}

#[tauri::command]
fn list_scripts() -> Vec<ScriptInfo> {
    built_in_scripts()
        .into_iter()
        .map(|(info, _)| info)
        .collect()
}

/// Whether each interpreter the script runner supports is actually
/// usable on this machine (docs/SPEC.md item 6: "Detect whether Python
/// and Node are installed... bash is unavailable on stock Windows, so
/// say so") -- a real probe (`nk_scripts::detect_interpreters`), not a
/// PATH-presence guess; see DECISIONS.md for why that distinction
/// matters (Windows' Python Store stub, the WSL `bash.exe` stub).
#[derive(Debug, Clone, Serialize, TS)]
pub struct InterpreterAvailability {
    pub language: String,
    pub available: bool,
}

#[tauri::command]
async fn list_available_interpreters(
    executor: tauri::State<'_, Executor>,
) -> Result<Vec<InterpreterAvailability>, TypedError> {
    let found = nk_scripts::detect_interpreters(&executor, "system").await;
    Ok(nk_scripts::Interpreter::ALL
        .iter()
        .map(|interpreter| InterpreterAvailability {
            language: format!("{interpreter:?}").to_lowercase(),
            available: found.iter().any(|d| d.interpreter == *interpreter),
        })
        .collect())
}

fn scripts_root() -> std::path::PathBuf {
    data_root().join("scripts")
}

/// Runs a built-in script (docs/SPEC.md item 6) against `chain`'s
/// environment. Refuses outright if the script is `regtest_only` and
/// `chain` isn't Regtest -- re-checked here regardless of what the
/// frontend already knew, same "the backend is the enforcement point"
/// shape as `console_run`. Does not require bitcoind/ord to be
/// running: a script that needs one will simply get a connection error
/// from it, same as a user's own script would.
#[tauri::command]
async fn run_script(
    chain: Chain,
    script_id: String,
    args: Vec<String>,
    store: tauri::State<'_, Arc<Mutex<Store>>>,
    executor: tauri::State<'_, Executor>,
) -> Result<String, TypedError> {
    let (info, source) = built_in_scripts()
        .into_iter()
        .find(|(info, _)| info.id == script_id)
        .ok_or_else(|| TypedError::from(format!("unknown script: {script_id}")))?;

    if !nk_scripts::script_allowed_on_chain(info.regtest_only, chain == Chain::Regtest) {
        return Err(TypedError::from(format!(
            "\"{}\" is restricted to Regtest -- not available on {chain:?}.",
            info.name
        )));
    }

    let interpreter = match info.language.as_str() {
        "python" => nk_scripts::Interpreter::Python,
        other => {
            return Err(TypedError::from(format!(
                "unsupported script language: {other}"
            )))
        }
    };
    let found = nk_scripts::detect_interpreters(&executor, "system").await;
    let detected = found
        .into_iter()
        .find(|d| d.interpreter == interpreter)
        .ok_or_else(|| {
            TypedError::from(format!(
                "No working {} interpreter was found on this machine.",
                info.language
            ))
        })?;

    let environment = Environment::new_default(chain, &environment_data_root(&store));
    let cookie_path = environment.bitcoin_cookie_path();
    let rpc_url = format!("http://127.0.0.1:{}", environment.rpc_port);
    let ord_url = format!("http://127.0.0.1:{}", environment.ord_port);
    let env_vars = nk_scripts::script_env_vars(&environment.name, &rpc_url, &cookie_path, &ord_url);

    let dir = scripts_root();
    std::fs::create_dir_all(&dir).map_err(|e| TypedError::from(e.to_string()))?;
    let script_path = dir.join(format!("{script_id}.{}", interpreter.file_extension()));
    std::fs::write(&script_path, source).map_err(|e| TypedError::from(e.to_string()))?;

    let outcome = nk_scripts::run_script(
        &executor,
        &detected,
        &script_path,
        args,
        env_vars,
        &environment.name,
    )
    .await
    .map_err(|e| TypedError::from(e.to_string()))?;

    let mut output = String::from_utf8_lossy(&outcome.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&outcome.stderr);
    if !stderr.is_empty() {
        output.push_str(&stderr);
    }
    if outcome.exit_code != Some(0) {
        return Err(TypedError::from(format!(
            "script exited with status {:?}:\n{output}",
            outcome.exit_code
        )));
    }
    Ok(output)
}

/// Fee-rate estimate in sat/vB for the Send screen (docs/SPEC.md item
/// 3: "estimates only from the local node"). `conf_target` is in
/// blocks (a smaller number asks for a faster, more expensive
/// estimate). `None` means bitcoind has no estimate yet -- confirmed
/// live (DECISIONS.md Phase 5 VERIFY) this is regtest's normal
/// response, not an error; the frontend falls back to a configurable
/// regtest default or requires manual entry on mainnet, per spec.
/// Doesn't need ord at all, so only requires the node running, not the
/// wallet (`bitcoin_rpc_context`, not `wallet_context`).
#[tauri::command]
async fn wallet_fee_estimate(
    chain: Chain,
    conf_target: u32,
    node_manager: tauri::State<'_, NodeManager>,
    store: tauri::State<'_, Arc<Mutex<Store>>>,
    executor: tauri::State<'_, Executor>,
) -> Result<Option<f64>, TypedError> {
    let (_environment, rpc) = bitcoin_rpc_context(chain, &node_manager, &store, &executor)?;
    let response = rpc
        .estimate_smart_fee(conf_target, false)
        .await
        .map_err(|e| TypedError::from(e.to_string()))?;
    // bitcoind reports BTC/kvB; sat/vB is BTC/kvB * 100_000 (100_000_000
    // sats/BTC / 1000 vB/kvB).
    Ok(response
        .get("feerate")
        .and_then(|v| v.as_f64())
        .map(|btc_per_kvb| btc_per_kvb * 100_000.0))
}

/// docs/SPEC.md item 4's Inscribe studio: single inscribe's result --
/// shared by the plain-inscribe and reinscribe Tauri commands, and by
/// each entry of a batch's result (`parse_inscribe_results`).
#[derive(Debug, Clone, Serialize, TS)]
pub struct InscribeResult {
    pub id: String,
    pub location: String,
    #[ts(type = "number")]
    pub fee: u64,
}

fn parse_inscribe_results(response: &serde_json::Value) -> Result<Vec<InscribeResult>, TypedError> {
    let entries = response
        .get("inscriptions")
        .and_then(|v| v.as_array())
        .ok_or_else(|| TypedError::from("ord did not return an inscriptions array".to_string()))?;
    let fee = response
        .get("total_fees")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| TypedError::from("ord did not return total_fees".to_string()))?;
    entries
        .iter()
        .map(|entry| {
            let id = entry
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    TypedError::from("ord did not return an inscription id".to_string())
                })?
                .to_string();
            let location = entry
                .get("location")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    TypedError::from("ord did not return an inscription location".to_string())
                })?
                .to_string();
            Ok(InscribeResult { id, location, fee })
        })
        .collect()
}

fn parse_single_inscribe_result(response: serde_json::Value) -> Result<InscribeResult, TypedError> {
    parse_inscribe_results(&response)?
        .into_iter()
        .next()
        .ok_or_else(|| TypedError::from("ord did not return an inscription".to_string()))
}

/// Preview only -- like `wallet_send_dry_run`, ord's `--dry-run` needs
/// no wallet unlock at all (confirmed live for `send`, DECISIONS.md
/// Phase 5 VERIFY; the same flag works the same way across every `ord
/// wallet` subcommand), so this never touches `WalletSession`.
/// `reinscribe_satpoint: Some(sp)` previews a reinscribe instead of a
/// plain inscribe -- same underlying `ord wallet inscribe`, just with
/// `--satpoint`/`--reinscribe` added (DECISIONS.md Phase 6 VERIFY).
#[tauri::command]
#[allow(clippy::too_many_arguments)]
async fn wallet_inscribe_dry_run(
    chain: Chain,
    file_path: String,
    fee_rate: f64,
    postage: Option<u64>,
    parent: Option<String>,
    reinscribe_satpoint: Option<String>,
    node_manager: tauri::State<'_, NodeManager>,
    store: tauri::State<'_, Arc<Mutex<Store>>>,
    executor: tauri::State<'_, Executor>,
) -> Result<InscribeResult, TypedError> {
    let ctx = wallet_context(chain, &node_manager, &store, &executor)?;
    let response = nk_ord::wallet::inscribe(
        &executor,
        &ctx.target(),
        std::path::Path::new(&file_path),
        fee_rate,
        postage,
        parent.as_deref(),
        reinscribe_satpoint.as_deref(),
        true,
    )
    .await
    .map_err(TypedError::from)?;
    parse_single_inscribe_result(response)
}

/// The real, signing inscribe (and reinscribe, via `reinscribe_
/// satpoint`) -- same unlock/remember/always-relock shape as
/// `wallet_send`.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
async fn wallet_inscribe(
    chain: Chain,
    file_path: String,
    fee_rate: f64,
    postage: Option<u64>,
    parent: Option<String>,
    reinscribe_satpoint: Option<String>,
    passphrase: Option<String>,
    remember: bool,
    node_manager: tauri::State<'_, NodeManager>,
    store: tauri::State<'_, Arc<Mutex<Store>>>,
    executor: tauri::State<'_, Executor>,
    wallet_session: tauri::State<'_, WalletSession>,
) -> Result<InscribeResult, TypedError> {
    let ctx = wallet_context(chain, &node_manager, &store, &executor)?;

    let response = with_wallet_unlocked(
        chain,
        &ctx.rpc,
        &wallet_session,
        passphrase,
        remember,
        || async {
            nk_ord::wallet::inscribe(
                &executor,
                &ctx.target(),
                std::path::Path::new(&file_path),
                fee_rate,
                postage,
                parent.as_deref(),
                reinscribe_satpoint.as_deref(),
                false,
            )
            .await
            .map_err(TypedError::from)
        },
    )
    .await?;

    parse_single_inscribe_result(response)
}

/// docs/SPEC.md item 4's "Visual batch-YAML builder." No `passphrase`-
/// less dry-run split like single inscribe has its own pair of
/// commands -- `dry_run` is just a bool here, since both paths share
/// every other parameter and batch has no reinscribe option to also
/// branch on (DECISIONS.md Phase 6 VERIFY: ord 0.29.0's batch schema
/// rejects a per-entry `reinscribe` field outright).
#[tauri::command]
async fn wallet_inscribe_batch_dry_run(
    chain: Chain,
    file_paths: Vec<String>,
    fee_rate: f64,
    node_manager: tauri::State<'_, NodeManager>,
    store: tauri::State<'_, Arc<Mutex<Store>>>,
    executor: tauri::State<'_, Executor>,
) -> Result<Vec<InscribeResult>, TypedError> {
    let ctx = wallet_context(chain, &node_manager, &store, &executor)?;
    let entries: Vec<_> = file_paths
        .into_iter()
        .map(|p| nk_ord::wallet::BatchInscriptionEntry {
            file_path: std::path::PathBuf::from(p),
        })
        .collect();
    let response =
        nk_ord::wallet::batch_inscribe(&executor, &ctx.target(), &entries, fee_rate, true)
            .await
            .map_err(TypedError::from)?;
    parse_inscribe_results(&response)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
async fn wallet_inscribe_batch(
    chain: Chain,
    file_paths: Vec<String>,
    fee_rate: f64,
    passphrase: Option<String>,
    remember: bool,
    node_manager: tauri::State<'_, NodeManager>,
    store: tauri::State<'_, Arc<Mutex<Store>>>,
    executor: tauri::State<'_, Executor>,
    wallet_session: tauri::State<'_, WalletSession>,
) -> Result<Vec<InscribeResult>, TypedError> {
    let ctx = wallet_context(chain, &node_manager, &store, &executor)?;

    let entries: Vec<_> = file_paths
        .into_iter()
        .map(|p| nk_ord::wallet::BatchInscriptionEntry {
            file_path: std::path::PathBuf::from(p),
        })
        .collect();
    let response = with_wallet_unlocked(
        chain,
        &ctx.rpc,
        &wallet_session,
        passphrase,
        remember,
        || async {
            nk_ord::wallet::batch_inscribe(&executor, &ctx.target(), &entries, fee_rate, false)
                .await
                .map_err(TypedError::from)
        },
    )
    .await?;

    parse_inscribe_results(&response)
}

/// docs/SPEC.md item 4: "Drag-and-drop file, preview... content-type
/// check, size warning" -- reads the dropped file's metadata (Tauri's
/// drag-drop event hands the frontend a real filesystem path, not a
/// browser `File` object, so there's no size/type available client-
/// side without this). `content_type` is a best-effort guess from the
/// file extension, same basis ord itself uses at inscribe time -- not
/// authoritative, just enough for the UI's warning label. `data_url` is
/// `None` above `MAX_PREVIEW_BYTES` (the sandboxed preview iframe falls
/// back to a "too large to preview" placeholder instead of embedding
/// megabytes of base64 through IPC for no benefit the user can see
/// anyway in a small iframe).
const MAX_PREVIEW_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, TS)]
pub struct FilePreview {
    #[ts(type = "number")]
    pub size_bytes: u64,
    pub content_type: String,
    pub data_url: Option<String>,
}

fn guess_content_type(path: &std::path::Path) -> &'static str {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "html" | "htm" => "text/html",
        "txt" => "text/plain",
        "css" => "text/css",
        "js" | "mjs" => "text/javascript",
        "json" => "application/json",
        "md" => "text/markdown",
        "pdf" => "application/pdf",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "gltf" => "model/gltf+json",
        "glb" => "model/gltf-binary",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        _ => "application/octet-stream",
    }
}

#[tauri::command]
fn inscribe_file_preview(path: String) -> Result<FilePreview, TypedError> {
    let path = std::path::Path::new(&path);
    let metadata = std::fs::metadata(path).map_err(|e| TypedError::from(e.to_string()))?;
    let size_bytes = metadata.len();
    let content_type = guess_content_type(path);
    let data_url = if size_bytes <= MAX_PREVIEW_BYTES {
        let bytes = std::fs::read(path).map_err(|e| TypedError::from(e.to_string()))?;
        use base64::Engine;
        let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
        Some(format!("data:{content_type};base64,{encoded}"))
    } else {
        None
    };
    Ok(FilePreview {
        size_bytes,
        content_type: content_type.to_string(),
        data_url,
    })
}

/// docs/SPEC.md item 4's REINSCRIBE MODE: picking an owned inscription
/// fills in its satpoint, and `sat` is the entry point into that sat's
/// full inscription history (`sat_inscriptions` below). `sat: None`
/// means the running ord server's `--index-sats` is off (Foundation F)
/// -- absent, not a made-up value -- confirmed live (DECISIONS.md
/// Phase 6 VERIFY) that ord's own inscription-detail JSON sets `sat`
/// to `null` in exactly that case.
#[derive(Debug, Clone, Serialize, TS)]
pub struct InscriptionDetail {
    pub id: String,
    pub satpoint: String,
    #[ts(type = "number | null")]
    pub sat: Option<u64>,
    #[ts(type = "number")]
    pub number: i64,
}

#[tauri::command]
async fn inscription_detail(
    chain: Chain,
    id: String,
    node_manager: tauri::State<'_, NodeManager>,
    store: tauri::State<'_, Arc<Mutex<Store>>>,
    executor: tauri::State<'_, Executor>,
) -> Result<InscriptionDetail, TypedError> {
    let client = ord_client(chain, &node_manager, &store, &executor)?;
    let response = client
        .inscription(&id, false)
        .await
        .map_err(|e| TypedError::from(e.to_string()))?;
    let satpoint = response
        .get("satpoint")
        .and_then(|v| v.as_str())
        .ok_or_else(|| TypedError::from("ord did not return a satpoint".to_string()))?
        .to_string();
    let sat = response.get("sat").and_then(|v| v.as_u64());
    let number = response
        .get("number")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| TypedError::from("ord did not return an inscription number".to_string()))?;
    Ok(InscriptionDetail {
        id,
        satpoint,
        sat,
        number,
    })
}

/// docs/SPEC.md item 4: "Show all existing inscriptions on that sat, in
/// order." VERIFIED live (DECISIONS.md Phase 6) that `GET /sat/<n>`'s
/// `inscriptions` array is already ordered oldest-first. Returns just
/// the ids -- the frontend calls `inscription_detail` on each for its
/// number/preview, same reasoning `WalletInscriptionEntry` already has
/// for the regular gallery (small, review-screen-only list, not a hot
/// loop, so the extra round trips are fine).
#[tauri::command]
async fn sat_inscriptions(
    chain: Chain,
    sat: u64,
    node_manager: tauri::State<'_, NodeManager>,
    store: tauri::State<'_, Arc<Mutex<Store>>>,
    executor: tauri::State<'_, Executor>,
) -> Result<Vec<String>, TypedError> {
    let client = ord_client(chain, &node_manager, &store, &executor)?;
    let response = client
        .sat(sat, false)
        .await
        .map_err(|e| TypedError::from(e.to_string()))?;
    let ids = response
        .get("inscriptions")
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            TypedError::from("ord did not return a sat's inscriptions array".to_string())
        })?
        .iter()
        .map(|v| {
            v.as_str().map(|s| s.to_string()).ok_or_else(|| {
                TypedError::from("a sat's inscriptions array entry was not a string".to_string())
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ids)
}

#[tauri::command]
async fn start_node(
    chain: Chain,
    node_manager: tauri::State<'_, NodeManager>,
    store: tauri::State<'_, Arc<Mutex<Store>>>,
    executor: tauri::State<'_, Executor>,
) -> Result<(), TypedError> {
    let binary_path = configured_bitcoind_path(&store)?;
    let environment = Environment::new_default(chain, &environment_data_root(&store));
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
    let environment = Environment::new_default(chain, &environment_data_root(&store));
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
    let environment = Environment::new_default(chain, &environment_data_root(&store));
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
    let environment = Environment::new_default(chain, &environment_data_root(&store));
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
fn tail_debug_log(
    chain: Chain,
    store: tauri::State<'_, Arc<Mutex<Store>>>,
) -> Result<nk_core::log_tail::LogWindow, TypedError> {
    let environment = Environment::new_default(chain, &environment_data_root(&store));
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
    store: tauri::State<'_, Arc<Mutex<Store>>>,
) -> Result<nk_core::log_tail::LogWindow, TypedError> {
    let environment = Environment::new_default(chain, &environment_data_root(&store));
    nk_core::log_tail::page_before(
        &environment.bitcoin_debug_log_path(),
        end_offset,
        DEFAULT_LOG_WINDOW_BYTES,
    )
    .map_err(TypedError::from)
}

const MAX_LOG_SEARCH_MATCHES: usize = 500;

#[tauri::command]
fn search_debug_log(
    chain: Chain,
    query: String,
    store: tauri::State<'_, Arc<Mutex<Store>>>,
) -> Result<Vec<String>, TypedError> {
    let environment = Environment::new_default(chain, &environment_data_root(&store));
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

/// The directory the running executable lives in -- `current_exe()`
/// itself points at the binary file, not its containing folder.
/// Falls back to `.` (the process's cwd) only if the OS somehow can't
/// report the executable's own path, which in practice never happens
/// on any of this app's target platforms.
fn exe_dir() -> std::path::PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
        .unwrap_or_else(|| std::path::PathBuf::from("."))
}

/// docs/SPEC.md item 12: portable mode is a `config` directory sitting
/// next to the executable -- the "prepare a new portable drive" wizard
/// creates it once, up front, as a deliberate signal, rather than this
/// being inferred by some fragile heuristic (drive letter, path
/// shape, ...). Split out from `data_root()` so the actual branching
/// condition has real test coverage without needing to control where
/// `cargo test`'s own binary happens to run from.
fn is_portable_layout(exe_dir: &std::path::Path) -> bool {
    exe_dir.join("config").is_dir()
}

fn is_portable_install() -> bool {
    is_portable_layout(&exe_dir())
}

/// Where Nodekeeper's own settings database, scripts, and downloaded
/// binaries live (docs/SPEC.md item 12's `/config`, item 1's binary
/// cache) -- portable mode: `<exe_dir>/config`, all-relative-paths as
/// the spec requires. Installed mode: the OS's own per-user
/// application-data directory (`dirs::data_dir()`, matching what
/// Tauri's own `app.path().app_data_dir()` resolves to, joined with a
/// friendly folder name instead of the reverse-DNS bundle identifier --
/// this predates any Tauri `App`/`AppHandle` existing, since it has to
/// be resolved before the settings database can even be opened).
fn data_root() -> std::path::PathBuf {
    if is_portable_install() {
        exe_dir().join("config")
    } else {
        dirs::data_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("Nodekeeper")
    }
}

/// Where environment data (bitcoind/ord's own directories, which can
/// legitimately be huge) lives before the user ever picks their own via
/// the data-directory picker (docs/SPEC.md item 1). Portable mode:
/// `<exe_dir>/data`, the spec's fixed layout. Installed mode: an
/// `environments` subfolder of `data_root()` -- keeps the previous
/// single-directory-by-default behavior (everything under one place
/// unless the user chooses otherwise) while still not literally
/// sharing a folder with the settings database.
fn default_environment_data_root() -> std::path::PathBuf {
    if is_portable_install() {
        exe_dir().join("data")
    } else {
        data_root().join("environments")
    }
}

/// Where every environment's data actually lives -- `ENVIRONMENT_
/// DATA_ROOT_SETTING` if the user has ever chosen one (the
/// data-directory picker, docs/SPEC.md item 1), else
/// `default_environment_data_root()`. Every `Environment::new_default
/// (chain, ...)` call site in this file uses this, not `data_root()`
/// directly, *except* `run()`'s own settings-database bootstrap (see
/// `ENVIRONMENT_DATA_ROOT_SETTING`'s doc comment for why that one has
/// to stay fixed).
fn environment_data_root(store: &tauri::State<'_, Arc<Mutex<Store>>>) -> std::path::PathBuf {
    store
        .lock()
        .unwrap()
        .get_setting(ENVIRONMENT_DATA_ROOT_SETTING)
        .ok()
        .flatten()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(default_environment_data_root)
}

/// Whether this launch is running in portable mode (docs/SPEC.md item
/// 12) -- exposed so the frontend can show portable-only affordances
/// (safe eject, drive info) without duplicating the detection logic.
#[tauri::command]
fn is_portable_mode() -> bool {
    is_portable_install()
}

/// docs/SPEC.md item 12: "unclean-shutdown recovery with clear
/// guidance if an ord index needs rebuilding." Checks both bitcoind's
/// and ord's own stale-pid-file signal (`nk_proc::bitcoind_had_unclean_
/// shutdown`/`ord_had_unclean_shutdown`) for `chain`'s environment --
/// either one being true means the *previous* run of this environment
/// didn't exit cleanly, so this launch may take longer than usual
/// while bitcoind/ord verify or rebuild. A read-only filesystem check,
/// safe to call before anything is started.
#[tauri::command]
fn had_unclean_shutdown(chain: Chain, store: tauri::State<'_, Arc<Mutex<Store>>>) -> bool {
    let environment = Environment::new_default(chain, &environment_data_root(&store));
    nk_proc::bitcoind_had_unclean_shutdown(&environment)
        || nk_proc::ord_had_unclean_shutdown(&environment)
}

/// The Regtest Test Lab's "Mine blocks"/"Get test coins" controls
/// (docs/SPEC.md item 11): mines `count` blocks to the current
/// environment's own wallet via `generatetoaddress`. Regtest-only, not
/// just by policy -- `generatetoaddress` needs regtest's trivial
/// difficulty to be useful at all; on any other chain it would just
/// hang or fail against real proof-of-work.
#[tauri::command]
async fn mine_blocks(
    chain: Chain,
    count: u32,
    node_manager: tauri::State<'_, NodeManager>,
    store: tauri::State<'_, Arc<Mutex<Store>>>,
    executor: tauri::State<'_, Executor>,
) -> Result<Vec<String>, TypedError> {
    if chain != Chain::Regtest {
        return Err(TypedError::from(
            "Mining blocks is only available on Regtest.".to_string(),
        ));
    }
    let ctx = wallet_context(chain, &node_manager, &store, &executor)?;
    let receive = nk_ord::wallet::wallet_receive(&executor, &ctx.target(), None)
        .await
        .map_err(TypedError::from)?;
    let address = receive
        .get("addresses")
        .and_then(|a| a.as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.as_str())
        .ok_or_else(|| TypedError::from("ord did not return a receive address".to_string()))?;
    ctx.rpc
        .generate_to_address(count, address)
        .await
        .map_err(|e| TypedError::from(e.to_string()))
}

/// "Reset Test Lab" (docs/SPEC.md item 11): stops Regtest's bitcoind/ord
/// gracefully if running, then deletes *only* Regtest's own data
/// directory -- never any other environment's. Regtest-only for the
/// same reason as `mine_blocks`: this exists to let the user blow away
/// throwaway test-coin state and start clean, which only makes sense
/// on the chain whose coins have no real value.
#[tauri::command]
async fn reset_test_lab(
    node_manager: tauri::State<'_, NodeManager>,
    store: tauri::State<'_, Arc<Mutex<Store>>>,
) -> Result<(), TypedError> {
    if node_manager.is_ord_running(Chain::Regtest) {
        node_manager
            .stop_ord(Chain::Regtest, std::time::Duration::from_secs(30))
            .await
            .map_err(TypedError::from)?;
    }
    if node_manager.is_running(Chain::Regtest) {
        node_manager
            .stop(Chain::Regtest, std::time::Duration::from_secs(120))
            .await
            .map_err(TypedError::from)?;
    }
    delete_regtest_data_only(&environment_data_root(&store))
        .map_err(|e| TypedError::from(e.to_string()))
}

/// The actual deletion, pulled out of `reset_test_lab` so it has real
/// test coverage of its own -- "Reset Test Lab deletes only regtest
/// data" is a docs/SPEC.md Phase 8 [CI] acceptance criterion, not just
/// an implied detail of a Tauri command handler this project's own
/// convention doesn't unit-test directly. Takes no `chain` parameter at
/// all (not even from the caller) -- `Chain::Regtest` is hardcoded, so
/// there's no way for this to ever resolve to a different environment's
/// directory, by construction rather than by a runtime check.
fn delete_regtest_data_only(environment_data_root: &std::path::Path) -> std::io::Result<()> {
    let environment = Environment::new_default(Chain::Regtest, environment_data_root);
    if environment.data_root.is_dir() {
        std::fs::remove_dir_all(&environment.data_root)?;
    }
    Ok(())
}

/// Stops every running environment's ord then bitcoind, gracefully, in
/// that order -- ord first so it's never left trying to talk to a
/// bitcoind that already vanished out from under it, matching
/// docs/SPEC.md item 12's "stop ALL running environments... (ord
/// first, then bitcoind)". Shared by the tray's "Quit" (best-effort:
/// the app is exiting regardless of whether a stop fails) and
/// `safe_eject` (docs/SPEC.md item 12's "Safely shut down and eject"
/// button, which needs to know whether every stop actually succeeded
/// before telling the user it's safe to unplug the drive -- silently
/// swallowing a failure there would be actively wrong, not just
/// unhelpful).
///
/// `environments` (one per chain) is where the pid files live: after the
/// stops, anything still alive is reported even though the manager no
/// longer tracks it, so a retry after a failure can't wrongly come back
/// clean. See `NodeManager::stop_everything`.
async fn stop_every_running_environment(
    node_manager: &NodeManager,
    environments: &[Environment],
) -> Result<(), TypedError> {
    stop_failures_to_result(node_manager.stop_everything(environments).await)
}

/// One default `Environment` per chain under the current data root --
/// what `stop_every_running_environment` needs to find each chain's pid
/// files. (Only the data root and chain matter for that; the per-chain
/// index options `list_default_environments` also fills in do not.)
fn all_environments(store: &tauri::State<'_, Arc<Mutex<Store>>>) -> Vec<Environment> {
    let root = environment_data_root(store);
    Chain::ALL
        .iter()
        .map(|&chain| Environment::new_default(chain, &root))
        .collect()
}

/// Turns `NodeManager::stop_everything`'s failure list into the single
/// error callers surface. Every failure is named in the message; the
/// structured error `code` is only kept when there is exactly one
/// failure (with several, no single code describes the situation).
fn stop_failures_to_result(failures: Vec<node_manager::StopFailure>) -> Result<(), TypedError> {
    match failures.as_slice() {
        [] => Ok(()),
        [only] => Err(TypedError {
            code: only.error.code(),
            message: only.to_string(),
        }),
        several => Err(TypedError {
            code: None,
            message: format!(
                "{} services did not stop: {}",
                several.len(),
                several
                    .iter()
                    .map(|f| f.to_string())
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        }),
    }
}

/// The tray menu's "Quit" action: stops every running environment
/// gracefully before actually exiting, rather than leaving them as
/// orphaned processes -- the window-hide-to-tray behavior above only
/// works because something keeps them running intentionally; an
/// explicit quit should still shut them down cleanly first. Best-
/// effort: a stop failing here shouldn't block the app from exiting
/// when the user explicitly asked it to.
async fn stop_everything_and_exit(app: tauri::AppHandle) {
    let node_manager = app.state::<NodeManager>();
    let environments = all_environments(&app.state::<Arc<Mutex<Store>>>());
    let _ = stop_every_running_environment(&node_manager, &environments).await;
    app.exit(0);
}

/// How long after a successful Safe Eject the app closes itself: long
/// enough to read "safe to unplug -- Nodekeeper is closing".
const SAFE_EJECT_CLOSE_DELAY: std::time::Duration = std::time::Duration::from_secs(4);

/// docs/SPEC.md item 12: "Safely shut down and eject." Unlike the tray
/// Quit path above, a failed stop here must be reported, not
/// swallowed -- telling the user it's safe to unplug the drive when
/// something didn't actually stop would risk real data corruption.
///
/// On success **Nodekeeper then closes itself** (after a few seconds, so
/// the message can be read). Stopping the services is not enough to make a
/// drive safe to unplug: the app is still running *from* that drive in
/// portable mode, with its settings database open and its single-instance
/// lock file (`config/.nodekeeper.lock`) on it -- which, left in place, also
/// makes the next computer that opens the drive refuse with "in use on
/// another computer". Quitting releases all of that (the lock on the exit
/// event). The close goes through `stop_everything_and_exit`, so anything
/// started again in the meantime is stopped first rather than orphaned.
#[tauri::command]
async fn safe_eject(
    app: tauri::AppHandle,
    node_manager: tauri::State<'_, NodeManager>,
    store: tauri::State<'_, Arc<Mutex<Store>>>,
) -> Result<(), TypedError> {
    let environments = all_environments(&store);
    stop_every_running_environment(&node_manager, &environments).await?;
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(SAFE_EJECT_CLOSE_DELAY).await;
        stop_everything_and_exit(app).await;
    });
    Ok(())
}

/// Shows and focuses the main window -- shared by the tray icon's left
/// click and its "Show Nodekeeper" menu item.
fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// docs/SPEC.md item 8: "Optional 'prevent sleep during sync' setting."
/// Holds at most one `keepawake::KeepAwake` RAII guard -- creating one
/// while `enabled(true)` inhibits system sleep (not display sleep;
/// this is a background sync, not video playback) until it's dropped,
/// which happens automatically the moment `set_prevent_sleep(false)`
/// replaces the slot with `None`. The frontend (`PreventSleepWatcher`)
/// owns deciding *when* to call this -- the setting being on isn't
/// enough by itself, only "on AND something is actually syncing right
/// now" should hold the guard.
struct PreventSleepGuard(Mutex<Option<keepawake::KeepAwake>>);

impl PreventSleepGuard {
    fn new() -> Self {
        Self(Mutex::new(None))
    }
}

#[tauri::command]
fn set_prevent_sleep(
    enabled: bool,
    guard: tauri::State<'_, PreventSleepGuard>,
) -> Result<(), TypedError> {
    let mut slot = guard.0.lock().expect("mutex should not be poisoned");
    if enabled {
        if slot.is_none() {
            let awake = keepawake::Builder::default()
                .sleep(true)
                .reason("Bitcoin Core is syncing")
                .app_name("Nodekeeper")
                .app_reverse_domain("com.nodekeeper.desktop")
                .create()
                .map_err(|e| TypedError::from(e.to_string()))?;
            *slot = Some(awake);
        }
    } else {
        *slot = None;
    }
    Ok(())
}

/// Opens the settings database and trims the command history to `keep`
/// rows per environment. docs/SPEC.md item 7 makes the history a *rolling*
/// window (last 5,000 per environment): new commands prune as they go
/// (`nk_store::persist_exec_events`), and this shrinks anything already
/// over the limit at launch -- a history that grew before pruning existed,
/// or while the app was closed mid-batch. A failed trim is not fatal.
fn open_store(db_path: &std::path::Path, keep: u32) -> Store {
    let store = Store::open(db_path).expect("failed to open the settings database");
    if let Err(e) = store.prune_all_command_history(keep) {
        eprintln!("could not prune the command history at startup: {e}");
    }
    // Erase private key material that an earlier version let into the
    // history (the console used to run `listdescriptors true` and `ord
    // wallet dump` and record what they printed). Idempotent.
    match store.scrub_private_keys_from_command_history(
        nk_core::console_safety::history_row_reveals_secrets,
    ) {
        Ok(report) if !report.is_empty() => eprintln!(
            "removed private key material from the command history: {} row(s) deleted, {} \
             row(s) scrubbed",
            report.rows_deleted, report.rows_scrubbed
        ),
        Ok(_) => {}
        Err(e) => {
            // Fail closed: a history that may still hold a secret is not
            // shown (and is deleted if that can be done); the next launch
            // tries the scrub again.
            eprintln!("could not scrub the command history at startup: {e}");
            store.quarantine_command_history();
        }
    }
    // Once, ever: bytes freed by an earlier build (which did not zero what it
    // deleted) stay in the file's free pages until the file is rewritten,
    // whether or not the scrub above found anything. Retried at the next
    // launch if it fails.
    if let Err(e) = store.vacuum_once(HISTORY_VACUUMED_MARKER) {
        eprintln!("could not compact the settings database at startup: {e}");
    }
    store
}

/// The settings key that records that the database file was rewritten once
/// after `secure_delete` was turned on -- see `open_store`.
const HISTORY_VACUUMED_MARKER: &str = "history_vacuumed_v1";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let data_dir = data_root();

    // docs/SPEC.md Foundation C: one copy of Nodekeeper per data folder
    // (including a portable drive opened from two computers). Taken
    // *before* the settings database is opened, so a second copy never
    // touches it; a refusal is shown in a message box and ends the
    // process. Released on `RunEvent::Exit` below. (Acquiring also creates
    // the folder, and a folder that cannot be created gets the same plain
    // message instead of a silent failure.)
    let instance_lock = match instance_lock::acquire_or_refuse(&data_dir) {
        Ok(lock) => instance_lock::InstanceLock::new(lock),
        Err(refusal) => instance_lock::show_refusal_and_exit(refusal),
    };

    let db_path = data_dir.join("nodekeeper.sqlite3");
    let store = open_store(&db_path, nk_store::COMMAND_HISTORY_KEEP_PER_ENVIRONMENT);
    let store = Arc::new(Mutex::new(store));

    let executor = Executor::new();
    let node_manager = NodeManager::new();
    let wallet_session = WalletSession::new();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .manage(store.clone())
        .manage(executor.clone())
        .manage(node_manager)
        .manage(wallet_session)
        .manage(PreventSleepGuard::new())
        .manage(instance_lock)
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

            // docs/SPEC.md item 8: "Tray: minimize to tray while
            // services run." The icon reuses the app's own configured
            // window icon (`tauri.conf.json`'s `bundle.icon`) rather
            // than shipping a second image just for this.
            let show_i = MenuItem::with_id(app, "show", "Show Nodekeeper", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit Nodekeeper", true, None::<&str>)?;
            let tray_menu = Menu::with_items(app, &[&show_i, &quit_i])?;
            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main_window(tray.app_handle());
                    }
                })
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => show_main_window(app),
                    "quit" => {
                        tauri::async_runtime::spawn(stop_everything_and_exit(app.clone()));
                    }
                    _ => {}
                })
                .build(app)?;

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let node_manager = window.state::<NodeManager>();
                if !node_manager.any_running() {
                    // Nothing running: a normal close (quit) is what
                    // the user expects, same in both modes.
                    return;
                }
                if is_portable_install() {
                    // docs/SPEC.md item 12: unlike installed mode,
                    // portable mode must not silently hide to the tray
                    // and keep bitcoind/ord holding files open on a
                    // drive that could be unplugged at any moment --
                    // warn and offer to stop everything first instead.
                    api.prevent_close();
                    let app = window.app_handle().clone();
                    window
                        .dialog()
                        .message(
                            "Bitcoin Core or ord is still running. Closing now will stop \
                             them so it's safe to unplug this drive. Continue?",
                        )
                        .title("Services are still running")
                        .kind(tauri_plugin_dialog::MessageDialogKind::Warning)
                        .buttons(tauri_plugin_dialog::MessageDialogButtons::YesNo)
                        .show(move |confirmed| {
                            if confirmed {
                                tauri::async_runtime::spawn(stop_everything_and_exit(app));
                            }
                        });
                } else {
                    // docs/SPEC.md item 8: "minimize to tray while
                    // services run" -- installed mode keeps everything
                    // running in the background instead.
                    let _ = window.hide();
                    api.prevent_close();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            system_check,
            list_default_environments,
            get_environment_data_root,
            set_environment_data_root,
            download_and_verify_bitcoin_core,
            download_and_verify_ord,
            set_index_options,
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
            wallet_inscriptions,
            wallet_transaction_history,
            wallet_send_dry_run,
            wallet_send,
            console_classify,
            console_run,
            list_scripts,
            list_available_interpreters,
            run_script,
            mine_blocks,
            reset_test_lab,
            wallet_fee_estimate,
            wallet_inscribe_dry_run,
            wallet_inscribe,
            wallet_inscribe_batch_dry_run,
            wallet_inscribe_batch,
            inscribe_file_preview,
            inscription_detail,
            sat_inscriptions,
            tail_debug_log,
            page_debug_log_before,
            search_debug_log,
            list_command_history,
            set_prevent_sleep,
            is_portable_mode,
            safe_eject,
            had_unclean_shutdown,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // Tauri ends the process with `std::process::exit`, which
            // skips destructors -- so the lock is released here, not by
            // `Drop`. (A crash still leaves it behind; the next launch
            // detects that as stale.)
            if let tauri::RunEvent::Exit = event {
                app.state::<instance_lock::InstanceLock>().release();
            }
        });
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
        WalletInscriptionEntry::export_all(&config).unwrap();
        WalletTransactionEntry::export_all(&config).unwrap();
        WalletSendResult::export_all(&config).unwrap();
        InscribeResult::export_all(&config).unwrap();
        FilePreview::export_all(&config).unwrap();
        InscriptionDetail::export_all(&config).unwrap();
        nk_core::log_tail::LogWindow::export_all(&config).unwrap();
        nk_store::CommandHistoryEntry::export_all(&config).unwrap();
        nk_exec::ExecEvent::export_all(&config).unwrap();
        DownloadProgress::export_all(&config).unwrap();
        ConsoleCommandPreview::export_all(&config).unwrap();
        ScriptInfo::export_all(&config).unwrap();
        InterpreterAvailability::export_all(&config).unwrap();
    }

    fn stop_failure(
        chain: Chain,
        service: node_manager::Service,
        error: NodeManagerError,
    ) -> node_manager::StopFailure {
        node_manager::StopFailure {
            chain,
            service,
            error,
        }
    }

    /// The startup half of the rolling command history (docs/SPEC.md item
    /// 7): opening the settings database trims each environment to the
    /// limit, on a real file database reopened the way a launch does, and
    /// leaves an environment that is already within it alone.
    #[test]
    fn opening_the_store_trims_an_over_long_command_history() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nodekeeper.sqlite3");
        {
            let store = Store::open(&path).unwrap();
            for (environment, rows) in [("regtest", 9), ("mainnet", 3)] {
                for i in 0..rows {
                    store
                        .record_command_started(
                            &format!("{environment}-{i}"),
                            environment,
                            "ordcli",
                            "t",
                            "cmd",
                            i,
                            false,
                        )
                        .unwrap();
                }
            }
        }

        let store = open_store(&path, 5);
        let count = |environment: &str| {
            store
                .list_command_history(Some(environment), 100)
                .unwrap()
                .len()
        };
        assert_eq!(count("regtest"), 5);
        assert_eq!(count("mainnet"), 3);
    }

    /// The private-key half of opening the store: history recorded by an
    /// older version (which ran `listdescriptors true`) is cleaned on
    /// launch, on a real file database reopened the way a launch does.
    #[test]
    fn opening_the_store_erases_private_key_material_from_old_history() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nodekeeper.sqlite3");
        let key = format!("tprv{}", "A".repeat(107));
        {
            let store = Store::open(&path).unwrap();
            for (id, display, output) in [
                (
                    "dump",
                    "bitcoin-cli listdescriptors true",
                    format!("desc {key}"),
                ),
                (
                    "ord-dump",
                    "ord.exe --regtest wallet --name ord dump",
                    format!("desc {key}"),
                ),
                (
                    "stray",
                    "bitcoin-cli getdescriptorinfo",
                    format!("saw {key}"),
                ),
                ("clean", "bitcoin-cli getblockcount", "7".to_string()),
                // A passphrase recorded in the clear by an older version.
                (
                    "createwallet-clear",
                    "bitcoin-cli createwallet w false false hunter2",
                    "{}".to_string(),
                ),
                (
                    "createwallet-hidden",
                    "bitcoin-cli createwallet w false false [redacted]",
                    "{}".to_string(),
                ),
                // A recovery phrase in the output of a command that is not
                // itself on any list.
                (
                    "mnemonic",
                    "ord.exe --regtest wallet --name ord something",
                    "{\"mnemonic\": \"abandon ability able\"}".to_string(),
                ),
                // The hole the leading-option fix closed: it printed a recovery
                // phrase into the history.
                (
                    "ord-create",
                    "ord.exe --regtest wallet --name ord --no-sync create",
                    "{\"mnemonic\": \"abandon ability able\"}".to_string(),
                ),
                // Passphrases recorded in the clear in the shapes a review
                // found: a numeric one, one with spaces, a decorated line.
                (
                    "unlock-numeric",
                    "bitcoin-cli -regtest walletpassphrase 48213907 60",
                    "null".to_string(),
                ),
                (
                    "unlock-words",
                    "bitcoin-cli -regtest walletpassphrase [redacted] horse battery staple 60",
                    "null".to_string(),
                ),
                (
                    "decorated",
                    "bitcoin-cli -regtest bitcoin-cli.exe walletpassphrase hunter2 60",
                    "null".to_string(),
                ),
                // The rows the app itself writes must survive: the Wallet
                // screen's create (placeholder output), the unlock call, an
                // offer.
                (
                    "wallet-screen-create",
                    "ord.exe --regtest wallet --name ord create",
                    "[sensitive output hidden]".to_string(),
                ),
                (
                    "unlock",
                    "bitcoin-cli -regtest walletpassphrase [redacted] 60",
                    "null".to_string(),
                ),
                (
                    "offer",
                    "ord.exe --regtest wallet --name ord offer create --inscription abc",
                    "{}".to_string(),
                ),
            ] {
                store
                    .record_command_started(id, "regtest", "bitcoincli", "t", display, 0, false)
                    .unwrap();
                store.append_command_output(id, &output).unwrap();
            }
        }

        let store = open_store(&path, 5_000);
        let rows = store.list_command_history(None, 100).unwrap();
        let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
        let mut kept = ids.clone();
        kept.sort_unstable();
        assert_eq!(
            kept,
            [
                "clean",
                "createwallet-hidden",
                "offer",
                "stray",
                "unlock",
                "wallet-screen-create"
            ],
            "every row that revealed a secret is gone, and the app's own rows are kept"
        );
        assert!(rows.iter().all(|r| !r.output.contains("prv")));
    }

    // ---- the unlock glue, against a real Bitcoin Core ---------------------------

    const GLUE_PASSPHRASE: &str = "glue-test-passphrase-5150";
    const GLUE_NEW_PASSPHRASE: &str = "glue-test-passphrase-8842";

    fn glue_client(
        fixture: &nk_testkit::RegtestFixture,
        executor: &Executor,
        wallet_path: &str,
        chain: Chain,
    ) -> nk_rpc::RpcClient {
        nk_rpc::RpcClient::from_cookie_file(
            format!(
                "http://127.0.0.1:{}{wallet_path}",
                fixture.environment.rpc_port
            ),
            &fixture.environment.bitcoin_cookie_path(),
            executor.clone(),
            "regtest".to_string(),
            chain,
        )
        .unwrap()
    }

    /// `unlocked_until` of the wallet, through a client of its own (independent
    /// of the code under test).
    async fn glue_unlocked_until(
        fixture: &nk_testkit::RegtestFixture,
        executor: &Executor,
    ) -> Option<i64> {
        let client = glue_client(
            fixture,
            executor,
            &format!("/wallet/{DEFAULT_WALLET_NAME}"),
            Chain::Regtest,
        );
        client
            .call("getwalletinfo", vec![], "test", vec![], true)
            .await
            .expect("getwalletinfo")
            .get("unlocked_until")
            .and_then(|v| v.as_i64())
    }

    /// `with_wallet_unlocked` and the console's mainnet gate against a real
    /// node -- the parts the live tests of the RPC primitives do not reach: that
    /// the action runs only when the wallet is ready, that it is re-locked after
    /// it whatever it returns, that a passphrase is remembered only when it really
    /// unlocked something, and that a stale remembered passphrase is dropped.
    #[tokio::test]
    #[serial_test::serial(real_bitcoind)]
    async fn the_unlock_glue_runs_the_action_only_when_ready_and_relocks_after_it() {
        use nk_core::console_safety::OrdCommandClass;
        use std::sync::atomic::{AtomicBool, Ordering};

        let Some(bitcoind_path) = nk_core::live_tests::live_binary("NK_TEST_BITCOIND") else {
            eprintln!("skipping: NK_TEST_BITCOIND not set");
            return;
        };
        let fixture = nk_testkit::RegtestFixture::start(&std::path::PathBuf::from(bitcoind_path))
            .await
            .expect("bitcoind should start");
        let executor = Executor::new();
        let regtest = glue_client(&fixture, &executor, "", Chain::Regtest);
        let mainnet_label = glue_client(&fixture, &executor, "", Chain::Mainnet);
        let session = WalletSession::new();
        let ran = AtomicBool::new(false);
        let typed = |args: &[&str]| -> Vec<String> { args.iter().map(|a| a.to_string()).collect() };

        regtest
            .call(
                "createwallet",
                vec![serde_json::json!(DEFAULT_WALLET_NAME)],
                "test setup",
                vec![],
                true,
            )
            .await
            .expect("createwallet");

        // --- unencrypted wallet ---------------------------------------------
        // A test chain: the action runs with nothing unlocked, and a passphrase
        // offered "to remember" is NOT remembered (nothing was unlocked with it).
        let out = with_wallet_unlocked(
            Chain::Regtest,
            &regtest,
            &session,
            Some("offered anyway".to_string()),
            true,
            || async {
                ran.store(true, Ordering::SeqCst);
                Ok::<_, TypedError>(7)
            },
        )
        .await
        .expect("an unencrypted test-chain wallet is ready with no unlock");
        assert_eq!(out, 7);
        assert!(ran.swap(false, Ordering::SeqCst));
        assert_eq!(
            session.get(Chain::Regtest),
            None,
            "nothing was unlocked, so nothing is remembered"
        );

        // Mainnet: refused, and the action never runs -- through the guided path
        // and through the console's gate alike.
        let refused = with_wallet_unlocked(
            Chain::Mainnet,
            &mainnet_label,
            &session,
            Some("anything".to_string()),
            false,
            || async {
                ran.store(true, Ordering::SeqCst);
                Ok::<_, TypedError>(())
            },
        )
        .await
        .expect_err("an unencrypted mainnet wallet is refused");
        assert_eq!(refused.code, Some(AppErrorCode::WalletNotEncrypted));
        assert!(!ran.load(Ordering::SeqCst), "the action did not run");
        let send = typed(&["send", "addr", "1btc"]);
        let gated = require_encrypted_wallet_for_console_signing(
            Chain::Mainnet,
            &mainnet_label,
            &send,
            OrdCommandClass::StateChangingWithDryRun,
            false,
        )
        .await
        .expect_err("the console's ord send is refused on an unencrypted mainnet wallet");
        assert_eq!(gated.code, Some(AppErrorCode::WalletNotEncrypted));

        // Encrypt it.
        regtest
            .encrypt_wallet(DEFAULT_WALLET_NAME, GLUE_PASSPHRASE)
            .await
            .expect("encryptwallet");

        // --- encrypted wallet -----------------------------------------------
        // No passphrase anywhere: WALLET_LOCKED, action not run.
        let locked =
            with_wallet_unlocked(Chain::Regtest, &regtest, &session, None, false, || async {
                ran.store(true, Ordering::SeqCst);
                Ok::<_, TypedError>(())
            })
            .await
            .expect_err("an encrypted wallet needs its passphrase");
        assert_eq!(locked.code, Some(AppErrorCode::WalletLocked));
        assert!(!ran.load(Ordering::SeqCst));

        // The right passphrase (remember = true): the action *sees* the wallet
        // unlocked, it is locked again after, and only now is it remembered.
        let seen = with_wallet_unlocked(
            Chain::Regtest,
            &regtest,
            &session,
            Some(GLUE_PASSPHRASE.to_string()),
            true,
            || async { Ok::<_, TypedError>(glue_unlocked_until(&fixture, &executor).await) },
        )
        .await
        .expect("unlock, run, re-lock");
        assert!(
            seen.unwrap_or(0) > 0,
            "the action ran with the wallet unlocked"
        );
        assert_eq!(
            glue_unlocked_until(&fixture, &executor).await,
            Some(0),
            "re-locked afterwards"
        );
        assert_eq!(
            session.get(Chain::Regtest).as_deref().map(String::as_str),
            Some(GLUE_PASSPHRASE)
        );

        // No passphrase this time: the remembered one is used.
        with_wallet_unlocked(Chain::Regtest, &regtest, &session, None, false, || async {
            Ok::<_, TypedError>(())
        })
        .await
        .expect("the remembered passphrase unlocks the wallet");
        assert_eq!(glue_unlocked_until(&fixture, &executor).await, Some(0));

        // An action that FAILS: its error comes back, and the wallet is locked.
        let failed = with_wallet_unlocked(
            Chain::Regtest,
            &regtest,
            &session,
            Some(GLUE_PASSPHRASE.to_string()),
            false,
            || async { Err::<(), _>(TypedError::from("the send failed".to_string())) },
        )
        .await
        .expect_err("the action's error is returned");
        assert_eq!(failed.message, "the send failed");
        assert_eq!(
            glue_unlocked_until(&fixture, &executor).await,
            Some(0),
            "re-locked even though the action failed"
        );

        // The console's gate on an encrypted wallet: signing commands pass.
        require_encrypted_wallet_for_console_signing(
            Chain::Mainnet,
            &mainnet_label,
            &send,
            OrdCommandClass::StateChangingWithDryRun,
            false,
        )
        .await
        .expect("an encrypted mainnet wallet may sign through the console");
        // ... but not under another wallet's name, which the check never saw.
        let renamed = require_encrypted_wallet_for_console_signing(
            Chain::Mainnet,
            &mainnet_label,
            &typed(&["--name", "other", "send", "addr", "1btc"]),
            OrdCommandClass::StateChangingWithDryRun,
            false,
        )
        .await
        .expect_err("--name is refused on a mainnet signing command");
        assert!(renamed.message.contains("--name"), "{}", renamed.message);

        // A stale remembered passphrase: the wallet's passphrase is changed, the
        // session still holds the old one. It is dropped and the user is asked.
        regtest
            .call(
                "walletpassphrasechange",
                vec![
                    serde_json::json!(GLUE_PASSPHRASE),
                    serde_json::json!(GLUE_NEW_PASSPHRASE),
                ],
                "test",
                vec![GLUE_PASSPHRASE.to_string(), GLUE_NEW_PASSPHRASE.to_string()],
                false,
            )
            .await
            .expect("walletpassphrasechange");
        assert!(session.get(Chain::Regtest).is_some());
        let stale =
            with_wallet_unlocked(Chain::Regtest, &regtest, &session, None, false, || async {
                ran.store(true, Ordering::SeqCst);
                Ok::<_, TypedError>(())
            })
            .await
            .expect_err("the old remembered passphrase no longer works");
        assert_eq!(stale.code, Some(AppErrorCode::WalletLocked));
        assert!(!ran.load(Ordering::SeqCst));
        assert_eq!(
            session.get(Chain::Regtest),
            None,
            "the stale one was forgotten"
        );
        with_wallet_unlocked(
            Chain::Regtest,
            &regtest,
            &session,
            Some(GLUE_NEW_PASSPHRASE.to_string()),
            false,
            || async { Ok::<_, TypedError>(()) },
        )
        .await
        .expect("the new passphrase works");

        // --- the console's gate needs no node where it does not apply ----------
        // A client aimed at a closed port: any RPC would fail, so success proves
        // that none was made.
        let nowhere = nk_rpc::RpcClient::new(
            "http://127.0.0.1:1".to_string(),
            "u".to_string(),
            "p".to_string(),
            executor.clone(),
            "mainnet".to_string(),
            Chain::Mainnet,
        );
        for (chain, args, class, dry_run) in [
            (
                Chain::Mainnet,
                typed(&["balance"]),
                OrdCommandClass::ReadOnly,
                false,
            ),
            (
                Chain::Mainnet,
                send.clone(),
                OrdCommandClass::StateChangingWithDryRun,
                true,
            ),
            (
                Chain::Regtest,
                send.clone(),
                OrdCommandClass::StateChangingWithDryRun,
                false,
            ),
            (
                Chain::Signet,
                typed(&["mint"]),
                OrdCommandClass::StateChangingNoDryRun,
                false,
            ),
        ] {
            require_encrypted_wallet_for_console_signing(chain, &nowhere, &args, class, dry_run)
                .await
                .unwrap_or_else(|e| {
                    panic!("{chain:?} {args:?} should not be gated: {}", e.message)
                });
        }
        // Where it does apply, an unreadable answer is a refusal, not a pass.
        let unreadable = require_encrypted_wallet_for_console_signing(
            Chain::Mainnet,
            &nowhere,
            &send,
            OrdCommandClass::StateChangingWithDryRun,
            false,
        )
        .await;
        assert!(
            unreadable.is_err(),
            "fails closed when Core cannot be asked"
        );

        fixture.stop().await.expect("bitcoind should stop cleanly");
    }

    /// How a refused unlock reaches the frontend: only "no passphrase yet" carries
    /// the code the frontend prompts on; an unencrypted mainnet wallet is a plain
    /// error (no prompt could fix it), and it says what to do.
    #[test]
    fn unlock_refusals_reach_the_frontend_with_the_right_code() {
        let locked = unlock_error_to_typed(nk_rpc::UnlockError::PassphraseRequired);
        assert_eq!(locked.code, Some(AppErrorCode::WalletLocked));
        assert!(locked.message.contains("passphrase"), "{}", locked.message);

        let not_encrypted = unlock_error_to_typed(nk_rpc::UnlockError::MainnetWalletNotEncrypted);
        assert_eq!(
            not_encrypted.code,
            Some(AppErrorCode::WalletNotEncrypted),
            "its own code, so the frontend can say what to do (not a passphrase prompt)"
        );
        assert!(
            not_encrypted.message.contains("not encrypted")
                && not_encrypted.message.contains("mainnet"),
            "{}",
            not_encrypted.message
        );

        let rpc = unlock_error_to_typed(nk_rpc::UnlockError::Rpc(nk_rpc::RpcError::Rpc {
            code: -14,
            message: "The wallet passphrase entered was incorrect.".to_string(),
        }));
        assert_eq!(rpc.code, None);
        assert!(rpc.message.contains("incorrect"), "{}", rpc.message);
    }

    fn args(line: &str) -> Vec<String> {
        nk_core::console_parse::tokenize(line).unwrap()
    }

    /// What the console shows and records for a bitcoin-cli call: a secret
    /// argument is hidden by its **position**, whatever the typed text
    /// looks like -- each case is one where hiding by matching the text
    /// used to show the secret.
    #[test]
    fn the_console_hides_secret_arguments_by_position() {
        let display = |line: &str| {
            let tokens = args(line);
            let (method, rest) = tokens.split_first().unwrap();
            console_secrets(method, rest).display
        };
        // From the secret to the END: the timeout is part of the hidden run
        // (what follows a passphrase may be the rest of it), and the run is
        // ONE marker -- how many words it has is not shown either.
        assert_eq!(
            display("walletpassphrase hunter2 60"),
            "walletpassphrase [redacted]"
        );
        assert_eq!(
            display("walletpassphrase correct horse battery staple 60"),
            "walletpassphrase [redacted]"
        );
        assert_eq!(
            display("walletpassphrase 'correct horse battery staple' 60"),
            "walletpassphrase [redacted]"
        );
        // The method name in another case still hides it (the node rejects
        // the call, but it was already recorded).
        assert_eq!(
            display("WalletPassphrase hunter2 60"),
            "WalletPassphrase [redacted]"
        );
        // createwallet's passphrase is its 4th argument; what is typed after
        // it (more words of it) is hidden with it.
        assert_eq!(
            display("createwallet w false false hunter2"),
            "createwallet w false false [redacted]"
        );
        assert_eq!(
            display("createwallet w false false correct horse battery"),
            "createwallet w false false [redacted]"
        );
        // A JSON blob that the tokenizer splits over several tokens (a
        // space, then a quote): every piece of it is hidden.
        assert_eq!(
            display(r#"importdescriptors [{"desc":"wpkh(SECRET)", "timestamp":"now"}]"#),
            "importdescriptors [redacted]"
        );
        // Named arguments: the passphrase is not where it usually is.
        assert_eq!(
            display("createwallet wallet_name=w passphrase=hunter2"),
            "createwallet [redacted]"
        );
        // A method Bitcoin Core does not have: nothing about its arguments
        // is known, so none is shown -- a typo, a legacy key import, a word
        // that only looks like a command.
        for line in [
            "walletpasspharse hunter2 60",
            "importprivkey KwDiBf89QgGbjEhKnhXJuH7LrciVrZi3qYjgd9M7rFU73sVHnoWn",
            "sudo bitcoin-cli walletpassphrase hunter2 60",
        ] {
            let shown = display(line);
            assert_eq!(shown.split(' ').count(), 2, "{shown}");
            assert!(shown.ends_with(" [redacted]"), "{shown}");
        }
        // An ordinary command is shown as typed.
        assert_eq!(display("getblockhash 100"), "getblockhash 100");
        // A private key in a descriptor argument (a position no method is
        // known to treat as secret) is scrubbed from the display as well.
        let key = format!("tprv{}", "A".repeat(107));
        let shown = display(&format!("getdescriptorinfo wpkh({key}/0/*)"));
        assert!(!shown.contains("tprv"), "{shown}");
        assert!(
            shown.contains(nk_exec::redact::PRIVATE_KEY_PLACEHOLDER),
            "{shown}"
        );
    }

    /// A pasted `bitcoin-cli ...` line is refused before anything is shown
    /// or run, and the refusal does not repeat what was pasted.
    #[test]
    fn a_pasted_bitcoin_cli_line_is_refused_without_echoing_it() {
        for line in [
            "bitcoin-cli walletpassphrase hunter2 60",
            "BITCOIN-CLI -regtest walletpassphrase hunter2 60",
            "-regtest walletpassphrase hunter2 60",
            "bitcoin-cli.exe walletpassphrase hunter2 60",
            "./bitcoin-cli walletpassphrase hunter2 60",
            "C:\\tools\\bitcoin-cli.exe walletpassphrase hunter2 60",
            "\u{feff}bitcoin-cli walletpassphrase hunter2 60",
        ] {
            let error = console_classify(line.to_string()).unwrap_err();
            assert!(!error.message.contains("hunter2"), "{}", error.message);
            assert!(error.message.contains("without"), "{}", error.message);
        }
        assert!(console_classify("getblockchaininfo".to_string()).is_ok());
    }

    /// Bitcoin Core answers a descriptor with a bad key by quoting it back
    /// (`key '<key>' is not valid`); the console's error text and result are
    /// scrubbed like what the executor records.
    #[test]
    fn what_the_console_hands_back_is_scrubbed_of_keys() {
        let wif = format!("c{}", "B".repeat(51));
        let error = scrub_error_for_screen(TypedError::from(format!(
            "rpc error -5: key '{wif}' is not valid"
        )));
        assert!(!error.message.contains("BBBB"), "{}", error.message);
        assert!(error.message.contains("is not valid"));

        let value =
            scrub_value_for_screen(serde_json::json!({"note": format!("saw {wif}"), "n": 1}));
        assert!(!value.to_string().contains("BBBB"), "{value}");
        assert_eq!(value["n"], 1, "the rest of the value is intact");
        let untouched = serde_json::json!({"blocks": 812345});
        assert_eq!(scrub_value_for_screen(untouched.clone()), untouched);
    }

    /// A refused ord line is shown without its arguments (`--passphrase`, a
    /// recovery phrase after `restore`).
    #[test]
    fn a_refused_ord_line_is_shown_without_its_arguments() {
        let preview = console_classify("ord create --passphrase hunter2".to_string()).unwrap();
        assert_eq!(preview.display, "ord wallet create [redacted]");
        assert!(preview.blocked_reason.is_some());
        let preview =
            console_classify("ord restore --from mnemonic abandon ability able".to_string())
                .unwrap();
        assert!(!preview.display.contains("abandon"), "{}", preview.display);
        // An ordinary ord command is shown as typed.
        let preview = console_classify("ord balance".to_string()).unwrap();
        assert_eq!(preview.display, "ord wallet balance");
    }

    #[test]
    fn classifying_a_walletpassphrase_never_shows_the_passphrase() {
        let preview = console_classify("walletpassphrase hunter2 60".to_string()).unwrap();
        assert_eq!(preview.display, "walletpassphrase [redacted]");
        assert!(!preview.display.contains("hunter2"));
    }

    #[test]
    fn no_stop_failures_is_success() {
        assert!(stop_failures_to_result(vec![]).is_ok());
    }

    #[test]
    fn a_single_stop_failure_keeps_its_structured_error_code() {
        let error = stop_failures_to_result(vec![stop_failure(
            Chain::Regtest,
            node_manager::Service::Ord,
            NodeManagerError::OrdProcess(nk_proc::OrdProcessError::PortInUse { port: 1 }),
        )])
        .unwrap_err();
        assert_eq!(error.code, Some(AppErrorCode::PortInUse));
        assert!(error.message.contains("ord"), "{}", error.message);
        assert!(error.message.contains("Regtest"), "{}", error.message);
    }

    #[test]
    fn several_stop_failures_are_all_named_and_carry_no_single_code() {
        let error = stop_failures_to_result(vec![
            stop_failure(
                Chain::Mainnet,
                node_manager::Service::Ord,
                NodeManagerError::OrdProcess(nk_proc::OrdProcessError::StopTimeout),
            ),
            stop_failure(
                Chain::Regtest,
                node_manager::Service::Bitcoind,
                NodeManagerError::BitcoindNotRunning {
                    chain: Chain::Regtest,
                },
            ),
        ])
        .unwrap_err();
        assert_eq!(error.code, None);
        assert!(error.message.starts_with("2 services did not stop"));
        assert!(error.message.contains("ord (Mainnet)"), "{}", error.message);
        assert!(
            error.message.contains("bitcoind (Regtest)"),
            "{}",
            error.message
        );
    }

    /// The literal docs/SPEC.md Phase 8 [CI] acceptance criterion:
    /// "Reset Test Lab deletes only regtest data." Real filesystem
    /// operations against a real tempdir, not mocked -- creates both a
    /// regtest environment's directory and a mainnet environment's
    /// directory (with a file inside each, so an empty-directory
    /// special case can't accidentally pass), runs the actual deletion
    /// function `reset_test_lab` calls, and asserts regtest's directory
    /// is gone while mainnet's directory and its file survive untouched.
    #[test]
    fn reset_test_lab_deletes_only_regtest_data() {
        let root = tempfile::tempdir().unwrap();

        let regtest_env = Environment::new_default(Chain::Regtest, root.path());
        std::fs::create_dir_all(&regtest_env.data_root).unwrap();
        std::fs::write(regtest_env.data_root.join("regtest.dat"), b"regtest").unwrap();

        let mainnet_env = Environment::new_default(Chain::Mainnet, root.path());
        std::fs::create_dir_all(&mainnet_env.data_root).unwrap();
        std::fs::write(mainnet_env.data_root.join("mainnet.dat"), b"mainnet").unwrap();

        delete_regtest_data_only(root.path()).unwrap();

        assert!(
            !regtest_env.data_root.exists(),
            "regtest's data directory should have been deleted"
        );
        assert!(
            mainnet_env.data_root.join("mainnet.dat").is_file(),
            "mainnet's data must survive a regtest reset untouched"
        );
    }

    /// A reset before the environment was ever used (no directory to
    /// delete yet) must succeed, not error -- it's a no-op, not a
    /// missing-file failure.
    #[test]
    fn reset_test_lab_is_a_no_op_when_regtest_has_no_data_yet() {
        let root = tempfile::tempdir().unwrap();
        delete_regtest_data_only(root.path()).unwrap();
    }

    /// docs/SPEC.md item 12: portable mode is signaled by a `config`
    /// directory next to the executable -- a fresh installed-mode
    /// launch (no such directory) must not be mistaken for portable.
    #[test]
    fn a_directory_with_no_config_subfolder_is_not_portable() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!is_portable_layout(dir.path()));
    }

    /// The exact signal the "prepare a new portable drive" wizard is
    /// responsible for creating once, up front.
    #[test]
    fn a_directory_with_a_config_subfolder_is_portable() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("config")).unwrap();
        assert!(is_portable_layout(dir.path()));
    }

    /// A same-named `config` *file* (not a directory) must not be
    /// mistaken for the real marker -- guards against a stray file
    /// (e.g. a leftover `config.ini` typo) accidentally flipping a
    /// normal installed launch into portable mode.
    #[test]
    fn a_config_file_instead_of_a_directory_is_not_portable() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("config"), b"not a directory").unwrap();
        assert!(!is_portable_layout(dir.path()));
    }
}
