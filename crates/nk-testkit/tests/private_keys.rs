//! Private keys and recovery phrases must never reach the Live Command
//! Monitor, the command history or an export (CLAUDE.md, Secrets). Two
//! independent layers guard that for the raw console, and both are pinned
//! here against the real thing:
//!
//! 1. **Refuse** the commands that print them -- `ord wallet dump`,
//!    `create`, `restore` (however a leading option tries to hide the
//!    subcommand) -- *before anything runs*.
//! 2. **Backstop**: whatever does get through, the executor removes
//!    extended private keys from every line it emits, so the event stream
//!    and the history never hold one even when nobody thought to classify
//!    the command that produced it.
//! 3. **Position**: a passphrase typed as an argument is hidden by its
//!    *position* in the command, not by matching its text -- which failed
//!    for a numeric passphrase (typed with JSON quotes, rendered without).

use nk_exec::Executor;
use nk_ord::wallet::{run_console_subcommand, WalletError, WalletTarget};
use serde_json::json;
use serial_test::serial;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Layer 1, with no real binaries: the refusal must come *before* a
/// process is spawned. The binary path here does not exist, so if a
/// command were let through it would fail with a spawn error -- which is
/// exactly what the control command at the end does.
#[tokio::test]
async fn the_console_refuses_ord_commands_that_print_secrets_before_running_anything() {
    let dir = tempfile::tempdir().unwrap();
    let environment = nk_core::Environment::new_default(nk_core::Chain::Regtest, dir.path());
    let missing_binary = PathBuf::from("this-ord-does-not-exist");
    let cookie = dir.path().join(".cookie");
    let datadir = dir.path().to_path_buf();
    let target = WalletTarget {
        binary_path: &missing_binary,
        environment: &environment,
        cookie_path: &cookie,
        bitcoin_datadir: &datadir,
        server_url: "http://127.0.0.1:1",
        wallet_name: "ord",
    };
    let executor = Executor::new();

    for args in [
        &["dump"][..],
        &["--no-sync", "dump"],
        &["--name", "other", "dump"],
        &["create"],
        &["--no-sync", "create"],
        &["--server-url", "http://x", "restore"],
        &["--no-sync", "--name", "x", "restore", "--mnemonic", "a b c"],
    ] {
        let result = run_console_subcommand(
            &executor,
            &target,
            args.iter().map(|a| a.to_string()).collect(),
            "test",
        )
        .await;
        assert!(
            matches!(result, Err(WalletError::Blocked(_))),
            "{args:?} must be refused before running, got {result:?}"
        );
    }

    // Control: an ordinary read-only command is *not* refused -- it gets as
    // far as trying to launch the (missing) binary.
    let control =
        run_console_subcommand(&executor, &target, vec!["balance".to_string()], "test").await;
    assert!(
        matches!(control, Err(WalletError::Exec(_))),
        "an ordinary command must not be refused: {control:?}"
    );
}

/// Layer 2, against a real Bitcoin Core: `listdescriptors true` genuinely
/// returns private descriptors (the positive control -- the key is real and
/// the *caller* receives it), and yet the event stream and the history
/// contain none. Bypasses the console's classification on purpose, by
/// calling the RPC client directly: this is what protects the paths that
/// classification cannot see.
#[tokio::test]
#[serial(real_bitcoind)]
async fn private_descriptors_never_reach_the_event_stream_or_the_history() {
    let Some(bitcoind_path) = nk_core::live_tests::live_binary("NK_TEST_BITCOIND") else {
        eprintln!("skipping: NK_TEST_BITCOIND not set");
        return;
    };
    let fixture = nk_testkit::RegtestFixture::start(&PathBuf::from(bitcoind_path))
        .await
        .expect("bitcoind should start");
    // A descriptor wallet with private keys (Bitcoin Core's default).
    fixture
        .rpc
        .call(
            "createwallet",
            vec![json!("keys")],
            "test setup",
            vec![],
            true,
        )
        .await
        .expect("createwallet");

    // A separate executor we can watch, feeding a real history store the way
    // the app does.
    let executor = Executor::new();
    let mut events = executor.subscribe();
    // A real file database, as the app uses.
    let store_dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Mutex::new(
        nk_store::Store::open(&store_dir.path().join("history.sqlite3")).unwrap(),
    ));
    let bridge = tokio::spawn(nk_store::persist_exec_events(
        store.clone(),
        executor.subscribe(),
    ));
    let rpc = nk_rpc::RpcClient::from_cookie_file(
        format!(
            "http://127.0.0.1:{}/wallet/keys",
            fixture.environment.rpc_port
        ),
        &fixture.environment.bitcoin_cookie_path(),
        executor.clone(),
        "regtest".to_string(),
        nk_core::Chain::Regtest,
    )
    .unwrap();

    let returned = rpc
        .call(
            "listdescriptors",
            vec![json!(true)],
            "console",
            vec![],
            false,
        )
        .await
        .expect("listdescriptors true");
    assert!(
        returned.to_string().contains("tprv"),
        "positive control: the caller really is handed the private descriptors"
    );

    // Everything the executor broadcast about that call.
    let mut seen = Vec::new();
    while let Ok(event) = events.try_recv() {
        match event {
            nk_exec::ExecEvent::Started {
                command_display, ..
            } => seen.push(command_display),
            nk_exec::ExecEvent::Output { chunk, .. } => seen.push(chunk),
            nk_exec::ExecEvent::Finished { .. } => {}
        }
    }
    assert!(!seen.is_empty());
    for text in &seen {
        assert!(!text.contains("tprv") && !text.contains("xprv"), "{text}");
    }
    assert!(
        seen.iter()
            .any(|t| t.contains(nk_exec::redact::PRIVATE_KEY_PLACEHOLDER)),
        "the output should show that something was removed: {seen:?}"
    );

    // And the persisted history.
    let rows = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let rows = store
                .lock()
                .unwrap()
                .list_command_history(None, 100)
                .unwrap();
            if rows
                .iter()
                .any(|r| r.command_display.contains("listdescriptors") && r.exit_code.is_some())
            {
                return rows;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the call should be recorded in the history");
    for row in &rows {
        assert!(
            !row.output.contains("tprv") && !row.command_display.contains("tprv"),
            "history row leaks a private key: {row:?}"
        );
    }
    assert!(rows
        .iter()
        .any(|r| r.output.contains(nk_exec::redact::PRIVATE_KEY_PLACEHOLDER)));

    bridge.abort();
    fixture.stop().await.expect("bitcoind should stop cleanly");
}

/// Layer 3, against a real Bitcoin Core: a numeric passphrase typed the only
/// way Core accepts one (JSON-quoted, so the console's raw token is
/// `"48213907"` with the quotes, and the parsed parameter renders as the bare
/// digits) must not appear in the event stream or the history -- for
/// `createwallet` (4th argument) and `walletpassphrase` (1st) alike. The
/// positive controls: the wallet really is encrypted with that passphrase, and
/// unlocking with it really works.
#[tokio::test]
#[serial(real_bitcoind)]
async fn a_numeric_passphrase_is_hidden_by_position_and_really_works() {
    let Some(bitcoind_path) = nk_core::live_tests::live_binary("NK_TEST_BITCOIND") else {
        eprintln!("skipping: NK_TEST_BITCOIND not set");
        return;
    };
    const PASSPHRASE: &str = "48213907";
    // What the console's tokenizer hands over for `"\"48213907\""`.
    let raw_token = format!("\"{PASSPHRASE}\"");

    let fixture = nk_testkit::RegtestFixture::start(&PathBuf::from(bitcoind_path))
        .await
        .expect("bitcoind should start");

    let executor = Executor::new();
    let mut events = executor.subscribe();
    let store_dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Mutex::new(
        nk_store::Store::open(&store_dir.path().join("history.sqlite3")).unwrap(),
    ));
    let bridge = tokio::spawn(nk_store::persist_exec_events(
        store.clone(),
        executor.subscribe(),
    ));
    let client = |path: &str| {
        nk_rpc::RpcClient::from_cookie_file(
            format!("http://127.0.0.1:{}{path}", fixture.environment.rpc_port),
            &fixture.environment.bitcoin_cookie_path(),
            executor.clone(),
            "regtest".to_string(),
            nk_core::Chain::Regtest,
        )
        .unwrap()
    };

    // `createwallet enc false false "48213907"`, the passphrase (index 3)
    // hidden by position; only the *quoted* token is offered for text
    // substitution, as the console does.
    let typed = |args: &[&str]| -> Vec<String> { args.iter().map(|a| a.to_string()).collect() };
    let mask = nk_core::console_safety::bitcoin_rpc_secret_arg_mask(
        "createwallet",
        &typed(&["enc", "false", "false", &raw_token]),
    );
    assert_eq!(mask, vec![false, false, false, true]);
    client("/")
        .call_masked(
            "createwallet",
            vec![json!("enc"), json!(false), json!(false), json!(PASSPHRASE)],
            "console",
            vec![raw_token.clone()],
            &mask,
            false,
        )
        .await
        .expect("createwallet with a passphrase");

    let wallet = client("/wallet/enc");
    let info = wallet
        .call("getwalletinfo", vec![], "test", vec![], true)
        .await
        .expect("getwalletinfo");
    assert!(
        info.get("unlocked_until").is_some(),
        "positive control: the wallet is encrypted (it has `unlocked_until`): {info}"
    );

    let mask = nk_core::console_safety::bitcoin_rpc_secret_arg_mask(
        "walletpassphrase",
        &typed(&[&raw_token, "60"]),
    );
    assert_eq!(mask, vec![true, true]);
    wallet
        .call_masked(
            "walletpassphrase",
            vec![json!(PASSPHRASE), json!(60)],
            "console",
            vec![raw_token],
            &mask,
            false,
        )
        .await
        .expect("walletpassphrase with the numeric passphrase");
    let info = wallet
        .call("getwalletinfo", vec![], "test", vec![], true)
        .await
        .unwrap();
    assert!(
        info["unlocked_until"].as_i64().unwrap_or(0) > 0,
        "positive control: the passphrase really unlocked the wallet: {info}"
    );

    // Everything the executor broadcast.
    let mut seen = Vec::new();
    while let Ok(event) = events.try_recv() {
        match event {
            nk_exec::ExecEvent::Started {
                command_display, ..
            } => seen.push(command_display),
            nk_exec::ExecEvent::Output { chunk, .. } => seen.push(chunk),
            nk_exec::ExecEvent::Finished { .. } => {}
        }
    }
    assert!(
        seen.iter().any(|t| t.contains("createwallet"))
            && seen.iter().any(|t| t.contains("walletpassphrase")),
        "both calls were broadcast: {seen:?}"
    );
    for text in &seen {
        assert!(
            !text.contains(PASSPHRASE),
            "event leaks the passphrase: {text}"
        );
    }

    // And the persisted history.
    let rows = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let rows = store
                .lock()
                .unwrap()
                .list_command_history(None, 100)
                .unwrap();
            let finished = |name: &str| {
                rows.iter()
                    .any(|r| r.command_display.contains(name) && r.exit_code.is_some())
            };
            if finished("createwallet") && finished("walletpassphrase") {
                return rows;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("both calls should be recorded in the history");
    for row in &rows {
        assert!(
            !row.command_display.contains(PASSPHRASE) && !row.output.contains(PASSPHRASE),
            "history row leaks the passphrase: {row:?}"
        );
    }
    assert!(rows
        .iter()
        .any(|r| r.command_display.ends_with("walletpassphrase [redacted]")));

    bridge.abort();
    fixture.stop().await.expect("bitcoind should stop cleanly");
}

/// The same, for a passphrase with **spaces** -- the normal case for a
/// mainnet wallet. Typed *quoted* it is one argument; typed *unquoted* (or in
/// single quotes, which this console does not understand) it arrives as
/// several, and every word of it must be hidden, not just the first. Against a
/// real Bitcoin Core: the wallet really is encrypted with the phrase and the
/// quoted form really unlocks it (positive controls); the unquoted form is
/// rejected by the node (too many parameters) -- after the call was
/// displayed, so the display is what matters.
#[tokio::test]
#[serial(real_bitcoind)]
async fn a_passphrase_with_spaces_is_hidden_word_by_word_and_really_works() {
    let Some(bitcoind_path) = nk_core::live_tests::live_binary("NK_TEST_BITCOIND") else {
        eprintln!("skipping: NK_TEST_BITCOIND not set");
        return;
    };
    // Distinctive words: none can occur elsewhere in a command or its output.
    const WORDS: [&str; 4] = ["quokka", "zephyrine", "marmalade", "trombonist"];
    let phrase = WORDS.join(" ");

    let fixture = nk_testkit::RegtestFixture::start(&PathBuf::from(bitcoind_path))
        .await
        .expect("bitcoind should start");
    let executor = Executor::new();
    let mut events = executor.subscribe();
    let store_dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Mutex::new(
        nk_store::Store::open(&store_dir.path().join("history.sqlite3")).unwrap(),
    ));
    let bridge = tokio::spawn(nk_store::persist_exec_events(
        store.clone(),
        executor.subscribe(),
    ));
    let client = |path: &str| {
        nk_rpc::RpcClient::from_cookie_file(
            format!("http://127.0.0.1:{}{path}", fixture.environment.rpc_port),
            &fixture.environment.bitcoin_cookie_path(),
            executor.clone(),
            "regtest".to_string(),
            nk_core::Chain::Regtest,
        )
        .unwrap()
    };
    let typed = |args: &[&str]| -> Vec<String> { args.iter().map(|a| a.to_string()).collect() };

    // `createwallet enc false false quokka zephyrine marmalade trombonist`,
    // unquoted: four arguments after the third. (The node takes only one
    // passphrase, so the console would send the words separately and the node
    // would reject that; what matters is the display. The wallet is created
    // with the phrase as one argument, the way the quoted form sends it.)
    let words: Vec<&str> = ["enc", "false", "false"].into_iter().chain(WORDS).collect();
    let unquoted_mask =
        nk_core::console_safety::bitcoin_rpc_secret_arg_mask("createwallet", &typed(&words));
    assert_eq!(
        unquoted_mask,
        vec![false, false, false, true, true, true, true]
    );
    let rejected = client("/")
        .call_masked(
            "createwallet",
            words.iter().map(|w| json!(w)).collect(),
            "console",
            vec![WORDS[0].to_string()],
            &unquoted_mask,
            false,
        )
        .await;
    assert!(rejected.is_err(), "the node rejects an unquoted phrase");

    // Quoted: one argument. This one really creates an encrypted wallet.
    let quoted_mask = nk_core::console_safety::bitcoin_rpc_secret_arg_mask(
        "createwallet",
        &typed(&["enc", "false", "false", &phrase]),
    );
    client("/")
        .call_masked(
            "createwallet",
            vec![json!("enc"), json!(false), json!(false), json!(phrase)],
            "console",
            vec![phrase.clone()],
            &quoted_mask,
            false,
        )
        .await
        .expect("createwallet with a passphrase that has spaces");
    let wallet = client("/wallet/enc");
    let info = wallet
        .call("getwalletinfo", vec![], "test", vec![], true)
        .await
        .unwrap();
    assert!(
        info.get("unlocked_until").is_some(),
        "positive control: the wallet is encrypted: {info}"
    );

    // `walletpassphrase quokka zephyrine marmalade trombonist 60`, unquoted:
    // five arguments (rejected by the node), all hidden.
    let unlock_words: Vec<&str> = WORDS.into_iter().chain(["60"]).collect();
    let unlock_mask = nk_core::console_safety::bitcoin_rpc_secret_arg_mask(
        "walletpassphrase",
        &typed(&unlock_words),
    );
    assert_eq!(unlock_mask, vec![true; 5]);
    let rejected = wallet
        .call_masked(
            "walletpassphrase",
            unlock_words.iter().map(|w| json!(w)).collect(),
            "console",
            vec![],
            &unlock_mask,
            false,
        )
        .await;
    assert!(rejected.is_err(), "the node rejects an unquoted phrase");

    // Quoted: really unlocks the wallet (positive control).
    let quoted_unlock = nk_core::console_safety::bitcoin_rpc_secret_arg_mask(
        "walletpassphrase",
        &typed(&[&phrase, "60"]),
    );
    wallet
        .call_masked(
            "walletpassphrase",
            vec![json!(phrase), json!(60)],
            "console",
            vec![phrase.clone()],
            &quoted_unlock,
            false,
        )
        .await
        .expect("walletpassphrase with the quoted phrase");
    let info = wallet
        .call("getwalletinfo", vec![], "test", vec![], true)
        .await
        .unwrap();
    assert!(
        info["unlocked_until"].as_i64().unwrap_or(0) > 0,
        "positive control: the phrase really unlocked the wallet: {info}"
    );

    let mut seen = Vec::new();
    while let Ok(event) = events.try_recv() {
        match event {
            nk_exec::ExecEvent::Started {
                command_display, ..
            } => seen.push(command_display),
            nk_exec::ExecEvent::Output { chunk, .. } => seen.push(chunk),
            nk_exec::ExecEvent::Finished { .. } => {}
        }
    }
    for text in &seen {
        for word in WORDS {
            assert!(!text.contains(word), "event leaks {word:?}: {text}");
        }
    }
    // Both spellings were displayed as a single marker.
    assert!(
        seen.iter()
            .filter(|t| t.ends_with("createwallet enc false false [redacted]"))
            .count()
            >= 2,
        "{seen:?}"
    );

    let rows = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let rows = store
                .lock()
                .unwrap()
                .list_command_history(None, 100)
                .unwrap();
            if rows.iter().filter(|r| r.exit_code.is_some()).count() >= 5 {
                return rows;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the calls should be recorded in the history");
    for row in &rows {
        for word in WORDS {
            assert!(
                !row.command_display.contains(word) && !row.output.contains(word),
                "history row leaks {word:?}: {row:?}"
            );
        }
    }

    bridge.abort();
    fixture.stop().await.expect("bitcoind should stop cleanly");
}

/// Against a real Bitcoin Core: a key that Core rejects comes back **inside
/// its error message** (`key '<what you typed>' is not valid`), and that
/// message reaches the executor's event stream and the history. The caller
/// still gets the real text (positive control); nothing recorded contains
/// the key.
#[tokio::test]
#[serial(real_bitcoind)]
async fn a_key_that_core_echoes_back_in_an_error_is_not_recorded() {
    let Some(bitcoind_path) = nk_core::live_tests::live_binary("NK_TEST_BITCOIND") else {
        eprintln!("skipping: NK_TEST_BITCOIND not set");
        return;
    };
    // A syntactically WIF-shaped throwaway, not a real key.
    let wif = format!("c{}", "B".repeat(51));

    let fixture = nk_testkit::RegtestFixture::start(&PathBuf::from(bitcoind_path))
        .await
        .expect("bitcoind should start");
    let executor = Executor::new();
    let mut events = executor.subscribe();
    let store_dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Mutex::new(
        nk_store::Store::open(&store_dir.path().join("history.sqlite3")).unwrap(),
    ));
    let bridge = tokio::spawn(nk_store::persist_exec_events(
        store.clone(),
        executor.subscribe(),
    ));
    let rpc = nk_rpc::RpcClient::from_cookie_file(
        format!("http://127.0.0.1:{}", fixture.environment.rpc_port),
        &fixture.environment.bitcoin_cookie_path(),
        executor.clone(),
        "regtest".to_string(),
        nk_core::Chain::Regtest,
    )
    .unwrap();

    let error = rpc
        .call(
            "getdescriptorinfo",
            vec![json!(format!("wpkh({wif})"))],
            "console",
            vec![],
            false,
        )
        .await
        .expect_err("Core rejects a key that is not valid");
    assert!(
        error.to_string().contains(&wif),
        "positive control: Core really does echo the key in its error: {error}"
    );

    let mut seen = Vec::new();
    while let Ok(event) = events.try_recv() {
        match event {
            nk_exec::ExecEvent::Started {
                command_display, ..
            } => seen.push(command_display),
            nk_exec::ExecEvent::Output { chunk, .. } => seen.push(chunk),
            nk_exec::ExecEvent::Finished { .. } => {}
        }
    }
    assert!(!seen.is_empty());
    for text in &seen {
        assert!(!text.contains(&wif), "event leaks the key: {text}");
    }
    let rows = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let rows = store
                .lock()
                .unwrap()
                .list_command_history(None, 100)
                .unwrap();
            if rows.iter().any(|r| r.exit_code.is_some()) {
                return rows;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the call should be recorded in the history");
    for row in &rows {
        assert!(
            !row.command_display.contains(&wif) && !row.output.contains(&wif),
            "history row leaks the key: {row:?}"
        );
    }

    bridge.abort();
    fixture.stop().await.expect("bitcoind should stop cleanly");
}

/// The list of known methods (which decides what arguments may be shown) is
/// checked against a real node: every method the node itself lists must be in
/// it. Fails, on purpose, when a newer Bitcoin Core adds a method -- a reminder
/// to look at it (does it take a secret?) and to update
/// `KNOWN_BITCOIN_RPC_METHODS`.
#[tokio::test]
#[serial(real_bitcoind)]
async fn the_known_method_list_covers_every_method_of_the_real_node() {
    let Some(bitcoind_path) = nk_core::live_tests::live_binary("NK_TEST_BITCOIND") else {
        eprintln!("skipping: NK_TEST_BITCOIND not set");
        return;
    };
    let fixture = nk_testkit::RegtestFixture::start(&PathBuf::from(bitcoind_path))
        .await
        .expect("bitcoind should start");
    let help = fixture
        .rpc
        .call("help", vec![], "test", vec![], true)
        .await
        .expect("help");
    let help = help.as_str().expect("help returns text").to_string();
    let methods: Vec<&str> = help
        .lines()
        .filter(|line| line.starts_with(|c: char| c.is_ascii_lowercase()))
        .filter_map(|line| line.split_whitespace().next())
        .collect();
    assert!(methods.len() > 100, "parsed {} methods", methods.len());
    let unknown: Vec<&&str> = methods
        .iter()
        .filter(|m| !nk_core::console_safety::is_known_bitcoin_rpc_method(m))
        .collect();
    assert!(
        unknown.is_empty(),
        "Bitcoin Core has methods that KNOWN_BITCOIN_RPC_METHODS does not: {unknown:?}"
    );
    fixture.stop().await.expect("bitcoind should stop cleanly");
}
