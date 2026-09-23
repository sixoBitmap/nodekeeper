//! The shared vocabulary of plain-language error codes (docs/SPEC.md
//! item 8: "Plain-language errors: map common failures... to friendly
//! messages with a 'What to do' button. Technical details are available
//! behind a toggle.").
//!
//! A code alone carries no message text — the frontend owns the
//! code -> i18n message / "what to do" mapping (`ErrorPanel`, Phase 1),
//! so translations live in one place instead of being duplicated or
//! hard-coded in the Rust backend. Backend error types attach a code
//! where one of theirs matches a case the spec explicitly calls out;
//! errors with no good code stay backend-only typed errors, shown (if
//! surfaced at all) only as technical details.

use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AppErrorCode {
    PortInUse,
    DiskFull,
    IndexBehind,
    IndexOptionDisabled,
    WalletLocked,
    RpcWarmingUp,
    OrdNotSynced,
    BinaryNotVerified,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins the wire format to the spec's literal code names (docs/SPEC.md
    /// item 8 names them exactly this way) rather than trusting
    /// `rename_all`'s case conversion to do the right thing silently.
    #[test]
    fn serializes_to_the_exact_spec_literal_codes() {
        let cases = [
            (AppErrorCode::PortInUse, "PORT_IN_USE"),
            (AppErrorCode::DiskFull, "DISK_FULL"),
            (AppErrorCode::IndexBehind, "INDEX_BEHIND"),
            (AppErrorCode::IndexOptionDisabled, "INDEX_OPTION_DISABLED"),
            (AppErrorCode::WalletLocked, "WALLET_LOCKED"),
            (AppErrorCode::RpcWarmingUp, "RPC_WARMING_UP"),
            (AppErrorCode::OrdNotSynced, "ORD_NOT_SYNCED"),
            (AppErrorCode::BinaryNotVerified, "BINARY_NOT_VERIFIED"),
        ];
        for (code, expected) in cases {
            assert_eq!(serde_json::to_value(code).unwrap(), expected);
        }
    }
}
