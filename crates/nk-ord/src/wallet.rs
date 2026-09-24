//! `ord wallet` CLI subcommand wrappers (docs/SPEC.md item 3, Phase 5).
//! Every call is routed through `nk-exec`'s central executor
//! (docs/SPEC.md Foundation B) exactly like `nk-core::ord_conf`'s
//! server-launch args, so it's visible in the Live Command Monitor.
//!
//! `--server-url`/`--name` are flags *on* the `wallet` subcommand, not
//! top-level flags before it, and ord's default `--server-url` is
//! `http://localhost:80` -- always overridden. Both confirmed live
//! against a real running ord server (DECISIONS.md, Phase 5 VERIFY).
//!
//! `create`/`restore` are tagged `Sensitivity::Sensitive`: their output
//! (`create`) or input (`restore`, via stdin) carries the mnemonic,
//! which must never reach the broadcast stream, `command_history`, or
//! any log. Every other wallet command here is `Sensitivity::Normal` --
//! a PSBT, txid, address, or balance isn't a secret.

use nk_core::{AppErrorCode, Environment};
use nk_exec::{CommandSource, CommandSpec, Executor, Sensitivity};
use serde_json::Value;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WalletError {
    #[error("exec error: {0}")]
    Exec(#[from] nk_exec::ExecError),
    #[error("ord exited with status {exit_code:?}: {stderr}")]
    NonZeroExit {
        exit_code: Option<i32>,
        stderr: String,
    },
    #[error("could not parse ord's output as JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),
}

impl WalletError {
    /// Maps the two `NonZeroExit` failure texts confirmed live
    /// (DECISIONS.md Phase 5 VERIFY) to their shared plain-language
    /// codes (docs/SPEC.md item 8): a locked wallet's real bitcoind
    /// error text ("Please enter the wallet passphrase...") surfacing
    /// through ord's own exit, and ord's own "N blocks behind
    /// bitcoind" sync gate. Anything else stays a technical-details-only
    /// error -- pattern-matching stderr text is inherently best-effort,
    /// not a substitute for the caller checking `ord_status().caught_up`
    /// and wallet-lock state proactively where it can.
    pub fn code(&self) -> Option<AppErrorCode> {
        match self {
            Self::NonZeroExit { stderr, .. } => {
                if stderr.contains("wallet passphrase") {
                    Some(AppErrorCode::WalletLocked)
                } else if stderr.contains("blocks behind") {
                    Some(AppErrorCode::OrdNotSynced)
                } else {
                    None
                }
            }
            Self::Exec(_) | Self::InvalidJson(_) => None,
        }
    }

    /// Whether this failure means "the named wallet has never been
    /// created," not a real error -- confirmed live (DECISIONS.md
    /// Phase 5 VERIFY): `ord wallet --name <nonexistent> <any command>`
    /// fails with `Failed to load wallet <name>: ... "Path does not
    /// exist."`. Used by `wallet_exists`.
    fn is_wallet_not_found(&self) -> bool {
        matches!(self, Self::NonZeroExit { stderr, .. } if stderr.contains("Path does not exist"))
    }
}

/// Everything needed to run a single `ord wallet` subcommand, bundled
/// so each wrapper function below doesn't take eight positional
/// arguments. `binary_path` should already be verified
/// (`nk_verify::ord`) by the caller, same scoping note as
/// `NodeManager`'s bitcoind binary path.
pub struct WalletTarget<'a> {
    pub binary_path: &'a Path,
    pub environment: &'a Environment,
    pub cookie_path: &'a Path,
    pub bitcoin_datadir: &'a Path,
    pub server_url: &'a str,
    pub wallet_name: &'a str,
}

impl WalletTarget<'_> {
    fn base_args(&self) -> Vec<String> {
        let mut args = nk_core::ord_conf::ord_base_args(
            self.environment,
            self.cookie_path,
            self.bitcoin_datadir,
        );
        args.push("wallet".to_string());
        args.push("--server-url".to_string());
        args.push(self.server_url.to_string());
        args.push("--name".to_string());
        args.push(self.wallet_name.to_string());
        args
    }
}

/// `ord wallet create`. Response JSON: `{"mnemonic": "...", "passphrase":
/// ""}` -- per the "no BIP39 passphrase support" approved deviation,
/// Nodekeeper never passes ord's own `--passphrase` flag, so this is
/// always `""`. The mnemonic must go straight from this return value
/// into the sensitive-channel UI flow (`SensitiveSeedView`) and nowhere
/// else.
pub async fn create_wallet(
    executor: &Executor,
    target: &WalletTarget<'_>,
) -> Result<Value, WalletError> {
    let mut args = target.base_args();
    args.push("create".to_string());
    let outcome = run(
        executor,
        target,
        args,
        None,
        "create wallet",
        Sensitivity::Sensitive,
    )
    .await?;
    Ok(serde_json::from_slice(&outcome.stdout)?)
}

/// `ord wallet restore --from mnemonic --timestamp <timestamp>`, the
/// mnemonic passed via stdin (confirmed live, DECISIONS.md Phase 5
/// VERIFY) -- never as an argument. `timestamp` is `"now"` to skip
/// scanning entirely, a unix timestamp to scan from a known point, or
/// `"0"` for a full rescan from genesis; the caller decides (the UI
/// asks the user when the wallet was first used, or defaults to
/// `"now"` for a brand-new restore with no prior history to find).
pub async fn restore_wallet(
    executor: &Executor,
    target: &WalletTarget<'_>,
    mnemonic: &str,
    timestamp: &str,
) -> Result<(), WalletError> {
    let mut args = target.base_args();
    args.push("restore".to_string());
    args.push("--from".to_string());
    args.push("mnemonic".to_string());
    args.push("--timestamp".to_string());
    args.push(timestamp.to_string());
    let stdin = mnemonic.as_bytes().to_vec();
    run(
        executor,
        target,
        args,
        Some(stdin),
        "restore wallet",
        Sensitivity::Sensitive,
    )
    .await?;
    Ok(())
}

/// `ord wallet balance`: `{"cardinal", "ordinal", "total"}` (sats).
pub async fn wallet_balance(
    executor: &Executor,
    target: &WalletTarget<'_>,
) -> Result<Value, WalletError> {
    run_json(
        executor,
        target,
        vec!["balance".to_string()],
        "check wallet balance",
    )
    .await
}

/// `ord wallet receive [-n <count>]`: `{"addresses": [...]}`.
pub async fn wallet_receive(
    executor: &Executor,
    target: &WalletTarget<'_>,
    count: Option<u32>,
) -> Result<Value, WalletError> {
    let mut args = vec!["receive".to_string()];
    if let Some(n) = count {
        args.push("-n".to_string());
        args.push(n.to_string());
    }
    run_json(executor, target, args, "get receive address").await
}

/// `ord wallet addresses`.
pub async fn wallet_addresses(
    executor: &Executor,
    target: &WalletTarget<'_>,
) -> Result<Value, WalletError> {
    run_json(
        executor,
        target,
        vec!["addresses".to_string()],
        "list wallet addresses",
    )
    .await
}

/// Whether `target`'s wallet has ever been created or restored.
/// Confirmed live (DECISIONS.md Phase 5 VERIFY): `ord wallet` commands
/// call `loadwallet` internally before running, transparently
/// reloading an existing on-disk wallet even after a full bitcoind/ord
/// restart -- Nodekeeper needs no explicit reload logic anywhere. A
/// wallet that was never created fails with a distinguishable "Path
/// does not exist" error, which this treats as `Ok(false)` rather than
/// an error; any other failure (ord not synced, connection refused,
/// ...) still propagates normally. Uses `wallet_addresses` as a cheap,
/// side-effect-free probe.
pub async fn wallet_exists(
    executor: &Executor,
    target: &WalletTarget<'_>,
) -> Result<bool, WalletError> {
    match wallet_addresses(executor, target).await {
        Ok(_) => Ok(true),
        Err(e) if e.is_wallet_not_found() => Ok(false),
        Err(e) => Err(e),
    }
}

/// `ord wallet inscriptions` -- the gallery's data source.
pub async fn wallet_inscriptions(
    executor: &Executor,
    target: &WalletTarget<'_>,
) -> Result<Value, WalletError> {
    run_json(
        executor,
        target,
        vec!["inscriptions".to_string()],
        "list wallet inscriptions",
    )
    .await
}

/// `ord wallet transactions [--limit <n>]` -- transaction history.
pub async fn wallet_transactions(
    executor: &Executor,
    target: &WalletTarget<'_>,
    limit: Option<u32>,
) -> Result<Value, WalletError> {
    let mut args = vec!["transactions".to_string()];
    if let Some(n) = limit {
        args.push("--limit".to_string());
        args.push(n.to_string());
    }
    run_json(executor, target, args, "list wallet transactions").await
}

/// `ord wallet cardinals` -- unspent cardinal (non-inscribed) outputs,
/// used for the balance screen's cardinal/inscribed breakdown.
pub async fn wallet_cardinals(
    executor: &Executor,
    target: &WalletTarget<'_>,
) -> Result<Value, WalletError> {
    run_json(
        executor,
        target,
        vec!["cardinals".to_string()],
        "list cardinal outputs",
    )
    .await
}

/// `ord wallet send [--dry-run] --fee-rate <rate> <address> <asset>`.
/// `dry_run: true` needs no wallet unlock (confirmed live, works
/// against a locked encrypted wallet -- DECISIONS.md Phase 5 VERIFY);
/// `dry_run: false` is a real signing action and requires the caller
/// to have already unlocked the wallet via `nk_rpc::RpcClient::
/// wallet_passphrase` first (and to lock it again afterward).
pub async fn wallet_send(
    executor: &Executor,
    target: &WalletTarget<'_>,
    address: &str,
    asset: &str,
    fee_rate: f64,
    dry_run: bool,
) -> Result<Value, WalletError> {
    let triggering_action = if dry_run { "preview send" } else { "send" };
    run_json(
        executor,
        target,
        send_args(address, asset, fee_rate, dry_run),
        triggering_action,
    )
    .await
}

fn send_args(address: &str, asset: &str, fee_rate: f64, dry_run: bool) -> Vec<String> {
    let mut args = vec!["send".to_string()];
    if dry_run {
        args.push("--dry-run".to_string());
    }
    args.push("--fee-rate".to_string());
    args.push(fee_rate.to_string());
    args.push(address.to_string());
    args.push(asset.to_string());
    args
}

async fn run_json(
    executor: &Executor,
    target: &WalletTarget<'_>,
    subcommand_args: Vec<String>,
    triggering_action: &str,
) -> Result<Value, WalletError> {
    let mut args = target.base_args();
    args.extend(subcommand_args);
    let outcome = run(
        executor,
        target,
        args,
        None,
        triggering_action,
        Sensitivity::Normal,
    )
    .await?;
    Ok(serde_json::from_slice(&outcome.stdout)?)
}

async fn run(
    executor: &Executor,
    target: &WalletTarget<'_>,
    args: Vec<String>,
    stdin: Option<Vec<u8>>,
    triggering_action: &str,
    sensitivity: Sensitivity,
) -> Result<nk_exec::ExecOutcome, WalletError> {
    let outcome = executor
        .execute(CommandSpec {
            program: target.binary_path.display().to_string(),
            args,
            stdin,
            environment: target.environment.name.clone(),
            source: CommandSource::OrdCli,
            triggering_action: triggering_action.to_string(),
            sensitivity,
            redact: vec![],
            background: false,
        })
        .await?;

    if outcome.exit_code != Some(0) {
        return Err(WalletError::NonZeroExit {
            exit_code: outcome.exit_code,
            stderr: String::from_utf8_lossy(&outcome.stderr).into_owned(),
        });
    }
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nk_core::Chain;

    #[test]
    fn non_zero_exit_maps_known_stderr_texts_to_their_shared_codes() {
        let locked = WalletError::NonZeroExit {
            exit_code: Some(1),
            stderr: "error: JSON-RPC error: RPC error response: RpcError { code: -13, message: \
                     \"Error: Please enter the wallet passphrase with walletpassphrase first.\" }"
                .to_string(),
        };
        assert_eq!(locked.code(), Some(AppErrorCode::WalletLocked));

        let behind = WalletError::NonZeroExit {
            exit_code: Some(1),
            stderr: "error: `ord server` 6 blocks behind `bitcoind`, consider using \
                     `--no-sync` to ignore this error"
                .to_string(),
        };
        assert_eq!(behind.code(), Some(AppErrorCode::OrdNotSynced));

        let other = WalletError::NonZeroExit {
            exit_code: Some(1),
            stderr: "some other ord error".to_string(),
        };
        assert_eq!(other.code(), None);
    }

    #[test]
    fn is_wallet_not_found_matches_the_exact_real_error_text() {
        let not_found = WalletError::NonZeroExit {
            exit_code: Some(1),
            stderr: "error: Failed to load wallet never-created: JSON-RPC error: RPC error \
                     response: RpcError { code: -18, message: \"Wallet file verification \
                     failed. Failed to load database path '...\\wallets\\never-created'. Path \
                     does not exist.\" }"
                .to_string(),
        };
        assert!(not_found.is_wallet_not_found());

        let locked = WalletError::NonZeroExit {
            exit_code: Some(1),
            stderr: "Please enter the wallet passphrase with walletpassphrase first.".to_string(),
        };
        assert!(!locked.is_wallet_not_found());
    }

    fn target(environment: &Environment) -> WalletTarget<'_> {
        WalletTarget {
            binary_path: Path::new("/ord"),
            environment,
            cookie_path: Path::new("/cookie"),
            bitcoin_datadir: Path::new("/bitcoin"),
            server_url: "http://127.0.0.1:8081",
            wallet_name: "ord",
        }
    }

    /// Pins the exact flag placement confirmed live (DECISIONS.md
    /// Phase 5 VERIFY): `--server-url`/`--name` come *after* `wallet`,
    /// not as top-level flags -- a real "unexpected argument" error
    /// otherwise.
    #[test]
    fn base_args_put_server_url_and_name_after_the_wallet_subcommand() {
        let environment = Environment::new_default(Chain::Regtest, Path::new("/data"));
        let args = target(&environment).base_args();

        let wallet_pos = args.iter().position(|a| a == "wallet").unwrap();
        assert!(args
            .iter()
            .take(wallet_pos)
            .all(|a| a != "--server-url" && a != "--name"));
        assert!(args
            .windows(2)
            .any(|w| w == ["--server-url", "http://127.0.0.1:8081"]));
        assert!(args.windows(2).any(|w| w == ["--name", "ord"]));
    }

    #[test]
    fn base_args_still_include_the_chain_and_path_flags() {
        let environment = Environment::new_default(Chain::Regtest, Path::new("/data"));
        let args = target(&environment).base_args();
        // Same underlying nk_core::ord_conf::ord_base_args this
        // extends -- just confirming the two are actually composed,
        // not that ord_conf's own logic is correct (that's ord_conf's
        // own test suite's job).
        assert!(args.contains(&"--regtest".to_string()));
        assert!(args.contains(&"--cookie-file".to_string()));
    }

    #[test]
    fn send_args_include_dry_run_only_when_requested() {
        assert_eq!(
            send_args("bcrt1qexample", "1btc", 2.0, true),
            vec![
                "send",
                "--dry-run",
                "--fee-rate",
                "2",
                "bcrt1qexample",
                "1btc"
            ]
        );
        assert_eq!(
            send_args("bcrt1qexample", "1btc", 2.0, false),
            vec!["send", "--fee-rate", "2", "bcrt1qexample", "1btc"]
        );
    }
}
