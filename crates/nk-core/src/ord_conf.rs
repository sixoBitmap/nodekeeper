//! ord CLI argument generation per environment (docs/SPEC.md item 1,
//! Foundation F). Unlike bitcoind, ord takes all its configuration as
//! CLI arguments — there's no config-file equivalent to generate, so
//! this returns argument arrays directly (docs/SPEC.md Foundation B:
//! "never build this by interpolating a shell string").

use crate::environment::Environment;
use std::path::Path;

/// Base arguments that precede any ord subcommand: chain selection,
/// paths, and index options — e.g. `ord <these> server <server args>`
/// or `ord <these> wallet balance`.
pub fn ord_base_args(
    environment: &Environment,
    cookie_path: &Path,
    bitcoin_datadir: &Path,
) -> Vec<String> {
    let mut args = Vec::new();
    if let Some(flag) = environment.chain.ord_cli_flag() {
        args.push(flag.to_string());
    }
    args.push("--data-dir".to_string());
    args.push(environment.ord_datadir_arg().display().to_string());
    args.push("--cookie-file".to_string());
    args.push(cookie_path.display().to_string());
    // Points ord at Nodekeeper's own bitcoind instead of its default
    // (`~/.bitcoin`) — Nodekeeper's bitcoind is never in that location
    // (Foundation A's portable-mode-friendly, app-data-root-relative
    // layout).
    args.push("--bitcoin-data-dir".to_string());
    args.push(bitcoin_datadir.display().to_string());
    // Without this, ord silently assumes bitcoind's RPC is on the
    // chain's *standard* port (18443 for regtest, etc.) and fails to
    // connect on any environment using a different one -- confirmed
    // live (DECISIONS.md): pointing ord at a regtest bitcoind on a
    // non-default RPC port with no `--bitcoin-rpc-url` produces
    // "Failed to connect to Bitcoin Core RPC at `127.0.0.1:18443/`"
    // even though `--cookie-file`/`--bitcoin-data-dir` were correct.
    // `host:port`, no scheme (confirmed live -- a `http://` prefix was
    // never tried against, but the bare form is what ord's own `--help`
    // shows and what worked).
    args.push("--bitcoin-rpc-url".to_string());
    args.push(format!("127.0.0.1:{}", environment.rpc_port));

    if environment.index_options.index_sats {
        args.push("--index-sats".to_string());
    }
    if environment.index_options.index_runes {
        args.push("--index-runes".to_string());
    }
    if environment.index_options.index_addresses {
        args.push("--index-addresses".to_string());
    }

    args
}

/// Arguments for `ord <base args> server <these>`. RPC bound to
/// 127.0.0.1 only (docs/SPEC.md Foundation D), matching bitcoind's own
/// `rpcbind`/`rpcallowip` rule: ord's `--address` defaults to
/// `0.0.0.0` (VERIFY'd live, DECISIONS.md Phase 4) and must always be
/// overridden explicitly, never left to the default.
pub fn ord_server_args(environment: &Environment) -> Vec<String> {
    vec![
        "server".to_string(),
        "--address".to_string(),
        "127.0.0.1".to_string(),
        "--http".to_string(),
        "--http-port".to_string(),
        environment.ord_port.to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Chain;
    use std::path::Path;

    fn env(chain: Chain) -> Environment {
        Environment::new_default(chain, Path::new("/data"))
    }

    #[test]
    fn mainnet_has_no_chain_flag_but_every_other_chain_does() {
        for chain in Chain::ALL {
            let args = ord_base_args(&env(chain), Path::new("/cookie"), Path::new("/btc"));
            // The chain flag, if present, is always the first arg.
            let expected_first = chain.ord_cli_flag().unwrap_or("--data-dir");
            assert_eq!(args.first().map(String::as_str), Some(expected_first));
        }
    }

    #[test]
    fn base_args_include_paths() {
        // Build expectations via `.display()` too, the same way the
        // real code does -- a hardcoded "/data/..." literal string
        // would only match on Unix, since Windows renders the same
        // PathBuf with backslashes.
        let cookie_path = Path::new("/data/.cookie");
        let bitcoin_datadir = Path::new("/data/bitcoin");
        let environment = env(Chain::Regtest);
        let args = ord_base_args(&environment, cookie_path, bitcoin_datadir);

        assert!(args.windows(2).any(|w| w
            == [
                "--data-dir".to_string(),
                environment.ord_datadir_arg().display().to_string()
            ]));
        assert!(args.windows(2).any(|w| w
            == [
                "--cookie-file".to_string(),
                cookie_path.display().to_string()
            ]));
        assert!(args.windows(2).any(|w| w
            == [
                "--bitcoin-data-dir".to_string(),
                bitcoin_datadir.display().to_string()
            ]));
    }

    /// Regression test for a real bug caught by a live integration test
    /// (DECISIONS.md): without this flag, ord silently assumes
    /// bitcoind's RPC is on the chain's standard port and fails to
    /// connect on any environment using a different one.
    #[test]
    fn base_args_point_ord_at_the_environments_actual_rpc_port() {
        let mut environment = env(Chain::Regtest);
        environment.rpc_port = 54321;
        let args = ord_base_args(&environment, Path::new("/c"), Path::new("/b"));
        assert!(args
            .windows(2)
            .any(|w| w == ["--bitcoin-rpc-url", "127.0.0.1:54321"]));
    }

    #[test]
    fn only_enabled_index_options_are_passed() {
        let mut environment = env(Chain::Mainnet); // defaults to all off
        assert!(
            !ord_base_args(&environment, Path::new("/c"), Path::new("/b"))
                .contains(&"--index-sats".to_string())
        );

        environment.index_options.index_sats = true;
        let args = ord_base_args(&environment, Path::new("/c"), Path::new("/b"));
        assert!(args.contains(&"--index-sats".to_string()));
        assert!(!args.contains(&"--index-runes".to_string()));
        assert!(!args.contains(&"--index-addresses".to_string()));
    }

    #[test]
    fn regtest_default_passes_all_three_index_options() {
        let args = ord_base_args(&env(Chain::Regtest), Path::new("/c"), Path::new("/b"));
        assert!(args.contains(&"--index-sats".to_string()));
        assert!(args.contains(&"--index-runes".to_string()));
        assert!(args.contains(&"--index-addresses".to_string()));
    }

    #[test]
    fn server_args_bind_to_localhost_only_and_use_the_environments_ord_port() {
        let environment = env(Chain::Regtest);
        let args = ord_server_args(&environment);
        assert!(args.windows(2).any(|w| w == ["--address", "127.0.0.1"]));
        assert!(args
            .windows(2)
            .any(|w| w == ["--http-port", &environment.ord_port.to_string()]));
        assert!(args.contains(&"--http".to_string()));
        // Never the wildcard bind -- Foundation D's "ord server is not
        // reachable from another machine on the LAN" depends on this.
        assert!(!args.contains(&"0.0.0.0".to_string()));
    }
}
