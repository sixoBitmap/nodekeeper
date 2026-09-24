//! `ord` CLI and `ord server` HTTP API wrappers. CLI calls are routed
//! through `nk-exec`; see `DECISIONS.md` for the verified `ord` CLI
//! behavior this crate is built against (version, flags, dry-run output
//! shapes, index-option gating).
//!
//! The client here wraps `ord server`'s `/status` endpoint -- ord's own
//! sync-status API (DECISIONS.md Phase 4 VERIFY: `GET /status` with
//! header `Accept: application/json`, otherwise ord serves its HTML
//! explorer at the same path). Like `nk-rpc`, calls are routed through
//! the central executor so they show up in the Live Command Monitor
//! (docs/SPEC.md Foundation B), displayed as the equivalent `curl`
//! command a user could run by hand. Returns the raw JSON `Value` --
//! same convention `nk-rpc`'s typed methods use for bitcoind's
//! responses -- so callers pick out just the fields they need (e.g.
//! `height`, compared against bitcoind's `getblockchaininfo.blocks` to
//! decide whether ord is caught up).

use nk_exec::{CommandSource, Executor, RecordSpec, Sensitivity};
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OrdApiError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
}

#[derive(Clone)]
pub struct OrdClient {
    http: reqwest::Client,
    base_url: String,
    executor: Executor,
    environment: String,
}

impl OrdClient {
    pub fn new(base_url: String, executor: Executor, environment: String) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url,
            executor,
            environment,
        }
    }

    /// `background` is docs/SPEC.md item 7's "Background polling is
    /// hidden by default with a Show background polling toggle" -- the
    /// caller decides, since this is both the Dashboard's periodic ord
    /// poll and the one-shot wait-for-sync check `nk-proc`'s
    /// `OrdProcess` will use during startup.
    pub async fn status(&self, background: bool) -> Result<Value, OrdApiError> {
        let url = status_url(&self.base_url);
        let display = format!(r#"curl -H "Accept: application/json" {url}"#);
        let http = self.http.clone();
        let request_url = url;

        self.executor
            .record(
                RecordSpec {
                    environment: self.environment.clone(),
                    source: CommandSource::OrdApi,
                    triggering_action: "check ord sync status".to_string(),
                    command_display: display,
                    redact: vec![],
                    sensitivity: Sensitivity::Normal,
                    background,
                },
                move || async move { do_status(http, request_url).await },
            )
            .await
    }
}

fn status_url(base_url: &str) -> String {
    format!("{}/status", base_url.trim_end_matches('/'))
}

async fn do_status(http: reqwest::Client, url: String) -> Result<Value, OrdApiError> {
    let response = http
        .get(&url)
        .header("Accept", "application/json")
        .send()
        .await?;
    Ok(response.json().await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_url_strips_a_trailing_slash_on_the_base_url() {
        assert_eq!(
            status_url("http://127.0.0.1:8081/"),
            "http://127.0.0.1:8081/status"
        );
        assert_eq!(
            status_url("http://127.0.0.1:8081"),
            "http://127.0.0.1:8081/status"
        );
    }
}
