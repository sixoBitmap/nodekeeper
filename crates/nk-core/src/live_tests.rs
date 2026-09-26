//! Support for the tests that need the real Bitcoin Core / `ord` binaries
//! (`NK_TEST_BITCOIND`, `NK_TEST_ORD`).
//!
//! Those tests used to `return` quietly when the variable was not set, so a
//! plain `cargo test` -- and, until this existed, `just check` -- could be
//! green while every wallet, restore and inscribe test did nothing. The quality
//! gate now sets `NK_REQUIRE_LIVE=1`, and with it a missing binary is a **test
//! failure**, not a skip.
//!
//! Test-only in spirit; lives here because every crate with such tests already
//! depends on `nk-core`.

use std::ffi::OsString;

/// The path in environment variable `var` (`NK_TEST_BITCOIND` or `NK_TEST_ORD`).
///
/// - Set: `Some(path)`.
/// - Not set, and `NK_REQUIRE_LIVE=1`: **panics** -- the test would otherwise
///   pass without testing anything.
/// - Not set otherwise: prints that the test is being skipped, and `None` (the
///   caller returns).
pub fn live_binary(var: &str) -> Option<OsString> {
    let value = std::env::var_os(var);
    if value.is_none() {
        if live_tests_are_required() {
            panic!(
                "NK_REQUIRE_LIVE=1 but {var} is not set: this test would silently skip. Set {var} \
                 to the verified binary (the Justfile does)."
            );
        }
        eprintln!("skipping: {var} not set");
    }
    value
}

/// Whether a missing real binary is a failure (`NK_REQUIRE_LIVE=1`).
pub fn live_tests_are_required() -> bool {
    std::env::var_os("NK_REQUIRE_LIVE").is_some_and(|value| value == "1")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A variable that is set comes back as it is, in either mode.
    #[test]
    fn a_set_variable_is_returned() {
        // PATH is set on every machine that runs this test.
        assert!(live_binary("PATH").is_some());
    }

    /// Not set and not required: a skip. (Not set and required is a panic,
    /// exercised for real by running any live test without the variable under
    /// `NK_REQUIRE_LIVE=1` -- see DECISIONS.md, "The quality gate runs the live
    /// tests".) The environment is process-wide, so this only asserts the
    /// non-required half and only when the gate is not requiring them.
    #[test]
    fn a_missing_variable_is_a_skip_when_live_tests_are_not_required() {
        if live_tests_are_required() {
            return;
        }
        assert!(live_binary("NK_TEST_SURELY_NOT_SET_ANYWHERE").is_none());
    }
}
