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
    /// The filesystem of the disk that contains `data_dir` (e.g.
    /// `"NTFS"`, `"exFAT"`, `"apfs"`, `"ext4"`), or `None` if no mounted
    /// disk could be matched -- docs/SPEC.md item 12: "warn if exFAT
    /// (corruption risk on unplug, no permission bits...); recommend
    /// NTFS if the user only uses Windows and Linux."
    pub disk_filesystem: Option<String>,
    /// Whether `disk_filesystem` is the one this app warns about
    /// (`is_risky_portable_filesystem`) -- computed here, once, so the
    /// frontend renders a warning without needing its own copy of what
    /// counts as "risky" (a second, driftable copy of that judgment).
    pub disk_filesystem_is_risky: bool,
}

/// Runs the system check. `data_dir` is the path the user picked (or the
/// default) for Nodekeeper's data folder — disk space is reported for
/// whichever disk actually contains it, not just the OS drive.
pub fn run_system_check(data_dir: &std::path::Path) -> SystemCheck {
    let mut sys = sysinfo::System::new_all();
    sys.refresh_all();

    let disks = Disks::new_with_refreshed_list();
    let matched = matched_disk_for_path(&disks, data_dir);
    let disk_free_bytes = matched.map(|d| d.available_space());
    let disk_filesystem = matched.map(|d| d.file_system().to_string_lossy().into_owned());
    let disk_filesystem_is_risky = disk_filesystem
        .as_deref()
        .is_some_and(is_risky_portable_filesystem);

    SystemCheck {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        cpu_cores: sys.cpus().len(),
        total_memory_bytes: sys.total_memory(),
        available_memory_bytes: sys.available_memory(),
        disk_free_bytes,
        disk_filesystem,
        disk_filesystem_is_risky,
    }
}

/// Finds the disk with the longest mount-point prefix match for `path`
/// (the standard way to resolve "which filesystem is this path on" from
/// a flat disk list).
fn matched_disk_for_path<'a>(
    disks: &'a Disks,
    path: &std::path::Path,
) -> Option<&'a sysinfo::Disk> {
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
}

/// Finds the disk with the longest mount-point prefix match for `path`
/// and returns its free space. Shared with `disk.rs`'s dashboard disk
/// monitor.
pub(crate) fn disk_free_space_for_path(disks: &Disks, path: &std::path::Path) -> Option<u64> {
    matched_disk_for_path(disks, path).map(|d| d.available_space())
}

/// docs/SPEC.md item 12: whether a filesystem name (as reported by the
/// OS, e.g. `"exFAT"`, `"NTFS"`) is the one this app specifically warns
/// about -- corruption risk on an unplug, no permission bits, and (on
/// macOS) leaves `._` metadata files behind everywhere. Case-
/// insensitive since casing isn't consistent across platforms/tools
/// (confirmed live below: this dev machine's own NTFS system drive
/// reports as all-caps `"NTFS"`, not title-case).
pub fn is_risky_portable_filesystem(fs_name: &str) -> bool {
    fs_name.eq_ignore_ascii_case("exfat")
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

    /// Real, not synthetic: whichever disk the test binary actually
    /// runs from must report some real, non-empty filesystem name --
    /// proves the mount-point matching wires all the way through to a
    /// real OS value, not just that the code compiles.
    #[test]
    fn run_system_check_reports_a_real_filesystem_for_the_current_directory() {
        let check = run_system_check(std::path::Path::new("."));
        let fs = check
            .disk_filesystem
            .expect("current directory should resolve to some disk");
        assert!(!fs.is_empty());
        assert_eq!(
            check.disk_filesystem_is_risky,
            is_risky_portable_filesystem(&fs)
        );
    }

    #[test]
    fn exfat_is_flagged_regardless_of_case() {
        assert!(is_risky_portable_filesystem("exFAT"));
        assert!(is_risky_portable_filesystem("EXFAT"));
        assert!(is_risky_portable_filesystem("exfat"));
    }

    #[test]
    fn ntfs_and_other_filesystems_are_not_flagged() {
        assert!(!is_risky_portable_filesystem("NTFS"));
        assert!(!is_risky_portable_filesystem("ext4"));
        assert!(!is_risky_portable_filesystem("apfs"));
    }
}
