//! ord release download + verification against pinned SHA-256 hashes
//! (docs/SPEC.md item 1). Unlike Bitcoin Core, ord publishes no
//! maintainer-signed checksums file (VERIFY'd live, DECISIONS.md Phase
//! 4), so Nodekeeper's own pinned hash *is* the verification here, not
//! a supplement to a published one.

use crate::download::{download_with_sha256_and_progress, DownloadError};
use crate::extract::{extract_archive, ExtractError};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use thiserror::Error;

/// The pinned ord release Nodekeeper downloads and verifies -- single
/// source of truth shared by `examples/fetch_ord.rs` (CI) and the real
/// setup wizard (`src-tauri`), so there's exactly one place recording
/// this security-relevant pinned value (DECISIONS.md Phase 4).
pub const VERSION: &str = "0.29.0";

/// ord's release asset filename and the path to `ord` inside its
/// extracted archive, for the platform this binary was compiled for.
/// ord's archive layout (confirmed live, DECISIONS.md Phase 4): a
/// version-named folder one level down, like Bitcoin Core's, but with no
/// `bin/` subfolder -- the binary sits directly in `ord-<version>/`.
pub fn platform_asset_and_bin_subpath() -> (&'static str, &'static str) {
    if cfg!(target_os = "windows") {
        (
            "ord-0.29.0-x86_64-pc-windows-msvc.zip",
            if cfg!(windows) {
                "ord-0.29.0\\ord.exe"
            } else {
                "ord-0.29.0/ord.exe"
            },
        )
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            ("ord-0.29.0-aarch64-apple-darwin.tar.gz", "ord-0.29.0/ord")
        } else {
            ("ord-0.29.0-x86_64-apple-darwin.tar.gz", "ord-0.29.0/ord")
        }
    } else {
        (
            "ord-0.29.0-x86_64-unknown-linux-gnu.tar.gz",
            "ord-0.29.0/ord",
        )
    }
}

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
    download_and_verify_ord_asset_with_progress(version, asset_url, dest_dir, |_, _| {}).await
}

/// Same as `download_and_verify_ord_asset`, but reports progress (bytes
/// downloaded so far, total if known) as the download proceeds.
pub async fn download_and_verify_ord_asset_with_progress(
    version: &str,
    asset_url: &str,
    dest_dir: &Path,
    on_progress: impl FnMut(u64, Option<u64>) + Send,
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
    let actual = download_with_sha256_and_progress(asset_url, &dest_path, on_progress).await?;
    verify_checksum(&filename, expected, &actual)?;

    Ok(VerifiedOrdRelease {
        path: dest_path,
        sha256: actual,
    })
}

#[derive(Debug, Error)]
pub enum InstallError {
    #[error(transparent)]
    Verification(#[from] OrdVerificationError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("couldn't extract the downloaded archive: {0}")]
    Extract(#[from] ExtractError),
    #[error(
        "expected to find ord at {0} after extraction, but it isn't there -- the archive's \
         internal layout doesn't match what Nodekeeper expects for this platform"
    )]
    BinaryNotFoundAfterExtraction(PathBuf),
}

pub struct InstalledBinary {
    pub binary_path: PathBuf,
    pub sha256: String,
}

/// The setup wizard's actual entry point for ord: downloads the pinned
/// release for this platform (docs/SPEC.md item 1), verifies its SHA-256
/// against Nodekeeper's pinned hash, extracts it, and returns the path to
/// the `ord` binary inside. Fails closed at every step. Mirrors
/// `bitcoin_core::download_verify_and_install_bitcoin_core`'s
/// cache-reuse behavior -- see that function's doc comment for the
/// reasoning.
pub async fn download_verify_and_install_ord(
    dest_dir: &Path,
    on_progress: impl FnMut(u64, Option<u64>) + Send,
) -> Result<InstalledBinary, InstallError> {
    let (asset_name, bin_subpath) = platform_asset_and_bin_subpath();
    let extracted_dir = dest_dir.join("extracted");
    let bin_path = extracted_dir.join(bin_subpath);

    if bin_path.is_file() {
        let bytes = std::fs::read(&bin_path)?;
        return Ok(InstalledBinary {
            binary_path: dunce::canonicalize(&bin_path)?,
            sha256: hex_sha256(&bytes),
        });
    }

    std::fs::create_dir_all(dest_dir)?;
    let base_url = format!("https://github.com/ordinals/ord/releases/download/{VERSION}");
    let verified = download_and_verify_ord_asset_with_progress(
        VERSION,
        &format!("{base_url}/{asset_name}"),
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
        sha256: verified.sha256,
    })
}

fn pinned_sha256_for(version: &str, filename: &str) -> Option<&'static str> {
    PINNED_ORD_HASHES
        .iter()
        .find(|(v, f, _)| *v == version && *f == filename)
        .map(|(_, _, hash)| *hash)
}

/// Only used to report a real digest back to the caller on a cache hit
/// (see `download_verify_and_install_ord`) -- the actual trust decision
/// already happened the first time this binary was extracted, not here.
fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
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

    /// Real end-to-end coverage of the actual setup-wizard entry point:
    /// download, verify, pure-Rust extract (no `unzip` shell-out), and
    /// locate the real binary for whatever platform this test runs on.
    #[tokio::test]
    async fn download_verify_and_install_extracts_a_real_working_binary() {
        let dir = tempfile::tempdir().unwrap();
        let mut progress_calls = 0;
        let mut last_progress = (0u64, None::<u64>);
        let installed = download_verify_and_install_ord(dir.path(), |downloaded, total| {
            progress_calls += 1;
            last_progress = (downloaded, total);
        })
        .await
        .expect("a real, untampered release must install");

        assert!(installed.binary_path.is_file());
        assert!(!installed.sha256.is_empty());
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

        let installed = download_verify_and_install_ord(dir.path(), |_, _| {
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
