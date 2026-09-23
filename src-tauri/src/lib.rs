mod node_manager;

use nk_core::system_check::{run_system_check, SystemCheck};
use nk_core::{AppErrorCode, Chain, Environment};
use nk_exec::Executor;
use nk_store::Store;
use node_manager::{NodeManager, NodeManagerError};
use serde::Serialize;
use std::sync::{Arc, Mutex};
use tauri::Emitter;
use ts_rs::TS;

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

impl From<String> for TypedError {
    fn from(message: String) -> Self {
        TypedError {
            code: None,
            message,
        }
    }
}

/// The settings key an (eventual) setup wizard writes once it downloads
/// and verifies a Bitcoin Core binary into a real install location —
/// see `node_manager`'s doc comment for why start/stop reads this
/// instead of locating a binary itself.
const BITCOIND_PATH_SETTING: &str = "bitcoind_path";

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

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(store.clone())
        .manage(executor.clone())
        .manage(node_manager)
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
    }
}
