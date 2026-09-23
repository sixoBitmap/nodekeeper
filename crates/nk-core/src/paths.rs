//! Per-chain path resolution, plus a Windows long-path helper.
//!
//! Every path bitcoind/ord/Nodekeeper need is resolved here — nothing
//! else in the codebase should hard-code a cookie/wallet/index path
//! (docs/SPEC.md Foundation A: "Resolve cookie, wallet, and index paths
//! per chain in one path-resolution module; never hard-code them").

use crate::environment::Environment;
use std::path::{Path, PathBuf};

impl Environment {
    /// Directory to pass as bitcoind's `-datadir`.
    pub fn bitcoin_datadir_arg(&self) -> PathBuf {
        self.data_root.join("bitcoin")
    }

    /// bitcoind's actual chain-specific data directory: the `-datadir`
    /// value itself for mainnet, or `<-datadir>/<chain>` for every other
    /// chain (confirmed live against real bitcoind — DECISIONS.md).
    pub fn bitcoin_chain_dir(&self) -> PathBuf {
        join_chain_subdir(&self.bitcoin_datadir_arg(), self.chain.data_subdir())
    }

    pub fn bitcoin_cookie_path(&self) -> PathBuf {
        self.bitcoin_chain_dir().join(".cookie")
    }

    pub fn bitcoin_wallets_dir(&self) -> PathBuf {
        self.bitcoin_chain_dir().join("wallets")
    }

    /// bitcoind's log file (docs/SPEC.md item 2's log viewer) — the same
    /// directory as `.cookie`/`bitcoind.pid`/`wallets/` (confirmed live
    /// — DECISIONS.md).
    pub fn bitcoin_debug_log_path(&self) -> PathBuf {
        self.bitcoin_chain_dir().join("debug.log")
    }

    /// Directory to pass as ord's `--data-dir`.
    pub fn ord_datadir_arg(&self) -> PathBuf {
        self.data_root.join("ord")
    }

    /// Where ord's index actually lands. ord nests its own chain subfolder
    /// under `--data-dir` even when given explicitly (confirmed live —
    /// DECISIONS.md), the same pattern as bitcoind. Needed for disk-usage
    /// reporting and "Reset Test Lab" (which must delete only regtest
    /// data): the `--data-dir` argument value alone is *not* where the
    /// files are.
    pub fn ord_index_dir(&self) -> PathBuf {
        join_chain_subdir(&self.ord_datadir_arg(), self.chain.data_subdir())
    }
}

fn join_chain_subdir(base: &Path, subdir: Option<&str>) -> PathBuf {
    match subdir {
        Some(sub) => base.join(sub),
        None => base.to_path_buf(),
    }
}

/// Converts an absolute Windows path to its `\\?\`-prefixed verbatim form,
/// which bypasses the 260-character MAX_PATH limit unconditionally at the
/// Win32 API level — unlike the app-manifest `longPathAware` opt-in (also
/// set, in `src-tauri/build.rs`), this doesn't depend on a machine-wide
/// registry setting the user may not have. No-op on non-Windows, on
/// relative paths (verbatim paths can't be relative), and on paths that
/// are already verbatim or are UNC-verbatim.
///
/// Use this before any `std::fs` call on a path that might be deeply
/// nested (e.g. inside the app's own data directory), especially in
/// portable mode on an external drive with a long mount path.
pub fn to_verbatim(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        use std::path::Component;
        let s = path.as_os_str().to_string_lossy();
        if s.starts_with(r"\\?\") || !path.is_absolute() {
            return path.to_path_buf();
        }
        if let Component::Prefix(prefix) = path
            .components()
            .next()
            .expect("absolute path has a prefix component")
        {
            if let std::path::Prefix::UNC(server, share) = prefix.kind() {
                let mut verbatim = PathBuf::from(format!(
                    r"\\?\UNC\{}\{}",
                    server.to_string_lossy(),
                    share.to_string_lossy()
                ));
                verbatim.extend(path.components().skip(2));
                return verbatim;
            }
        }
        PathBuf::from(format!(r"\\?\{}", s))
    }
    #[cfg(not(windows))]
    {
        path.to_path_buf()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::Chain;

    fn env(chain: Chain) -> Environment {
        Environment::new_default(chain, Path::new("/data"))
    }

    #[test]
    fn mainnet_paths_have_no_chain_subfolder() {
        let e = env(Chain::Mainnet);
        assert_eq!(e.bitcoin_chain_dir(), Path::new("/data/mainnet/bitcoin"));
        assert_eq!(
            e.bitcoin_cookie_path(),
            Path::new("/data/mainnet/bitcoin/.cookie")
        );
        assert_eq!(
            e.bitcoin_debug_log_path(),
            Path::new("/data/mainnet/bitcoin/debug.log")
        );
        assert_eq!(e.ord_index_dir(), Path::new("/data/mainnet/ord"));
    }

    #[test]
    fn every_other_chain_gets_a_nested_chain_subfolder() {
        for chain in [Chain::Regtest, Chain::Signet, Chain::Testnet4] {
            let e = env(chain);
            let sub = chain.data_subdir().unwrap();
            assert_eq!(
                e.bitcoin_cookie_path(),
                e.bitcoin_datadir_arg().join(sub).join(".cookie")
            );
            assert_eq!(
                e.bitcoin_wallets_dir(),
                e.bitcoin_datadir_arg().join(sub).join("wallets")
            );
            assert_eq!(e.ord_index_dir(), e.ord_datadir_arg().join(sub));
            // the ord/bitcoin *argument* values themselves never carry the
            // chain subfolder -- only the resolved on-disk paths do.
            assert_eq!(e.ord_datadir_arg(), e.data_root.join("ord"));
        }
    }

    #[test]
    fn environments_are_fully_isolated_from_each_other() {
        let roots: std::collections::HashSet<_> =
            Chain::ALL.iter().map(|&c| env(c).data_root).collect();
        assert_eq!(roots.len(), Chain::ALL.len());
    }

    #[cfg(windows)]
    #[test]
    fn to_verbatim_prefixes_absolute_windows_paths() {
        let p = to_verbatim(Path::new(r"C:\Users\test\data"));
        assert_eq!(p, Path::new(r"\\?\C:\Users\test\data"));
        // idempotent
        assert_eq!(to_verbatim(&p), p);
    }

    #[cfg(windows)]
    #[test]
    fn to_verbatim_leaves_relative_paths_alone() {
        let p = Path::new(r"data\regtest");
        assert_eq!(to_verbatim(p), p);
    }

    /// The actual point of longPathAware/`\\?\`: create a directory tree
    /// and a file whose *total* path exceeds Windows' 260-char MAX_PATH,
    /// nested under a real temp directory (so it also exercises an
    /// arbitrary, non-fake OS path prefix) — and prove it fails without
    /// the verbatim form and succeeds with it.
    #[cfg(windows)]
    #[test]
    fn deeply_nested_path_beyond_max_path_is_only_usable_verbatim() {
        let base = tempfile::tempdir().unwrap();
        // Each segment is comfortably under Windows' 255-char single
        // component limit; enough segments push the *total* path past
        // MAX_PATH (260).
        let segment = "a".repeat(50);
        let mut deep = base.path().to_path_buf();
        for _ in 0..8 {
            deep.push(&segment);
        }
        assert!(
            deep.as_os_str().len() > 260,
            "test path should exceed MAX_PATH"
        );

        // Plain (non-verbatim) path: expected to fail without the
        // registry-gated longPathAware behavior, which CI/dev machines
        // can't be assumed to have opted into.
        let plain_result = std::fs::create_dir_all(&deep);

        let verbatim = to_verbatim(&deep);
        std::fs::create_dir_all(&verbatim).expect("verbatim path must bypass MAX_PATH");
        let file = verbatim.join("f.txt");
        std::fs::write(&file, b"ok").expect("write should succeed on the verbatim path");
        assert_eq!(std::fs::read(&file).unwrap(), b"ok");

        if plain_result.is_ok() {
            // This host has LongPathsEnabled=1 machine-wide, so the plain
            // path worked too -- not a failure, just means we can't
            // demonstrate the contrast on this particular machine.
            eprintln!(
                "note: plain long path also succeeded on this host (LongPathsEnabled is set) \
                 -- to_verbatim is still required for hosts where it isn't"
            );
        }
    }
}
