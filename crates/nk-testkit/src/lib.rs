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
    /// Also covers bitcoind failing to become ready in time
    /// (`BitcoindError::StartupTimeout`) — `start_and_wait_ready`
    /// folds spawn-and-wait into one call, so there's no separate
    /// startup-timeout case at this layer anymore.
    #[error("bitcoind error: {0}")]
    Bitcoind(#[from] nk_proc::BitcoindError),
    #[error("rpc error: {0}")]
    Rpc(#[from] nk_rpc::RpcError),
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

        let (process, rpc) = BitcoindProcess::start_and_wait_ready(
            binary_path,
            &environment,
            format!("http://127.0.0.1:{}", environment.rpc_port),
            Executor::new(),
            "regtest".to_string(),
            // 60s, not 30s: CI runs several crates' real-bitcoind tests
            // concurrently, and windows-latest runners in particular
            // have shown real bitcoind startup taking >30s under that
            // load (not a logic bug -- see DECISIONS.md's "CI-only bug"
            // entries for this same timeout).
            Duration::from_secs(60),
        )
        .await?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    /// The actual Phase 2 [CI] acceptance criterion: "regtest bitcoind
    /// starts, mines 101 blocks, and stops cleanly." Uses a real
    /// bitcoind binary -- set `NK_TEST_BITCOIND` to its path (CI caches
    /// a verified download there); skipped locally if unset rather than
    /// failing, so `cargo test` doesn't require a pre-staged binary on
    /// every dev machine.
    ///
    /// `#[serial(real_bitcoind)]` on every test in this module that
    /// starts a real node: running several full bitcoind processes at
    /// once starved CI's windows-latest runners of enough time to
    /// become ready even at a 60s timeout (DECISIONS.md) -- one real
    /// node at a time is far cheaper than continuing to chase the
    /// timeout upward.
    #[tokio::test]
    #[serial(real_bitcoind)]
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

    /// Real-node coverage for the dashboard's RPC methods (docs/SPEC.md
    /// item 2: peers, mempool) -- their field names were VERIFY'd live
    /// against a throwaway node during development (DECISIONS.md, Phase
    /// 3); this pins the same shape against a fixture-managed node so a
    /// future bitcoind upgrade that renames a field fails a test instead
    /// of silently breaking the dashboard.
    #[tokio::test]
    #[serial(real_bitcoind)]
    async fn dashboard_rpc_methods_return_the_expected_fields() {
        let Some(binary_path) = std::env::var_os("NK_TEST_BITCOIND") else {
            eprintln!("skipping: NK_TEST_BITCOIND not set");
            return;
        };
        let binary_path = std::path::PathBuf::from(binary_path);

        let fixture = RegtestFixture::start(&binary_path)
            .await
            .expect("bitcoind should start");

        let network_info = fixture
            .rpc
            .get_network_info()
            .await
            .expect("getnetworkinfo should succeed");
        assert!(network_info
            .get("connections")
            .and_then(|v| v.as_u64())
            .is_some());

        let mempool_info = fixture
            .rpc
            .get_mempool_info()
            .await
            .expect("getmempoolinfo should succeed");
        assert!(mempool_info.get("size").and_then(|v| v.as_u64()).is_some());
        assert!(mempool_info.get("bytes").and_then(|v| v.as_u64()).is_some());

        fixture.stop().await.expect("bitcoind should stop cleanly");
    }

    #[tokio::test]
    #[serial(real_bitcoind)]
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
