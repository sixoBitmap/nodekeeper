//! Manual verification helper: does the real `bitcoind` (and `ord`) start
//! on one chain using *Nodekeeper's own* generated config and process
//! manager, reach the right network, find peers, and stop cleanly? Started
//! as the Phase 2 [MANUAL] mainnet check ("mainnet bitcoind starts,
//! connects to peers, and stops cleanly (no full sync required)");
//! generalised in Phase 10 step 2 because only `[regtest]` and `[main]`
//! had ever been run live, while the `[signet]` / `[testnet4]` section
//! names and ord's `--signet` / `--testnet4` handling were only assumed.
//! Not part of the shipped app -- a one-off tool, so this check exercises
//! Nodekeeper's own conf generation and process manager instead of
//! hand-rolling bitcoin.conf and CLI flags separately.
//!
//! Usage (from the repo root):
//!   cargo run -p nk-testkit --example chain_smoke_test -- \
//!       <mainnet|signet|testnet4> <path-to-bitcoind> [path-to-ord]
//!
//! Get verified binaries first with:
//!   cargo run -p nk-verify --example fetch_bitcoin_core
//!   cargo run -p nk-verify --example fetch_ord
//! (the same bitcoind binary works for any chain).
//!
//! Uses a temp data directory (deleted on exit) and the chain's default
//! ports. Waits up to three minutes for at least one peer, watches the
//! sync for a few seconds, then (if an ord path is given) starts ord on
//! top of the still-syncing node to record how it behaves, and stops
//! everything gracefully. Needs real internet access to that network's
//! P2P; does not wait for any sync to finish, and stops after a few
//! seconds of it, so the download is small.

use nk_core::{
    bitcoin_conf::generate_bitcoin_conf, system_check::run_system_check, Chain, Environment,
};
use nk_exec::Executor;
use nk_proc::{BitcoindProcess, OrdProcess};
use nk_rpc::RpcClient;
use std::path::Path;
use std::time::{Duration, Instant};

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("chain_smoke_test FAILED: {e}");
        std::process::exit(1);
    }
}

/// What `getblockchaininfo` calls each chain (mainnet is "main").
fn rpc_chain_name(chain: Chain) -> &'static str {
    match chain {
        Chain::Mainnet => "main",
        Chain::Regtest => "regtest",
        Chain::Signet => "signet",
        Chain::Testnet4 => "testnet4",
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let usage =
        "usage: chain_smoke_test <mainnet|signet|testnet4> <path-to-bitcoind> [path-to-ord]";
    let mut args = std::env::args().skip(1);
    let chain_arg = args.next().ok_or(usage)?;
    let chain = Chain::ALL
        .iter()
        .copied()
        .find(|c| c.dir_name() == chain_arg)
        .ok_or(usage)?;
    let bitcoind_path = std::path::PathBuf::from(args.next().ok_or(usage)?);
    let ord_path = args.next().map(std::path::PathBuf::from);
    let label = chain.dir_name();

    let tempdir = tempfile::tempdir()?;
    eprintln!(
        "[{label}] temp data directory: {}",
        tempdir.path().display()
    );

    let environment = Environment::new_default(chain, tempdir.path());
    let datadir = environment.bitcoin_datadir_arg();
    std::fs::create_dir_all(&datadir)?;

    let system = run_system_check(tempdir.path());
    let conf = generate_bitcoin_conf(
        chain,
        environment.rpc_port,
        environment.p2p_port,
        system.available_memory_bytes,
        0,
    );
    std::fs::write(datadir.join("bitcoin.conf"), &conf)?;
    eprintln!("[{label}] generated bitcoin.conf:\n{conf}");

    eprintln!("[{label}] starting bitcoind and waiting for it to become ready...");
    let started = Instant::now();
    let (process, rpc) = BitcoindProcess::start_and_wait_ready(
        &bitcoind_path,
        &environment,
        format!("http://127.0.0.1:{}", environment.rpc_port),
        Executor::new(),
        label.to_string(),
        Duration::from_secs(60),
    )
    .await?;
    eprintln!(
        "[{label}] bitcoind ready after {} ms (pid {})",
        started.elapsed().as_millis(),
        process.pid
    );

    let info = rpc.get_blockchain_info(false).await?;
    let reported = info.get("chain").and_then(|c| c.as_str()).unwrap_or("?");
    eprintln!(
        "[{label}] getblockchaininfo: chain={reported:?} blocks={:?} headers={:?} \
         initialblockdownload={:?}",
        info.get("blocks"),
        info.get("headers"),
        info.get("initialblockdownload")
    );
    if reported != rpc_chain_name(chain) {
        return Err(format!(
            "bitcoind reports chain {reported:?}, expected {:?} -- the conf section or the \
             network flag selected the wrong network",
            rpc_chain_name(chain)
        )
        .into());
    }
    eprintln!("[{label}] CHECK OK: bitcoind reports the right chain ({reported})");

    eprintln!("[{label}] waiting for at least one peer connection (up to 3 minutes)...");
    let peers = wait_for_a_peer(&rpc, Duration::from_secs(180)).await?;
    eprintln!("[{label}] CHECK OK: connected to {peers} peer(s)");

    // A few seconds of sync progress, just to record that it is moving.
    for _ in 0..3 {
        tokio::time::sleep(Duration::from_secs(5)).await;
        let info = rpc.get_blockchain_info(false).await?;
        eprintln!(
            "[{label}] sync: blocks={:?} headers={:?} progress={:?}",
            info.get("blocks"),
            info.get("headers"),
            info.get("verificationprogress")
        );
    }

    if let Some(ord_path) = ord_path {
        ord_on_top(&ord_path, &environment, label).await;
    }

    eprintln!("[{label}] stopping bitcoind gracefully...");
    let t = Instant::now();
    process.stop(&rpc, Duration::from_secs(120)).await?;
    eprintln!(
        "[{label}] CHECK OK: bitcoind stopped cleanly in {} ms",
        t.elapsed().as_millis()
    );
    eprintln!("[{label}] SMOKE TEST PASSED");
    Ok(())
}

/// Starts `ord server` on the still-syncing node, using Nodekeeper's own
/// argument generation, and reports what happens -- ord has never been run
/// against signet/testnet4 here, nor against a node that has not finished
/// its initial block download. Reports rather than fails: what ord does
/// there is the finding.
async fn ord_on_top(ord_path: &Path, environment: &Environment, label: &str) {
    eprintln!("[{label}] starting ord server on the syncing node...");
    let cookie_path = environment.bitcoin_cookie_path();
    let bitcoin_datadir = environment.bitcoin_datadir_arg();
    let started = Instant::now();
    let result = OrdProcess::start_and_wait_ready(
        ord_path,
        environment,
        &cookie_path,
        &bitcoin_datadir,
        format!("http://127.0.0.1:{}", environment.ord_port),
        Executor::new(),
        label.to_string(),
        Duration::from_secs(45),
    )
    .await;
    match result {
        Ok((process, client)) => {
            eprintln!(
                "[{label}] CHECK OK: ord answered /status after {} ms",
                started.elapsed().as_millis()
            );
            match client.status(false).await {
                Ok(status) => eprintln!("[{label}] ord /status: {status}"),
                Err(e) => eprintln!("[{label}] ord /status error: {e}"),
            }
            let t = Instant::now();
            match process.stop(environment, Duration::from_secs(30)).await {
                Ok(exit) => eprintln!(
                    "[{label}] CHECK OK: ord stopped gracefully in {} ms (exit {:?})",
                    t.elapsed().as_millis(),
                    exit.code()
                ),
                Err(e) => eprintln!("[{label}] ord stop FAILED: {e}"),
            }
        }
        Err(e) => eprintln!(
            "[{label}] FINDING: ord did not become ready on the syncing node \
             (after {} ms): {e}",
            started.elapsed().as_millis()
        ),
    }
}

async fn wait_for_a_peer(
    rpc: &RpcClient,
    timeout: Duration,
) -> Result<u64, Box<dyn std::error::Error>> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let count = rpc
            .call(
                "getconnectioncount",
                vec![],
                "chain smoke test",
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
