use nk_core::system_check::{run_system_check, SystemCheck};

/// Runs the setup-wizard system check (OS/CPU/RAM/disk) for the given data
/// directory. Doubles as the Phase 1 typed-IPC scaffold: a real command
/// whose return type is generated into `ui/src/bindings/` by ts-rs (see
/// the `export_bindings` test below), proving the Rust-type -> TS-type ->
/// UI-call pipeline end to end.
#[tauri::command]
fn system_check(data_dir: String) -> SystemCheck {
    run_system_check(std::path::Path::new(&data_dir))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![system_check])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use ts_rs::{Config, TS};

    /// Regenerates `ui/src/bindings/SystemCheck.ts` from the Rust type.
    /// Run as part of `just check` (`cargo test`); commit the generated
    /// file so the frontend never hand-writes a duplicate type.
    #[test]
    fn export_bindings() {
        let config = Config::new().with_out_dir(PathBuf::from("../ui/src/bindings/"));
        SystemCheck::export_all(&config).unwrap();
    }
}
