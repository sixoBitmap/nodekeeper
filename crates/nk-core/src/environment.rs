//! An environment: one independent network (mainnet/regtest/signet/
//! testnet4) with its own ports and data root (docs/SPEC.md Foundation A).
//! Path resolution lives in `paths.rs`, as methods on `Environment`.

use crate::chain::Chain;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use ts_rs::TS;

/// ord's index options (docs/SPEC.md Foundation F): each one is
/// "effectively permanent" once ord has indexed with it disabled —
/// enabling it later means a full reindex, so these are surfaced (and
/// chosen) up front, not toggled casually.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct IndexOptions {
    /// Unlocks sat-level views (docs/SPEC.md Foundation F), including
    /// "all inscriptions on this sat" in reinscribe mode.
    pub index_sats: bool,
    /// Unlocks rune balances (Foundation F). Missing this fails *soft*
    /// in ord, not hard (VERIFY'd in Phase 0) — rune listing itself
    /// still works, just without balance data.
    pub index_runes: bool,
    /// Unlocks address lookups in the explorer (Foundation F).
    pub index_addresses: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Environment {
    pub chain: Chain,
    /// User-facing name. Defaults to the chain's default label but is
    /// editable — environments have "its own... name" per Foundation A,
    /// distinct from the chain itself (e.g. a user could rename their
    /// regtest environment to "Dev Box").
    pub name: String,
    pub rpc_port: u16,
    pub p2p_port: u16,
    pub ord_port: u16,
    /// This environment's own data root, e.g. `<app-data-root>/regtest` —
    /// always relative to the app's shared data root (portable-mode
    /// friendly, Foundation A). Every environment's bitcoind/ord/wallets/
    /// logs/history live under here, fully isolated from every other
    /// environment.
    #[ts(type = "string")]
    pub data_root: PathBuf,
    pub index_options: IndexOptions,
}

impl Environment {
    /// Builds an environment with every default for `chain`, rooted under
    /// `app_data_root` (e.g. the app's `data/` folder).
    pub fn new_default(chain: Chain, app_data_root: &Path) -> Self {
        Self {
            chain,
            name: chain.default_label().to_string(),
            rpc_port: chain.default_rpc_port(),
            p2p_port: chain.default_p2p_port(),
            ord_port: chain.default_ord_port(),
            data_root: app_data_root.join(chain.dir_name()),
            index_options: chain.default_index_options(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_default_roots_under_the_chains_dir_name() {
        let env = Environment::new_default(Chain::Regtest, Path::new("/data"));
        assert_eq!(env.data_root, Path::new("/data/regtest"));
        assert_eq!(env.name, "Regtest");
        assert_eq!(env.rpc_port, 18443);
        assert!(env.index_options.index_sats);
    }

    #[test]
    fn new_default_uses_the_chains_default_index_options() {
        let mainnet = Environment::new_default(Chain::Mainnet, Path::new("/data"));
        assert!(!mainnet.index_options.index_sats);
        let regtest = Environment::new_default(Chain::Regtest, Path::new("/data"));
        assert!(regtest.index_options.index_sats);
    }
}
