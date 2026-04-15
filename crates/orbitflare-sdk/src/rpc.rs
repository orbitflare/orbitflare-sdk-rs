use reqwest::Client;
use serde_json::{Value, json};
use std::time::Duration;

use crate::endpoint::EndpointSet;
use crate::error::{Error, Result, SanitizedUrl};
use crate::retry::RetryPolicy;

pub struct RpcClient {
    http: Client,
    endpoints: EndpointSet,
    api_key: Option<String>,
    commitment: String,
    retry: RetryPolicy,
}

impl RpcClient {
    pub fn commitment(&self) -> &str {
        &self.commitment
    }

    pub async fn request(&self, method: &str, params: Value) -> Result<Value> {
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        });
        self.execute(&body, method).await
    }

    pub async fn request_raw(&self, body: &str) -> Result<Value> {
        let parsed: Value = serde_json::from_str(body)?;
        let method = parsed
            .get("method")
            .and_then(|m| m.as_str())
            .unwrap_or("raw");
        self.execute(&parsed, method).await
    }

    pub async fn get_slot(&self) -> Result<u64> {
        let r = self
            .request("getSlot", json!([{"commitment": self.commitment}]))
            .await?;
        r.as_u64()
            .ok_or_else(|| Error::Serialization("expected u64 for slot".into()))
    }

    pub async fn get_balance(&self, address: &str) -> Result<u64> {
        let r = self
            .request(
                "getBalance",
                json!([address, {"commitment": self.commitment}]),
            )
            .await?;
        r.get("value")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| Error::Serialization("expected balance value".into()))
    }

    pub async fn get_account_info(&self, address: &str) -> Result<Option<Value>> {
        let r = self
            .request(
                "getAccountInfo",
                json!([address, {"encoding": "base64", "commitment": self.commitment}]),
            )
            .await?;
        let value = &r["value"];
        if value.is_null() {
            Ok(None)
        } else {
            Ok(Some(value.clone()))
        }
    }

    pub async fn get_multiple_accounts(&self, addresses: &[&str]) -> Result<Vec<Option<Value>>> {
        let mut all = Vec::with_capacity(addresses.len());
        for chunk in addresses.chunks(100) {
            let r = self
                .request(
                    "getMultipleAccounts",
                    json!([chunk, {"encoding": "base64", "commitment": self.commitment}]),
                )
                .await?;
            if let Some(arr) = r.get("value").and_then(|v| v.as_array()) {
                for item in arr {
                    all.push(if item.is_null() {
                        None
                    } else {
                        Some(item.clone())
                    });
                }
            }
        }
        Ok(all)
    }

    pub async fn get_latest_blockhash(&self) -> Result<(String, u64)> {
        let r = self
            .request(
                "getLatestBlockhash",
                json!([{"commitment": self.commitment}]),
            )
            .await?;
        let v = &r["value"];
        let hash = v["blockhash"]
            .as_str()
            .ok_or_else(|| Error::Serialization("missing blockhash".into()))?
            .to_string();
        let height = v["lastValidBlockHeight"]
            .as_u64()
            .ok_or_else(|| Error::Serialization("missing lastValidBlockHeight".into()))?;
        Ok((hash, height))
    }

    pub async fn get_transaction(&self, signature: &str) -> Result<Value> {
        self.request(
            "getTransaction",
            json!([signature, {
                "encoding": "json",
                "commitment": self.commitment,
                "maxSupportedTransactionVersion": 0
            }]),
        )
        .await
    }

    pub async fn get_signatures_for_address(
        &self,
        address: &str,
        limit: u32,
    ) -> Result<Vec<Value>> {
        let r = self
            .request(
                "getSignaturesForAddress",
                json!([address, {"limit": limit, "commitment": self.commitment}]),
            )
            .await?;
        r.as_array()
            .cloned()
            .ok_or_else(|| Error::Serialization("expected array".into()))
    }

    pub async fn get_program_accounts(&self, program_id: &str) -> Result<Vec<Value>> {
        let r = self
            .request(
                "getProgramAccounts",
                json!([program_id, {"encoding": "base64", "commitment": self.commitment}]),
            )
            .await?;
        r.as_array()
            .cloned()
            .ok_or_else(|| Error::Serialization("expected array".into()))
    }

    pub async fn get_recent_prioritization_fees(&self, addresses: &[&str]) -> Result<Vec<Value>> {
        let r = self
            .request("getRecentPrioritizationFees", json!([addresses]))
            .await?;
        r.as_array()
            .cloned()
            .ok_or_else(|| Error::Serialization("expected array".into()))
    }

    pub async fn send_transaction(&self, tx_base64: &str) -> Result<String> {
        let r = self
            .request(
                "sendTransaction",
                json!([tx_base64, {
                    "encoding": "base64",
                    "skipPreflight": false,
                    "preflightCommitment": self.commitment
                }]),
            )
            .await?;
        r.as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| Error::Serialization("expected signature string".into()))
    }

    pub async fn simulate_transaction(&self, tx_base64: &str) -> Result<Value> {
        self.request(
            "simulateTransaction",
            json!([tx_base64, {"encoding": "base64", "commitment": self.commitment}]),
        )
        .await
    }

    pub async fn get_token_accounts_by_owner(
        &self,
        owner: &str,
        mint: Option<&str>,
        program_id: Option<&str>,
    ) -> Result<Vec<Value>> {
        let filter = if let Some(m) = mint {
            json!({"mint": m})
        } else if let Some(p) = program_id {
            json!({"programId": p})
        } else {
            json!({"programId": "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"})
        };
        let r = self
            .request(
                "getTokenAccountsByOwner",
                json!([owner, filter, {"encoding": "jsonParsed", "commitment": self.commitment}]),
            )
            .await?;
        r.get("value")
            .and_then(|v| v.as_array())
            .cloned()
            .ok_or_else(|| Error::Serialization("expected array".into()))
    }

    pub async fn get_transactions_for_address(
        &self,
        address: &str,
        options: GetTransactionsOptions,
    ) -> Result<GetTransactionsResult> {
        let result = self
            .request("getTransactionsForAddress", json!([address, options]))
            .await?;
        Ok(GetTransactionsResult {
            data: result["data"].as_array().cloned().unwrap_or_default(),
            pagination_token: result["paginationToken"].as_str().map(|s| s.to_string()),
        })
    }

    async fn execute(&self, body: &Value, method: &str) -> Result<Value> {
        let mut last_err = None;
        let mut tried = 0;

        while tried < self.endpoints.len() {
            let idx = self.endpoints.pick();
            let url = self.endpoints.get(idx);
            let mut attempt = 0u32;
            tried += 1;

            loop {
                attempt += 1;
                let _span = tracing::debug_span!(
                    "rpc",
                    method,
                    endpoint = %SanitizedUrl(&url),
                    attempt,
                )
                .entered();

                match self.post(&url, body).await {
                    Ok(result) => {
                        self.endpoints.mark_success(idx);
                        return Ok(result);
                    }
                    Err(e) => {
                        let retryable = e.is_retryable();

                        if retryable && self.retry.has_attempts_left(attempt) {
                            let delay = e
                                .retry_after()
                                .unwrap_or_else(|| self.retry.delay_for_attempt(attempt));
                            tracing::warn!(
                                method,
                                attempt,
                                delay_ms = delay.as_millis() as u64,
                                error = %e,
                                "retrying",
                            );
                            tokio::time::sleep(delay).await;
                            continue;
                        }

                        self.endpoints.mark_failure(idx);
                        tracing::warn!(
                            method,
                            endpoint = %SanitizedUrl(&url),
                            error = %e,
                            "failing over",
                        );
                        last_err = Some(e.with_endpoint(&url));
                        break;
                    }
                }
            }
        }

        Err(last_err
            .unwrap_or_else(|| Error::Transport("all endpoints exhausted".to_string().into())))
    }

    fn resolve_api_key(&self) -> Option<String> {
        self.api_key
            .clone()
            .or_else(|| std::env::var("ORBITFLARE_LICENSE_KEY").ok())
    }

    async fn post(&self, url: &str, body: &Value) -> Result<Value> {
        let auth_url = match self.resolve_api_key() {
            Some(key) => crate::credentials::apply_api_key(url, &key),
            None => url.to_string(),
        };

        let resp = self
            .http
            .post(&auth_url)
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await?;

        let status = resp.status();

        if status.as_u16() == 429 {
            let retry_after = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .map(std::time::Duration::from_secs);
            let _ = resp.text().await;
            return Err(Error::RateLimited { retry_after });
        }

        let text = resp.text().await?;

        if status.is_server_error() {
            return Err(Error::transport(HttpError(status.as_u16(), text)));
        }

        if let Ok(parsed) = serde_json::from_str::<Value>(&text) {
            if let Some(error) = parsed.get("error") {
                let code = error.get("code").and_then(|c| c.as_i64()).unwrap_or(0);
                let msg = error
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("unknown rpc error");
                return Err(Error::Rpc {
                    code,
                    message: msg.to_string(),
                });
            }
            if status.is_success() {
                return Ok(parsed["result"].clone());
            }
        }

        Err(Error::Rpc {
            code: -(status.as_u16() as i64),
            message: text.trim().to_string(),
        })
    }
}

pub struct RpcClientBuilder {
    url: Option<String>,
    fallbacks: Vec<String>,
    api_key: Option<String>,
    commitment: String,
    retry: RetryPolicy,
    timeout: Duration,
}

impl Default for RpcClientBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl RpcClientBuilder {
    pub fn new() -> Self {
        Self {
            url: None,
            fallbacks: Vec::new(),
            api_key: None,
            commitment: "confirmed".into(),
            retry: RetryPolicy::default(),
            timeout: Duration::from_secs(30),
        }
    }

    pub fn url(mut self, url: &str) -> Self {
        self.url = Some(url.to_string());
        self
    }

    pub fn urls(mut self, urls: &[&str]) -> Self {
        if let Some((first, rest)) = urls.split_first() {
            self.url = Some(first.to_string());
            self.fallbacks = rest.iter().map(|s| s.to_string()).collect();
        }
        self
    }

    pub fn fallback_url(mut self, url: &str) -> Self {
        self.fallbacks.push(url.to_string());
        self
    }

    pub fn fallback_urls(mut self, urls: &[&str]) -> Self {
        self.fallbacks.extend(urls.iter().map(|s| s.to_string()));
        self
    }

    pub fn api_key(mut self, key: &str) -> Self {
        self.api_key = Some(key.to_string());
        self
    }

    pub fn commitment(mut self, commitment: &str) -> Self {
        self.commitment = commitment.to_string();
        self
    }

    pub fn retry(mut self, policy: RetryPolicy) -> Self {
        self.retry = policy;
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn build(self) -> Result<RpcClient> {
        let url = self
            .url
            .or_else(|| std::env::var("ORBITFLARE_RPC_URL").ok())
            .ok_or_else(|| {
                Error::Config(
                    "no RPC URL provided. Pass .url() to the builder \
                     or set ORBITFLARE_RPC_URL in your environment"
                        .into(),
                )
            })?;

        let http = Client::builder()
            .timeout(self.timeout)
            .build()
            .map_err(Error::transport)?;

        let endpoints = EndpointSet::new(&url, &self.fallbacks);

        Ok(RpcClient {
            http,
            endpoints,
            api_key: self.api_key,
            commitment: self.commitment,
            retry: self.retry,
        })
    }
}

#[derive(Debug, Clone)]
pub struct GetTransactionsResult {
    pub data: Vec<Value>,
    pub pagination_token: Option<String>,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTransactionsOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction_details: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pagination_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commitment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoding: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_supported_transaction_version: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_context_slot: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filters: Option<GetTransactionsFilters>,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTransactionsFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_accounts: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_time: Option<RangeFilter<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slot: Option<RangeFilter<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<RangeFilter<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct RangeFilter<T> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gte: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gt: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lte: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lt: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eq: Option<T>,
}

impl GetTransactionsOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn transaction_details(mut self, v: &str) -> Self {
        self.transaction_details = Some(v.to_string());
        self
    }

    pub fn limit(mut self, v: u32) -> Self {
        self.limit = Some(v);
        self
    }

    pub fn sort_order(mut self, v: &str) -> Self {
        self.sort_order = Some(v.to_string());
        self
    }

    pub fn pagination_token(mut self, v: &str) -> Self {
        self.pagination_token = Some(v.to_string());
        self
    }

    pub fn commitment(mut self, v: &str) -> Self {
        self.commitment = Some(v.to_string());
        self
    }

    pub fn filters(mut self, f: GetTransactionsFilters) -> Self {
        self.filters = Some(f);
        self
    }
}

impl GetTransactionsFilters {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn token_accounts(mut self, v: &str) -> Self {
        self.token_accounts = Some(v.to_string());
        self
    }

    pub fn block_time(mut self, range: RangeFilter<i64>) -> Self {
        self.block_time = Some(range);
        self
    }

    pub fn slot(mut self, range: RangeFilter<u64>) -> Self {
        self.slot = Some(range);
        self
    }

    pub fn status(mut self, v: &str) -> Self {
        self.status = Some(v.to_string());
        self
    }
}

#[derive(Debug)]
struct HttpError(u16, String);

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HTTP {}: {}", self.0, self.1.trim())
    }
}

impl std::error::Error for HttpError {}
