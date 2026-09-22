//! PGP signature verification against pinned keys. Fails closed: every
//! error variant here means "do not trust this download."

use crate::armor::split_armor_blocks;
use pgp::composed::{Deserializable, DetachedSignature, SignedPublicKey};
use pgp::types::{Fingerprint, KeyDetails};
use std::collections::HashSet;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum VerifyError {
    #[error("signature file is not valid UTF-8")]
    InvalidUtf8,
    #[error("no OpenPGP signature blocks found in the signature file")]
    NoSignatures,
    #[error(
        "only {found} of the required {required} valid signatures from pinned keys were found"
    )]
    TooFewValidSignatures { found: usize, required: usize },
}

/// Verifies `data` against every signature block in
/// `detached_sig_armored` (which may — and for Bitcoin Core's
/// `SHA256SUMS.asc`, does — contain multiple concatenated single-signer
/// blocks), checking each against every key in `pinned_keys`. Returns
/// the number of *distinct* pinned keys that produced a valid signature;
/// errors if that's below `required`.
///
/// A signature block that fails to parse, or that doesn't verify against
/// any pinned key, is silently skipped rather than treated as an error
/// on its own — that's the normal case for e.g. a builder who isn't in
/// the pinned set, or signed with an unrelated key. Only the *count* of
/// valid, pinned-key signatures at the end matters.
pub fn verify_detached_signatures(
    pinned_keys: &[SignedPublicKey],
    data: &[u8],
    detached_sig_armored: &[u8],
    required: usize,
) -> Result<usize, VerifyError> {
    let text = std::str::from_utf8(detached_sig_armored).map_err(|_| VerifyError::InvalidUtf8)?;
    let blocks = split_armor_blocks(text, "SIGNATURE");
    if blocks.is_empty() {
        return Err(VerifyError::NoSignatures);
    }

    let mut valid_fingerprints: HashSet<Fingerprint> = HashSet::new();
    for block in blocks {
        let Ok((sig, _headers)) = DetachedSignature::from_armor_single(block.as_bytes()) else {
            continue;
        };
        for key in pinned_keys {
            if sig.verify(key, data).is_ok() {
                valid_fingerprints.insert(key.fingerprint());
                break;
            }
        }
    }

    let found = valid_fingerprints.len();
    if found < required {
        return Err(VerifyError::TooFewValidSignatures { found, required });
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pinned_keys::bitcoin_core_builder_keys;

    // Real Bitcoin Core 31.1 release files, fetched during the Phase 0
    // spike (spikes/bitcoin/) and copied here so this test doesn't need
    // network access. If Bitcoin Core ever rotates past what the pinned
    // keys cover, this fixture (not the pinned keys) is what to refresh.
    const SHA256SUMS: &[u8] = include_bytes!("../test-fixtures/SHA256SUMS");
    const SHA256SUMS_ASC: &[u8] = include_bytes!("../test-fixtures/SHA256SUMS.asc");

    #[test]
    fn real_bitcoin_core_release_verifies_with_at_least_3_signatures() {
        let keys = bitcoin_core_builder_keys();
        let found = verify_detached_signatures(&keys, SHA256SUMS, SHA256SUMS_ASC, 3).unwrap();
        assert!(found >= 3);
    }

    #[test]
    fn requiring_more_signatures_than_exist_fails_closed() {
        let keys = bitcoin_core_builder_keys();
        let result = verify_detached_signatures(&keys, SHA256SUMS, SHA256SUMS_ASC, 1000);
        assert!(matches!(
            result,
            Err(VerifyError::TooFewValidSignatures { required: 1000, .. })
        ));
    }

    #[test]
    fn tampered_data_fails_closed() {
        let keys = bitcoin_core_builder_keys();
        let mut tampered = SHA256SUMS.to_vec();
        tampered.push(b'\n'); // any change invalidates every signature over it
        let result = verify_detached_signatures(&keys, &tampered, SHA256SUMS_ASC, 3);
        assert!(matches!(
            result,
            Err(VerifyError::TooFewValidSignatures { found: 0, .. })
        ));
    }

    #[test]
    fn signatures_not_from_pinned_keys_do_not_count() {
        // No pinned keys at all -> nothing can verify, regardless of how
        // good the signatures are.
        let result = verify_detached_signatures(&[], SHA256SUMS, SHA256SUMS_ASC, 1);
        assert!(matches!(
            result,
            Err(VerifyError::TooFewValidSignatures { found: 0, .. })
        ));
    }
}
