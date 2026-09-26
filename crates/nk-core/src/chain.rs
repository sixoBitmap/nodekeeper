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

    /// `ord`'s network-selection flag (double-dash, unlike bitcoind's
    /// single-dash convention) — confirmed live against `ord --help`
    /// 0.29.0 (DECISIONS.md, Phase 4). `None` for mainnet (the
    /// default). Deliberately *not* `--testnet`: ord's CLI treats
    /// `testnet` (legacy testnet3) and `testnet4` as distinct chain
    /// values, not aliases — Nodekeeper only ever means testnet4.
    pub fn ord_cli_flag(self) -> Option<&'static str> {
        match self {
            Chain::Mainnet => None,
            Chain::Regtest => Some("--regtest"),
            Chain::Signet => Some("--signet"),
            Chain::Testnet4 => Some("--testnet4"),
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

    /// docs/SPEC.md Foundation F: "Regtest enables all index options by
    /// default, since they cost almost nothing there." Every other
    /// chain defaults to none enabled — each option is effectively
    /// permanent once ord has indexed without it (enabling it later
    /// means a full reindex), so the real cost/time tradeoff belongs to
    /// the setup wizard, not a silent default.
    pub fn default_index_options(self) -> crate::environment::IndexOptions {
        let all = self == Chain::Regtest;
        crate::environment::IndexOptions {
            index_sats: all,
            index_runes: all,
            index_addresses: all,
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
    fn only_mainnet_has_no_ord_cli_flag() {
        for chain in Chain::ALL {
            assert_eq!(chain.ord_cli_flag().is_none(), chain == Chain::Mainnet);
        }
    }

    #[test]
    fn only_regtest_enables_all_index_options_by_default() {
        for chain in Chain::ALL {
            let opts = chain.default_index_options();
            let all_enabled = opts.index_sats && opts.index_runes && opts.index_addresses;
            assert_eq!(all_enabled, chain == Chain::Regtest);
        }
    }

    #[test]
    fn ord_testnet4_flag_is_not_the_legacy_testnet_flag() {
        // ord's CLI treats `testnet` (legacy testnet3) and `testnet4` as
        // distinct chain values -- confirmed live (DECISIONS.md, Phase
        // 4). Nodekeeper must never accidentally emit the bare
        // `--testnet` flag when it means testnet4.
        assert_eq!(Chain::Testnet4.ord_cli_flag(), Some("--testnet4"));
    }

    #[test]
    fn only_mainnet_has_no_data_subdir() {
        for chain in Chain::ALL {
            assert_eq!(chain.data_subdir().is_none(), chain == Chain::Mainnet);
        }
    }

    /// The exact strings the real `bitcoind` 31.1 and `ord` 0.29.0
    /// accepted when each chain was started live through Nodekeeper's own
    /// config generation and process manager (regtest and mainnet in Phase
    /// 2/4, signet and testnet4 in Phase 10 step 2 -- DECISIONS.md, "Live
    /// smoke test of signet and testnet4"). A wrong `[section]` name is a
    /// *fatal bitcoind startup error*, so this pins them: changing one has
    /// to be a deliberate, re-verified decision, not a tidy-up.
    #[test]
    fn the_conf_sections_and_flags_are_the_ones_verified_live_against_the_real_binaries() {
        let expected = [
            (Chain::Mainnet, "main", None, None),
            (
                Chain::Regtest,
                "regtest",
                Some("-regtest"),
                Some("--regtest"),
            ),
            (Chain::Signet, "signet", Some("-signet"), Some("--signet")),
            (
                Chain::Testnet4,
                "testnet4",
                Some("-testnet4"),
                Some("--testnet4"),
            ),
        ];
        for (chain, section, bitcoin_flag, ord_flag) in expected {
            assert_eq!(chain.conf_section_name(), section, "{chain:?}");
            assert_eq!(chain.bitcoin_cli_flag(), bitcoin_flag, "{chain:?}");
            assert_eq!(chain.ord_cli_flag(), ord_flag, "{chain:?}");
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
