//! Pinned Bitcoin Core builder public keys, embedded into the binary at
//! compile time (docs/SPEC.md: "PINNED inside Nodekeeper... updated only
//! through Nodekeeper updates"). See DECISIONS.md for the fetch source,
//! date, and how to independently re-check every fingerprint.

use crate::armor::split_armor_blocks;
use pgp::composed::{Deserializable, SignedPublicKey};

const BITCOIN_CORE_BUILDER_KEYS_BUNDLE: &[u8] =
    include_bytes!("../pinned-keys/bitcoin-core-builder-keys.gpg");

/// Loads the pinned Bitcoin Core builder keys.
///
/// One pinned key (builder "kvaciral", fingerprint ending `...D00D38C3`)
/// uses the `secp256k1` curve, which `rpgp` (the pure-Rust OpenPGP
/// library used here — see DECISIONS.md for why not sequoia-openpgp)
/// doesn't support parsing. That key is silently skipped rather than
/// failing the whole bundle: excluding one key from the pinned pool only
/// *shrinks* which keys can possibly contribute a valid signature, it
/// doesn't weaken the "N distinct valid signatures required" guarantee
/// verification actually relies on (still comfortably met from the
/// remaining ~37 keys against a real release — see `verify`'s tests).
/// A build-time bug that broke parsing more broadly would still panic
/// via `must_parse_at_least` below.
pub fn bitcoin_core_builder_keys() -> Vec<SignedPublicKey> {
    let text = std::str::from_utf8(BITCOIN_CORE_BUILDER_KEYS_BUNDLE)
        .expect("embedded builder-key bundle must be valid UTF-8");
    let blocks = split_armor_blocks(text, "PUBLIC KEY BLOCK");
    let keys: Vec<SignedPublicKey> = blocks
        .iter()
        .filter_map(
            |block| match SignedPublicKey::from_armor_single(block.as_bytes()) {
                Ok((key, _headers)) => Some(key),
                Err(e) => {
                    tracing::warn!("skipping unparseable pinned builder key: {e}");
                    None
                }
            },
        )
        .collect();
    must_parse_at_least(blocks.len(), keys.len());
    keys
}

/// A parse-failure rate this high means something is structurally wrong
/// with the embedded bundle (e.g. corrupted at build time), not an
/// isolated per-key format gap — that should fail loudly, not silently
/// hand back a near-empty (and therefore useless, or worse,
/// trivially-satisfiable) pinned-key set.
fn must_parse_at_least(total_blocks: usize, parsed: usize) {
    let min_expected = total_blocks.saturating_sub(3);
    assert!(
        parsed >= min_expected,
        "only {parsed}/{total_blocks} pinned builder-key blocks parsed; expected at most a \
         couple of isolated failures, not this many — the embedded bundle is likely corrupt"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_every_pinned_key_except_the_one_known_unsupported_curve() {
        let keys = bitcoin_core_builder_keys();
        // 39 files went into the bundle; one (secp256k1) can't be parsed
        // by rpgp, and a couple map to keys already covered by another
        // file (see DECISIONS.md) -- so distinct keys land a bit under
        // 39, but should be nowhere near 1.
        assert!(keys.len() > 30, "expected ~37 keys, got {}", keys.len());
    }
}
