//! Splits a buffer containing multiple concatenated ASCII-armored OpenPGP
//! blocks into individual single-block strings.
//!
//! Needed because `pgp`'s multi-item parsers (e.g.
//! `PublicOrSecret::from_armor_many`) only read packets out of the FIRST
//! armor block in a stream and silently stop there — confirmed
//! empirically against this crate's own pinned-keys bundle (`cargo test`
//! showed 1 key parsed out of 38, with no error), not something the
//! crate's docs call out. Bitcoin Core's own multi-signer files
//! (`SHA256SUMS.asc`) and this crate's pinned builder-key bundle are
//! exactly this shape: N separate single-item blocks concatenated
//! together (confirmed: a real `SHA256SUMS.asc` has 11
//! `-----BEGIN PGP SIGNATURE-----` markers).

/// `block_type` is the armor header's block type, e.g. `"PUBLIC KEY
/// BLOCK"` or `"SIGNATURE"`.
pub fn split_armor_blocks<'a>(data: &'a str, block_type: &str) -> Vec<&'a str> {
    let begin = format!("-----BEGIN PGP {block_type}-----");
    let end = format!("-----END PGP {block_type}-----");
    let mut blocks = Vec::new();
    let mut rest = data;
    while let Some(start) = rest.find(&begin) {
        let after_begin = &rest[start..];
        match after_begin.find(&end) {
            Some(end_pos) => {
                let block_end = end_pos + end.len();
                blocks.push(&after_begin[..block_end]);
                rest = &after_begin[block_end..];
            }
            None => break,
        }
    }
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_the_real_pinned_builder_key_bundle_into_every_block() {
        let data = include_str!("../pinned-keys/bitcoin-core-builder-keys.gpg");
        let blocks = split_armor_blocks(data, "PUBLIC KEY BLOCK");
        // 39 files were concatenated to build this bundle (DECISIONS.md).
        assert_eq!(blocks.len(), 39);
        for block in &blocks {
            assert!(block.starts_with("-----BEGIN PGP PUBLIC KEY BLOCK-----"));
            assert!(block.ends_with("-----END PGP PUBLIC KEY BLOCK-----"));
        }
    }

    #[test]
    fn no_matching_blocks_returns_empty() {
        assert_eq!(
            split_armor_blocks("not armor at all", "SIGNATURE"),
            Vec::<&str>::new()
        );
    }

    #[test]
    fn an_unterminated_block_is_dropped_rather_than_returned_truncated() {
        let data = "-----BEGIN PGP SIGNATURE-----\nabc\n(no end marker)";
        assert_eq!(split_armor_blocks(data, "SIGNATURE"), Vec::<&str>::new());
    }
}
