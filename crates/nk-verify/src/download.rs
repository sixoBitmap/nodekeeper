//! Streams a download to disk while computing its SHA-256 digest, and
//! parses `SHA256SUMS`-style checksum files.

use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use std::path::Path;
use thiserror::Error;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Error)]
pub enum DownloadError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Downloads `url` to `dest`, returning the lowercase-hex SHA-256 digest
/// of exactly what was written to disk (computed incrementally as each
/// chunk arrives, not re-read afterward).
pub async fn download_with_sha256(url: &str, dest: &Path) -> Result<String, DownloadError> {
    let response = reqwest::get(url).await?.error_for_status()?;
    let mut stream = response.bytes_stream();
    let mut file = tokio::fs::File::create(dest).await?;
    let mut hasher = Sha256::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        hasher.update(&chunk);
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    Ok(hex_encode(&hasher.finalize()))
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Looks up `filename`'s expected hash in a `SHA256SUMS`-format text file
/// (`sha256sum` output: `<hex digest>  <filename>`, two spaces, or
/// `<hex digest> *<filename>` for binary mode). Returns `None` if the
/// filename isn't listed.
pub fn expected_sha256_for(sha256sums_text: &str, filename: &str) -> Option<String> {
    sha256sums_text.lines().find_map(|line| {
        let (hash, rest) = line.split_once(char::is_whitespace)?;
        let rest = rest.trim_start().trim_start_matches('*');
        (rest == filename).then(|| hash.to_lowercase())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA256SUMS: &str = include_str!("../test-fixtures/SHA256SUMS");

    #[test]
    fn finds_the_expected_hash_for_a_real_release_file() {
        let hash = expected_sha256_for(SHA256SUMS, "bitcoin-31.1-win64.zip").unwrap();
        assert_eq!(
            hash,
            "c99ef173471c58e6766d9eebd12e6c35349082eeed3939bc99eed58ef57db587"
        );
    }

    #[test]
    fn unknown_filename_is_none() {
        assert_eq!(expected_sha256_for(SHA256SUMS, "not-a-real-file.zip"), None);
    }

    #[test]
    fn hex_encode_matches_known_vector() {
        // SHA-256("") -- a standard test vector, independent of any
        // Bitcoin Core file.
        let digest = Sha256::digest(b"");
        assert_eq!(
            hex_encode(&digest),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
