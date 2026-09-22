//! The four networks Nodekeeper can run an environment on (docs/SPEC.md
//! Foundation A).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Chain {
    Mainnet,
    Regtest,
    Signet,
    Testnet4,
}

impl Chain {
    pub const ALL: [Chain; 4] = [
        Chain::Mainnet,
        Chain::Regtest,
        Chain::Signet,
        Chain::Testnet4,
    ];

    /// The on-disk chain subfolder bitcoind and ord both nest their data
    /// under — confirmed empirically against real binaries for every
    /// chain, including with `--data-dir`/`-datadir` given explicitly
    /// (see DECISIONS.md, "Per-chain data paths" and the Phase 1 entry
    /// below it). `None` for mainnet, which uses no subfolder for either.
    pub fn data_subdir(self) -> Option<&'static str> {
        match self {
            Chain::Mainnet => None,
            Chain::Regtest => Some("regtest"),
            Chain::Signet => Some("signet"),
            Chain::Testnet4 => Some("testnet4"),
        }
    }

    /// This environment's folder name under the app's data root
    /// (`data/<name>/...`, Foundation A). Unlike `data_subdir`, mainnet
    /// does have one here — every environment gets its own top-level
    /// folder regardless of chain.
    pub fn dir_name(self) -> &'static str {
        match self {
            Chain::Mainnet => "mainnet",
            Chain::Regtest => "regtest",
            Chain::Signet => "signet",
            Chain::Testnet4 => "testnet4",
        }
    }

    /// The `bitcoin.conf` section name for this chain's network-specific
    /// settings (`rpcbind`, `rpcallowip`, `rpcport`, `port`, ...) —
    /// confirmed live (not assumed): bitcoind 31.1 treats these settings
    /// as **errors at startup**, not mere warnings, when they're at the
    /// top level instead of under the right `[section]`, even though the
    /// network itself is selected via a CLI flag (see DECISIONS.md).
    /// Unlike `data_subdir`, mainnet *does* have one here — `[main]`,
    /// also confirmed live, not `[mainnet]`.
    pub fn conf_section_name(self) -> &'static str {
        match self {
            Chain::Mainnet => "main",
            Chain::Regtest => "regtest",
            Chain::Signet => "signet",
            Chain::Testnet4 => "testnet4",
        }
    }

    /// The `bitcoind`/`bitcoin-cli` network-selection flag for this
    /// chain. `None` for mainnet, which has no flag (it's the default).
    pub fn bitcoin_cli_flag(self) -> Option<&'static str> {
        match self {
            Chain::Mainnet => None,
            Chain::Regtest => Some("-regtest"),
            Chain::Signet => Some("-signet"),
            Chain::Testnet4 => Some("-testnet4"),
        }
    }

    pub fn default_label(self) -> &'static str {
        match self {
            Chain::Mainnet => "Mainnet",
            Chain::Regtest => "Regtest",
            Chain::Signet => "Signet",
            Chain::Testnet4 => "Testnet4",
        }
    }

    /// RPC/P2P defaults from docs/SPEC.md Foundation A.
    pub fn default_rpc_port(self) -> u16 {
        match self {
            Chain::Mainnet => 8332,
            Chain::Regtest => 18443,
            Chain::Signet => 38332,
            Chain::Testnet4 => 48332,
        }
    }

    pub fn default_p2p_port(self) -> u16 {
        match self {
            Chain::Mainnet => 8333,
            Chain::Regtest => 18444,
            Chain::Signet => 38333,
            Chain::Testnet4 => 48333,
        }
    }

    /// ord's HTTP port default. The spec's example list (8080, 8081, 8082,
    /// 8083) is followed in this same mainnet/regtest/signet/testnet4
    /// order; ports are user-configurable in settings regardless.
    pub fn default_ord_port(self) -> u16 {
        match self {
            Chain::Mainnet => 8080,
            Chain::Regtest => 8081,
            Chain::Signet => 8082,
            Chain::Testnet4 => 8083,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_chain_has_a_unique_dir_name() {
        let names: std::collections::HashSet<_> = Chain::ALL.iter().map(|c| c.dir_name()).collect();
        assert_eq!(names.len(), Chain::ALL.len());
    }

    #[test]
    fn conf_section_names_are_all_distinct_and_mainnet_is_main_not_mainnet() {
        assert_eq!(Chain::Mainnet.conf_section_name(), "main");
        let names: std::collections::HashSet<_> =
            Chain::ALL.iter().map(|c| c.conf_section_name()).collect();
        assert_eq!(names.len(), Chain::ALL.len());
    }

    #[test]
    fn only_mainnet_has_no_cli_flag() {
        for chain in Chain::ALL {
            assert_eq!(chain.bitcoin_cli_flag().is_none(), chain == Chain::Mainnet);
        }
    }

    #[test]
    fn only_mainnet_has_no_data_subdir() {
        for chain in Chain::ALL {
            assert_eq!(chain.data_subdir().is_none(), chain == Chain::Mainnet);
        }
    }

    #[test]
    fn every_chain_has_distinct_ports() {
        let mut ports = std::collections::HashSet::new();
        for chain in Chain::ALL {
            assert!(ports.insert(chain.default_rpc_port()));
            assert!(ports.insert(chain.default_p2p_port()));
            assert!(ports.insert(chain.default_ord_port()));
        }
    }
}
