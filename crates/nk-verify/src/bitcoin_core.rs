//! Ties `download`, `pinned_keys`, and `verify` together into one
//! fail-closed Bitcoin Core release verification flow (docs/SPEC.md
//! Foundation / setup wizard item 1).

use crate::download::{download_with_sha256, expected_sha256_for, DownloadError};
use crate::pinned_keys::bitcoin_core_builder_keys;
use crate::verify::{verify_detached_signatures, VerifyError};
use std::path::Path;
use thiserror::Error;

/// docs/SPEC.md: "Require at least 3 valid signatures from pinned keys."
pub const REQUIRED_VALID_SIGNATURES: usize = 3;

#[derive(Debug, Error)]
pub enum ReleaseVerificationError {
    #[error("download failed: {0}")]
    Download(#[from] DownloadError),
    #[error("{filename} is not listed in SHA256SUMS")]
    NotInChecksumFile { filename: String },
    #[error(
        "SHA-256 mismatch for {filename}: expected {expected}, got {actual} -- the download is \
         corrupt or has been tampered with"
    )]
    ChecksumMismatch {
        filename: String,
        expected: String,
        actual: String,
    },
    #[error("SHA256SUMS.asc signature check failed: {0}")]
    SignatureVerification(#[from] VerifyError),
}

pub struct VerifiedRelease {
    pub path: std::path::PathBuf,
    pub sha256: String,
    pub valid_signatures: usize,
}

/// Downloads a Bitcoin Core release asset plus its `SHA256SUMS`/
/// `SHA256SUMS.asc`, and verifies it end to end:
/// 1. the file's SHA-256 matches its entry in `SHA256SUMS`;
/// 2. `SHA256SUMS.asc` carries >= `REQUIRED_VALID_SIGNATURES` valid
///    signatures from the embedded pinned builder keys.
///
/// Fails closed: any error variant here means the downloaded file must
/// not be used. `dest_dir` receives the binary and both checksum files;
/// the caller is responsible for cleaning up on failure if desired
/// (left in place by default so a failed verification can be inspected).
pub async fn download_and_verify_bitcoin_core_asset(
    asset_url: &str,
    sha256sums_url: &str,
    sha256sums_asc_url: &str,
    dest_dir: &Path,
) -> Result<VerifiedRelease, ReleaseVerificationError> {
    let filename = asset_url
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("download")
        .to_string();

    let dest_path = dest_dir.join(&filename);
    let actual_sha256 = download_with_sha256(asset_url, &dest_path).await?;

    let sha256sums_path = dest_dir.join("SHA256SUMS");
    download_with_sha256(sha256sums_url, &sha256sums_path).await?;
    let sha256sums_text = tokio::fs::read_to_string(&sha256sums_path)
        .await
        .map_err(DownloadError::from)?;

    let sha256sums_asc_path = dest_dir.join("SHA256SUMS.asc");
    download_with_sha256(sha256sums_asc_url, &sha256sums_asc_path).await?;
    let sha256sums_asc = tokio::fs::read(&sha256sums_asc_path)
        .await
        .map_err(DownloadError::from)?;

    verify_checksum(&filename, &sha256sums_text, &actual_sha256)?;

    let pinned_keys = bitcoin_core_builder_keys();
    let valid_signatures = verify_detached_signatures(
        &pinned_keys,
        sha256sums_text.as_bytes(),
        &sha256sums_asc,
        REQUIRED_VALID_SIGNATURES,
    )?;

    Ok(VerifiedRelease {
        path: dest_path,
        sha256: actual_sha256,
        valid_signatures,
    })
}

/// Pulled out of the async download flow so the mismatch/not-listed
/// cases can be unit-tested directly, without needing a real (or mocked)
/// network round-trip just to exercise this comparison.
fn verify_checksum(
    filename: &str,
    sha256sums_text: &str,
    actual_sha256: &str,
) -> Result<(), ReleaseVerificationError> {
    let expected_sha256 = expected_sha256_for(sha256sums_text, filename).ok_or_else(|| {
        ReleaseVerificationError::NotInChecksumFile {
            filename: filename.to_string(),
        }
    })?;
    if actual_sha256 != expected_sha256 {
        return Err(ReleaseVerificationError::ChecksumMismatch {
            filename: filename.to_string(),
            expected: expected_sha256,
            actual: actual_sha256.to_string(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real network access, real bitcoincore.org / GitHub URLs, real
    /// pinned keys -- the actual end-to-end path, not a mock. Slow-ish
    /// and needs network, but this is exactly the Phase 2 [CI]
    /// acceptance criterion ("real Bitcoin Core binaries download and
    /// verify with 3+ pinned signatures").
    #[tokio::test]
    async fn a_real_bitcoin_core_release_downloads_and_verifies() {
        let dir = tempfile::tempdir().unwrap();
        let result = download_and_verify_bitcoin_core_asset(
            "https://bitcoincore.org/bin/bitcoin-core-31.1/bitcoin-31.1-x86_64-linux-gnu.tar.gz",
            "https://bitcoincore.org/bin/bitcoin-core-31.1/SHA256SUMS",
            "https://bitcoincore.org/bin/bitcoin-core-31.1/SHA256SUMS.asc",
            dir.path(),
        )
        .await
        .expect("a real, untampered release must verify");

        assert!(result.valid_signatures >= REQUIRED_VALID_SIGNATURES);
        assert!(result.path.exists());
    }

    #[test]
    fn a_tampered_or_corrupted_download_is_rejected() {
        let sha256sums = include_str!("../test-fixtures/SHA256SUMS");
        // The real file's hash, deliberately not what a corrupted/
        // tampered download would actually hash to.
        let wrong_hash = "0".repeat(64);
        let result = verify_checksum("bitcoin-31.1-win64.zip", sha256sums, &wrong_hash);
        assert!(matches!(
            result,
            Err(ReleaseVerificationError::ChecksumMismatch { .. })
        ));
    }

    #[test]
    fn a_correct_hash_for_a_real_release_file_passes() {
        let sha256sums = include_str!("../test-fixtures/SHA256SUMS");
        let correct_hash = "c99ef173471c58e6766d9eebd12e6c35349082eeed3939bc99eed58ef57db587";
        let result = verify_checksum("bitcoin-31.1-win64.zip", sha256sums, correct_hash);
        assert!(result.is_ok());
    }

    #[test]
    fn a_filename_not_in_sha256sums_is_rejected() {
        let sha256sums = include_str!("../test-fixtures/SHA256SUMS");
        let result = verify_checksum("not-a-real-release-file.zip", sha256sums, &"a".repeat(64));
        assert!(matches!(
            result,
            Err(ReleaseVerificationError::NotInChecksumFile { .. })
        ));
    }

    #[tokio::test]
    async fn an_asset_not_listed_in_sha256sums_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let result = download_and_verify_bitcoin_core_asset(
            "https://bitcoincore.org/bin/bitcoin-core-31.1/does-not-exist-in-checksums.bin",
            "https://bitcoincore.org/bin/bitcoin-core-31.1/SHA256SUMS",
            "https://bitcoincore.org/bin/bitcoin-core-31.1/SHA256SUMS.asc",
            dir.path(),
        )
        .await;

        // The URL itself 404s for a nonexistent asset, which surfaces as
        // a Download error -- also a fail-closed outcome, just at an
        // earlier stage than NotInChecksumFile.
        assert!(result.is_err());
    }
}
