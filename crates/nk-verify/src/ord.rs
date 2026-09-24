//! ord release download + verification against pinned SHA-256 hashes
//! (docs/SPEC.md item 1). Unlike Bitcoin Core, ord publishes no
//! maintainer-signed checksums file (VERIFY'd live, DECISIONS.md Phase
//! 4), so Nodekeeper's own pinned hash *is* the verification here, not
//! a supplement to a published one.

use crate::download::{download_with_sha256, DownloadError};
use std::path::Path;
use thiserror::Error;

/// Pinned per-platform SHA-256 hashes for ord releases Nodekeeper
/// supports. Independently computed by downloading each asset and
/// hashing it locally, then cross-checked against GitHub's reported
/// digest -- never taken from a single unverified source (DECISIONS.md
/// Phase 4, which also records the source URL and fetch date).
const PINNED_ORD_HASHES: &[(&str, &str, &str)] = &[
    (
        "0.29.0",
        "ord-0.29.0-x86_64-pc-windows-msvc.zip",
        "93de82db792ccc37ae385c49646c0f649d38049f4e959499c6e7c5d1a81bf2ad",
    ),
    (
        "0.29.0",
        "ord-0.29.0-x86_64-unknown-linux-gnu.tar.gz",
        "f65c758d71549954470aa7fe23b197478688fb4f910e84c2956cf9144078a94e",
    ),
    (
        "0.29.0",
        "ord-0.29.0-x86_64-apple-darwin.tar.gz",
        "a0085f296057563a31258402437c1182fc13bb9559826d1f5490feb4be6dbb75",
    ),
    (
        "0.29.0",
        "ord-0.29.0-aarch64-apple-darwin.tar.gz",
        "9360e97054a1d96624190634882c187126b02647a889b344cb601627ed1bd80c",
    ),
];

#[derive(Debug, Error)]
pub enum OrdVerificationError {
    #[error("download failed: {0}")]
    Download(#[from] DownloadError),
    #[error(
        "{filename} (ord {version}) is not a pinned release -- refusing to trust an unverified \
         binary rather than skipping verification"
    )]
    UnpinnedVersion { version: String, filename: String },
    #[error(
        "SHA-256 mismatch for {filename}: expected {expected}, got {actual} -- the download is \
         corrupt or has been tampered with"
    )]
    ChecksumMismatch {
        filename: String,
        expected: String,
        actual: String,
    },
}

pub struct VerifiedOrdRelease {
    pub path: std::path::PathBuf,
    pub sha256: String,
}

/// Downloads an ord release asset and verifies its SHA-256 against
/// Nodekeeper's pinned hash for `version`. Checks the pin *before*
/// downloading anything, so an unpinned version is refused without
/// spending any bandwidth on a binary that will never be trusted.
/// Fails closed: any error variant means the file must not be used.
pub async fn download_and_verify_ord_asset(
    version: &str,
    asset_url: &str,
    dest_dir: &Path,
) -> Result<VerifiedOrdRelease, OrdVerificationError> {
    let filename = asset_url
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("download")
        .to_string();

    let expected = pinned_sha256_for(version, &filename).ok_or_else(|| {
        OrdVerificationError::UnpinnedVersion {
            version: version.to_string(),
            filename: filename.clone(),
        }
    })?;

    let dest_path = dest_dir.join(&filename);
    let actual = download_with_sha256(asset_url, &dest_path).await?;
    verify_checksum(&filename, expected, &actual)?;

    Ok(VerifiedOrdRelease {
        path: dest_path,
        sha256: actual,
    })
}

fn pinned_sha256_for(version: &str, filename: &str) -> Option<&'static str> {
    PINNED_ORD_HASHES
        .iter()
        .find(|(v, f, _)| *v == version && *f == filename)
        .map(|(_, _, hash)| *hash)
}

/// Pulled out of the async download flow so the mismatch case can be
/// unit-tested directly, without a real (or mocked) network round-trip.
fn verify_checksum(
    filename: &str,
    expected: &str,
    actual: &str,
) -> Result<(), OrdVerificationError> {
    if actual != expected {
        return Err(OrdVerificationError::ChecksumMismatch {
            filename: filename.to_string(),
            expected: expected.to_string(),
            actual: actual.to_string(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real network access, the real ordinals/ord GitHub release -- the
    /// actual end-to-end path, not a mock. This is the Phase 4 [CI]
    /// acceptance criterion ("ord verifies").
    #[tokio::test]
    async fn a_real_ord_release_downloads_and_verifies() {
        let dir = tempfile::tempdir().unwrap();
        let result = download_and_verify_ord_asset(
            "0.29.0",
            "https://github.com/ordinals/ord/releases/download/0.29.0/ord-0.29.0-x86_64-unknown-linux-gnu.tar.gz",
            dir.path(),
        )
        .await
        .expect("a real, untampered release must verify");

        assert_eq!(
            result.sha256,
            "f65c758d71549954470aa7fe23b197478688fb4f910e84c2956cf9144078a94e"
        );
        assert!(result.path.exists());
    }

    #[test]
    fn an_unpinned_version_is_refused() {
        let result = pinned_sha256_for("99.99.99", "ord-99.99.99-x86_64-unknown-linux-gnu.tar.gz");
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn an_unpinned_version_is_refused_before_any_network_call() {
        let dir = tempfile::tempdir().unwrap();
        // A URL that would fail if actually requested -- proves the
        // pin check happens first, not just that *some* error occurred.
        let result = download_and_verify_ord_asset(
            "99.99.99",
            "https://this-host-does-not-exist.invalid/ord-99.99.99.tar.gz",
            dir.path(),
        )
        .await;

        assert!(matches!(
            result,
            Err(OrdVerificationError::UnpinnedVersion { .. })
        ));
    }

    #[test]
    fn a_tampered_or_corrupted_download_is_rejected() {
        let result = verify_checksum(
            "ord-0.29.0-x86_64-unknown-linux-gnu.tar.gz",
            "abc123",
            "0".repeat(64).as_str(),
        );
        assert!(matches!(
            result,
            Err(OrdVerificationError::ChecksumMismatch { .. })
        ));
    }

    #[test]
    fn a_correct_hash_passes() {
        let result = verify_checksum(
            "ord-0.29.0-x86_64-unknown-linux-gnu.tar.gz",
            "abc123",
            "abc123",
        );
        assert!(result.is_ok());
    }
}
