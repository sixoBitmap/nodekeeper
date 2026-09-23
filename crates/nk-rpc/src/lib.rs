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
    pub async fn call(
        &self,
        method: &str,
        params: Vec<Value>,
        triggering_action: &str,
    ) -> Result<Value, RpcError> {
        let display = self.equivalent_bitcoin_cli(method, &params);
        let http = self.http.clone();
        let url = self.url.clone();
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
                    redact: vec![],
                    sensitivity: Sensitivity::Normal,
                },
                move || async move { do_call(http, url, user, password, method, params).await },
            )
            .await
    }

    /// docs/SPEC.md Foundation C: bitcoind's graceful stop is the `stop`
    /// RPC, then waiting for the process to exit (the waiting is
    /// nk-proc's job, not this call's).
    pub async fn stop(&self) -> Result<(), RpcError> {
        self.call("stop", vec![], "stop node").await?;
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
            )
            .await?;
        serde_json::from_value(result).map_err(|e| RpcError::UnexpectedResponse(e.to_string()))
    }

    pub async fn get_new_address(&self) -> Result<String, RpcError> {
        let result = self
            .call("getnewaddress", vec![], "get new address")
            .await?;
        result
            .as_str()
            .map(String::from)
            .ok_or_else(|| RpcError::UnexpectedResponse("expected a string address".to_string()))
    }

    pub async fn get_blockchain_info(&self) -> Result<Value, RpcError> {
        self.call("getblockchaininfo", vec![], "check sync status")
            .await
    }

    /// Peer count for the dashboard (docs/SPEC.md item 2: "peers").
    /// Field names VERIFY'd live against a real regtest node, not
    /// assumed (DECISIONS.md, Phase 3) — `connections` lives on
    /// `getnetworkinfo`, not `getblockchaininfo`.
    pub async fn get_network_info(&self) -> Result<Value, RpcError> {
        self.call("getnetworkinfo", vec![], "check peer count")
            .await
    }

    /// Mempool stats for the dashboard (docs/SPEC.md item 2: "mempool").
    pub async fn get_mempool_info(&self) -> Result<Value, RpcError> {
        self.call("getmempoolinfo", vec![], "check mempool").await
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
