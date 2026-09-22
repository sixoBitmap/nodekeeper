//! Setup-wizard system check: OS, CPU, RAM, and disk space (docs/SPEC.md
//! item 1, "Check OS, CPU, RAM, and disk space; warn that a mainnet node
//! plus ord index needs 1 TB+ on an SSD").

use serde::Serialize;
use sysinfo::Disks;
use ts_rs::TS;

// u64 fields are annotated `#[ts(type = "number")]`: ts-rs maps `u64` to TS
// `bigint` by default (technically lossless), but Tauri's IPC serializes
// values as plain JSON via serde_json, which decodes as a JS `number` on
// the frontend, not `bigint` — so the default mapping would be a type that
// doesn't match what actually arrives at runtime. `number` is safe here
// because byte counts (RAM/disk) never approach 2^53. Apply the same
// annotation to any other u64/i64 IPC field for the same reason (e.g.
// future satoshi amounts, block heights).
#[derive(Debug, Clone, Serialize, TS)]
pub struct SystemCheck {
    pub os: String,
    pub arch: String,
    pub cpu_cores: usize,
    #[ts(type = "number")]
    pub total_memory_bytes: u64,
    #[ts(type = "number")]
    pub available_memory_bytes: u64,
    /// Free space on the disk that contains `data_dir`, or `None` if no
    /// mounted disk could be matched to that path.
    #[ts(type = "number | null")]
    pub disk_free_bytes: Option<u64>,
}

/// Runs the system check. `data_dir` is the path the user picked (or the
/// default) for Nodekeeper's data folder — disk space is reported for
/// whichever disk actually contains it, not just the OS drive.
pub fn run_system_check(data_dir: &std::path::Path) -> SystemCheck {
    let mut sys = sysinfo::System::new_all();
    sys.refresh_all();

    let disks = Disks::new_with_refreshed_list();
    let disk_free_bytes = disk_free_space_for_path(&disks, data_dir);

    SystemCheck {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        cpu_cores: sys.cpus().len(),
        total_memory_bytes: sys.total_memory(),
        available_memory_bytes: sys.available_memory(),
        disk_free_bytes,
    }
}

/// Finds the disk with the longest mount-point prefix match for `path`
/// (the standard way to resolve "which filesystem is this path on" from a
/// flat disk list), and returns its free space.
fn disk_free_space_for_path(disks: &Disks, path: &std::path::Path) -> Option<u64> {
    // The path itself may not exist yet (e.g. a not-yet-created data
    // dir); walk up to the nearest existing ancestor so canonicalize()
    // succeeds. Use `dunce::canonicalize` rather than
    // `std::fs::canonicalize`: on Windows the latter returns
    // `\\?\`-prefixed extended-length paths, which don't prefix-match
    // sysinfo's plain (non-verbatim) disk mount points.
    let existing = path
        .ancestors()
        .find(|p| p.exists())
        .unwrap_or(std::path::Path::new("."));
    let canonical = dunce::canonicalize(existing).unwrap_or_else(|_| existing.to_path_buf());

    disks
        .list()
        .iter()
        .filter(|d| canonical.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| d.available_space())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_system_check_reports_plausible_values() {
        let check = run_system_check(std::path::Path::new("."));
        assert!(!check.os.is_empty());
        assert!(!check.arch.is_empty());
        assert!(check.cpu_cores >= 1);
        assert!(check.total_memory_bytes > 0);
        // The current directory always resolves to some disk in CI/dev.
        assert!(check.disk_free_bytes.is_some());
    }
}
