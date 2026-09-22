//! CI helper: downloads and verifies the current pinned Bitcoin Core
//! release for this platform using the *real* `nk_verify` verification
//! path (not a reimplementation in the CI YAML), extracts `bitcoind`,
//! and prints its path on success. Exits non-zero on any verification
//! failure — fails closed, same as the library it's calling.
//!
//! Usage: `cargo run --release -p nk-verify --example fetch_bitcoin_core`
//!
//! Shells out to `unzip`/`tar` to extract the (already verified) archive
//! — the disallowed-methods rule (docs/SPEC.md Foundation B: commands
//! only through nk-exec/nk-proc) is about the *shipped app's* runtime
//! behavior; this is a CI/dev-only build helper, never bundled or run by
//! Nodekeeper itself.
#![allow(clippy::disallowed_methods)]

use nk_verify::bitcoin_core::download_and_verify_bitcoin_core_asset;

const VERSION: &str = "31.1";

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("fetch_bitcoin_core failed: {e}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let (asset_name, bin_subpath) = platform_asset_and_binary_path();
    let base_url = format!("https://bitcoincore.org/bin/bitcoin-core-{VERSION}");
    // A fixed, repo-relative location (not the OS temp dir) so CI's
    // actions/cache path and this example's actual output directory are
    // unambiguously the same thing -- already covered by the top-level
    // .gitignore (everything under target/ is ignored).
    let dest_dir =
        std::path::Path::new("target").join(format!("nodekeeper-bitcoin-core-{VERSION}"));
    std::fs::create_dir_all(&dest_dir)?;

    // If a prior run already got this far (typically: actions/cache
    // restored it), the binary is already sitting there extracted --
    // skip the network round-trip and re-verification entirely. Only
    // reachable via this exact path if a previous run of *this same
    // script* got all the way through a successful verify+extract, so
    // its presence is real evidence, not just an assumption.
    let extracted_dir = dest_dir.join("extracted");
    let bin_path = extracted_dir.join(bin_subpath);
    if bin_path.is_file() {
        let absolute = dunce::canonicalize(&bin_path)?;
        eprintln!("Using cached, previously-verified {}", absolute.display());
        println!("{}", absolute.display());
        return Ok(());
    }

    eprintln!("Downloading and verifying {asset_name}...");
    let verified = download_and_verify_bitcoin_core_asset(
        &format!("{base_url}/{asset_name}"),
        &format!("{base_url}/SHA256SUMS"),
        &format!("{base_url}/SHA256SUMS.asc"),
        &dest_dir,
    )
    .await?;
    eprintln!(
        "Verified {} ({} valid signature(s) from pinned keys)",
        verified.path.display(),
        verified.valid_signatures
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

    // The one line of intentional stdout output: the verified binary's
    // *absolute* path, for the CI step to capture -- `cargo test` runs
    // each crate's tests with cwd set to that crate's own directory, not
    // the workspace root, so a relative path here would resolve
    // differently (and wrongly) once nk-testkit's tests read it back via
    // NK_TEST_BITCOIND.
    println!("{}", dunce::canonicalize(&bin_path)?.display());
    Ok(())
}

fn platform_asset_and_binary_path() -> (&'static str, &'static str) {
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
