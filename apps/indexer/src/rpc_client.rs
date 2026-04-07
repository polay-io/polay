//! Simple JSON-RPC client for communicating with a POLAY node.

use anyhow::{Context, Result};
use tracing::debug;

use crate::types::{
    AccountData, BlockData, ChainInfo, JsonRpcRequest, JsonRpcResponse,
};

/// A client that talks to a POLAY node over JSON-RPC.
pub struct RpcClient {
    url: String,
    client: reqwest::Client,
    next_id: std::sync::atomic::AtomicU64,
}

impl RpcClient {
    /// Create a new RPC client pointing at the given URL.
    pub fn new(url: &str) -> Self {
        Self {
            url: url.to_string(),
            client: reqwest::Client::new(),
            next_id: std::sync::atomic::AtomicU64::new(1),
        }
    }

    /// Get the next request ID.
    fn next_id(&self) -> u64 {
        self.next_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    /// Send a JSON-RPC request and return the deserialized result.
    async fn call<P: serde::Serialize, R: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        params: P,
    ) -> Result<Option<R>> {
        let id = self.next_id();
        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            method,
            params,
            id,
        };

        debug!(method, id, "sending RPC request");

        let resp = self
            .client
            .post(&self.url)
            .json(&request)
            .send()
            .await
            .with_context(|| format!("RPC request to {} failed", self.url))?;

        let body = resp
            .text()
            .await
            .context("failed to read RPC response body")?;

        let rpc_resp: JsonRpcResponse<R> =
            serde_json::from_str(&body).with_context(|| {
                format!(
                    "failed to parse RPC response for method '{}': {}",
                    method,
                    if body.len() > 200 {
                        &body[..200]
                    } else {
                        &body
                    }
                )
            })?;

        if let Some(err) = rpc_resp.error {
            anyhow::bail!(
                "RPC error (code {}): {}",
                err.code,
                err.message
            );
        }

        Ok(rpc_resp.result)
    }

    /// Fetch chain metadata (height, latest hash, etc.).
    pub async fn get_chain_info(&self) -> Result<ChainInfo> {
        let info: Option<ChainInfo> = self.call("polay_getChainInfo", ()).await?;
        info.ok_or_else(|| anyhow::anyhow!("polay_getChainInfo returned null"))
    }

    /// Fetch a block by height. Returns `None` if the block does not exist.
    pub async fn get_block(&self, height: u64) -> Result<Option<BlockData>> {
        self.call("polay_getBlock", [height]).await
    }

    /// Fetch an account by hex address. Returns `None` if the account does not
    /// exist.
    pub async fn get_account(&self, address: &str) -> Result<Option<AccountData>> {
        self.call("polay_getAccount", [address]).await
    }
}
