//! Manual verification helper for the Phase 2 [MANUAL] acceptance
//! criterion "mainnet bitcoind starts, connects to peers, and stops
//! cleanly (no full sync required)" (docs/SPEC.md, PROGRESS.md). Not
//! part of the shipped app -- a one-off tool so this check exercises
//! Nodekeeper's own conf generation and process manager instead of
//! hand-rolling bitcoin.conf and CLI flags separately.
//!
//! Usage (from the repo root):
//!   cargo run -p nk-testkit --example mainnet_smoke_test -- <path-to-bitcoind>
//!
//! Get a verified binary first with:
//!   cargo run -p nk-verify --example fetch_bitcoin_core
//! (the same bitcoind binary works for any chain).
//!
//! Uses a temp data directory (deleted on exit), waits up to two minutes
//! for at least one peer connection, then stops bitcoind gracefully via
//! RPC. Needs real internet access to Bitcoin's P2P network; does not
//! wait for any block sync.

use nk_core::{
    bitcoin_conf::generate_bitcoin_conf, system_check::run_system_check, Chain, Environment,
};
use nk_exec::Executor;
use nk_proc::BitcoindProcess;
use nk_rpc::RpcClient;
use std::time::Duration;

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("mainnet_smoke_test failed: {e}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let binary_path = std::env::args()
        .nth(1)
        .ok_or("usage: mainnet_smoke_test <path-to-bitcoind>")?;
    let binary_path = std::path::PathBuf::from(binary_path);

    let tempdir = tempfile::tempdir()?;
    eprintln!("Using temp data directory: {}", tempdir.path().display());

    let environment = Environment::new_default(Chain::Mainnet, tempdir.path());
    let datadir = environment.bitcoin_datadir_arg();
    std::fs::create_dir_all(&datadir)?;

    let system = run_system_check(tempdir.path());
    let conf = generate_bitcoin_conf(
        Chain::Mainnet,
        environment.rpc_port,
        environment.p2p_port,
        system.available_memory_bytes,
        0,
    );
    std::fs::write(datadir.join("bitcoin.conf"), &conf)?;
    eprintln!("Generated bitcoin.conf:\n{conf}");

    eprintln!("Starting mainnet bitcoind and waiting for it to become ready...");
    let (process, rpc) = BitcoindProcess::start_and_wait_ready(
        &binary_path,
        &environment,
        format!("http://127.0.0.1:{}", environment.rpc_port),
        Executor::new(),
        "mainnet".to_string(),
        Duration::from_secs(60),
    )
    .await?;
    eprintln!("bitcoind started (pid {})", process.pid);

    let info = rpc.get_blockchain_info(false).await?;
    eprintln!(
        "getblockchaininfo: chain={:?} blocks={:?}",
        info.get("chain"),
        info.get("blocks")
    );

    eprintln!("Waiting for at least one peer connection (up to 2 minutes)...");
    let peer_count = wait_for_a_peer(&rpc).await?;
    eprintln!("SUCCESS: connected to {peer_count} peer(s) on mainnet.");

    eprintln!("Stopping bitcoind gracefully...");
    process.stop(&rpc, Duration::from_secs(60)).await?;
    eprintln!("bitcoind stopped cleanly. Smoke test passed.");
    Ok(())
}

async fn wait_for_a_peer(rpc: &RpcClient) -> Result<u64, Box<dyn std::error::Error>> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
    loop {
        let count = rpc
            .call(
                "getconnectioncount",
                vec![],
                "mainnet smoke test",
                vec![],
                true,
            )
            .await?
            .as_u64()
            .ok_or("getconnectioncount did not return a number")?;
        if count > 0 {
            return Ok(count);
        }
        if tokio::time::Instant::now() >= deadline {
            return Err("timed out waiting for a peer connection".into());
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}
