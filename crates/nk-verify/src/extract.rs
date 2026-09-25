//! Pure-Rust archive extraction -- no `unzip`/`tar` shell-out. Unlike
//! `examples/fetch_bitcoin_core.rs`/`fetch_ord.rs` (CI/dev-only helpers,
//! explicitly exempted from the rule), this module's callers run inside
//! the shipped app itself, where CLAUDE.md's "all commands through the
//! executor" rule blocks `std::process::Command`/`tokio::process::Command`
//! outside `nk-exec`/`nk-proc` -- and shelling out to `unzip`/`tar` would
//! also assume those tools are on `PATH`, which isn't guaranteed
//! (especially on Windows).

use std::fs::File;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ExtractError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("zip error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("don't know how to extract {0} -- expected a .zip or .tar.gz")]
    UnsupportedArchive(String),
}

/// Extracts `archive` (a `.zip` or `.tar.gz`, dispatched on filename) into
/// `dest_dir`, which is created if missing. Both the `zip` and `tar`
/// crates sanitize entry paths against `..`/absolute-path escapes
/// (zip-slip/tar-slip) internally, so this is safe even though it isn't
/// the actual trust boundary here -- the SHA-256 + signature check the
/// caller already ran before extracting is.
pub fn extract_archive(archive: &Path, dest_dir: &Path) -> Result<(), ExtractError> {
    std::fs::create_dir_all(dest_dir)?;
    let name = archive.to_string_lossy();
    if name.ends_with(".zip") {
        extract_zip(archive, dest_dir)
    } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        extract_tar_gz(archive, dest_dir)
    } else {
        Err(ExtractError::UnsupportedArchive(name.into_owned()))
    }
}

fn extract_zip(archive: &Path, dest_dir: &Path) -> Result<(), ExtractError> {
    let file = File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file)?;
    zip.extract(dest_dir)?;
    Ok(())
}

fn extract_tar_gz(archive: &Path, dest_dir: &Path) -> Result<(), ExtractError> {
    let file = File::open(archive)?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut tar = tar::Archive::new(decoder);
    tar.unpack(dest_dir)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn extracts_a_zip_archive_to_the_expected_path_with_content_intact() {
        let dir = tempfile::tempdir().unwrap();
        let archive_path = dir.path().join("test.zip");
        {
            let file = File::create(&archive_path).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            zip.start_file("sub/hello.txt", zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(b"hello from zip").unwrap();
            zip.finish().unwrap();
        }

        let dest = dir.path().join("out");
        extract_archive(&archive_path, &dest).unwrap();

        let extracted = std::fs::read_to_string(dest.join("sub").join("hello.txt")).unwrap();
        assert_eq!(extracted, "hello from zip");
    }

    #[test]
    fn extracts_a_tar_gz_archive_to_the_expected_path_with_content_intact() {
        let dir = tempfile::tempdir().unwrap();
        let archive_path = dir.path().join("test.tar.gz");
        {
            let file = File::create(&archive_path).unwrap();
            let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
            let mut builder = tar::Builder::new(encoder);
            let content = b"hello from tar.gz";
            let mut header = tar::Header::new_gnu();
            header.set_size(content.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(&mut header, "sub/hello.txt", &content[..])
                .unwrap();
            builder.into_inner().unwrap().finish().unwrap();
        }

        let dest = dir.path().join("out");
        extract_archive(&archive_path, &dest).unwrap();

        let extracted = std::fs::read_to_string(dest.join("sub").join("hello.txt")).unwrap();
        assert_eq!(extracted, "hello from tar.gz");
    }

    #[test]
    fn an_unrecognized_extension_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let archive_path = dir.path().join("test.rar");
        std::fs::write(&archive_path, b"not actually an archive").unwrap();

        let result = extract_archive(&archive_path, &dir.path().join("out"));
        assert!(matches!(result, Err(ExtractError::UnsupportedArchive(_))));
    }
}
