//! Dashboard disk monitor (docs/SPEC.md item 2: "Disk monitor: projected
//! usage including the ord index; warn well before free space gets low").
//!
//! Distinct from `system_check`'s one-time setup-wizard check: this
//! reports the space *this environment's own data* is actually using,
//! polled repeatedly while a dashboard is open, not just free space on
//! the volume.

use crate::system_check::disk_free_space_for_path;
use serde::Serialize;
use std::path::Path;
use sysinfo::Disks;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, TS)]
pub struct DiskUsage {
    /// Total size of everything under this environment's `data_root`
    /// (bitcoind's data dir, ord's index once Phase 4 adds it, logs).
    #[ts(type = "number")]
    pub used_by_data_bytes: u64,
    /// Free space on the volume containing `data_root`, or `None` if no
    /// mounted disk could be matched to it.
    #[ts(type = "number | null")]
    pub free_on_volume_bytes: Option<u64>,
}

/// Measures `data_root`'s on-disk size and the free space on its volume.
pub fn disk_usage_for(data_root: &Path) -> DiskUsage {
    let disks = Disks::new_with_refreshed_list();
    DiskUsage {
        used_by_data_bytes: directory_size(data_root),
        free_on_volume_bytes: disk_free_space_for_path(&disks, data_root),
    }
}

/// Recursively sums file sizes under `path`. Missing directories (e.g. an
/// environment that has never been started yet) report 0, not an error --
/// "no data yet" is a normal, expected state here, not a failure.
fn directory_size(path: &Path) -> u64 {
    let verbatim = crate::paths::to_verbatim(path);
    let Ok(entries) = std::fs::read_dir(&verbatim) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| match entry.metadata() {
            Ok(metadata) if metadata.is_dir() => directory_size(&entry.path()),
            Ok(metadata) => metadata.len(),
            Err(_) => 0,
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_directory_reports_zero_used_bytes() {
        let usage = disk_usage_for(Path::new("this/path/does/not/exist/anywhere"));
        assert_eq!(usage.used_by_data_bytes, 0);
    }

    #[test]
    fn sums_file_sizes_recursively_across_nested_directories() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), vec![0u8; 100]).unwrap();
        let nested = dir.path().join("nested");
        std::fs::create_dir(&nested).unwrap();
        std::fs::write(nested.join("b.txt"), vec![0u8; 250]).unwrap();

        let usage = disk_usage_for(dir.path());
        assert_eq!(usage.used_by_data_bytes, 350);
    }

    #[test]
    fn reports_free_space_on_the_containing_volume() {
        let dir = tempfile::tempdir().unwrap();
        let usage = disk_usage_for(dir.path());
        // The temp dir always resolves to some real disk in CI/dev, same
        // assumption `system_check`'s own test already relies on.
        assert!(usage.free_on_volume_bytes.is_some());
    }
}
