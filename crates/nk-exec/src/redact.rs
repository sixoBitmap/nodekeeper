//! Scrubs known secret values out of text before it can reach a log,
//! the event stream, storage, or an export (docs/SPEC.md Foundation B).

const REDACTED_PLACEHOLDER: &str = "[redacted]";

/// Replaces every occurrence of every string in `secrets` with
/// `[redacted]`. Empty strings in `secrets` are skipped (an empty needle
/// would otherwise match everywhere and turn `text` into garbage).
pub fn redact(text: &str, secrets: &[String]) -> String {
    let mut out = text.to_string();
    for secret in secrets {
        if secret.is_empty() {
            continue;
        }
        out = out.replace(secret.as_str(), REDACTED_PLACEHOLDER);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_a_simple_secret() {
        let out = redact("the passphrase is hunter2 ok", &["hunter2".to_string()]);
        assert_eq!(out, "the passphrase is [redacted] ok");
    }

    #[test]
    fn redacts_every_occurrence_not_just_the_first() {
        let out = redact("hunter2 and hunter2 again", &["hunter2".to_string()]);
        assert_eq!(out, "[redacted] and [redacted] again");
    }

    #[test]
    fn redacts_every_secret_type_when_multiple_are_given() {
        let secrets = vec!["hunter2".to_string(), "abandon ability able".to_string()];
        let out = redact(
            "passphrase hunter2, mnemonic abandon ability able",
            &secrets,
        );
        assert_eq!(out, "passphrase [redacted], mnemonic [redacted]");
    }

    #[test]
    fn text_with_no_secrets_present_is_unchanged() {
        let out = redact("nothing sensitive here", &["hunter2".to_string()]);
        assert_eq!(out, "nothing sensitive here");
    }

    #[test]
    fn empty_secret_values_are_never_matched() {
        // A caller passing an empty/unset secret (e.g. no passphrase set)
        // must not turn every character of the text into a redaction.
        let out = redact("hello world", &[String::new()]);
        assert_eq!(out, "hello world");
    }

    #[test]
    fn no_secrets_at_all_is_a_no_op() {
        assert_eq!(redact("hello world", &[]), "hello world");
    }
}
