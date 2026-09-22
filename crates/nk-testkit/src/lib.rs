//! Regtest fixture for integration tests: starts real `bitcoind` in a
//! temporary directory on random free ports, can mine blocks, and always
//! tears down (even on panic or test failure) — docs/SPEC.md
//! Implementation Guide, "Regtest test kit". Every feature that touches
//! Bitcoin Core gets at least one integration test using this.

use nk_core::{
    bitcoin_conf::generate_bitcoin_conf, system_check::run_system_check, Chain, Environment,
};
use nk_exec::Executor;
use nk_proc::BitcoindProcess;
use nk_rpc::RpcClient;
use std::net::TcpListener;
use std::path::Path;
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FixtureError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("bitcoind error: {0}")]
    Bitcoind(#[from] nk_proc::BitcoindError),
    #[error("rpc error: {0}")]
    Rpc(#[from] nk_rpc::RpcError),
    #[error("bitcoind did not become ready within the startup timeout")]
    StartupTimeout,
}

pub struct RegtestFixture {
    process: Option<BitcoindProcess>,
    pub environment: Environment,
    pub rpc: RpcClient,
    // Kept alive for the fixture's lifetime; deleted on drop.
    _tempdir: tempfile::TempDir,
}

impl RegtestFixture {
    /// Starts a fresh regtest bitcoind. `binary_path` should already be
    /// verified (`nk_verify::bitcoin_core`) by the caller in real app
    /// code; test callers typically point this at a binary downloaded
    /// once and cached for the CI run.
    pub async fn start(binary_path: &Path) -> Result<Self, FixtureError> {
        let tempdir = tempfile::tempdir()?;

        let mut environment = Environment::new_default(Chain::Regtest, tempdir.path());
        environment.rpc_port = random_free_port()?;
        environment.p2p_port = random_free_port()?;

        let datadir = environment.bitcoin_datadir_arg();
        std::fs::create_dir_all(&datadir)?;
        let system = run_system_check(tempdir.path());
        let conf = generate_bitcoin_conf(
            Chain::Regtest,
            environment.rpc_port,
            environment.p2p_port,
            system.available_memory_bytes,
            0,
        );
        std::fs::write(datadir.join("bitcoin.conf"), conf)?;

        let process = BitcoindProcess::start(binary_path, &environment).await?;

        wait_for_cookie(&environment).await?;
        let rpc = RpcClient::from_cookie_file(
            format!("http://127.0.0.1:{}", environment.rpc_port),
            &environment.bitcoin_cookie_path(),
            Executor::new(),
            "regtest".to_string(),
            Chain::Regtest,
        )?;
        wait_for_rpc_ready(&rpc).await?;

        Ok(Self {
            process: Some(process),
            environment,
            rpc,
            _tempdir: tempdir,
        })
    }

    /// Mines `n` blocks to a fresh address in the node's own wallet
    /// (creating a default wallet first if none exists yet).
    pub async fn mine_blocks(&self, n: u32) -> Result<(), FixtureError> {
        let address = match self.rpc.get_new_address().await {
            Ok(addr) => addr,
            Err(_) => {
                // No wallet loaded yet -- create one and retry once.
                self.rpc
                    .call(
                        "createwallet",
                        vec![serde_json::json!("test")],
                        "test setup",
                    )
                    .await?;
                self.rpc.get_new_address().await?
            }
        };
        self.rpc.generate_to_address(n, &address).await?;
        Ok(())
    }

    /// Graceful stop (docs/SPEC.md Foundation C), consuming the fixture.
    /// Prefer this over letting the fixture just drop when a test wants
    /// to assert the stop itself succeeded cleanly.
    pub async fn stop(mut self) -> Result<(), FixtureError> {
        if let Some(process) = self.process.take() {
            process.stop(&self.rpc, Duration::from_secs(30)).await?;
        }
        Ok(())
    }
}

impl Drop for RegtestFixture {
    fn drop(&mut self) {
        // Best-effort force-kill if `stop()` was never called (e.g. the
        // test panicked) -- Drop can't be async, so this can't be the
        // graceful RPC-stop path; it only guarantees no orphaned process
        // survives the test.
        if let Some(mut process) = self.process.take() {
            process.kill_sync();
        }
    }
}

fn random_free_port() -> std::io::Result<u16> {
    Ok(TcpListener::bind("127.0.0.1:0")?.local_addr()?.port())
}

async fn wait_for_cookie(environment: &Environment) -> Result<(), FixtureError> {
    let cookie_path = environment.bitcoin_cookie_path();
    poll_until(Duration::from_secs(30), Duration::from_millis(100), || {
        cookie_path.exists()
    })
    .await
}

async fn wait_for_rpc_ready(rpc: &RpcClient) -> Result<(), FixtureError> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        if rpc.get_blockchain_info().await.is_ok() {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(FixtureError::StartupTimeout);
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

async fn poll_until(
    timeout: Duration,
    interval: Duration,
    mut condition: impl FnMut() -> bool,
) -> Result<(), FixtureError> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if condition() {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(FixtureError::StartupTimeout);
        }
        tokio::time::sleep(interval).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The actual Phase 2 [CI] acceptance criterion: "regtest bitcoind
    /// starts, mines 101 blocks, and stops cleanly." Uses a real
    /// bitcoind binary -- set `NK_TEST_BITCOIND` to its path (CI caches
    /// a verified download there); skipped locally if unset rather than
    /// failing, so `cargo test` doesn't require a pre-staged binary on
    /// every dev machine.
    #[tokio::test]
    async fn starts_mines_101_blocks_and_stops_cleanly() {
        let Some(binary_path) = std::env::var_os("NK_TEST_BITCOIND") else {
            eprintln!("skipping: NK_TEST_BITCOIND not set");
            return;
        };
        let binary_path = std::path::PathBuf::from(binary_path);

        let fixture = RegtestFixture::start(&binary_path)
            .await
            .expect("bitcoind should start");
        fixture
            .mine_blocks(101)
            .await
            .expect("mining 101 blocks should succeed");

        let info = fixture
            .rpc
            .get_blockchain_info()
            .await
            .expect("node should respond to RPC");
        assert_eq!(info.get("blocks").and_then(|v| v.as_u64()), Some(101));

        fixture.stop().await.expect("bitcoind should stop cleanly");
    }

    #[tokio::test]
    async fn a_fixture_dropped_without_stop_does_not_leave_an_orphan() {
        let Some(binary_path) = std::env::var_os("NK_TEST_BITCOIND") else {
            eprintln!("skipping: NK_TEST_BITCOIND not set");
            return;
        };
        let binary_path = std::path::PathBuf::from(binary_path);

        let pid = {
            let fixture = RegtestFixture::start(&binary_path)
                .await
                .expect("bitcoind should start");
            fixture.process.as_ref().unwrap().pid
        };
        // fixture dropped here without calling stop() -- Drop's
        // kill_sync() must have terminated it.
        tokio::time::sleep(Duration::from_millis(500)).await;

        let mut sys = sysinfo::System::new();
        sys.refresh_all();
        assert!(
            sys.process(sysinfo::Pid::from_u32(pid)).is_none(),
            "bitcoind (pid {pid}) should not survive an un-stopped fixture being dropped"
        );
    }
}
