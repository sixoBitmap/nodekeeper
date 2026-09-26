//! Scrubs known secret values out of text before it can reach a log,
//! the event stream, storage, or an export (docs/SPEC.md Foundation B).

use std::borrow::Cow;

const REDACTED_PLACEHOLDER: &str = "[redacted]";

/// What a private key removed by [`scrub_private_keys`] is replaced with.
pub const PRIVATE_KEY_PLACEHOLDER: &str = "[private key removed]";

/// Replaces every occurrence of every string in `secrets` with
/// `[redacted]`, and then removes any private key that is still in the text
/// (see [`scrub_private_keys`]) -- a secret nobody thought to list must not
/// be the one that leaks. Empty strings in `secrets` are skipped (an empty
/// needle would otherwise match everywhere and turn `text` into garbage).
///
/// Longer secrets are replaced first: with two secrets where one contains
/// the other (a new passphrase that is the old one plus a few characters --
/// the usual way to "change" one), replacing the shorter first would leave
/// the rest of the longer one sitting in the output.
pub fn redact(text: &str, secrets: &[String]) -> String {
    let mut ordered: Vec<&String> = secrets.iter().filter(|s| !s.is_empty()).collect();
    ordered.sort_by_key(|s| std::cmp::Reverse(s.len()));

    let mut out = text.to_string();
    for secret in ordered {
        out = out.replace(secret.as_str(), REDACTED_PLACEHOLDER);
    }
    scrub_private_keys(&out).into_owned()
}

/// Removes private keys from `text`, replacing each with
/// [`PRIVATE_KEY_PLACEHOLDER`]. Two kinds are recognised, both **leniently**
/// -- a key that is a little wrong is exactly the kind Bitcoin Core rejects
/// and then *echoes back* in its error message (`key '<what you typed>' is
/// not valid`), so a pattern that only matches perfect keys misses the case
/// that actually happens:
///
/// - **BIP 32 extended private keys**: `xprv`/`yprv`/`zprv`/`Yprv`/`Zprv`
///   on mainnet and `tprv`/`uprv`/`vprv`/`Uprv`/`Vprv` on the test
///   networks, followed by at least 60 letters/digits -- the whole
///   alphanumeric run is removed (a real key has 107 after the prefix; a
///   mistyped one may contain `0`/`O`/`I`/`l`, which are not base58). Found
///   anywhere in the text, with no "must not be part of a longer word"
///   condition: a key after a JSON `\n`, a tab, an ANSI colour code or `%28`
///   (all end in a letter or digit) must not walk through. Public keys
///   (`xpub`...) are untouched.
/// - **WIF keys**: one alphanumeric token, bounded on both sides by
///   anything that is not a letter or digit (a space, a quote, a bracket, a
///   comma, the start or end of the text, or a JSON `\n`-style escape), 50
///   to 53 characters long (a real one is 51 or 52), starting with a WIF's
///   leading character (`5`/`9` uncompressed, `K`/`L`/`c` compressed) and
///   with at most one character outside the base58 alphabet (one mistyped
///   character). Bare, in a descriptor, or inside the quotes of an error
///   message alike.
///
/// This is the **backstop**, not the primary defence. The console refuses
/// the commands that print private keys and hides secret arguments, but
/// neither can see every path: a script, a command nobody has classified, a
/// key pasted into an argument that is not known to carry one, an error
/// message that quotes the input. Every line the executor emits goes
/// through here (never the value the caller gets back -- a feature that
/// legitimately needs the key, such as an encrypted backup, still receives
/// it).
///
/// It does not validate a checksum. A false positive only costs a placeholder
/// in a *displayed* copy; a missed key is the leak. The WIF rule was measured
/// against 200,000 random 64-hex-digit ids (a txid or block hash) and 200,000
/// random bech32-looking addresses: no hit (see the tests). A long base64
/// blob (a PSBT) *can* contain a 50-53 character run between its `+` and `/`
/// characters that starts with one of those letters, so an occasional
/// placeholder in a *stored copy* of a PSBT is possible -- the copy handed to
/// the caller is never touched. Not caught: a key that has been split by
/// whitespace (the tail survives) or that is not valid enough to be
/// recognisable as a key at all.
pub fn scrub_private_keys(text: &str) -> Cow<'_, str> {
    let bytes = text.as_bytes();
    let mut out: Option<String> = None;
    let mut copied_to = 0;
    let mut i = 0;
    while i < bytes.len() {
        let key_end = extended_key_end(bytes, i).or_else(|| {
            if is_token_start(bytes, i) {
                let end = alphanumeric_run_end(bytes, i);
                is_wif_token(&bytes[i..end]).then_some(end)
            } else {
                None
            }
        });
        if let Some(end) = key_end {
            let scrubbed = out.get_or_insert_with(String::new);
            // `i` and `end` sit on ASCII bytes, so they are char boundaries.
            scrubbed.push_str(&text[copied_to..i]);
            scrubbed.push_str(PRIVATE_KEY_PLACEHOLDER);
            copied_to = end;
            i = end;
            continue;
        }
        i += 1;
    }
    match out {
        Some(mut scrubbed) => {
            scrubbed.push_str(&text[copied_to..]);
            Cow::Owned(scrubbed)
        }
        None => Cow::Borrowed(text),
    }
}

/// If an extended private key starts at `at`, the index just past it (the
/// end of the alphanumeric run it starts).
fn extended_key_end(bytes: &[u8], at: usize) -> Option<usize> {
    if at + 4 > bytes.len() || !is_private_key_prefix(&bytes[at..at + 4]) {
        return None;
    }
    let end = alphanumeric_run_end(bytes, at + 4);
    (end - (at + 4) >= MIN_EXTENDED_KEY_BODY_LEN).then_some(end)
}

/// A real extended key has a 4-character prefix and 107 characters after it.
/// 60 leaves room for a damaged copy, and is more than can follow a
/// `tprv`/`xprv`/... **inside a bech32 address** (at most 62 characters in
/// all for the standard ones): every letter of those prefixes is in the bech32
/// alphabet, so a random address contains one about once in 4,000 -- a
/// threshold of 20 turned those addresses into `[private key removed]` (found
/// by the random-address test below).
const MIN_EXTENDED_KEY_BODY_LEN: usize = 60;

fn is_private_key_prefix(prefix: &[u8]) -> bool {
    matches!(
        prefix[0],
        b'x' | b'y' | b'z' | b'Y' | b'Z' | b't' | b'u' | b'v' | b'U' | b'V'
    ) && &prefix[1..] == b"prv"
}

/// Where an alphanumeric token can begin: the start of the text, right after
/// a character that is not a letter or digit, or right after a JSON string
/// escape (`\n`, `\t`, ... -- whose letter is not part of the token that
/// follows it).
fn is_token_start(bytes: &[u8], at: usize) -> bool {
    if at == 0 || !bytes[at - 1].is_ascii_alphanumeric() {
        return true;
    }
    at >= 2 && bytes[at - 2] == b'\\' && matches!(bytes[at - 1], b'n' | b'r' | b't' | b'b' | b'f')
}

fn alphanumeric_run_end(bytes: &[u8], from: usize) -> usize {
    let mut end = from;
    while end < bytes.len() && bytes[end].is_ascii_alphanumeric() {
        end += 1;
    }
    end
}

/// Whether `token` (already known to be one alphanumeric run) is shaped like
/// a WIF private key: 50-53 characters, a WIF's leading character, at most
/// one character that is not base58.
fn is_wif_token(token: &[u8]) -> bool {
    if !(50..=53).contains(&token.len()) || !matches!(token[0], b'5' | b'9' | b'K' | b'L' | b'c') {
        return false;
    }
    token.iter().filter(|b| !is_base58(**b)).count() <= 1
}

/// The base58 alphabet: alphanumerics without `0`, `O`, `I`, `l`.
fn is_base58(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() && !matches!(byte, b'0' | b'O' | b'I' | b'l')
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

    /// `walletpassphrasechange old new` where the new passphrase is the old
    /// one with characters added -- the usual way to "change" one. Replacing
    /// the shorter secret first used to leave the rest of the longer one
    /// (`hunter2` + `2` here) in the output.
    #[test]
    fn a_secret_that_contains_another_is_fully_redacted_in_either_order() {
        let text = "walletpassphrasechange hunter2 hunter2XYZ!";
        for secrets in [
            vec!["hunter2".to_string(), "hunter2XYZ!".to_string()],
            vec!["hunter2XYZ!".to_string(), "hunter2".to_string()],
        ] {
            assert_eq!(
                redact(text, &secrets),
                "walletpassphrasechange [redacted] [redacted]",
                "{secrets:?}"
            );
        }
    }

    /// A synthetic key-shaped string -- prefix plus 107 base58 characters,
    /// which is what a real extended key looks like. Never a real key.
    fn fake_key(prefix: &str) -> String {
        format!("{prefix}{}", "A".repeat(107))
    }

    /// A synthetic WIF-shaped string: `c` + 51 base58 characters (52 total,
    /// a compressed testnet WIF). Never a real key.
    fn fake_wif() -> String {
        format!("c{}", "B".repeat(51))
    }

    #[test]
    fn every_extended_private_key_prefix_is_scrubbed() {
        for prefix in [
            "xprv", "yprv", "zprv", "Yprv", "Zprv", "tprv", "uprv", "vprv", "Uprv", "Vprv",
        ] {
            let text = format!("key: {}", fake_key(prefix));
            assert_eq!(
                scrub_private_keys(&text),
                format!("key: {PRIVATE_KEY_PLACEHOLDER}"),
                "{prefix}"
            );
        }
    }

    #[test]
    fn a_private_key_inside_a_descriptor_is_scrubbed_and_the_rest_kept() {
        let descriptor = format!("wpkh({}/84h/1h/0h/0/*)#checksum", fake_key("tprv"));
        assert_eq!(
            scrub_private_keys(&descriptor),
            format!("wpkh({PRIVATE_KEY_PLACEHOLDER}/84h/1h/0h/0/*)#checksum")
        );
    }

    #[test]
    fn several_keys_in_one_line_are_all_scrubbed() {
        let text = format!("{} and {}", fake_key("xprv"), fake_key("tprv"));
        assert_eq!(
            scrub_private_keys(&text),
            format!("{PRIVATE_KEY_PLACEHOLDER} and {PRIVATE_KEY_PLACEHOLDER}")
        );
    }

    #[test]
    fn public_keys_and_ordinary_text_are_left_alone() {
        for text in [
            fake_key("xpub"),
            fake_key("tpub"),
            "the word xprv on its own".to_string(),
            "tprv1234 is too short to be a key".to_string(),
            "a normal line of output".to_string(),
            String::new(),
        ] {
            assert_eq!(scrub_private_keys(&text), text);
        }
    }

    /// The guard this replaced ("not preceded by an alphanumeric") let a key
    /// through after anything ending in a letter or digit. Every one of
    /// these is a realistic way for a key to appear in captured output.
    #[test]
    fn a_key_is_scrubbed_whatever_precedes_it() {
        let key = fake_key("tprv");
        for before in [
            "A",         // a letter
            "7",         // a digit
            "\\n",       // a JSON newline escape (ends in a letter)
            "\\t",       // ... tab
            "\\r",       // ... carriage return
            "%28",       // a URL-encoded "("
            "\u{1b}[1m", // an ANSI bold code (ends in a letter)
            "line one\\n",
        ] {
            let text = format!("{{\"out\":\"{before}{key}\"}}");
            let scrubbed = scrub_private_keys(&text);
            assert!(
                !scrubbed.contains("prvAAAA"),
                "a key after {before:?} survived: {scrubbed}"
            );
            assert!(scrubbed.contains(PRIVATE_KEY_PLACEHOLDER), "{before:?}");
        }
    }

    #[test]
    fn non_ascii_text_around_a_key_survives_intact() {
        let text = format!("clé `{}` fin", fake_key("tprv"));
        assert_eq!(
            scrub_private_keys(&text),
            format!("clé `{PRIVATE_KEY_PLACEHOLDER}` fin")
        );
    }

    #[test]
    fn redact_scrubs_keys_even_when_no_secret_was_listed() {
        // The point of the backstop: nobody had to know this text carried a
        // key.
        let out = redact(&format!("out: {}", fake_key("xprv")), &[]);
        assert_eq!(out, format!("out: {PRIVATE_KEY_PLACEHOLDER}"));
        assert!(!out.contains("prvA"));
    }

    // ---- lenient matching: keys Core rejects and echoes back -------------

    /// Bitcoin Core 31.1 answers a descriptor with a bad key with
    /// `key '<the key as typed>' is not valid` (the message template is in
    /// `bitcoind.exe`), so the key comes back inside single quotes, in an
    /// error line the executor emits.
    #[test]
    fn a_key_echoed_back_inside_an_error_message_is_scrubbed() {
        let wif = fake_wif();
        for text in [
            format!("error: rpc error -5: key '{wif}' is not valid"),
            format!("error: rpc error -5: key \"{wif}\" is not valid"),
            format!("Key '{wif}' is invalid due to whitespace"),
            format!("Pubkey '{wif}' is invalid"),
            format!("{wif} is not valid"),
            format!("{{\"error\":{{\"message\":\"key {wif} is not valid\"}}}}"),
        ] {
            let scrubbed = scrub_private_keys(&text);
            assert!(!scrubbed.contains("BBBB"), "{text} -> {scrubbed}");
            assert!(scrubbed.contains(PRIVATE_KEY_PLACEHOLDER), "{scrubbed}");
        }
    }

    #[test]
    fn a_wif_after_a_json_escape_or_whitespace_is_scrubbed() {
        let wif = fake_wif();
        for before in ["\\n", "\\t", " ", "\t", "\n", ":", "=", "[", "\"", "'"] {
            let text = format!("x{before}{wif}");
            let scrubbed = scrub_private_keys(&text);
            assert!(!scrubbed.contains("BBBB"), "{text:?} -> {scrubbed}");
        }
    }

    /// A key with one character outside the base58 alphabet (a typo: `0`,
    /// `O`, `I` or `l`) or one character too many/few is what Core rejects.
    #[test]
    fn a_mistyped_key_is_still_scrubbed() {
        let good = fake_wif();
        // One non-base58 character in a WIF.
        let with_l = format!("c{}l{}", "B".repeat(20), "B".repeat(30));
        // One character dropped (51 characters) and one added (53).
        let short = good[..good.len() - 1].to_string();
        let long = format!("{good}B");
        for wif in [with_l, short, long] {
            for text in [
                format!("getdescriptorinfo wpkh({wif})"),
                format!("key '{wif}' is not valid"),
            ] {
                let scrubbed = scrub_private_keys(&text);
                assert!(!scrubbed.contains("BBBB"), "{text} -> {scrubbed}");
            }
        }
        // An extended key with a `0`, an `O` and an `l` in its body.
        let odd = format!(
            "tprv{}0{}O{}l{}",
            "A".repeat(20),
            "A".repeat(30),
            "A".repeat(30),
            "A".repeat(20)
        );
        let text = format!("wpkh({odd}/0/*)");
        let scrubbed = scrub_private_keys(&text);
        assert_eq!(scrubbed, format!("wpkh({PRIVATE_KEY_PLACEHOLDER}/0/*)"));
    }

    // ---- WIF keys in descriptors -----------------------------------------

    #[test]
    fn a_wif_key_in_a_descriptor_is_scrubbed_in_every_position_it_can_take() {
        let wif = fake_wif();
        let uncompressed = format!("9{}", "B".repeat(50));
        for (text, expected) in [
            (
                format!("wpkh({wif})#abcd1234"),
                format!("wpkh({PRIVATE_KEY_PLACEHOLDER})#abcd1234"),
            ),
            (
                format!("pkh({uncompressed})"),
                format!("pkh({PRIVATE_KEY_PLACEHOLDER})"),
            ),
            (
                format!("wpkh({wif}/0/*)"),
                format!("wpkh({PRIVATE_KEY_PLACEHOLDER}/0/*)"),
            ),
            (
                format!("multi(2,{wif},{wif})"),
                format!("multi(2,{PRIVATE_KEY_PLACEHOLDER},{PRIVATE_KEY_PLACEHOLDER})"),
            ),
            (
                format!("wpkh([d34db33f/84h/1h/0h]{wif})"),
                format!("wpkh([d34db33f/84h/1h/0h]{PRIVATE_KEY_PLACEHOLDER})"),
            ),
            (
                format!("tr({wif},pk({wif}))"),
                format!("tr({PRIVATE_KEY_PLACEHOLDER},pk({PRIVATE_KEY_PLACEHOLDER}))"),
            ),
            // A bare WIF: a whole argument on its own.
            (wif.clone(), PRIVATE_KEY_PLACEHOLDER.to_string()),
            (
                format!("importprivkey {wif}"),
                format!("importprivkey {PRIVATE_KEY_PLACEHOLDER}"),
            ),
        ] {
            assert_eq!(scrub_private_keys(&text), expected, "{text}");
        }
    }

    /// Nothing else may be touched: ids, addresses, keys that are not
    /// private, hex, words.
    #[test]
    fn things_that_merely_look_like_a_wif_are_left_alone() {
        let wif = fake_wif();
        for text in [
            // Wrong length.
            format!("wpkh(c{})", "B".repeat(40)),
            format!("wpkh(c{})", "B".repeat(60)),
            // A WIF token with two non-base58 characters is not "one typo".
            format!("c{}00{}", "B".repeat(20), "B".repeat(29)),
            // Not a WIF's leading character.
            format!("wpkh(M{})", "B".repeat(51)),
            // Part of a longer alphanumeric run (a hash, an id).
            format!("x{wif}"),
            format!("{wif}{}", "B".repeat(10)),
            // A public key / x-only key / txid in a descriptor.
            format!("wpkh(02{})", "a".repeat(64)),
            format!("tr({})", "c".to_string() + &"a".repeat(63)),
            // Hex and an ordinary address.
            "0200000001abcdef".to_string(),
            "bcrt1qw508d6qejxtdg4y5r3zarvary0c5xw7kygt080".to_string(),
            // An inscription id (txid + "i" + index).
            format!("{}i0", "c".to_string() + &"a".repeat(63)),
        ] {
            assert_eq!(scrub_private_keys(&text), text, "{text}");
        }
    }

    /// The false-positive measurement the doc comment cites, made
    /// reproducible: pseudo-random 64-hex-digit ids and bech32-looking
    /// addresses (which is what nearly all of a node's output is made of)
    /// never match.
    #[test]
    fn random_ids_and_addresses_are_never_mistaken_for_keys() {
        let mut state: u64 = 0x2545_F491_4F6C_DD1D;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        const HEX: &[u8] = b"0123456789abcdef";
        const BECH32: &[u8] = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l";
        for _ in 0..200_000 {
            let txid: String = (0..64)
                .map(|_| HEX[(next() % 16) as usize] as char)
                .collect();
            let address: String = std::iter::once("bcrt1q".to_string())
                .chain((0..38).map(|_| (BECH32[(next() % 32) as usize] as char).to_string()))
                .collect();
            let line = format!("{{\"txid\":\"{txid}\",\"address\":\"{address}\"}}");
            assert_eq!(scrub_private_keys(&line), line);
        }
    }

    /// Linear time: a very long line with no key in it (a big hex blob, as a
    /// raw transaction is) must not be slow -- this runs on every output
    /// line of every command.
    #[test]
    fn a_huge_line_is_scrubbed_in_linear_time() {
        let hex = "abcdef0123456789".repeat(1_000_000);
        let start = std::time::Instant::now();
        assert_eq!(scrub_private_keys(&hex), hex.as_str());
        assert!(
            start.elapsed() < std::time::Duration::from_secs(5),
            "{:?}",
            start.elapsed()
        );
    }
}
