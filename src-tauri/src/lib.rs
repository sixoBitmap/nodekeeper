use nk_core::system_check::{run_system_check, SystemCheck};
use nk_core::{Chain, Environment};
use nk_store::Store;
use std::sync::Mutex;

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
    let data_root = std::path::Path::new("data");
    Chain::ALL
        .iter()
        .map(|&chain| Environment::new_default(chain, data_root))
        .collect()
}

#[tauri::command]
fn get_setting(store: tauri::State<Mutex<Store>>, key: String) -> Result<Option<String>, String> {
    store
        .lock()
        .unwrap()
        .get_setting(&key)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn set_setting(
    store: tauri::State<Mutex<Store>>,
    key: String,
    value: String,
) -> Result<(), String> {
    store
        .lock()
        .unwrap()
        .set_setting(&key, &value)
        .map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Placeholder path (see `list_default_environments`'s doc comment for
    // the same caveat): proper OS-specific app-data-dir resolution is a
    // later-phase concern (tied to the setup wizard's installed vs.
    // portable mode handling). `data/` matches the same relative-path
    // convention used everywhere else in Foundation A.
    let db_path = std::path::Path::new("data").join("nodekeeper.sqlite3");
    std::fs::create_dir_all(db_path.parent().unwrap()).expect("failed to create data directory");
    let store = Store::open(&db_path).expect("failed to open the settings database");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(Mutex::new(store))
        .invoke_handler(tauri::generate_handler![
            system_check,
            list_default_environments,
            get_setting,
            set_setting
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
    }
}
