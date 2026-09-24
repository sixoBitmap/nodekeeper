//! Types shared between the executor and its consumers. `ExecEvent` and
//! friends derive `TS` because Phase 3's Live Command Monitor streams
//! these to the frontend as-is (docs/SPEC.md item 7) — built now so the
//! wire format exists before that UI does, not the other way around.

use serde::Serialize;
use std::time::Duration;
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, TS)]
pub struct CommandId(pub Uuid);

impl CommandId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for CommandId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum CommandSource {
    OrdCli,
    BitcoinCli,
    /// An RPC call, shown as its equivalent bitcoin-cli command
    /// (docs/SPEC.md item 7: "RPC calls shown as their equivalent
    /// bitcoin-cli command so users can learn them").
    Rpc,
    /// A call to `ord server`'s HTTP API (currently just `/status`),
    /// shown as its equivalent `curl` command. Distinct from `OrdCli`
    /// because it's not an `ord` subprocess invocation at all -- same
    /// reasoning as `Rpc` being distinct from `BitcoinCli`.
    OrdApi,
}

impl CommandSource {
    /// The same lowercase form `#[serde(rename_all = "lowercase")]`
    /// produces — as a plain `&str` for non-serde consumers (e.g.
    /// `nk-store`'s `command_history.source` column) that would
    /// otherwise have to round-trip through `serde_json` just to get a
    /// string.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OrdCli => "ordcli",
            Self::BitcoinCli => "bitcoincli",
            Self::Rpc => "rpc",
            Self::OrdApi => "ordapi",
        }
    }
}

/// Whether a command's *output* may contain a mnemonic or similar secret
/// that must never reach the normal event stream (docs/SPEC.md Foundation
/// B's sensitive-output channel). This is about output, not input: a
/// command reading a passphrase from stdin is handled by never including
/// that passphrase in `CommandSpec.args`/`command_display` in the first
/// place, regardless of this flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
pub enum Sensitivity {
    Normal,
    /// Output goes only to `Executor::execute`'s return value, never to
    /// the broadcast event stream — the monitor shows
    /// `[sensitive output hidden]` instead (see `SENSITIVE_OUTPUT_PLACEHOLDER`).
    Sensitive,
}

pub const SENSITIVE_OUTPUT_PLACEHOLDER: &str = "[sensitive output hidden]";

/// A command to run. Constructing one of these and passing it to
/// `Executor::execute` is the *only* sanctioned way to spawn `ord`,
/// `bitcoin-cli`, or an RPC call anywhere in the app (docs/SPEC.md
/// Foundation B) — enforced mechanically elsewhere (the disallowed-
/// methods clippy check), not by this type itself.
pub struct CommandSpec {
    pub program: String,
    /// Never build this by interpolating a shell string — always a
    /// literal argument array (docs/SPEC.md Foundation B).
    pub args: Vec<String>,
    /// Written to the child's stdin and zeroized immediately after, if
    /// present. This — never a command-line argument — is how secrets
    /// (passphrases, mnemonics) are passed to a command that needs one
    /// (docs/SPEC.md Foundation B).
    pub stdin: Option<Vec<u8>>,
    pub environment: String,
    pub source: CommandSource,
    /// e.g. "Inscribe studio -> dry run" (docs/SPEC.md item 7: "Show
    /// which app action triggered each command").
    pub triggering_action: String,
    pub sensitivity: Sensitivity,
    /// Secret values that must never appear in any logged/displayed/
    /// stored/exported text derived from this command — e.g. a
    /// passphrase passed via `stdin`. Redacted from `command_display`,
    /// stdout, and stderr before anything leaves the executor. Does not
    /// apply to `Sensitivity::Sensitive` commands, whose entire output is
    /// already withheld from the normal channel regardless.
    pub redact: Vec<String>,
    /// docs/SPEC.md item 7: "Background polling is hidden by default
    /// with a Show background polling toggle." Set by the caller, not
    /// inferred here — the same RPC method (e.g. `getblockchaininfo`)
    /// can be a meaningful one-off check in one context and repetitive
    /// polling noise in another.
    pub background: bool,
}

/// Tagging info for `Executor::record` — the same shape as `CommandSpec`
/// minus the parts specific to spawning a real child process (`program`/
/// `args`/`stdin`), since `record` wraps an arbitrary async operation
/// instead (typically an RPC call).
pub struct RecordSpec {
    pub environment: String,
    pub source: CommandSource,
    pub triggering_action: String,
    /// What's shown for this operation — for an RPC call, the
    /// equivalent bitcoin-cli invocation (docs/SPEC.md item 7).
    pub command_display: String,
    pub redact: Vec<String>,
    pub sensitivity: Sensitivity,
    pub background: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum OutputStream {
    Stdout,
    Stderr,
}

/// Streamed to the Live Command Monitor (Phase 3) as a command runs.
/// Never carries sensitive-command output — see `Sensitivity`.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(tag = "type")]
pub enum ExecEvent {
    Started {
        id: CommandId,
        environment: String,
        source: CommandSource,
        triggering_action: String,
        /// The command and its arguments, redacted, for display —
        /// exactly what "Learn mode" and "copy command" show.
        command_display: String,
        background: bool,
    },
    Output {
        id: CommandId,
        stream: OutputStream,
        /// Redacted, or `SENSITIVE_OUTPUT_PLACEHOLDER` for a sensitive
        /// command.
        chunk: String,
    },
    Finished {
        id: CommandId,
        exit_code: Option<i32>,
        #[ts(type = "number")]
        duration_ms: u64,
    },
}

/// What `Executor::execute` returns directly to its caller (as opposed to
/// what it broadcasts via `ExecEvent`). For a `Sensitivity::Sensitive`
/// command, `stdout`/`stderr` here are the *real*, unredacted output —
/// this is the sensitive-output channel itself; callers handling a
/// sensitive command must not pass this further into anything that logs,
/// stores, or displays it outside the dedicated seed view.
pub struct ExecOutcome {
    pub id: CommandId,
    pub exit_code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub duration: Duration,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_source_as_str_matches_its_serde_serialization() {
        for source in [
            CommandSource::OrdCli,
            CommandSource::BitcoinCli,
            CommandSource::Rpc,
            CommandSource::OrdApi,
        ] {
            let via_serde = serde_json::to_value(source).unwrap();
            assert_eq!(via_serde.as_str().unwrap(), source.as_str());
        }
    }
}
