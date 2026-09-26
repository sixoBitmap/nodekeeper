//! How a wallet is made ready to sign (Phase 10 step 1(b)), against a real
//! Bitcoin Core and a real `ord`:
//!
//! - **VERIFY**: `getwalletinfo` has `unlocked_until` only for a wallet that is
//!   encrypted (absent when created without a passphrase, present while an
//!   encrypted one is locked) -- what `wallet_protection` relies on;
//! - an **unencrypted wallet on a test chain** is ready to sign with no
//!   passphrase (it used to dead-end asking for one that does not exist) and a
//!   real `ord wallet send` then works;
//! - an **unencrypted wallet on mainnet is refused**, whatever passphrase is
//!   offered (fail closed);
//! - an **encrypted wallet** needs its passphrase (none / wrong / right), and
//!   is locked again afterwards;
//! - the passphrase never reaches the event stream.

use nk_core::Chain;
use nk_exec::Executor;
use nk_rpc::{RpcClient, RpcError, SigningUnlock, UnlockError, WalletProtection};
use serde_json::json;
use serial_test::serial;
use std::path::PathBuf;
use std::time::Duration;

const PASSPHRASE: &str = "unlock-test-passphrase-7391";

/// A client for the fixture's node, labelled as `chain` (the label is all
/// that differs: it decides whether an unencrypted wallet is refused). Bitcoin
/// Core is regtest either way.
fn client_as(fixture: &nk_testkit::RegtestFixture, executor: &Executor, chain: Chain) -> RpcClient {
    RpcClient::from_cookie_file(
        format!("http://127.0.0.1:{}", fixture.environment.rpc_port),
        &fixture.environment.bitcoin_cookie_path(),
        executor.clone(),
        "regtest".to_string(),
        chain,
    )
    .unwrap()
}

/// `unlocked_until` as `getwalletinfo` reports it for `wallet`, read through a
/// client aimed at that wallet's own endpoint (independent of the code under
/// test, which only looks at whether the field is present).
async fn unlocked_until(
    fixture: &nk_testkit::RegtestFixture,
    executor: &Executor,
    wallet: &str,
) -> Option<i64> {
    let client = RpcClient::from_cookie_file(
        format!(
            "http://127.0.0.1:{}/wallet/{wallet}",
            fixture.environment.rpc_port
        ),
        &fixture.environment.bitcoin_cookie_path(),
        executor.clone(),
        "regtest".to_string(),
        Chain::Regtest,
    )
    .unwrap();
    let info = client
        .call("getwalletinfo", vec![], "test", vec![], true)
        .await
        .expect("getwalletinfo");
    info.get("unlocked_until").and_then(|v| v.as_i64())
}

#[tokio::test]
#[serial(real_bitcoind)]
async fn a_wallet_is_ready_to_sign_according_to_its_encryption_and_the_chain() {
    let Some(bitcoind_path) = nk_core::live_tests::live_binary("NK_TEST_BITCOIND") else {
        eprintln!("skipping: NK_TEST_BITCOIND not set");
        return;
    };
    let fixture = nk_testkit::RegtestFixture::start(&PathBuf::from(bitcoind_path))
        .await
        .expect("bitcoind should start");
    let executor = Executor::new();
    let mut events = executor.subscribe();
    let regtest = client_as(&fixture, &executor, Chain::Regtest);
    let mainnet_label = client_as(&fixture, &executor, Chain::Mainnet);

    // Two wallets: one created without a passphrase, one encrypted.
    for name in ["plain", "sealed"] {
        regtest
            .call(
                "createwallet",
                vec![json!(name)],
                "test setup",
                vec![],
                true,
            )
            .await
            .unwrap_or_else(|e| panic!("createwallet {name}: {e}"));
    }
    regtest
        .encrypt_wallet("sealed", PASSPHRASE)
        .await
        .expect("encryptwallet");

    // --- VERIFY: the field `wallet_protection` relies on -----------------
    let help = regtest
        .call("help", vec![json!("getwalletinfo")], "test", vec![], true)
        .await
        .expect("help getwalletinfo");
    // Core's own words for the field (31.1): "(numeric, optional) the UNIX epoch
    // time until which the wallet is unlocked for transfers, or 0 if the wallet
    // is locked (only present for passphrase-encrypted wallets)". If a newer
    // Core changes that, this fails -- and so must the encryption check.
    let help = help.as_str().expect("help returns text");
    assert!(
        help.contains("unlocked_until")
            && help.contains("only present for passphrase-encrypted wallets"),
        "{help}"
    );
    assert_eq!(
        regtest.wallet_protection("plain").await.unwrap(),
        WalletProtection::Unencrypted
    );
    assert_eq!(unlocked_until(&fixture, &executor, "plain").await, None);
    assert_eq!(
        regtest.wallet_protection("sealed").await.unwrap(),
        WalletProtection::Encrypted
    );
    assert_eq!(
        unlocked_until(&fixture, &executor, "sealed").await,
        Some(0),
        "an encrypted wallet that is locked reports unlocked_until = 0"
    );

    // --- unencrypted: test chain ready, mainnet refused ------------------
    assert_eq!(
        regtest.unlock_for_signing("plain", None, 60).await.unwrap(),
        SigningUnlock::NotNeeded,
        "an unencrypted test-chain wallet needs no passphrase"
    );
    assert_eq!(
        regtest
            .unlock_for_signing("plain", Some("ignored"), 60)
            .await
            .unwrap(),
        SigningUnlock::NotNeeded,
        "a passphrase offered for a wallet that has none is not used"
    );
    for offered in [None, Some("anything"), Some(PASSPHRASE)] {
        let refused = mainnet_label.unlock_for_signing("plain", offered, 60).await;
        assert!(
            matches!(refused, Err(UnlockError::MainnetWalletNotEncrypted)),
            "an unencrypted mainnet wallet is refused ({offered:?}): {refused:?}"
        );
    }

    // --- encrypted: none / wrong / right, and locked again ---------------
    let none = regtest.unlock_for_signing("sealed", None, 60).await;
    assert!(
        matches!(none, Err(UnlockError::PassphraseRequired)),
        "{none:?}"
    );
    let wrong = regtest
        .unlock_for_signing("sealed", Some("not the passphrase"), 60)
        .await;
    assert!(
        matches!(
            wrong,
            Err(UnlockError::Rpc(RpcError::Rpc { code: -14, .. }))
        ),
        "Core's own wrong-passphrase error (-14): {wrong:?}"
    );
    assert_eq!(unlocked_until(&fixture, &executor, "sealed").await, Some(0));

    assert_eq!(
        regtest
            .unlock_for_signing("sealed", Some(PASSPHRASE), 60)
            .await
            .unwrap(),
        SigningUnlock::Unlocked
    );
    assert!(
        unlocked_until(&fixture, &executor, "sealed")
            .await
            .unwrap_or(0)
            > 0
    );
    regtest.wallet_lock("sealed").await.expect("walletlock");
    assert_eq!(unlocked_until(&fixture, &executor, "sealed").await, Some(0));

    // The mainnet rule refuses an *unencrypted* wallet; an encrypted one is
    // unlocked on mainnet exactly as anywhere else.
    assert_eq!(
        mainnet_label
            .unlock_for_signing("sealed", Some(PASSPHRASE), 60)
            .await
            .unwrap(),
        SigningUnlock::Unlocked
    );
    regtest.wallet_lock("sealed").await.unwrap();

    // --- the passphrase never reached the event stream --------------------
    let mut seen = Vec::new();
    // (triggering action, background flag, display) of every command started.
    let mut started = Vec::new();
    while let Ok(event) = events.try_recv() {
        match event {
            nk_exec::ExecEvent::Started {
                command_display,
                triggering_action,
                background,
                ..
            } => {
                started.push((triggering_action, background, command_display.clone()));
                seen.push(command_display);
            }
            nk_exec::ExecEvent::Output { chunk, .. } => seen.push(chunk),
            nk_exec::ExecEvent::Finished { .. } => {}
        }
    }
    // The check is recorded through the executor under its OWN action name (the
    // test's own helper reads getwalletinfo too, as "test" / background, so
    // look for the app's).
    assert!(
        started.iter().any(|(action, background, display)| {
            action == "check wallet encryption" && !background && display.ends_with("getwalletinfo")
        }),
        "the encryption check is shown in the monitor like any other call: {started:?}"
    );
    // Positive controls for the leak scan: the unlock and encrypt calls really
    // were recorded, with the passphrase hidden.
    for action in ["unlock wallet", "encrypt wallet"] {
        assert!(
            started
                .iter()
                .any(|(a, _, display)| a == action && display.contains("[redacted]")),
            "{action} was recorded with the passphrase hidden: {started:?}"
        );
    }
    for text in &seen {
        assert!(
            !text.contains(PASSPHRASE),
            "event leaks the passphrase: {text}"
        );
        assert!(
            !text.contains("not the passphrase"),
            "event leaks a wrong passphrase: {text}"
        );
    }

    fixture.stop().await.expect("bitcoind should stop cleanly");
}

/// The dead end this step removes, end to end with the real thing: an
/// unencrypted regtest wallet (what the Wallet screen creates on a test
/// chain) signs a real `ord wallet send` with no passphrase; once encrypted,
/// the same send needs the passphrase and the wallet is locked again after.
#[tokio::test]
#[serial(real_bitcoind)]
async fn an_unencrypted_test_chain_wallet_can_send_and_an_encrypted_one_needs_its_passphrase() {
    let (Some(bitcoind_path), Some(ord_path)) = (
        nk_core::live_tests::live_binary("NK_TEST_BITCOIND"),
        nk_core::live_tests::live_binary("NK_TEST_ORD"),
    ) else {
        eprintln!("skipping: NK_TEST_BITCOIND and/or NK_TEST_ORD not set");
        return;
    };
    let ord_path = PathBuf::from(ord_path);
    let mut fixture = nk_testkit::RegtestFixture::start(&PathBuf::from(bitcoind_path))
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
    let rpc = client_as(&fixture, &executor, Chain::Regtest);

    nk_ord::wallet::create_wallet(&executor, &target)
        .await
        .expect("wallet create");
    let receive = nk_ord::wallet::wallet_receive(&executor, &target, None)
        .await
        .expect("wallet receive");
    let address = receive["addresses"][0].as_str().unwrap().to_string();
    fixture
        .rpc
        .generate_to_address(101, &address)
        .await
        .expect("mining to fund the wallet");
    nk_proc::wait_until_caught_up(
        fixture.ord.as_ref().unwrap(),
        &fixture.rpc,
        Duration::from_secs(30),
    )
    .await
    .expect("ord should catch up");

    // Unencrypted: ready with no passphrase, and a real send works.
    assert_eq!(
        rpc.unlock_for_signing("ord", None, 60).await.unwrap(),
        SigningUnlock::NotNeeded
    );
    nk_ord::wallet::wallet_send(&executor, &target, &address, "1btc", 2.0, false)
        .await
        .expect("a real send from an unencrypted wallet, no unlock");

    // Confirm that send and let ord index it: ord spends from its own view of
    // the wallet's outputs, so a second send right behind the first would
    // reference an output ord has not yet seen spent.
    fixture.rpc.generate_to_address(1, &address).await.unwrap();
    nk_proc::wait_until_caught_up(
        fixture.ord.as_ref().unwrap(),
        &fixture.rpc,
        Duration::from_secs(30),
    )
    .await
    .expect("ord should catch up after the send");

    // Encrypted: the passphrase is required; unlocked it sends; locked after.
    rpc.encrypt_wallet("ord", PASSPHRASE)
        .await
        .expect("encryptwallet");
    assert!(matches!(
        rpc.unlock_for_signing("ord", None, 60).await,
        Err(UnlockError::PassphraseRequired)
    ));
    assert_eq!(
        rpc.unlock_for_signing("ord", Some(PASSPHRASE), 60)
            .await
            .unwrap(),
        SigningUnlock::Unlocked
    );
    nk_ord::wallet::wallet_send(&executor, &target, &address, "1btc", 2.0, false)
        .await
        .expect("a real send once unlocked");
    rpc.wallet_lock("ord").await.unwrap();
    fixture.rpc.generate_to_address(1, &address).await.unwrap();
    nk_proc::wait_until_caught_up(
        fixture.ord.as_ref().unwrap(),
        &fixture.rpc,
        Duration::from_secs(30),
    )
    .await
    .expect("ord should catch up after the second send");
    let locked =
        nk_ord::wallet::wallet_send(&executor, &target, &address, "1btc", 2.0, false).await;
    assert!(
        locked.is_err(),
        "after the re-lock a send fails again: {locked:?}"
    );

    fixture
        .stop_ord()
        .await
        .expect("ord should stop gracefully");
    fixture.stop().await.expect("bitcoind should stop cleanly");
}
