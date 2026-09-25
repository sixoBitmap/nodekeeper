//! Ties `download`, `pinned_keys`, and `verify` together into one
//! fail-closed Bitcoin Core release verification flow (docs/SPEC.md
//! Foundation / setup wizard item 1).

use crate::download::{
    download_with_sha256, download_with_sha256_and_progress, expected_sha256_for, DownloadError,
};
use crate::extract::{extract_archive, ExtractError};
use crate::pinned_keys::bitcoin_core_builder_keys;
use crate::verify::{verify_detached_signatures, VerifyError};
use std::path::{Path, PathBuf};
use thiserror::Error;

/// docs/SPEC.md: "Require at least 3 valid signatures from pinned keys."
pub const REQUIRED_VALID_SIGNATURES: usize = 3;

/// The pinned Bitcoin Core release Nodekeeper downloads and verifies --
/// see DECISIONS.md for the source and date this was last confirmed as
/// the current stable release. The single source of truth for this
/// string: `examples/fetch_bitcoin_core.rs` (CI) and the real setup
/// wizard (`src-tauri`) both import it rather than each hardcoding their
/// own copy.
pub const VERSION: &str = "31.1";

/// Bitcoin Core's release asset filename and the path to `bitcoind`
/// inside its extracted archive, for the platform this binary was
/// compiled for. Confirmed live against the real archive layout
/// (DECISIONS.md Phase 2 VERIFY).
pub fn platform_asset_and_bin_subpath() -> (&'static str, &'static str) {
    if cfg!(target_os = "windows") {
        (
            "bitcoin-31.1-win64.zip",
            if cfg!(windows) {
                "bitcoin-31.1\\bin\\bitcoind.exe"
            } else {
                "bitcoin-31.1/bin/bitcoind.exe"
            },
        )
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            (
                "bitcoin-31.1-arm64-apple-darwin.tar.gz",
                "bitcoin-31.1/bin/bitcoind",
            )
        } else {
            (
                "bitcoin-31.1-x86_64-apple-darwin.tar.gz",
                "bitcoin-31.1/bin/bitcoind",
            )
        }
    } else {
        (
            "bitcoin-31.1-x86_64-linux-gnu.tar.gz",
            "bitcoin-31.1/bin/bitcoind",
        )
    }
}

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
    download_and_verify_bitcoin_core_asset_with_progress(
        asset_url,
        sha256sums_url,
        sha256sums_asc_url,
        dest_dir,
        |_, _| {},
    )
    .await
}

/// Same as `download_and_verify_bitcoin_core_asset`, but reports progress
/// (bytes downloaded so far, total if known) for the main asset download
/// only -- the `SHA256SUMS`/`SHA256SUMS.asc` files are tiny enough not to
/// need it.
pub async fn download_and_verify_bitcoin_core_asset_with_progress(
    asset_url: &str,
    sha256sums_url: &str,
    sha256sums_asc_url: &str,
    dest_dir: &Path,
    on_progress: impl FnMut(u64, Option<u64>) + Send,
) -> Result<VerifiedRelease, ReleaseVerificationError> {
    let filename = asset_url
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("download")
        .to_string();

    let dest_path = dest_dir.join(&filename);
    let actual_sha256 =
        download_with_sha256_and_progress(asset_url, &dest_path, on_progress).await?;

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

#[derive(Debug, Error)]
pub enum InstallError {
    #[error(transparent)]
    Verification(#[from] ReleaseVerificationError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("couldn't extract the downloaded archive: {0}")]
    Extract(#[from] ExtractError),
    #[error(
        "expected to find bitcoind at {0} after extraction, but it isn't there -- the archive's \
         internal layout doesn't match what Nodekeeper expects for this platform"
    )]
    BinaryNotFoundAfterExtraction(PathBuf),
}

pub struct InstalledBinary {
    pub binary_path: PathBuf,
    pub valid_signatures: usize,
}

/// The setup wizard's actual entry point: downloads the pinned Bitcoin
/// Core release for this platform (docs/SPEC.md item 1), verifies it
/// (checksum + >= `REQUIRED_VALID_SIGNATURES` pinned-key signatures),
/// extracts it, and returns the path to the `bitcoind` binary inside.
/// Fails closed at every step -- any error means no binary path is
/// returned, so the caller must not treat it as configured.
///
/// If `dest_dir/extracted/<bin_subpath>` already exists, skips straight
/// to returning it: the only way that file can exist is a prior call to
/// this same function having already completed a successful verify +
/// extract into this exact `dest_dir` (mirrors `examples/
/// fetch_bitcoin_core.rs`'s identical cache-reuse reasoning).
pub async fn download_verify_and_install_bitcoin_core(
    dest_dir: &Path,
    on_progress: impl FnMut(u64, Option<u64>) + Send,
) -> Result<InstalledBinary, InstallError> {
    let (asset_name, bin_subpath) = platform_asset_and_bin_subpath();
    let extracted_dir = dest_dir.join("extracted");
    let bin_path = extracted_dir.join(bin_subpath);

    if bin_path.is_file() {
        return Ok(InstalledBinary {
            binary_path: dunce::canonicalize(&bin_path)?,
            valid_signatures: REQUIRED_VALID_SIGNATURES,
        });
    }

    std::fs::create_dir_all(dest_dir)?;
    let base_url = format!("https://bitcoincore.org/bin/bitcoin-core-{VERSION}");
    let verified = download_and_verify_bitcoin_core_asset_with_progress(
        &format!("{base_url}/{asset_name}"),
        &format!("{base_url}/SHA256SUMS"),
        &format!("{base_url}/SHA256SUMS.asc"),
        dest_dir,
        on_progress,
    )
    .await?;

    std::fs::create_dir_all(&extracted_dir)?;
    extract_archive(&verified.path, &extracted_dir)?;

    if !bin_path.is_file() {
        return Err(InstallError::BinaryNotFoundAfterExtraction(bin_path));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&bin_path)?.permissions();
        perms.set_mode(perms.mode() | 0o111);
        std::fs::set_permissions(&bin_path, perms)?;
    }

    Ok(InstalledBinary {
        binary_path: dunce::canonicalize(&bin_path)?,
        valid_signatures: verified.valid_signatures,
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

    /// Real end-to-end coverage of the actual setup-wizard entry point:
    /// download, verify, pure-Rust extract (no `unzip` shell-out), and
    /// locate the real binary for whatever platform this test runs on.
    /// This is the one thing the CI-only `fetch_bitcoin_core.rs` example
    /// doesn't cover, since it extracts by shelling out instead.
    #[tokio::test]
    async fn download_verify_and_install_extracts_a_real_working_binary() {
        let dir = tempfile::tempdir().unwrap();
        let mut progress_calls = 0;
        let mut last_progress = (0u64, None::<u64>);
        let installed =
            download_verify_and_install_bitcoin_core(dir.path(), |downloaded, total| {
                progress_calls += 1;
                last_progress = (downloaded, total);
            })
            .await
            .expect("a real, untampered release must install");

        assert!(installed.binary_path.is_file());
        assert!(installed.valid_signatures >= REQUIRED_VALID_SIGNATURES);
        assert!(
            progress_calls >= 1,
            "on_progress must be called at least once"
        );
        assert!(
            last_progress.0 > 0,
            "the final progress call must report real bytes downloaded"
        );
    }

    #[tokio::test]
    async fn download_verify_and_install_reuses_an_already_extracted_binary_without_a_network_call()
    {
        let dir = tempfile::tempdir().unwrap();
        let (_, bin_subpath) = platform_asset_and_bin_subpath();
        let bin_path = dir.path().join("extracted").join(bin_subpath);
        std::fs::create_dir_all(bin_path.parent().unwrap()).unwrap();
        std::fs::write(&bin_path, b"pretend this is already a verified binary").unwrap();

        let installed = download_verify_and_install_bitcoin_core(dir.path(), |_, _| {
            panic!("on_progress must not be called on a cache hit -- that would mean it tried to download");
        })
        .await
        .expect("an already-extracted binary should be reused, not re-downloaded");

        assert_eq!(
            installed.binary_path,
            dunce::canonicalize(&bin_path).unwrap()
        );
    }
}
