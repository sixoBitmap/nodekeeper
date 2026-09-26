//! Parses a raw command line typed into the console (docs/SPEC.md item
//! 6) into a command name plus arguments. Shared by both the
//! `bitcoin-cli`-style and `ord`-style halves of the console -- the
//! tokenizing step (quote-aware whitespace splitting) is identical;
//! only `bitcoin-cli` additionally JSON-coerces each argument, since
//! ord's CLI just wants plain strings like any other `clap` program.
//!
//! The JSON-coercion behavior is bitcoin-cli's own real convention,
//! confirmed live rather than recalled from memory: `bitcoin-cli
//! getrawmempool true` returns the verbose object form (`{...}`), not
//! an error or the non-verbose array form -- proving a bare `true`
//! token is sent as the JSON boolean `true`, not the string `"true"`
//! (DECISIONS.md, "Phase 7 — VERIFY: the real RPC/CLI surface for the
//! console safety layer").

use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ParseCommandLineError {
    #[error("enter a command")]
    Empty,
    /// Deliberately does not carry (or print) the line that was typed: it
    /// may contain a passphrase or a private key, and an error message is
    /// the kind of text that gets shown, copied and logged.
    #[error("a quote was opened but never closed -- add the closing \" or remove it")]
    UnterminatedQuote,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCommandLine {
    /// The first token, e.g. `"getblockchaininfo"` or `"wallet"`.
    pub command: String,
    /// Every token after the first, unquoted but not yet JSON-coerced.
    pub args: Vec<String>,
}

/// Splits `line` into whitespace-separated tokens, treating a
/// double-quoted span (`"like this"`) as one token so arguments
/// containing spaces (an address label, a comment) can be typed
/// naturally. `\"` and `\\` are the only recognized escapes inside a
/// quoted span; unquoted tokens have no escape handling (matching a
/// plain shell's simplest behavior).
pub fn tokenize(line: &str) -> Result<Vec<String>, ParseCommandLineError> {
    let mut tokens = Vec::new();
    let mut chars = line.chars().peekable();

    loop {
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        if chars.peek().is_none() {
            break;
        }

        if chars.peek() == Some(&'"') {
            chars.next();
            let mut token = String::new();
            let mut closed = false;
            while let Some(c) = chars.next() {
                match c {
                    '"' => {
                        closed = true;
                        break;
                    }
                    '\\' if matches!(chars.peek(), Some('"') | Some('\\')) => {
                        token.push(chars.next().unwrap());
                    }
                    other => token.push(other),
                }
            }
            if !closed {
                return Err(ParseCommandLineError::UnterminatedQuote);
            }
            tokens.push(token);
        } else {
            let mut token = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() {
                    break;
                }
                token.push(c);
                chars.next();
            }
            tokens.push(token);
        }
    }

    Ok(tokens)
}

/// Tokenizes `line` and splits it into a command name (the first token)
/// and its arguments (every token after).
pub fn parse_command_line(line: &str) -> Result<ParsedCommandLine, ParseCommandLineError> {
    let tokens = tokenize(line)?;
    let mut iter = tokens.into_iter();
    let command = iter.next().ok_or(ParseCommandLineError::Empty)?;
    Ok(ParsedCommandLine {
        command,
        args: iter.collect(),
    })
}

/// bitcoin-cli's own argument convention (see module docs): each raw
/// token is parsed as JSON if it's valid JSON (numbers, booleans,
/// `null`, arrays, objects), otherwise sent as a plain JSON string.
pub fn coerce_json_args(args: &[String]) -> Vec<Value> {
    args.iter()
        .map(|a| serde_json::from_str(a).unwrap_or_else(|_| Value::String(a.clone())))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn splits_on_whitespace() {
        assert_eq!(
            tokenize("getblockhash 0").unwrap(),
            vec!["getblockhash", "0"]
        );
    }

    #[test]
    fn collapses_repeated_whitespace() {
        assert_eq!(
            tokenize("  getblockcount   ").unwrap(),
            vec!["getblockcount"]
        );
    }

    #[test]
    fn a_quoted_span_with_spaces_is_one_token() {
        assert_eq!(
            tokenize(r#"sendtoaddress "bcrt1qexample" 0.5 "a comment with spaces""#).unwrap(),
            vec![
                "sendtoaddress",
                "bcrt1qexample",
                "0.5",
                "a comment with spaces"
            ]
        );
    }

    #[test]
    fn escaped_quote_and_backslash_inside_a_quoted_span() {
        assert_eq!(
            tokenize(r#"setlabel "addr" "say \"hi\" \\ bye""#).unwrap(),
            vec!["setlabel", "addr", r#"say "hi" \ bye"#]
        );
    }

    #[test]
    fn an_unterminated_quote_is_an_error() {
        assert_eq!(
            tokenize(r#"sendtoaddress "bcrt1q..."#),
            Err(ParseCommandLineError::UnterminatedQuote)
        );
    }

    /// The error is shown to the user and may be copied or logged, so it must
    /// not repeat what was typed -- which can be a passphrase.
    #[test]
    fn the_unterminated_quote_error_does_not_echo_the_line() {
        let error = tokenize(r#"walletpassphrase "hunter2-secret 60"#).unwrap_err();
        assert!(!error.to_string().contains("hunter2"), "{error}");
        assert!(!format!("{error:?}").contains("hunter2"), "{error:?}");
    }

    #[test]
    fn empty_input_is_an_error() {
        assert_eq!(
            parse_command_line("   ").unwrap_err(),
            ParseCommandLineError::Empty
        );
    }

    #[test]
    fn splits_command_from_args() {
        let parsed = parse_command_line("listunspent 6 9999999").unwrap();
        assert_eq!(parsed.command, "listunspent");
        assert_eq!(parsed.args, vec!["6", "9999999"]);
    }

    #[test]
    fn a_command_with_no_args_has_an_empty_args_list() {
        let parsed = parse_command_line("getblockchaininfo").unwrap();
        assert_eq!(parsed.command, "getblockchaininfo");
        assert!(parsed.args.is_empty());
    }

    #[test]
    fn bare_numbers_and_booleans_coerce_to_their_json_type() {
        let args = vec![
            "true".to_string(),
            "false".to_string(),
            "42".to_string(),
            "1.5".to_string(),
        ];
        assert_eq!(
            coerce_json_args(&args),
            vec![json!(true), json!(false), json!(42), json!(1.5)]
        );
    }

    #[test]
    fn a_json_array_or_object_argument_coerces_too() {
        let args = vec![r#"["a","b"]"#.to_string(), r#"{"k":1}"#.to_string()];
        assert_eq!(
            coerce_json_args(&args),
            vec![json!(["a", "b"]), json!({"k": 1})]
        );
    }

    #[test]
    fn a_non_json_string_stays_a_plain_string() {
        // An address is not valid JSON on its own, so it must stay a
        // string, not fail or be misinterpreted.
        let args = vec!["bcrt1qexampleaddress".to_string()];
        assert_eq!(coerce_json_args(&args), vec![json!("bcrt1qexampleaddress")]);
    }

    #[test]
    fn a_numeric_looking_label_that_should_stay_a_string_still_coerces_to_a_number() {
        // Matches real bitcoin-cli behavior exactly (VERIFIED live) --
        // this is a known sharp edge users of the real bitcoin-cli
        // already learn to work around by quoting inside the JSON
        // themselves (e.g. a literal string "123" needs `\"123\"` at
        // the shell level); Nodekeeper's console isn't changing that
        // convention, just matching it.
        let args = vec!["123".to_string()];
        assert_eq!(coerce_json_args(&args), vec![json!(123)]);
    }
}
