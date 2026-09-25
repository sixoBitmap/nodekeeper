//! Classifies raw `bitcoin-cli`/`ord` commands for the console's safety
//! layer (docs/SPEC.md item 6). Fails closed the same way binary
//! verification does: an unrecognized command is never treated as safe
//! to run instantly -- see `classify_bitcoin_rpc`'s doc comment for why
//! this is an allowlist of known-read-only methods, not a denylist.
//!
//! Every method name below is taken from a real `bitcoin-cli help`
//! against bitcoind 31.1 and real `ord wallet <cmd> --help` output
//! against ord 0.29.0, not recalled from memory (DECISIONS.md, "Phase
//! 7 — VERIFY: the real RPC/CLI surface for the console safety layer").

/// How a raw bitcoin-cli-style RPC method should be treated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RpcCommandClass {
    /// Runs instantly, no confirmation (docs/SPEC.md item 6: "Read-only
    /// commands run instantly").
    ReadOnly,
    /// Shows a confirmation dialog explaining what it does before
    /// running (mutates node/wallet state, but doesn't move funds).
    StateChanging,
    /// A wallet-spend command ("sendtoaddress, sendmany, send, bumpfee,
    /// and similar") -- when the target wallet is one ord uses, this is
    /// blocked outright rather than shown a confirmation, per docs/
    /// SPEC.md's "Inscription protection".
    FundMoving,
}

/// Classifies a bitcoin-cli RPC method by name (case-sensitive, matching
/// bitcoin-cli's own method names exactly). Unknown methods -- including
/// any future RPC a newer bitcoind adds that this list doesn't know
/// about yet -- default to `StateChanging`, never `ReadOnly`: the
/// console must never silently skip confirmation for a command it
/// hasn't specifically vetted as safe.
pub fn classify_bitcoin_rpc(method: &str) -> RpcCommandClass {
    if FUND_MOVING.contains(&method) {
        return RpcCommandClass::FundMoving;
    }
    if READ_ONLY.contains(&method) {
        return RpcCommandClass::ReadOnly;
    }
    RpcCommandClass::StateChanging
}

/// "sendtoaddress, sendmany, send, bumpfee, and similar" (docs/SPEC.md
/// item 6) -- every real bitcoin-cli 31.1 wallet RPC that directly signs
/// and broadcasts a spend. `sendrawtransaction` also broadcasts but
/// takes no wallet parameter (any raw hex from any source), so it isn't
/// a *wallet's* spend command the same way and is classified
/// `StateChanging` instead (it still isn't `ReadOnly`).
const FUND_MOVING: &[&str] = &[
    "sendtoaddress",
    "sendmany",
    "send",
    "sendall",
    "bumpfee",
    "psbtbumpfee",
];

/// Every real bitcoin-cli 31.1 method confirmed to only read state --
/// grouped by `bitcoin-cli help`'s own categories for easy cross-
/// checking against a future version's `help` output.
const READ_ONLY: &[&str] = &[
    // == Blockchain == (queries only; excludes dumptxoutset,
    // getblockfrompeer, importmempool, loadtxoutset, preciousblock,
    // pruneblockchain, savemempool, scanblocks/scantxoutset (long-
    // running scans with side effects), and verifychain (CPU-heavy)).
    "getbestblockhash",
    "getblock",
    "getblockchaininfo",
    "getblockcount",
    "getblockfilter",
    "getblockhash",
    "getblockheader",
    "getblockstats",
    "getchainstates",
    "getchaintips",
    "getchaintxstats",
    "getdeploymentinfo",
    "getdescriptoractivity",
    "getdifficulty",
    "getmempoolancestors",
    "getmempoolcluster",
    "getmempooldescendants",
    "getmempoolentry",
    "getmempoolinfo",
    "getrawmempool",
    "gettxout",
    "gettxoutproof",
    "gettxoutsetinfo",
    "gettxspendingprevout",
    "verifytxoutproof",
    "waitforblock",
    "waitforblockheight",
    "waitfornewblock",
    // == Control ==
    "getmemoryinfo",
    "getrpcinfo",
    "help",
    "uptime",
    // == Mining == (queries only; excludes submitblock/submitheader)
    "getmininginfo",
    "getnetworkhashps",
    "getprioritisedtransactions",
    // == Network == (queries only; excludes addnode, clearbanned,
    // disconnectnode, setban, setnetworkactive)
    "getaddednodeinfo",
    "getaddrmaninfo",
    "getconnectioncount",
    "getnettotals",
    "getnetworkinfo",
    "getnodeaddresses",
    "getpeerinfo",
    "listbanned",
    "ping",
    // == Rawtransactions == (decode/analyze only; excludes
    // sendrawtransaction, submitpackage, and everything that builds/
    // signs/combines a transaction, since those are meant to be
    // followed by a broadcast and previewed as a unit, not run
    // instantly one RPC at a time)
    "analyzepsbt",
    "decodepsbt",
    "decoderawtransaction",
    "decodescript",
    "getrawtransaction",
    "testmempoolaccept",
    // == Util ==
    "createmultisig",
    "deriveaddresses",
    "estimatesmartfee",
    "getdescriptorinfo",
    "getindexinfo",
    "signmessagewithprivkey",
    "validateaddress",
    "verifymessage",
    // == Wallet == (queries and address-book reads only; excludes
    // every write in the VERIFY entry's list above, plus setlabel,
    // encryptwallet, importdescriptors, keypoolrefill, loadwallet,
    // unloadwallet, createwallet, restorewallet, migratewallet,
    // rescanblockchain, walletlock, walletpassphrase*, setwalletflag,
    // lockunspent, abandontransaction, abortrescan, backupwallet,
    // importprunedfunds, removeprunedfunds, signmessage,
    // signrawtransactionwithwallet, walletdisplayaddress,
    // walletprocesspsbt, createwalletdescriptor, walletcreatefundedpsbt).
    // `getnewaddress`/`getrawchangeaddress` technically advance the
    // keypool, but are the console's ordinary, expected, non-destructive
    // "get me an address" operations -- treated as read-only for the
    // same reason ord's own `wallet receive` is.
    "getaddressesbylabel",
    "getaddressinfo",
    "getbalance",
    "getbalances",
    "getnewaddress",
    "getrawchangeaddress",
    "getreceivedbyaddress",
    "getreceivedbylabel",
    "gethdkeys",
    "gettransaction",
    "getwalletinfo",
    "listaddressgroupings",
    "listdescriptors",
    "listlabels",
    "listlockunspent",
    "listreceivedbyaddress",
    "listreceivedbylabel",
    "listsinceblock",
    "listtransactions",
    "listunspent",
    "listwalletdir",
    "listwallets",
    "simulaterawtransaction",
];

/// How an `ord wallet <subcommand>` invocation should be treated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrdCommandClass {
    /// Runs instantly, no confirmation.
    ReadOnly,
    /// State-changing and supports `--dry-run` -- ord commands that
    /// support it are previewed with it first (docs/SPEC.md item 6).
    StateChangingWithDryRun,
    /// State-changing with no `--dry-run` support in ord 0.29.0 (`mint`,
    /// `offer accept`) -- falls back to the standard confirmation
    /// dialog, no preview step available.
    StateChangingNoDryRun,
}

/// Classifies an `ord wallet` subcommand by its first argument (e.g.
/// `["send", ...]` or `["offer", "create", ...]`). Anything not
/// recognized defaults to `StateChangingNoDryRun` -- fails closed the
/// same way `classify_bitcoin_rpc` does for an unknown RPC method.
pub fn classify_ord_wallet_subcommand(args: &[&str]) -> OrdCommandClass {
    let Some(&subcommand) = args.first() else {
        return OrdCommandClass::StateChangingNoDryRun;
    };
    match subcommand {
        "addresses" | "balance" | "cardinals" | "dump" | "inscriptions" | "label" | "outputs"
        | "pending" | "receive" | "runics" | "sats" | "transactions" => OrdCommandClass::ReadOnly,
        "send" | "inscribe" | "batch" | "burn" | "split" | "sweep" | "resume" => {
            OrdCommandClass::StateChangingWithDryRun
        }
        "offer" => match args.get(1) {
            Some(&"create") => OrdCommandClass::StateChangingWithDryRun,
            _ => OrdCommandClass::StateChangingNoDryRun,
        },
        // create, restore, sign, mint, and any unrecognized subcommand.
        _ => OrdCommandClass::StateChangingNoDryRun,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_named_fund_moving_command_is_classified_fund_moving() {
        for method in [
            "sendtoaddress",
            "sendmany",
            "send",
            "sendall",
            "bumpfee",
            "psbtbumpfee",
        ] {
            assert_eq!(
                classify_bitcoin_rpc(method),
                RpcCommandClass::FundMoving,
                "{method} should be FundMoving"
            );
        }
    }

    #[test]
    fn sendrawtransaction_is_state_changing_but_not_fund_moving() {
        // Broadcasts, but isn't a *wallet's* spend command (no wallet
        // parameter) -- see the VERIFY entry's reasoning.
        assert_eq!(
            classify_bitcoin_rpc("sendrawtransaction"),
            RpcCommandClass::StateChanging
        );
    }

    #[test]
    fn common_read_only_queries_run_instantly() {
        for method in [
            "getblockchaininfo",
            "getbalance",
            "listunspent",
            "gettransaction",
            "estimatesmartfee",
            "getnewaddress",
        ] {
            assert_eq!(
                classify_bitcoin_rpc(method),
                RpcCommandClass::ReadOnly,
                "{method} should be ReadOnly"
            );
        }
    }

    #[test]
    fn stop_and_wallet_management_commands_need_confirmation() {
        // docs/SPEC.md's own examples: "stop, wallet restore, etc."
        for method in [
            "stop",
            "restorewallet",
            "encryptwallet",
            "createwallet",
            "walletpassphrase",
            "signrawtransactionwithwallet",
        ] {
            assert_eq!(
                classify_bitcoin_rpc(method),
                RpcCommandClass::StateChanging,
                "{method} should be StateChanging"
            );
        }
    }

    #[test]
    fn an_unknown_future_rpc_method_defaults_to_state_changing_not_read_only() {
        assert_eq!(
            classify_bitcoin_rpc("somehypotheticalfuturerpc"),
            RpcCommandClass::StateChanging
        );
    }

    #[test]
    fn no_method_is_in_both_the_fund_moving_and_read_only_lists() {
        for method in FUND_MOVING {
            assert!(
                !READ_ONLY.contains(method),
                "{method} is listed as both FundMoving and ReadOnly"
            );
        }
    }

    #[test]
    fn ord_read_only_subcommands_run_instantly() {
        for cmd in ["balance", "receive", "inscriptions", "transactions"] {
            assert_eq!(
                classify_ord_wallet_subcommand(&[cmd]),
                OrdCommandClass::ReadOnly,
                "{cmd} should be ReadOnly"
            );
        }
    }

    #[test]
    fn ord_send_and_inscribe_support_dry_run() {
        for cmd in [
            "send", "inscribe", "batch", "burn", "split", "sweep", "resume",
        ] {
            assert_eq!(
                classify_ord_wallet_subcommand(&[cmd]),
                OrdCommandClass::StateChangingWithDryRun,
                "{cmd} should be StateChangingWithDryRun"
            );
        }
    }

    #[test]
    fn ord_mint_has_no_dry_run_support() {
        // Confirmed live against ord 0.29.0 --help: no --dry-run flag.
        assert_eq!(
            classify_ord_wallet_subcommand(&["mint"]),
            OrdCommandClass::StateChangingNoDryRun
        );
    }

    #[test]
    fn ord_offer_create_supports_dry_run_but_offer_accept_does_not() {
        assert_eq!(
            classify_ord_wallet_subcommand(&["offer", "create"]),
            OrdCommandClass::StateChangingWithDryRun
        );
        assert_eq!(
            classify_ord_wallet_subcommand(&["offer", "accept"]),
            OrdCommandClass::StateChangingNoDryRun
        );
    }

    #[test]
    fn an_unrecognized_ord_subcommand_defaults_to_no_dry_run_not_read_only() {
        assert_eq!(
            classify_ord_wallet_subcommand(&["somefuturecommand"]),
            OrdCommandClass::StateChangingNoDryRun
        );
    }

    #[test]
    fn empty_args_default_to_no_dry_run() {
        assert_eq!(
            classify_ord_wallet_subcommand(&[]),
            OrdCommandClass::StateChangingNoDryRun
        );
    }
}
