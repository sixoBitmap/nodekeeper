//! Regtest fixture for integration tests: starts real `bitcoind` in a
//! temporary directory on random free ports, can mine blocks, and always
//! tears down (even on panic or test failure) — docs/SPEC.md
//! Implementation Guide, "Regtest test kit". Every feature that touches
//! Bitcoin Core gets at least one integration test using this.

use nk_core::{
    bitcoin_conf::generate_bitcoin_conf, system_check::run_system_check, Chain, Environment,
};
use nk_exec::Executor;
use nk_proc::{BitcoindProcess, OrdProcess};
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
    /// Same "folds spawn-and-wait into one call" note as `Bitcoind`
    /// above, for ord's `start_and_wait_ready`.
    #[error("ord error: {0}")]
    Ord(#[from] nk_proc::OrdProcessError),
}

pub struct RegtestFixture {
    process: Option<BitcoindProcess>,
    ord_process: Option<OrdProcess>,
    pub environment: Environment,
    pub rpc: RpcClient,
    /// `Some` only once `start_ord` has been called — most tests only
    /// need bitcoind, so ord isn't started automatically.
    pub ord: Option<nk_ord::OrdClient>,
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
        let [rpc_port, p2p_port] = random_free_ports()?;
        environment.rpc_port = rpc_port;
        environment.p2p_port = p2p_port;
        // ord_port is deliberately *not* picked here: `start_ord` is
        // typically called well after this (post-`mine_blocks`, etc.),
        // and reserving a port that far ahead of actually using it
        // reproduced as a real `PortInUse` failure -- something else on
        // the dev machine grabbed the same ephemeral port in the gap.
        // Picking it right before use (in `start_ord`) keeps that
        // window as small as rpc_port/p2p_port's own (picked and used
        // within the same function call).

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
            ord_process: None,
            environment,
            rpc,
            ord: None,
            _tempdir: tempdir,
        })
    }

    /// Starts a real `ord server` pointed at this fixture's own
    /// already-running bitcoind (its cookie file and data directory) —
    /// only the tests that need ord call this; most don't. Waits for
    /// ord's HTTP server to answer `/status` before returning, the same
    /// "spawn and wait for readiness" shape as `start()`'s own bitcoind
    /// startup.
    pub async fn start_ord(&mut self, ord_binary_path: &Path) -> Result<(), FixtureError> {
        self.environment.ord_port = random_free_port()?;
        let cookie_path = self.environment.bitcoin_cookie_path();
        let bitcoin_datadir = self.environment.bitcoin_datadir_arg();

        let (process, client) = OrdProcess::start_and_wait_ready(
            ord_binary_path,
            &self.environment,
            &cookie_path,
            &bitcoin_datadir,
            format!("http://127.0.0.1:{}", self.environment.ord_port),
            Executor::new(),
            "regtest".to_string(),
            Duration::from_secs(30),
        )
        .await?;

        self.ord_process = Some(process);
        self.ord = Some(client);
        Ok(())
    }

    /// Graceful stop for ord (SIGINT/CTRL_BREAK, not an RPC call — ord
    /// has none), consuming only the ord half of the fixture. A test
    /// that wants to assert the graceful-stop path itself succeeded
    /// calls this explicitly, same reasoning as bitcoind's `stop()`.
    pub async fn stop_ord(&mut self) -> Result<(), FixtureError> {
        if let Some(process) = self.ord_process.take() {
            process
                .stop(&self.environment, Duration::from_secs(15))
                .await?;
        }
        self.ord = None;
        Ok(())
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
                        vec![],
                        false,
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
        // Best-effort force-kill if `stop()`/`stop_ord()` were never
        // called (e.g. the test panicked) -- Drop can't be async, so
        // this can't be the graceful stop path for either process; it
        // only guarantees no orphaned process survives the test. ord
        // first, since it depends on bitcoind still being reachable.
        if let Some(mut process) = self.ord_process.take() {
            process.kill_sync();
        }
        if let Some(mut process) = self.process.take() {
            process.kill_sync();
        }
    }
}

/// Picks 2 distinct free ports (rpc/p2p) by binding both listeners
/// *before* dropping either, then returning their ports together --
/// guarantees the pair can never collide with each other (two live
/// sockets can't share a port), unlike calling a single-port picker
/// twice in a row. A real, separate `PortInUse` race was found and
/// fixed while building this fixture's ord support (see `start_ord`'s
/// comment): reserving a port long before actually using it gives
/// something else on the machine time to grab that same ephemeral
/// port in between. rpc_port and p2p_port don't have that problem --
/// both are used immediately, in this same function -- so this helper
/// only needs to guard against the two of them landing on the same
/// port, not against that separate time-gap race.
fn random_free_ports() -> std::io::Result<[u16; 2]> {
    let listeners = [
        TcpListener::bind("127.0.0.1:0")?,
        TcpListener::bind("127.0.0.1:0")?,
    ];
    let mut ports = [0u16; 2];
    for (port, listener) in ports.iter_mut().zip(&listeners) {
        *port = listener.local_addr()?.port();
    }
    Ok(ports)
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
            .get_blockchain_info(false)
            .await
            .expect("node should respond to RPC");
        assert_eq!(info.get("blocks").and_then(|v| v.as_u64()), Some(101));

        fixture.stop().await.expect("bitcoind should stop cleanly");
    }

    /// Real-node coverage for Phase 5's wallet-unlock flow
    /// (docs/SPEC.md item 3): a signing RPC against an encrypted
    /// wallet fails cleanly while locked, `wallet_passphrase` unlocks
    /// it, the same signing RPC then succeeds, and `wallet_lock`
    /// re-locks it (confirmed by the signing RPC failing again).
    /// `sendtoaddress` stands in for any real signing action (e.g.
    /// ord's `wallet send`) -- the lock/unlock behavior is bitcoind's,
    /// not ord's (DECISIONS.md Phase 5 VERIFY).
    #[tokio::test]
    #[serial(real_bitcoind)]
    async fn wallet_unlock_and_lock_gate_a_real_signing_rpc() {
        let Some(binary_path) = std::env::var_os("NK_TEST_BITCOIND") else {
            eprintln!("skipping: NK_TEST_BITCOIND not set");
            return;
        };
        let binary_path = std::path::PathBuf::from(binary_path);

        let fixture = RegtestFixture::start(&binary_path)
            .await
            .expect("bitcoind should start");

        // Deliberately not `fixture.mine_blocks()` here: its own
        // fallback creates an (unwanted, differently-named) wallet the
        // first time no wallet exists yet, which then leaves *two*
        // wallets loaded once this test's own explicit `createwallet`
        // below runs -- reproduced live as a real
        // "Multiple wallets are loaded" RPC error. Create the named
        // wallet this test actually needs first, before anything else
        // touches the wallet subsystem.
        let wallet = "ord";
        fixture
            .rpc
            .call(
                "createwallet",
                vec![serde_json::json!(wallet)],
                "test setup",
                vec![],
                false,
            )
            .await
            .expect("createwallet should succeed");
        let address: String = serde_json::from_value(
            fixture
                .rpc
                .call("getnewaddress", vec![], "test setup", vec![], false)
                .await
                .expect("getnewaddress should succeed"),
        )
        .unwrap();
        fixture
            .rpc
            .generate_to_address(101, &address)
            .await
            .expect("mining to fund the wallet should succeed");

        fixture
            .rpc
            .encrypt_wallet(wallet, "test-passphrase-123")
            .await
            .expect("encrypt_wallet should succeed");

        let send = |fixture: &RegtestFixture, address: String| {
            let rpc = fixture.rpc.clone();
            async move {
                // Regtest has no mempool history to estimate a fee
                // from and `-fallbackfee` isn't enabled -- an explicit
                // `fee_rate` (position 10, confirmed live via
                // `bitcoin-cli help sendtoaddress`) avoids needing fee
                // estimation at all. Positions 3-9 are left at their
                // defaults via `null`.
                rpc.call(
                    "sendtoaddress",
                    vec![
                        serde_json::json!(address),
                        serde_json::json!(0.1),
                        serde_json::Value::Null,
                        serde_json::Value::Null,
                        serde_json::Value::Null,
                        serde_json::Value::Null,
                        serde_json::Value::Null,
                        serde_json::Value::Null,
                        serde_json::Value::Null,
                        serde_json::json!(1),
                    ],
                    "test send",
                    vec![],
                    false,
                )
                .await
            }
        };

        let locked_result = send(&fixture, address.clone()).await;
        assert!(
            matches!(locked_result, Err(nk_rpc::RpcError::Rpc { code: -13, .. })),
            "sending while locked should fail with bitcoind's -13 (wallet locked) error, got \
             {locked_result:?}"
        );

        fixture
            .rpc
            .wallet_passphrase(wallet, "test-passphrase-123", 60)
            .await
            .expect("wallet_passphrase should unlock the wallet");
        send(&fixture, address.clone())
            .await
            .expect("sending should succeed once unlocked");

        fixture
            .rpc
            .wallet_lock(wallet)
            .await
            .expect("wallet_lock should re-lock the wallet");
        let relocked_result = send(&fixture, address).await;
        assert!(
            matches!(
                relocked_result,
                Err(nk_rpc::RpcError::Rpc { code: -13, .. })
            ),
            "sending after wallet_lock should fail again with -13, got {relocked_result:?}"
        );

        fixture.stop().await.expect("bitcoind should stop cleanly");
    }

    /// Real end-to-end coverage for `nk-ord`'s wallet CLI wrapper
    /// (docs/SPEC.md item 3, Phase 5): create a wallet, fund it,
    /// preview a send (dry-run, needs no unlock), encrypt the wallet,
    /// confirm a real send fails while locked and succeeds once
    /// unlocked, then restore the same mnemonic under a different
    /// wallet name and confirm the restored wallet finds the same
    /// funds via a full rescan.
    #[tokio::test]
    #[serial(real_bitcoind)]
    async fn wallet_cli_create_fund_send_and_restore() {
        let (Some(bitcoind_path), Some(ord_path)) = (
            std::env::var_os("NK_TEST_BITCOIND"),
            std::env::var_os("NK_TEST_ORD"),
        ) else {
            eprintln!("skipping: NK_TEST_BITCOIND and/or NK_TEST_ORD not set");
            return;
        };
        let bitcoind_path = std::path::PathBuf::from(bitcoind_path);
        let ord_path = std::path::PathBuf::from(ord_path);

        let mut fixture = RegtestFixture::start(&bitcoind_path)
            .await
            .expect("bitcoind should start");
        fixture
            .start_ord(&ord_path)
            .await
            .expect("ord should start and become ready");

        let executor = Executor::new();
        let cookie_path = fixture.environment.bitcoin_cookie_path();
        let bitcoin_datadir = fixture.environment.bitcoin_datadir_arg();
        let server_url = format!("http://127.0.0.1:{}", fixture.environment.ord_port);
        let target = nk_ord::wallet::WalletTarget {
            binary_path: &ord_path,
            environment: &fixture.environment,
            cookie_path: &cookie_path,
            bitcoin_datadir: &bitcoin_datadir,
            server_url: &server_url,
            wallet_name: "ord",
        };

        let created = nk_ord::wallet::create_wallet(&executor, &target)
            .await
            .expect("wallet create should succeed");
        let mnemonic = created
            .get("mnemonic")
            .and_then(|v| v.as_str())
            .expect("create response should include a mnemonic")
            .to_string();

        let receive = nk_ord::wallet::wallet_receive(&executor, &target, None)
            .await
            .expect("wallet receive should succeed");
        let address = receive["addresses"][0]
            .as_str()
            .expect("receive response should include an address")
            .to_string();

        fixture
            .rpc
            .generate_to_address(101, &address)
            .await
            .expect("mining to fund the wallet should succeed");

        // ord wallet commands refuse to run while ord's index is
        // behind bitcoind -- confirmed live, reproduced as a real
        // "ord server N blocks behind bitcoind" failure from
        // `wallet_balance` immediately after mining. Exactly the
        // behavior docs/SPEC.md item 3 already warns about ("Until ord
        // is caught up, show... instead of errors").
        nk_proc::wait_until_caught_up(
            fixture.ord.as_ref().unwrap(),
            &fixture.rpc,
            Duration::from_secs(30),
        )
        .await
        .expect("ord should catch up after mining");

        let balance = nk_ord::wallet::wallet_balance(&executor, &target)
            .await
            .expect("wallet balance should succeed");
        let funded_total = balance["total"].as_u64().unwrap_or(0);
        assert!(
            funded_total > 0,
            "funded wallet should have a nonzero balance"
        );

        // Dry-run needs no unlock -- confirmed live even against a
        // *locked* wallet (DECISIONS.md Phase 5 VERIFY); here the
        // wallet isn't even encrypted yet, so this also just confirms
        // the wrapper's argument/response shape is right.
        let preview = nk_ord::wallet::wallet_send(&executor, &target, &address, "1btc", 2.0, true)
            .await
            .expect("dry-run send should succeed");
        assert!(preview.get("psbt").is_some());
        assert!(preview.get("fee").is_some());

        fixture
            .rpc
            .encrypt_wallet("ord", "test-passphrase-456")
            .await
            .expect("encrypt_wallet should succeed");

        let locked_send =
            nk_ord::wallet::wallet_send(&executor, &target, &address, "1btc", 2.0, false).await;
        assert!(
            matches!(
                locked_send,
                Err(nk_ord::wallet::WalletError::NonZeroExit { .. })
            ),
            "a real send against a locked wallet should fail, got {locked_send:?}"
        );

        fixture
            .rpc
            .wallet_passphrase("ord", "test-passphrase-456", 60)
            .await
            .expect("wallet_passphrase should unlock the wallet");
        nk_ord::wallet::wallet_send(&executor, &target, &address, "1btc", 2.0, false)
            .await
            .expect("send should succeed once unlocked");
        fixture
            .rpc
            .wallet_lock("ord")
            .await
            .expect("wallet_lock should re-lock the wallet");

        // Captured *after* the real send above (which paid a real fee,
        // even though it was a self-send) -- comparing the restored
        // wallet against this, not the earlier `funded_total`, so the
        // assertion below isn't tripped up by that fee.
        let pre_restore_balance = nk_ord::wallet::wallet_balance(&executor, &target)
            .await
            .expect("wallet balance should succeed")["total"]
            .as_u64()
            .unwrap_or(0);

        // Full rescan (`--timestamp 0`) so the restored wallet finds
        // every historical UTXO, not just ones after some cutoff --
        // proves restore genuinely recovers funds from the mnemonic
        // alone, not that it merely runs without error.
        let restored_target = nk_ord::wallet::WalletTarget {
            binary_path: &ord_path,
            environment: &fixture.environment,
            cookie_path: &cookie_path,
            bitcoin_datadir: &bitcoin_datadir,
            server_url: &server_url,
            wallet_name: "restored",
        };
        nk_ord::wallet::restore_wallet(&executor, &restored_target, &mnemonic, "0")
            .await
            .expect("restore should succeed");
        let restored_balance = nk_ord::wallet::wallet_balance(&executor, &restored_target)
            .await
            .expect("restored wallet balance should succeed");
        assert_eq!(
            restored_balance["total"].as_u64(),
            Some(pre_restore_balance),
            "restoring from the mnemonic alone should recover the same balance the \
             original wallet had"
        );

        fixture
            .stop_ord()
            .await
            .expect("ord should stop gracefully");
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
            .get_network_info(false)
            .await
            .expect("getnetworkinfo should succeed");
        assert!(network_info
            .get("connections")
            .and_then(|v| v.as_u64())
            .is_some());

        let mempool_info = fixture
            .rpc
            .get_mempool_info(false)
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

    /// The Phase 4 [CI] acceptance criteria: ord starts against a real
    /// bitcoind, indexes regtest and reports its own height via
    /// `/status`, and stops gracefully (SIGINT on macOS/Linux,
    /// `CTRL_BREAK_EVENT` on Windows) -- the real cross-platform
    /// verification Phase 0 could only do on Windows by hand.
    /// `NK_TEST_ORD` is set by CI the same way `NK_TEST_BITCOIND` is
    /// (`cargo run -p nk-verify --example fetch_ord`); skipped locally
    /// if unset.
    #[tokio::test]
    #[serial(real_bitcoind)]
    async fn ord_starts_indexes_regtest_and_stops_gracefully() {
        let (Some(bitcoind_path), Some(ord_path)) = (
            std::env::var_os("NK_TEST_BITCOIND"),
            std::env::var_os("NK_TEST_ORD"),
        ) else {
            eprintln!("skipping: NK_TEST_BITCOIND and/or NK_TEST_ORD not set");
            return;
        };
        let bitcoind_path = std::path::PathBuf::from(bitcoind_path);
        let ord_path = std::path::PathBuf::from(ord_path);

        let mut fixture = RegtestFixture::start(&bitcoind_path)
            .await
            .expect("bitcoind should start");
        fixture.mine_blocks(5).await.expect("mining should succeed");

        fixture
            .start_ord(&ord_path)
            .await
            .expect("ord should start and become ready");
        let ord_pid = fixture.ord_process.as_ref().unwrap().pid;

        // The actual Phase 4 [CI] acceptance criterion: ord indexes
        // regtest and stays caught up. A fresh regtest chain is small
        // enough that this should resolve in well under the timeout;
        // if it doesn't, that's a real regression, not flakiness.
        nk_proc::wait_until_caught_up(
            fixture.ord.as_ref().unwrap(),
            &fixture.rpc,
            Duration::from_secs(30),
        )
        .await
        .expect("ord should catch up with the node's height");

        let status = fixture
            .ord
            .as_ref()
            .unwrap()
            .status(false)
            .await
            .expect("ord should answer /status");
        assert_eq!(
            status.get("chain").and_then(|v| v.as_str()),
            Some("regtest")
        );
        assert_eq!(status.get("height").and_then(|v| v.as_u64()), Some(5));
        // All three index options default on for regtest
        // (Chain::default_index_options) and were passed through to the
        // real spawned process -- confirming ord's own report of its
        // active flags matches what Nodekeeper told it to enable.
        assert_eq!(
            status.get("sat_index").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            status.get("rune_index").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            status.get("address_index").and_then(|v| v.as_bool()),
            Some(true)
        );

        fixture
            .stop_ord()
            .await
            .expect("ord should stop gracefully");
        assert!(
            nk_proc::detect_running_ord(&fixture.environment).is_none(),
            "ord (pid {ord_pid}) should not still be detected as running after a graceful stop"
        );

        fixture.stop().await.expect("bitcoind should stop cleanly");
    }

    /// The other half of the "[CI] ord stops gracefully ... and
    /// restarts without reindexing" acceptance criterion (graceful
    /// stop itself is covered above): after a graceful stop and a
    /// restart against the *same* data directory, ord must resume from
    /// its persisted index rather than reindexing from block 0. Mines
    /// more blocks between stop and restart so this is unambiguous:
    /// ord's very first `/status` response after restart already
    /// reflects the pre-stop height (5) -- a real reindex would start
    /// back at/near 0, not jump straight there.
    #[tokio::test]
    #[serial(real_bitcoind)]
    async fn ord_restarts_from_its_persisted_index_without_reindexing() {
        let (Some(bitcoind_path), Some(ord_path)) = (
            std::env::var_os("NK_TEST_BITCOIND"),
            std::env::var_os("NK_TEST_ORD"),
        ) else {
            eprintln!("skipping: NK_TEST_BITCOIND and/or NK_TEST_ORD not set");
            return;
        };
        let bitcoind_path = std::path::PathBuf::from(bitcoind_path);
        let ord_path = std::path::PathBuf::from(ord_path);

        let mut fixture = RegtestFixture::start(&bitcoind_path)
            .await
            .expect("bitcoind should start");
        fixture.mine_blocks(5).await.expect("mining should succeed");

        fixture
            .start_ord(&ord_path)
            .await
            .expect("ord should start and become ready");
        nk_proc::wait_until_caught_up(
            fixture.ord.as_ref().unwrap(),
            &fixture.rpc,
            Duration::from_secs(30),
        )
        .await
        .expect("ord should catch up before the first stop");

        fixture
            .stop_ord()
            .await
            .expect("ord should stop gracefully");

        fixture.mine_blocks(5).await.expect("mining should succeed");

        fixture
            .start_ord(&ord_path)
            .await
            .expect("ord should restart against the same data directory");

        let status_right_after_restart = fixture
            .ord
            .as_ref()
            .unwrap()
            .status(false)
            .await
            .expect("ord should answer /status right after restart");
        let height_right_after_restart = status_right_after_restart
            .get("height")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        assert!(
            height_right_after_restart >= 5,
            "ord's height right after restart was {height_right_after_restart}, expected >= 5 \
             (its pre-stop height) -- a value near 0 would mean it reindexed from scratch \
             instead of resuming from its persisted index"
        );

        nk_proc::wait_until_caught_up(
            fixture.ord.as_ref().unwrap(),
            &fixture.rpc,
            Duration::from_secs(30),
        )
        .await
        .expect("ord should catch up to the new height after restarting");
        let final_status = fixture.ord.as_ref().unwrap().status(false).await.unwrap();
        assert_eq!(
            final_status.get("height").and_then(|v| v.as_u64()),
            Some(10)
        );

        fixture
            .stop_ord()
            .await
            .expect("ord should stop gracefully");
        fixture.stop().await.expect("bitcoind should stop cleanly");
    }

    #[tokio::test]
    #[serial(real_bitcoind)]
    async fn an_ord_process_dropped_without_stop_does_not_leave_an_orphan() {
        let (Some(bitcoind_path), Some(ord_path)) = (
            std::env::var_os("NK_TEST_BITCOIND"),
            std::env::var_os("NK_TEST_ORD"),
        ) else {
            eprintln!("skipping: NK_TEST_BITCOIND and/or NK_TEST_ORD not set");
            return;
        };
        let bitcoind_path = std::path::PathBuf::from(bitcoind_path);
        let ord_path = std::path::PathBuf::from(ord_path);

        let ord_pid = {
            let mut fixture = RegtestFixture::start(&bitcoind_path)
                .await
                .expect("bitcoind should start");
            fixture
                .start_ord(&ord_path)
                .await
                .expect("ord should start and become ready");
            fixture.ord_process.as_ref().unwrap().pid
        };
        // fixture dropped here without calling stop_ord() -- Drop's
        // kill_sync() must have terminated ord (and bitcoind).
        tokio::time::sleep(Duration::from_millis(500)).await;

        let mut sys = sysinfo::System::new();
        sys.refresh_all();
        assert!(
            sys.process(sysinfo::Pid::from_u32(ord_pid)).is_none(),
            "ord (pid {ord_pid}) should not survive an un-stopped fixture being dropped"
        );
    }
}
