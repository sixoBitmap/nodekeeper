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
    /// Refused in the raw console outright: this invocation can print
    /// private key material (`listdescriptors true`, `gethdkeys` with
    /// `private`). Whatever the console runs is also shown in the Live
    /// Command Monitor and stored in `command_history`, and private keys
    /// must reach neither (CLAUDE.md, Secrets). Decided per *call*, not per
    /// method -- the same methods without the private flag are read-only.
    /// See [`classify_bitcoin_rpc_call`].
    BlockedPrivateKeys,
}

/// What the console tells the user when it refuses a command that can
/// print private keys.
pub const PRIVATE_KEYS_BLOCKED_MESSAGE: &str = "This can print your private keys, and anything \
    printed in the console is also saved in your command history -- so Nodekeeper doesn't run it \
    here. (The same command without the private-keys option, e.g. `listdescriptors` on its own, \
    shows only public information and is fine.)";

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

/// [`classify_bitcoin_rpc`] for a specific *call*: the method **and its
/// arguments**. Refuses (`BlockedPrivateKeys`) a call that can print
/// private key material, otherwise falls back to the method's own class.
/// This is the classification the console must use; the method-only
/// function cannot see `listdescriptors true`.
pub fn classify_bitcoin_rpc_call(method: &str, args: &[String]) -> RpcCommandClass {
    if bitcoin_rpc_reveals_private_keys(method, args) {
        return RpcCommandClass::BlockedPrivateKeys;
    }
    classify_bitcoin_rpc(method)
}

/// Whether running `method` with `args` can print private key material.
/// Every method that *can* in bitcoin-cli 31.1 was found by scanning the
/// real `help <method>` text of all 151 methods (DECISIONS.md, "Console
/// private-key output"): only `listdescriptors` (`private` argument) and
/// `gethdkeys` (`private` option) print keys; the others that mention a
/// private key take one as *input* (`signmessagewithprivkey`,
/// `signrawtransactionwithkey`, `encryptwallet`, ...), which the argument
/// redaction and the output backstop deal with instead.
///
/// Deliberately **strict**: the private form is refused unless the call is
/// *provably* the public one. An argument that is anything other than the
/// exact public form -- an unparseable blob, a JSON escape spelling
/// "private", extra positionals -- is refused too; refusing a harmless
/// oddity costs the user a retype, allowing a real one leaks a key. The
/// legacy `dumpprivkey`/`dumpwallet` are refused unconditionally (not in
/// 31.1, but a newer or older node may have them).
pub fn bitcoin_rpc_reveals_private_keys(method: &str, args: &[String]) -> bool {
    match method {
        "dumpprivkey" | "dumpwallet" => true,
        // `listdescriptors ( private )`: public only with no argument, or
        // exactly the JSON value `false`.
        "listdescriptors" => !match args {
            [] => true,
            [only] => only.trim() == "false",
            _ => false,
        },
        // `gethdkeys ( {"active_only":bool,"private":bool,...} )`: public
        // only with no argument, or one JSON object whose `private` is
        // absent or `false`. Parsed, not searched for the substring, so a
        // JSON escape (`"priv\u0061te"`) cannot spell it past the check.
        "gethdkeys" => !match args {
            [] => true,
            [options] => match serde_json::from_str::<serde_json::Value>(options) {
                Ok(serde_json::Value::Object(map)) => {
                    matches!(
                        map.get("private"),
                        None | Some(serde_json::Value::Bool(false))
                    )
                }
                _ => false,
            },
            _ => false,
        },
        _ => false,
    }
}

/// Zero-based positional-argument indices that carry a secret for
/// bitcoin-cli methods whose real, VERIFIED signature takes a
/// passphrase or private key as plain text (`walletpassphrase
/// "passphrase" timeout`, `signmessagewithprivkey "privkey" "message"`,
/// and similar -- see the full signature list in the module docs'
/// VERIFY reference). The console must redact these positions from the
/// command display before it can reach the Live Command Monitor,
/// `command_history`, or an export (CLAUDE.md: "Secrets passed to
/// commands via stdin/RPC params, never argv" -- a console is exactly
/// the case where a secret *has* to go in as a plain argument, since
/// there's no other channel for a user-typed command, so this is the
/// redaction fallback for that one unavoidable exception). Returns an
/// empty slice for any method with nothing to redact.
pub fn secret_bitcoin_rpc_arg_indices(method: &str) -> &'static [usize] {
    match method {
        "walletpassphrase" | "encryptwallet" | "signmessagewithprivkey" => &[0],
        "walletpassphrasechange" => &[0, 1],
        // `createwallet "wallet_name" ( disable_private_keys blank
        // "passphrase" ... )` and `migratewallet ( "wallet_name"
        // "passphrase" )` -- found by scanning the real `help` text of every
        // 31.1 method that takes a passphrase-like argument (DECISIONS.md,
        // "Console private-key output"); a passphrase typed here used to be
        // shown and stored in plain text.
        "createwallet" => &[3],
        "migratewallet" => &[1],
        // `signrawtransactionwithkey "hexstring" ["privatekey",...] (...)`
        "signrawtransactionwithkey" => &[1],
        // `importdescriptors requests` -- `requests` is a JSON array
        // that can embed private descriptors; redact the whole blob
        // rather than trying to parse out just the private ones.
        "importdescriptors" => &[0],
        _ => &[],
    }
}

/// Every method of Bitcoin Core 31.1 (169): the 151 that `bitcoin-cli help`
/// lists plus 18 "hidden" ones that `help <name>` still answers for -- the
/// regtest tooling such as `generatetoaddress`, and test hooks -- sorted.
/// Produced by `crates/nk-testkit/scripts/scan_help_for_secrets.ps1
/// -ListOnly` against the real binary (DECISIONS.md, "Console private-key
/// output and secret arguments") and checked against a live node by
/// `nk-testkit/tests/private_keys.rs`, which fails when a newer Core adds a
/// method this list does not have.
///
/// What it is for: **arguments are shown only for a method that is on this
/// list.** For any other name -- a typo (`walletpasspharse`), a legacy or
/// future method (`importprivkey`, `sethdseed`), a decorated line that got
/// past the shape check (`sudo bitcoin-cli ...`) -- nothing is known about
/// which argument is a secret, so none is shown or stored (see
/// [`bitcoin_rpc_secret_arg_mask`]). That inverts the failure mode: the first
/// versions tried to recognise the dangerous spellings, and every review found
/// another one.
pub const KNOWN_BITCOIN_RPC_METHODS: &[&str] = &[
    "abandontransaction",
    "abortprivatebroadcast",
    "abortrescan",
    "addconnection",
    "addnode",
    "addpeeraddress",
    "analyzepsbt",
    "backupwallet",
    "bumpfee",
    "clearbanned",
    "combinepsbt",
    "combinerawtransaction",
    "converttopsbt",
    "createmultisig",
    "createpsbt",
    "createrawtransaction",
    "createwallet",
    "createwalletdescriptor",
    "decodepsbt",
    "decoderawtransaction",
    "decodescript",
    "deriveaddresses",
    "descriptorprocesspsbt",
    "disconnectnode",
    "dumptxoutset",
    "echo",
    "echoipc",
    "echojson",
    "encryptwallet",
    "enumeratesigners",
    "estimaterawfee",
    "estimatesmartfee",
    "finalizepsbt",
    "fundrawtransaction",
    "generate",
    "generateblock",
    "generatetoaddress",
    "generatetodescriptor",
    "getaddednodeinfo",
    "getaddressesbylabel",
    "getaddressinfo",
    "getaddrmaninfo",
    "getbalance",
    "getbalances",
    "getbestblockhash",
    "getblock",
    "getblockchaininfo",
    "getblockcount",
    "getblockfilter",
    "getblockfrompeer",
    "getblockhash",
    "getblockheader",
    "getblockstats",
    "getblocktemplate",
    "getchainstates",
    "getchaintips",
    "getchaintxstats",
    "getconnectioncount",
    "getdeploymentinfo",
    "getdescriptoractivity",
    "getdescriptorinfo",
    "getdifficulty",
    "gethdkeys",
    "getindexinfo",
    "getmemoryinfo",
    "getmempoolancestors",
    "getmempoolcluster",
    "getmempooldescendants",
    "getmempoolentry",
    "getmempoolinfo",
    "getmininginfo",
    "getnettotals",
    "getnetworkhashps",
    "getnetworkinfo",
    "getnewaddress",
    "getnodeaddresses",
    "getorphantxs",
    "getpeerinfo",
    "getprioritisedtransactions",
    "getprivatebroadcastinfo",
    "getrawaddrman",
    "getrawchangeaddress",
    "getrawmempool",
    "getrawtransaction",
    "getreceivedbyaddress",
    "getreceivedbylabel",
    "getrpcinfo",
    "gettransaction",
    "gettxout",
    "gettxoutproof",
    "gettxoutsetinfo",
    "gettxspendingprevout",
    "getwalletinfo",
    "getzmqnotifications",
    "help",
    "importdescriptors",
    "importmempool",
    "importprunedfunds",
    "invalidateblock",
    "joinpsbts",
    "keypoolrefill",
    "listaddressgroupings",
    "listbanned",
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
    "loadtxoutset",
    "loadwallet",
    "lockunspent",
    "logging",
    "migratewallet",
    "mockscheduler",
    "ping",
    "preciousblock",
    "prioritisetransaction",
    "pruneblockchain",
    "psbtbumpfee",
    "reconsiderblock",
    "removeprunedfunds",
    "rescanblockchain",
    "restorewallet",
    "savemempool",
    "scanblocks",
    "scantxoutset",
    "send",
    "sendall",
    "sendmany",
    "sendmsgtopeer",
    "sendrawtransaction",
    "sendtoaddress",
    "setban",
    "setlabel",
    "setmocktime",
    "setnetworkactive",
    "setwalletflag",
    "signmessage",
    "signmessagewithprivkey",
    "signrawtransactionwithkey",
    "signrawtransactionwithwallet",
    "simulaterawtransaction",
    "stop",
    "submitblock",
    "submitheader",
    "submitpackage",
    "syncwithvalidationinterfacequeue",
    "testmempoolaccept",
    "unloadwallet",
    "uptime",
    "utxoupdatepsbt",
    "validateaddress",
    "verifychain",
    "verifymessage",
    "verifytxoutproof",
    "waitforblock",
    "waitforblockheight",
    "waitfornewblock",
    "walletcreatefundedpsbt",
    "walletdisplayaddress",
    "walletlock",
    "walletpassphrase",
    "walletpassphrasechange",
    "walletprocesspsbt",
];

/// Whether `method` (in any letter case) is a method of Bitcoin Core 31.1 --
/// see [`KNOWN_BITCOIN_RPC_METHODS`].
pub fn is_known_bitcoin_rpc_method(method: &str) -> bool {
    KNOWN_BITCOIN_RPC_METHODS
        .binary_search(&method.to_ascii_lowercase().as_str())
        .is_ok()
}

/// Which of the `args` typed after `method` must be **hidden** wherever the
/// command is displayed or stored (`true` = hide). The rules, each of which
/// closes a way a secret used to reach the monitor or the history:
///
/// 1. A method that is **not a known Bitcoin Core method** hides *all* its
///    arguments. Nothing is known about it, so nothing is safe to show.
/// 2. A method that takes a secret (see [`secret_bitcoin_rpc_arg_indices`])
///    hides **everything from its first secret argument to the end** -- not
///    just that one position. The tokenizer splits on spaces and only knows
///    double quotes, so a passphrase with spaces that is unquoted (or in
///    single quotes) arrives as several tokens, and hiding only the first of
///    them shows the rest (`walletpassphrase correct horse battery staple
///    60` -> `[redacted] horse battery staple 60`). Whatever follows the
///    first secret position is treated as part of it.
/// 3. If any argument *before* that position is written in `name=value` or
///    JSON-object/array form (`createwallet wallet_name=w passphrase=x`), the
///    positions are not the usual ones, so **all** arguments are hidden.
/// 4. The method name is matched case-insensitively: a wrong-case call fails
///    at the node, but the passphrase was already recorded.
///
/// By *position*, never by matching the typed text: the first version
/// replaced the raw token in the rendered command, and the two differ
/// whenever the token is valid JSON (a quoted numeric passphrase renders
/// without its quotes).
pub fn bitcoin_rpc_secret_arg_mask(method: &str, args: &[String]) -> Vec<bool> {
    let lower = method.to_ascii_lowercase();
    if !is_known_bitcoin_rpc_method(&lower) {
        return vec![true; args.len()];
    }
    let mut mask = vec![false; args.len()];
    if let Some(first) = first_secret_arg_index(&lower) {
        let unusual_form = args
            .iter()
            .take(first)
            .any(|arg| looks_like_named_or_json_argument(arg));
        let from = if unusual_form { 0 } else { first };
        for hidden in mask.iter_mut().skip(from) {
            *hidden = true;
        }
    }
    mask
}

/// The first argument position that carries a secret for `lower` (a
/// lower-cased method name), if the method takes one.
fn first_secret_arg_index(lower: &str) -> Option<usize> {
    secret_bitcoin_rpc_arg_indices(lower).iter().copied().min()
}

/// `name=value`, or a JSON object/array: bitcoin-cli's named-argument form
/// and the object form of `createwallet`'s options.
fn looks_like_named_or_json_argument(arg: &str) -> bool {
    arg.contains('=') || arg.starts_with('{') || arg.starts_with('[')
}

/// What the console tells the user about a first word that is not a command
/// name.
const NOT_A_COMMAND_MESSAGE: &str = "A command is one plain word, like `getblockchaininfo` -- \
    type it without `bitcoin-cli`, a path or options like `-regtest` in front of it (the \
    environment is already chosen for this tab).";

/// The console takes a bare RPC command name (`getblockchaininfo`), not a
/// pasted command line. Anything whose first word is not a plain word made of
/// letters and digits -- `bitcoin-cli walletpassphrase ...`, `bitcoin-cli.exe
/// ...`, `./bitcoin-cli`, `-regtest ...`, a path, a `;`, a zero-width
/// character -- is **refused before anything is shown or run**, with a message
/// that does not repeat it: had it been sent, the secret after it would sit at
/// a position nothing knows to hide. (A plain word that is not a real method
/// -- `sudo`, a typo -- gets rule 1 of [`bitcoin_rpc_secret_arg_mask`]
/// instead.)
pub fn pasted_command_prefix_problem(method: &str) -> Option<&'static str> {
    let is_plain_word = method
        .chars()
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && method.chars().all(|c| c.is_ascii_alphanumeric());
    (!is_plain_word).then_some(NOT_A_COMMAND_MESSAGE)
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
    /// `create`/`restore`: refused in the raw console outright, not
    /// just shown a confirmation. Both can print a mnemonic to stdout
    /// (`create` always does; `restore` only if it derives a fresh one,
    /// but the console can't tell in advance) -- running them through
    /// the console's raw-output path would put a mnemonic through the
    /// Live Command Monitor and command history, directly violating
    /// CLAUDE.md's "mnemonics flow only through the sensitive-output
    /// channel ... never the monitor, logs, history, or exports." The
    /// existing Wallet screen's create/restore flow is the only place
    /// these are allowed to run.
    BlockedUseWalletScreen,
    /// `dump`: refused outright, like `BlockedUseWalletScreen`, but for a
    /// different reason -- it prints the wallet's private descriptors
    /// (`tprv…`/`xprv…`; confirmed live against ord 0.29.0), which would
    /// go through the Live Command Monitor and `command_history`. Once
    /// classified `ReadOnly`, i.e. "runs instantly" -- the leak this whole
    /// class exists to close. See [`PRIVATE_KEYS_BLOCKED_MESSAGE`].
    BlockedPrivateKeys,
}

/// The options `ord wallet` itself takes that come *before* the
/// subcommand and consume a value (`ord wallet --name <NAME> ...`,
/// `--server-url <URL>`); `--no-sync` takes none. From `ord wallet --help`
/// (0.29.0).
const ORD_WALLET_OPTIONS_WITH_VALUE: &[&str] = &["--name", "--server-url"];

/// The non-option tokens of `args`, in order, skipping options and the
/// values of the ones that take one -- so `["--no-sync", "create"]` and
/// `["--name", "x", "restore", ...]` are seen as `create` and `restore`.
/// Classifying on `args.first()` alone (what this used to do) let any
/// leading option hide the subcommand from the safety layer.
fn ord_wallet_words<'a>(args: &[&'a str]) -> Vec<&'a str> {
    let mut words = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let arg = args[i];
        if arg.starts_with('-') {
            if ORD_WALLET_OPTIONS_WITH_VALUE.contains(&arg) {
                i += 1; // its value is not a subcommand
            }
        } else {
            words.push(arg);
        }
        i += 1;
    }
    words
}

/// Classifies an `ord wallet` subcommand (e.g. `["send", ...]` or
/// `["offer", "create", ...]`), looking past any leading options.
/// Anything not recognized defaults to `StateChangingNoDryRun` -- fails
/// closed the same way `classify_bitcoin_rpc` does for an unknown RPC
/// method.
pub fn classify_ord_wallet_subcommand(args: &[&str]) -> OrdCommandClass {
    // `dump` anywhere in the arguments: there is no legitimate use of the
    // word elsewhere, and being wrong toward "refuse" only costs a retype.
    if args.contains(&"dump") {
        return OrdCommandClass::BlockedPrivateKeys;
    }
    let words = ord_wallet_words(args);
    let Some(&subcommand) = words.first() else {
        return OrdCommandClass::StateChangingNoDryRun;
    };
    match subcommand {
        "addresses" | "balance" | "cardinals" | "inscriptions" | "label" | "outputs"
        | "pending" | "receive" | "runics" | "sats" | "transactions" => OrdCommandClass::ReadOnly,
        "create" | "restore" => OrdCommandClass::BlockedUseWalletScreen,
        "send" | "inscribe" | "batch" | "burn" | "split" | "sweep" | "resume" => {
            OrdCommandClass::StateChangingWithDryRun
        }
        "offer" => match words.get(1) {
            Some(&"create") => OrdCommandClass::StateChangingWithDryRun,
            _ => OrdCommandClass::StateChangingNoDryRun,
        },
        // create, restore, sign, mint, and any unrecognized subcommand.
        _ => OrdCommandClass::StateChangingNoDryRun,
    }
}

/// What the console shows -- in the confirm dialog and the on-screen
/// scrollback -- for an `ord wallet` command line. A command it **refuses**
/// (`create`/`restore`/`dump`) shows only its subcommand: those are the ones
/// whose arguments can be secret (`--passphrase`, a recovery phrase) and the
/// line is displayed although nothing is run.
pub fn ord_console_display(args: &[&str], class: OrdCommandClass) -> String {
    let refused = matches!(
        class,
        OrdCommandClass::BlockedUseWalletScreen | OrdCommandClass::BlockedPrivateKeys
    );
    if !refused {
        return format!("ord wallet {}", args.join(" "));
    }
    let words = ord_wallet_words(args);
    let subcommand = if args.contains(&"dump") {
        "dump"
    } else {
        words.first().copied().unwrap_or("")
    };
    if args.len() > 1 {
        format!("ord wallet {subcommand} [redacted]")
    } else {
        format!("ord wallet {subcommand}")
    }
}

/// Whether a **stored** `command_history` row (its `command_display` and
/// `output`) is one an earlier version should not have kept -- for erasing,
/// at startup, rows recorded before the console refused the commands that
/// print private keys and before secret arguments were hidden:
///
/// - any row whose output has a `"mnemonic"` field (`ord wallet create`
///   through a path that did not go through the sensitive channel);
/// - a stored `bitcoin-cli` call (`bitcoin-cli [-flag] <method> <args>`)
///   that prints private keys (the same check as the live one), that has a
///   **secret argument in the clear** (the argument at the method's first
///   secret position is not `[redacted]`, or words follow a hidden one --
///   the rest of an unquoted passphrase), or whose method is **not a known
///   Bitcoin Core method** and has arguments other than a single
///   `[redacted]` (a decorated `bitcoin-cli.exe ...` line, a legacy
///   `importprivkey <key>`);
/// - a stored `ord ... wallet ... dump`.
///
/// It works on the flattened text a display is, and deliberately errs toward
/// deleting: a wrongly deleted history row costs nothing, a kept secret is
/// the leak. What it must **not** delete are the rows the app itself writes
/// -- the Wallet screen's `create`/`restore` (whose output is only the
/// sensitive placeholder), `offer create`, and the unlock call
/// `walletpassphrase [redacted] 60`.
pub fn history_row_reveals_secrets(display: &str, output: &str) -> bool {
    if output.contains("\"mnemonic\"") {
        return true;
    }
    let tokens: Vec<&str> = display.split_whitespace().collect();
    if tokens
        .first()
        .is_some_and(|t| t.eq_ignore_ascii_case("bitcoin-cli"))
    {
        return stored_rpc_call_reveals_secrets(&tokens[1..]);
    }
    match tokens.iter().position(|t| *t == "wallet") {
        Some(at) => tokens[at + 1..].contains(&"dump"),
        None => false,
    }
}

/// [`history_row_reveals_secrets`] for the tokens after `bitcoin-cli`.
fn stored_rpc_call_reveals_secrets(tokens: &[&str]) -> bool {
    // The chain flag the RPC client puts in front (`-regtest`, `-signet`...).
    let tokens = match tokens.first() {
        Some(flag) if flag.starts_with('-') => &tokens[1..],
        _ => tokens,
    };
    let Some((method, args)) = tokens.split_first() else {
        return false;
    };
    let lower = method.to_ascii_lowercase();

    if !is_known_bitcoin_rpc_method(&lower) {
        // What the console stores now for a method it does not know.
        return !(args.is_empty() || args == ["[redacted]"]);
    }
    if let Some(first) = first_secret_arg_index(&lower) {
        let unusual_form = args
            .iter()
            .take(first)
            .any(|arg| looks_like_named_or_json_argument(arg));
        let secret_at = if unusual_form { 0 } else { first };
        if let Some(secret) = args.get(secret_at) {
            let hidden = *secret == "[redacted]";
            // After a hidden secret, at most the numeric timeout of an unlock
            // (`walletpassphrase [redacted] 60`). Any other word after it is
            // the rest of a passphrase that had spaces in it.
            let tail = &args[secret_at + 1..];
            let tail_is_only_a_number =
                tail.is_empty() || (tail.len() == 1 && tail[0].bytes().all(|b| b.is_ascii_digit()));
            if !hidden || !tail_is_only_a_number {
                return true;
            }
        }
    }
    let owned: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    bitcoin_rpc_reveals_private_keys(&lower, &owned)
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
    fn known_secret_bearing_methods_flag_the_right_argument_positions() {
        assert_eq!(secret_bitcoin_rpc_arg_indices("walletpassphrase"), &[0]);
        assert_eq!(secret_bitcoin_rpc_arg_indices("encryptwallet"), &[0]);
        assert_eq!(
            secret_bitcoin_rpc_arg_indices("signmessagewithprivkey"),
            &[0]
        );
        assert_eq!(
            secret_bitcoin_rpc_arg_indices("walletpassphrasechange"),
            &[0, 1]
        );
        assert_eq!(
            secret_bitcoin_rpc_arg_indices("signrawtransactionwithkey"),
            &[1]
        );
        assert_eq!(secret_bitcoin_rpc_arg_indices("importdescriptors"), &[0]);
        // The 4th positional argument of createwallet is the passphrase.
        assert_eq!(secret_bitcoin_rpc_arg_indices("createwallet"), &[3]);
        assert_eq!(secret_bitcoin_rpc_arg_indices("migratewallet"), &[1]);
    }

    #[test]
    fn a_method_with_no_secret_arguments_returns_an_empty_slice() {
        assert_eq!(
            secret_bitcoin_rpc_arg_indices("getblockchaininfo"),
            &[] as &[usize]
        );
        assert_eq!(
            secret_bitcoin_rpc_arg_indices("sendtoaddress"),
            &[] as &[usize]
        );
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
    fn ord_create_and_restore_are_blocked_not_just_confirmed() {
        // Both can print a mnemonic to stdout -- running them through
        // the console's raw-output path would leak it into the Live
        // Command Monitor, violating the sensitive-output-channel rule.
        for cmd in ["create", "restore"] {
            assert_eq!(
                classify_ord_wallet_subcommand(&[cmd]),
                OrdCommandClass::BlockedUseWalletScreen,
                "{cmd} should be BlockedUseWalletScreen"
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

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn blocked(method: &str, list: &[&str]) -> bool {
        classify_bitcoin_rpc_call(method, &args(list)) == RpcCommandClass::BlockedPrivateKeys
    }

    // ---- listdescriptors / gethdkeys -------------------------------------

    #[test]
    fn listdescriptors_asking_for_private_keys_is_refused() {
        for list in [
            &["true"][..],
            &["1"],
            &["\"true\""],
            &["TRUE"],
            &["yes"],
            &["false", "extra"],
            &["true", "false"],
            &[""],
        ] {
            assert!(blocked("listdescriptors", list), "{list:?} must be refused");
        }
    }

    #[test]
    fn listdescriptors_that_is_provably_public_still_runs_instantly() {
        for list in [&[][..], &["false"], &[" false "]] {
            assert!(!blocked("listdescriptors", list), "{list:?} is public");
        }
        assert_eq!(
            classify_bitcoin_rpc_call("listdescriptors", &[]),
            RpcCommandClass::ReadOnly
        );
    }

    #[test]
    fn gethdkeys_asking_for_private_keys_is_refused() {
        for list in [
            &[r#"{"private":true}"#][..],
            &[r#"{"active_only":true,"private":true}"#],
            &[r#"{ "private" : true }"#],
            // A JSON escape spelling "private" past a substring check.
            &[r#"{"priv\u0061te":true}"#],
            &[r#"{"private":"true"}"#],
            &[r#"{"private":1}"#],
            // Not provably the public form: refused.
            &["true"],
            &["not json"],
            &["[]"],
            &["{}", "{}"],
        ] {
            assert!(blocked("gethdkeys", list), "{list:?} must be refused");
        }
    }

    #[test]
    fn gethdkeys_that_is_provably_public_still_runs_instantly() {
        for list in [
            &[][..],
            &["{}"],
            &[r#"{"active_only":true}"#],
            &[r#"{"private":false}"#],
        ] {
            assert!(!blocked("gethdkeys", list), "{list:?} is public");
        }
        assert_eq!(
            classify_bitcoin_rpc_call("gethdkeys", &[]),
            RpcCommandClass::ReadOnly
        );
    }

    #[test]
    fn the_legacy_key_dumps_are_refused_whatever_the_arguments() {
        for method in ["dumpprivkey", "dumpwallet"] {
            assert!(blocked(method, &[]), "{method}");
            assert!(blocked(method, &["anything"]), "{method}");
        }
    }

    #[test]
    fn other_methods_are_not_affected_by_the_private_key_check() {
        // Same first argument, different method: only the two methods that
        // can print keys care.
        for method in [
            "getblockhash",
            "getbalance",
            "getnewaddress",
            "walletpassphrase",
        ] {
            assert!(!blocked(method, &["true"]), "{method}");
        }
        assert_eq!(
            classify_bitcoin_rpc_call("sendtoaddress", &[]),
            RpcCommandClass::FundMoving
        );
    }

    // ---- ord wallet dump / hidden subcommands -----------------------------

    #[test]
    fn ord_wallet_dump_is_refused_in_every_position() {
        for list in [
            &["dump"][..],
            &["--no-sync", "dump"],
            &["--name", "other", "dump"],
            &["--server-url", "http://x", "--no-sync", "dump"],
            &["send", "dump"],
        ] {
            assert_eq!(
                classify_ord_wallet_subcommand(list),
                OrdCommandClass::BlockedPrivateKeys,
                "{list:?}"
            );
        }
    }

    /// The hole this closed: classifying on `args.first()` alone let a
    /// leading option hide `create`/`restore` -- which can print a recovery
    /// phrase into the monitor and the history -- from the safety layer.
    #[test]
    fn a_leading_option_cannot_hide_create_or_restore() {
        for list in [
            &["--no-sync", "create"][..],
            &["--name", "other", "create"],
            &["--server-url", "http://x", "restore"],
            &["--no-sync", "--name", "x", "restore", "--mnemonic", "a b c"],
        ] {
            assert_eq!(
                classify_ord_wallet_subcommand(list),
                OrdCommandClass::BlockedUseWalletScreen,
                "{list:?}"
            );
        }
    }

    #[test]
    fn leading_options_do_not_change_how_ordinary_subcommands_classify() {
        assert_eq!(
            classify_ord_wallet_subcommand(&["--no-sync", "balance"]),
            OrdCommandClass::ReadOnly
        );
        assert_eq!(
            classify_ord_wallet_subcommand(&["--name", "x", "send", "addr", "1000sat"]),
            OrdCommandClass::StateChangingWithDryRun
        );
        assert_eq!(
            classify_ord_wallet_subcommand(&["--no-sync", "offer", "create"]),
            OrdCommandClass::StateChangingWithDryRun
        );
        assert_eq!(
            classify_ord_wallet_subcommand(&["--no-sync", "offer", "accept"]),
            OrdCommandClass::StateChangingNoDryRun
        );
    }

    // ---- erasing old history --------------------------------------------

    #[test]
    fn stored_rows_of_the_commands_that_print_secrets_are_recognised() {
        for display in [
            "bitcoin-cli listdescriptors true",
            "bitcoin-cli -regtest listdescriptors true",
            "bitcoin-cli gethdkeys {\"private\":true}",
            "bitcoin-cli dumpprivkey bcrt1qexample",
            "C:\\ord\\ord.exe --regtest --data-dir X wallet --name ord dump",
            "ord wallet --no-sync dump",
        ] {
            assert!(history_row_reveals_secrets(display, ""), "{display}");
        }
        // Whatever the command, a recovery phrase in the output goes: this is
        // how the `--no-sync create` hole shows up in old history.
        assert!(history_row_reveals_secrets(
            "ord.exe --regtest wallet --name ord --no-sync create",
            "{\"mnemonic\":\"abandon ability able\"}"
        ));
        assert!(history_row_reveals_secrets(
            "some script",
            "{\"mnemonic\": \"x\"}"
        ));
    }

    /// Secret arguments recorded **in the clear** by earlier builds, in every
    /// shape a review found: a JSON-quoted numeric passphrase (rendered without
    /// its quotes), a wrong-case or misspelt method, words after a hidden one
    /// (an unquoted passphrase with spaces), named arguments, a decorated
    /// `bitcoin-cli.exe ...` line, and legacy key-import commands.
    #[test]
    fn stored_rows_with_a_secret_argument_in_the_clear_are_recognised() {
        for display in [
            "bitcoin-cli -regtest walletpassphrase 48213907 60",
            "bitcoin-cli -regtest walletpassphrase correct horse battery staple 60",
            "bitcoin-cli -regtest walletpassphrase [redacted] horse battery staple 60",
            "bitcoin-cli -regtest encryptwallet 12345678",
            "bitcoin-cli -regtest walletpassphrasechange 1.50 2.50",
            "bitcoin-cli -regtest createwallet w false false MyPassphrase1",
            "bitcoin-cli -regtest createwallet w false false [redacted] more words",
            "bitcoin-cli -regtest CreateWallet w false false hunter2",
            "bitcoin-cli -regtest createwallet wallet_name=w passphrase=hunter2",
            "bitcoin-cli migratewallet w MyPassphrase1",
            "bitcoin-cli -regtest signmessagewithprivkey KwDiBf89QgGbjEhKnhXJuH7LrciVrZi3qYjgd9M7rFU73sVHnoWn hi",
            "bitcoin-cli -regtest signrawtransactionwithkey 0200 [\"KwDi\"]",
            "bitcoin-cli -regtest importdescriptors [{\"desc\":\"wpkh(x)\"}]",
            // Not a method Bitcoin Core has: the arguments were recorded.
            "bitcoin-cli -regtest bitcoin-cli.exe walletpassphrase hunter2 60",
            "bitcoin-cli -regtest walletpasspharse hunter2 60",
            "bitcoin-cli -regtest importprivkey KwDiBf89QgGbjEhKnhXJuH7LrciVrZi3qYjgd9M7rFU73sVHnoWn",
            "bitcoin-cli -regtest sudo bitcoin-cli walletpassphrase hunter2 60",
        ] {
            assert!(history_row_reveals_secrets(display, "{}"), "{display}");
        }
    }

    #[test]
    fn stored_rows_of_everything_else_are_kept() {
        for (display, output) in [
            ("bitcoin-cli listdescriptors", "{}"),
            ("bitcoin-cli listdescriptors false", "{}"),
            ("bitcoin-cli gethdkeys", "[]"),
            ("bitcoin-cli getblockchaininfo", "{}"),
            ("C:\\ord\\ord.exe --regtest wallet --name ord balance", "{}"),
            ("ord wallet send addr 1000sat", "{}"),
            // The rows the app itself writes must survive every launch: the
            // Wallet screen's create/restore (output is only the sensitive
            // placeholder), an ordinary `offer create`, and the unlock call.
            (
                "C:\\ord\\ord.exe --regtest --data-dir X wallet --server-url http://127.0.0.1:8081 --name ord create",
                "[sensitive output hidden]",
            ),
            (
                "C:\\ord\\ord.exe --regtest wallet --name ord restore --from mnemonic --timestamp now",
                "[sensitive output hidden]",
            ),
            (
                "C:\\ord\\ord.exe --regtest wallet --name ord offer create --inscription abc --amount 5000sat --fee-rate 2",
                "{}",
            ),
            ("bitcoin-cli -regtest walletpassphrase [redacted] 60", "null"),
            ("bitcoin-cli -regtest encryptwallet [redacted]", "{}"),
            ("bitcoin-cli walletlock", "null"),
            // No passphrase, or one that was already hidden.
            ("bitcoin-cli -regtest createwallet w", "{}"),
            ("bitcoin-cli -regtest createwallet w false false [redacted]", "{}"),
            ("bitcoin-cli migratewallet w [redacted]", "{}"),
            ("bitcoin-cli -regtest importdescriptors [redacted]", "[]"),
            ("bitcoin-cli -regtest importprivkey [redacted]", "null"),
            ("", ""),
        ] {
            assert!(
                !history_row_reveals_secrets(display, output),
                "{display} / {output}"
            );
        }
    }

    // ---- hiding secret arguments -----------------------------------------

    fn mask(method: &str, args: &[&str]) -> Vec<bool> {
        let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
        bitcoin_rpc_secret_arg_mask(method, &args)
    }

    /// From the first secret position **to the end**: whatever follows it may
    /// be the rest of a passphrase that was typed with spaces.
    #[test]
    fn everything_from_the_first_secret_position_onward_is_hidden() {
        assert_eq!(mask("walletpassphrase", &["pw", "60"]), [true, true]);
        assert_eq!(mask("walletpassphrase", &["pw"]), [true]);
        assert_eq!(mask("walletpassphrasechange", &["a", "b"]), [true, true]);
        assert_eq!(mask("encryptwallet", &["pw"]), [true]);
        assert_eq!(
            mask("createwallet", &["w", "false", "false", "pw"]),
            [false, false, false, true]
        );
        assert_eq!(mask("migratewallet", &["w", "pw"]), [false, true]);
        assert_eq!(
            mask("signmessagewithprivkey", &["key", "message"]),
            [true, true]
        );
        assert_eq!(
            mask("signrawtransactionwithkey", &["hex", "[\"k\"]", "[]"]),
            [false, true, true]
        );
        assert_eq!(mask("importdescriptors", &["[{}]", "x"]), [true, true]);
    }

    /// The tokenizer splits on spaces and knows only double quotes, so a
    /// passphrase with spaces that is not double-quoted arrives as several
    /// tokens -- all of them are part of the secret.
    #[test]
    fn a_passphrase_typed_with_spaces_is_hidden_word_by_word() {
        assert_eq!(
            mask(
                "walletpassphrase",
                &["correct", "horse", "battery", "staple", "60"]
            ),
            [true; 5]
        );
        // ... even when the timeout was forgotten and a word sits where it goes.
        assert_eq!(mask("walletpassphrase", &["correct", "horse"]), [true; 2]);
        assert_eq!(
            mask("encryptwallet", &["my", "long", "passphrase", "here"]),
            [true; 4]
        );
        assert_eq!(
            mask(
                "createwallet",
                &["w", "false", "false", "correct", "horse", "battery"]
            ),
            [false, false, false, true, true, true]
        );
        // Single quotes are not quotes to this tokenizer.
        assert_eq!(
            mask("walletpassphrase", &["'correct", "horse", "battery'", "60"]),
            [true; 4]
        );
    }

    /// `createwallet wallet_name=w passphrase=hunter2` (bitcoin-cli's named
    /// form) or a JSON options object put the passphrase somewhere other than
    /// the usual position: hide everything.
    #[test]
    fn named_or_json_arguments_hide_every_argument() {
        assert_eq!(
            mask("createwallet", &["wallet_name=w", "passphrase=hunter2"]),
            [true, true]
        );
        assert_eq!(
            mask("createwallet", &["w", "passphrase=hunter2"]),
            [true, true]
        );
        assert_eq!(
            mask(
                "createwallet",
                &["{\"wallet_name\":\"w\",\"passphrase\":\"x\"}"]
            ),
            [true]
        );
        assert_eq!(
            mask("walletpassphrase", &["timeout=60", "passphrase=hunter2"]),
            [true, true]
        );
        assert_eq!(mask("migratewallet", &["wallet_name=w", "x"]), [true, true]);
    }

    /// A first word that is not a method of Bitcoin Core: nothing is known
    /// about its arguments, so none is shown -- typos, legacy and future
    /// methods, and words that only look like a command.
    #[test]
    fn an_unknown_method_hides_every_argument() {
        for method in [
            "walletpasspharse",
            "walletpassphras",
            "createwalet",
            "importdescriptor",
            "importprivkey",
            "sethdseed",
            "importmulti",
            "importwallet",
            "dumpprivkey",
            "sudo",
            "definitelynotamethod",
        ] {
            assert_eq!(mask(method, &["a", "b", "c"]), [true; 3], "{method}");
            assert_eq!(mask(method, &[]), Vec::<bool>::new(), "{method}");
        }
    }

    #[test]
    fn the_method_name_is_matched_case_insensitively() {
        assert_eq!(mask("WalletPassphrase", &["pw", "60"]), [true, true]);
        assert_eq!(
            mask("CREATEWALLET", &["w", "false", "false", "pw"]),
            [false, false, false, true]
        );
    }

    #[test]
    fn a_known_method_without_secrets_shows_its_arguments() {
        for method in [
            "getblockhash",
            "sendrawtransaction",
            "getbalance",
            "loadwallet",
            "unloadwallet",
            "stop",
            "walletlock",
            "getdescriptorinfo",
        ] {
            assert_eq!(mask(method, &["x", "y"]), [false, false], "{method}");
        }
        // Nothing typed, nothing to hide; a secret method with fewer
        // arguments than its secret position hides nothing.
        assert_eq!(mask("walletpassphrase", &[]), Vec::<bool>::new());
        assert_eq!(mask("createwallet", &["w"]), [false]);
        assert_eq!(
            mask("createwallet", &["w", "true", "false"]),
            [false, false, false]
        );
    }

    // ---- the list of known methods -----------------------------------------

    #[test]
    fn the_known_method_list_is_sorted_unique_and_complete_for_our_own_lists() {
        assert_eq!(KNOWN_BITCOIN_RPC_METHODS.len(), 169);
        let mut sorted = KNOWN_BITCOIN_RPC_METHODS.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            sorted, KNOWN_BITCOIN_RPC_METHODS,
            "binary search needs it sorted, and no duplicates"
        );
        for method in READ_ONLY.iter().chain(FUND_MOVING) {
            assert!(
                is_known_bitcoin_rpc_method(method),
                "{method} is classified but is not a method of Core 31.1"
            );
        }
        for method in [
            "walletpassphrase",
            "walletpassphrasechange",
            "encryptwallet",
            "signmessagewithprivkey",
            "signrawtransactionwithkey",
            "importdescriptors",
            "createwallet",
            "migratewallet",
            "listdescriptors",
            "gethdkeys",
        ] {
            assert!(is_known_bitcoin_rpc_method(method), "{method}");
        }
        assert!(!is_known_bitcoin_rpc_method("importprivkey"));
    }

    // ---- refusing a pasted command line ----------------------------------

    #[test]
    fn a_first_word_that_is_not_a_plain_word_is_refused() {
        for method in [
            "bitcoin-cli",
            "Bitcoin-CLI",
            "bitcoin-cli.exe",
            "./bitcoin-cli",
            "C:\\tools\\bitcoin-cli.exe",
            "-regtest",
            "-named",
            "-rpcwallet=x",
            "ord.exe",
            "wallet-passphrase",
            "walletpassphrase;",
            "bitcoin\u{200b}cli",
            "\u{feff}getblockcount",
            "1getblockcount",
            "",
        ] {
            assert!(
                pasted_command_prefix_problem(method).is_some(),
                "{method:?}"
            );
        }
        // Plain words pass this check -- an unknown one is handled by hiding
        // its arguments instead.
        for method in ["getblockchaininfo", "walletpassphrase", "ord", "sudo"] {
            assert!(pasted_command_prefix_problem(method).is_none(), "{method}");
        }
    }

    // ---- what a refused ord line shows --------------------------------------

    #[test]
    fn a_refused_ord_line_shows_only_its_subcommand() {
        use OrdCommandClass::*;
        assert_eq!(
            ord_console_display(
                &["create", "--passphrase", "hunter2"],
                BlockedUseWalletScreen
            ),
            "ord wallet create [redacted]"
        );
        assert_eq!(
            ord_console_display(&["--no-sync", "create"], BlockedUseWalletScreen),
            "ord wallet create [redacted]"
        );
        assert_eq!(
            ord_console_display(
                &["restore", "--from", "mnemonic", "abandon", "ability"],
                BlockedUseWalletScreen
            ),
            "ord wallet restore [redacted]"
        );
        assert_eq!(
            ord_console_display(&["dump"], BlockedPrivateKeys),
            "ord wallet dump"
        );
        assert_eq!(
            ord_console_display(&["--name", "x", "dump"], BlockedPrivateKeys),
            "ord wallet dump [redacted]"
        );
        // A command that runs is shown as typed.
        assert_eq!(
            ord_console_display(&["send", "addr", "1000sat"], StateChangingWithDryRun),
            "ord wallet send addr 1000sat"
        );
    }
}
