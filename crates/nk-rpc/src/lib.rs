//! Bitcoin Core JSON-RPC client. All calls are routed through `nk-exec`
//! so they are tagged, redacted, and streamed to the Live Command
//! Monitor like any other command — shown there as the equivalent
//! `bitcoin-cli` invocation (docs/SPEC.md item 7: "RPC calls shown as
//! their equivalent bitcoin-cli command so users can learn them").

use nk_core::Chain;
use nk_exec::{CommandSource, Executor, RecordSpec, Sensitivity};
use serde_json::{json, Value};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RpcError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("rpc error {code}: {message}")]
    Rpc { code: i64, message: String },
    #[error("could not read cookie file at {path}: {source}")]
    CookieFile {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("cookie file at {0} is not in the expected \"user:password\" format")]
    CookieFormat(String),
    #[error("unexpected response shape: {0}")]
    UnexpectedResponse(String),
}

#[derive(Clone)]
pub struct RpcClient {
    http: reqwest::Client,
    url: String,
    user: String,
    password: String,
    executor: Executor,
    environment: String,
    chain: Chain,
}

impl RpcClient {
    pub fn new(
        url: String,
        user: String,
        password: String,
        executor: Executor,
        environment: String,
        chain: Chain,
    ) -> Self {
        Self {
            http: reqwest::Client::new(),
            url,
            user,
            password,
            executor,
            environment,
            chain,
        }
    }

    /// Builds a client authenticated from a Bitcoin Core cookie file
    /// (`.cookie`, contents `__cookie__:<random password>`).
    pub fn from_cookie_file(
        url: String,
        cookie_path: &Path,
        executor: Executor,
        environment: String,
        chain: Chain,
    ) -> Result<Self, RpcError> {
        let contents =
            std::fs::read_to_string(cookie_path).map_err(|source| RpcError::CookieFile {
                path: cookie_path.display().to_string(),
                source,
            })?;
        let (user, password) = contents
            .trim()
            .split_once(':')
            .ok_or_else(|| RpcError::CookieFormat(cookie_path.display().to_string()))?;
        Ok(Self::new(
            url,
            user.to_string(),
            password.to_string(),
            executor,
            environment,
            chain,
        ))
    }

    /// Makes a raw JSON-RPC call. Prefer the typed methods below where
    /// one exists; this is here for RPCs Nodekeeper doesn't wrap yet.
    ///
    /// `redact`: secret param values (e.g. a wallet passphrase) that
    /// must never appear in `command_display` as broadcast to the Live
    /// Command Monitor or persisted to `command_history` — passed
    /// straight through to `nk_exec::RecordSpec::redact`, which is
    /// applied there. Empty for every RPC that carries no secret.
    ///
    /// `background` is docs/SPEC.md item 7's "Background polling is
    /// hidden by default with a Show background polling toggle" — the
    /// caller decides, since the same RPC method can be a meaningful
    /// one-off check in one context and repetitive polling noise in
    /// another (e.g. `getblockchaininfo` for a dashboard refresh vs. a
    /// startup readiness check).
    pub async fn call(
        &self,
        method: &str,
        params: Vec<Value>,
        triggering_action: &str,
        redact: Vec<String>,
        background: bool,
    ) -> Result<Value, RpcError> {
        self.call_at(
            self.url.clone(),
            method,
            params,
            triggering_action,
            redact,
            background,
        )
        .await
    }

    /// docs/SPEC.md Foundation C: bitcoind's graceful stop is the `stop`
    /// RPC, then waiting for the process to exit (the waiting is
    /// nk-proc's job, not this call's).
    pub async fn stop(&self) -> Result<(), RpcError> {
        self.call("stop", vec![], "stop node", vec![], false)
            .await?;
        Ok(())
    }

    pub async fn generate_to_address(
        &self,
        nblocks: u32,
        address: &str,
    ) -> Result<Vec<String>, RpcError> {
        let result = self
            .call(
                "generatetoaddress",
                vec![json!(nblocks), json!(address)],
                "mine blocks",
                vec![],
                false,
            )
            .await?;
        serde_json::from_value(result).map_err(|e| RpcError::UnexpectedResponse(e.to_string()))
    }

    pub async fn get_new_address(&self) -> Result<String, RpcError> {
        let result = self
            .call("getnewaddress", vec![], "get new address", vec![], false)
            .await?;
        result
            .as_str()
            .map(String::from)
            .ok_or_else(|| RpcError::UnexpectedResponse("expected a string address".to_string()))
    }

    pub async fn get_blockchain_info(&self, background: bool) -> Result<Value, RpcError> {
        self.call(
            "getblockchaininfo",
            vec![],
            "check sync status",
            vec![],
            background,
        )
        .await
    }

    /// Peer count for the dashboard (docs/SPEC.md item 2: "peers").
    /// Field names VERIFY'd live against a real regtest node, not
    /// assumed (DECISIONS.md, Phase 3) — `connections` lives on
    /// `getnetworkinfo`, not `getblockchaininfo`.
    pub async fn get_network_info(&self, background: bool) -> Result<Value, RpcError> {
        self.call(
            "getnetworkinfo",
            vec![],
            "check peer count",
            vec![],
            background,
        )
        .await
    }

    /// Mempool stats for the dashboard (docs/SPEC.md item 2: "mempool").
    pub async fn get_mempool_info(&self, background: bool) -> Result<Value, RpcError> {
        self.call(
            "getmempoolinfo",
            vec![],
            "check mempool",
            vec![],
            background,
        )
        .await
    }

    /// Fee-rate estimate for the Send screen (docs/SPEC.md item 3:
    /// "estimates only from the local node"). `conf_target` is in
    /// blocks. Returns the raw response -- `result.feerate` (BTC/kvB)
    /// is present only when bitcoind actually has an estimate;
    /// confirmed live (DECISIONS.md Phase 5 VERIFY) that regtest
    /// returns `{"errors": [...], "blocks": 0}` with no `feerate` field
    /// at all rather than an RPC error, so the caller checks for the
    /// field's absence, not a `Result::Err`.
    pub async fn estimate_smart_fee(
        &self,
        conf_target: u32,
        background: bool,
    ) -> Result<Value, RpcError> {
        self.call(
            "estimatesmartfee",
            vec![json!(conf_target)],
            "estimate fee rate",
            vec![],
            background,
        )
        .await
    }

    /// Unlocks the wallet for `timeout_secs` before a signing action
    /// (docs/SPEC.md item 3, Foundation D): `passphrase` is redacted
    /// from `command_display` so it never reaches the Live Command
    /// Monitor or `command_history` — confirmed necessary and correct
    /// live (DECISIONS.md, Phase 5 VERIFY): a real `walletpassphrase`
    /// call's equivalent bitcoin-cli display otherwise shows the
    /// passphrase in plain text.
    pub async fn wallet_passphrase(
        &self,
        wallet: &str,
        passphrase: &str,
        timeout_secs: u32,
    ) -> Result<(), RpcError> {
        self.wallet_call(
            wallet,
            "walletpassphrase",
            vec![json!(passphrase), json!(timeout_secs)],
            "unlock wallet",
            vec![passphrase.to_string()],
            false,
        )
        .await?;
        Ok(())
    }

    /// Re-locks the wallet immediately after a signing action, rather
    /// than waiting out `wallet_passphrase`'s timeout (docs/SPEC.md
    /// item 3: "unlocks it... for a short timeout and locks it again
    /// afterwards").
    pub async fn wallet_lock(&self, wallet: &str) -> Result<(), RpcError> {
        self.wallet_call(wallet, "walletlock", vec![], "lock wallet", vec![], false)
            .await?;
        Ok(())
    }

    /// Encrypts a not-yet-encrypted wallet (docs/SPEC.md item 3: every
    /// MAINNET wallet must be encrypted). Same redaction reasoning as
    /// `wallet_passphrase`.
    pub async fn encrypt_wallet(&self, wallet: &str, passphrase: &str) -> Result<(), RpcError> {
        self.wallet_call(
            wallet,
            "encryptwallet",
            vec![json!(passphrase)],
            "encrypt wallet",
            vec![passphrase.to_string()],
            false,
        )
        .await?;
        Ok(())
    }

    /// Same as `call`, but against `/wallet/<wallet>` — every wallet
    /// RPC (as opposed to node-level RPCs like `getblockchaininfo`)
    /// needs the wallet name in the URL path (confirmed live,
    /// DECISIONS.md Phase 5 VERIFY: `curl .../wallet/ord`).
    #[allow(clippy::too_many_arguments)]
    async fn wallet_call(
        &self,
        wallet: &str,
        method: &str,
        params: Vec<Value>,
        triggering_action: &str,
        redact: Vec<String>,
        background: bool,
    ) -> Result<Value, RpcError> {
        let url = format!("{}/wallet/{}", self.url.trim_end_matches('/'), wallet);
        self.call_at(url, method, params, triggering_action, redact, background)
            .await
    }

    /// Shared implementation behind `call`/`wallet_call`: only the
    /// target URL differs between a node-level RPC and a wallet RPC.
    #[allow(clippy::too_many_arguments)]
    async fn call_at(
        &self,
        url: String,
        method: &str,
        params: Vec<Value>,
        triggering_action: &str,
        redact: Vec<String>,
        background: bool,
    ) -> Result<Value, RpcError> {
        let display = self.equivalent_bitcoin_cli(method, &params);
        let http = self.http.clone();
        let user = self.user.clone();
        let password = self.password.clone();
        let method = method.to_string();

        self.executor
            .record(
                RecordSpec {
                    environment: self.environment.clone(),
                    source: CommandSource::Rpc,
                    triggering_action: triggering_action.to_string(),
                    command_display: display,
                    redact,
                    sensitivity: Sensitivity::Normal,
                    background,
                },
                move || async move { do_call(http, url, user, password, method, params).await },
            )
            .await
    }

    fn equivalent_bitcoin_cli(&self, method: &str, params: &[Value]) -> String {
        let mut parts = vec!["bitcoin-cli".to_string()];
        if let Some(flag) = self.chain.bitcoin_cli_flag() {
            parts.push(flag.to_string());
        }
        parts.push(method.to_string());
        parts.extend(params.iter().map(display_json_arg));
        parts.join(" ")
    }
}

fn display_json_arg(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

async fn do_call(
    http: reqwest::Client,
    url: String,
    user: String,
    password: String,
    method: String,
    params: Vec<Value>,
) -> Result<Value, RpcError> {
    let body = json!({"jsonrpc": "1.0", "id": "nodekeeper", "method": method, "params": params});
    let response = http
        .post(&url)
        .basic_auth(user, Some(password))
        .json(&body)
        .send()
        .await?;
    let parsed: Value = response.json().await?;

    if let Some(error) = parsed.get("error").filter(|e| !e.is_null()) {
        let code = error.get("code").and_then(Value::as_i64).unwrap_or(0);
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("unknown RPC error")
            .to_string();
        return Err(RpcError::Rpc { code, message });
    }

    Ok(parsed.get("result").cloned().unwrap_or(Value::Null))
}

#[cfg(test)]
mod tests {
    use super::*;
    use nk_exec::ExecEvent;

    /// The real, automatable slice of Phase 5's [CI] "fake-passphrase
    /// search test" acceptance criterion: a passphrase passed to
    /// `wallet_passphrase` must never appear in the `command_display`
    /// broadcast to the Live Command Monitor. Found live (DECISIONS.md,
    /// Phase 5 VERIFY) that `RpcClient::call` previously hardcoded
    /// `redact: vec![]`, which would have leaked it. The call itself
    /// fails (nothing is listening on port 1) -- irrelevant here, since
    /// the `Started` event (carrying `command_display`) is emitted
    /// *before* the call is attempted.
    #[tokio::test]
    async fn a_wallet_passphrase_never_appears_in_the_broadcast_command_display() {
        let executor = Executor::new();
        let mut events = executor.subscribe();
        let client = RpcClient::new(
            "http://127.0.0.1:1".to_string(),
            "u".to_string(),
            "p".to_string(),
            executor,
            "regtest".to_string(),
            Chain::Regtest,
        );

        let fake_passphrase = "correct horse battery staple fake";
        let _ = client.wallet_passphrase("ord", fake_passphrase, 30).await;

        match events.recv().await.unwrap() {
            ExecEvent::Started {
                command_display, ..
            } => {
                assert!(
                    !command_display.contains(fake_passphrase),
                    "command_display leaked the passphrase: {command_display}"
                );
            }
            other => panic!("expected a Started event first, got {other:?}"),
        }
    }

    /// Same reasoning and mechanism as the passphrase test above, for
    /// `encrypt_wallet`.
    #[tokio::test]
    async fn encrypt_wallet_never_appears_in_the_broadcast_command_display() {
        let executor = Executor::new();
        let mut events = executor.subscribe();
        let client = RpcClient::new(
            "http://127.0.0.1:1".to_string(),
            "u".to_string(),
            "p".to_string(),
            executor,
            "regtest".to_string(),
            Chain::Regtest,
        );

        let fake_passphrase = "another fake passphrase entirely";
        let _ = client.encrypt_wallet("ord", fake_passphrase).await;

        match events.recv().await.unwrap() {
            ExecEvent::Started {
                command_display, ..
            } => {
                assert!(
                    !command_display.contains(fake_passphrase),
                    "command_display leaked the passphrase: {command_display}"
                );
            }
            other => panic!("expected a Started event first, got {other:?}"),
        }
    }

    #[test]
    fn equivalent_bitcoin_cli_includes_the_chain_flag_except_for_mainnet() {
        let executor = Executor::new();
        let regtest = RpcClient::new(
            "http://127.0.0.1:18443".to_string(),
            "u".to_string(),
            "p".to_string(),
            executor.clone(),
            "regtest".to_string(),
            Chain::Regtest,
        );
        assert_eq!(
            regtest.equivalent_bitcoin_cli("generatetoaddress", &[json!(1), json!("bcrt1qx")]),
            "bitcoin-cli -regtest generatetoaddress 1 bcrt1qx"
        );

        let mainnet = RpcClient::new(
            "http://127.0.0.1:8332".to_string(),
            "u".to_string(),
            "p".to_string(),
            executor,
            "mainnet".to_string(),
            Chain::Mainnet,
        );
        assert_eq!(
            mainnet.equivalent_bitcoin_cli("getblockchaininfo", &[]),
            "bitcoin-cli getblockchaininfo"
        );
    }

    #[test]
    fn cookie_file_is_parsed_into_user_and_password() {
        let dir = tempfile::tempdir().unwrap();
        let cookie_path = dir.path().join(".cookie");
        std::fs::write(&cookie_path, "__cookie__:s3cr3t-random-value").unwrap();

        let client = RpcClient::from_cookie_file(
            "http://127.0.0.1:18443".to_string(),
            &cookie_path,
            Executor::new(),
            "regtest".to_string(),
            Chain::Regtest,
        )
        .unwrap();
        assert_eq!(client.user, "__cookie__");
        assert_eq!(client.password, "s3cr3t-random-value");
    }

    #[test]
    fn malformed_cookie_file_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let cookie_path = dir.path().join(".cookie");
        std::fs::write(&cookie_path, "not-a-valid-cookie-line").unwrap();

        let result = RpcClient::from_cookie_file(
            "http://127.0.0.1:18443".to_string(),
            &cookie_path,
            Executor::new(),
            "regtest".to_string(),
            Chain::Regtest,
        );
        assert!(matches!(result, Err(RpcError::CookieFormat(_))));
    }
}
