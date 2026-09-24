//! CI helper: downloads and verifies the current pinned ord release for
//! this platform using the *real* `nk_verify` verification path (not a
//! reimplementation in the CI YAML), extracts `ord`, and prints its path
//! on success. Mirrors `fetch_bitcoin_core.rs` closely -- see that file
//! for the shared reasoning (caching via a repo-relative `target/`
//! subdirectory, CI-only `disallowed-methods` exemption for the
//! unzip/tar shell-outs, printing an absolute path since `cargo test`
//! runs with each crate's own directory as cwd).
//!
//! Usage: `cargo run --release -p nk-verify --example fetch_ord`
#![allow(clippy::disallowed_methods)]

use nk_verify::ord::download_and_verify_ord_asset;

const VERSION: &str = "0.29.0";

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("fetch_ord failed: {e}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let (asset_name, bin_subpath) = platform_asset_and_binary_path();
    let base_url = format!("https://github.com/ordinals/ord/releases/download/{VERSION}");
    let dest_dir = std::path::Path::new("target").join(format!("nodekeeper-ord-{VERSION}"));
    std::fs::create_dir_all(&dest_dir)?;

    let extracted_dir = dest_dir.join("extracted");
    let bin_path = extracted_dir.join(bin_subpath);
    if bin_path.is_file() {
        let absolute = dunce::canonicalize(&bin_path)?;
        eprintln!("Using cached, previously-verified {}", absolute.display());
        println!("{}", absolute.display());
        return Ok(());
    }

    eprintln!("Downloading and verifying {asset_name}...");
    let verified =
        download_and_verify_ord_asset(VERSION, &format!("{base_url}/{asset_name}"), &dest_dir)
            .await?;
    eprintln!(
        "Verified {} (sha256: {})",
        verified.path.display(),
        verified.sha256
    );

    std::fs::create_dir_all(&extracted_dir)?;
    extract(&verified.path, &extracted_dir)?;

    if !bin_path.exists() {
        return Err(format!(
            "expected binary not found after extraction: {}",
            bin_path.display()
        )
        .into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&bin_path)?.permissions();
        perms.set_mode(perms.mode() | 0o111);
        std::fs::set_permissions(&bin_path, perms)?;
    }

    println!("{}", dunce::canonicalize(&bin_path)?.display());
    Ok(())
}

/// ord's archive layout (confirmed live, DECISIONS.md Phase 4): a
/// version-named folder one level down, like Bitcoin Core's, but with
/// no `bin/` subfolder -- the binary sits directly in `ord-<version>/`.
fn platform_asset_and_binary_path() -> (&'static str, &'static str) {
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

fn extract(archive: &std::path::Path, dest: &std::path::Path) -> std::io::Result<()> {
    let status = if archive.extension().and_then(|e| e.to_str()) == Some("zip") {
        std::process::Command::new("unzip")
            .args(["-q", "-o"])
            .arg(archive)
            .arg("-d")
            .arg(dest)
            .status()?
    } else {
        std::process::Command::new("tar")
            .args(["-xzf"])
            .arg(archive)
            .args(["-C"])
            .arg(dest)
            .status()?
    };
    if !status.success() {
        return Err(std::io::Error::other(format!(
            "extraction of {} failed",
            archive.display()
        )));
    }
    Ok(())
}
