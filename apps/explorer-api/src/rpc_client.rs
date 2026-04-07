//! Reusable JSON-RPC client for communicating with a POLAY node.
//!
//! Each method maps to a `polay_*` JSON-RPC call on the node, returning the
//! raw `serde_json::Value` response.

use serde_json::{json, Value};

/// A thin wrapper around `reqwest::Client` that speaks JSON-RPC 2.0.
#[derive(Clone)]
#[allow(dead_code)]
pub struct RpcClient {
    url: String,
    client: reqwest::Client,
}

#[allow(dead_code)]
impl RpcClient {
    /// Create a new `RpcClient` pointed at the given node JSON-RPC URL.
    pub fn new(url: &str) -> Self {
        Self {
            url: url.to_string(),
            client: reqwest::Client::new(),
        }
    }

    /// Send a raw JSON-RPC request and return the `result` field.
    async fn call(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        });

        let resp = self
            .client
            .post(&self.url)
            .json(&body)
            .send()
            .await
            .map_err(|e| RpcError::Transport(e.to_string()))?;

        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| RpcError::Transport(e.to_string()))?;

        if !status.is_success() {
            return Err(RpcError::Http {
                status: status.as_u16(),
                body: text,
            });
        }

        let rpc_resp: Value =
            serde_json::from_str(&text).map_err(|e| RpcError::Parse(e.to_string()))?;

        if let Some(err) = rpc_resp.get("error") {
            return Err(RpcError::Rpc(err.to_string()));
        }

        Ok(rpc_resp.get("result").cloned().unwrap_or(Value::Null))
    }

    // -- Block queries --------------------------------------------------------

    /// Get a block by height.
    pub async fn get_block(&self, height: u64) -> Result<Value, RpcError> {
        self.call("polay_getBlock", json!([height])).await
    }

    /// Get the latest block.
    pub async fn get_latest_block(&self) -> Result<Value, RpcError> {
        self.call("polay_getLatestBlock", json!([])).await
    }

    // -- Account queries ------------------------------------------------------

    /// Get account state by address.
    pub async fn get_account(&self, address: &str) -> Result<Value, RpcError> {
        self.call("polay_getAccount", json!([address])).await
    }

    /// Get native token balance for an address.
    pub async fn get_balance(&self, address: &str) -> Result<Value, RpcError> {
        self.call("polay_getBalance", json!([address])).await
    }

    /// Get asset balance for an address.
    pub async fn get_asset_balance(
        &self,
        asset_class_id: &str,
        owner: &str,
    ) -> Result<Value, RpcError> {
        self.call("polay_getAssetBalance", json!([asset_class_id, owner]))
            .await
    }

    // -- Asset queries --------------------------------------------------------

    /// Get asset class details by ID.
    pub async fn get_asset_class(&self, id: &str) -> Result<Value, RpcError> {
        self.call("polay_getAssetClass", json!([id])).await
    }

    // -- Listing queries ------------------------------------------------------

    /// Get a marketplace listing by ID.
    pub async fn get_listing(&self, id: &str) -> Result<Value, RpcError> {
        self.call("polay_getListing", json!([id])).await
    }

    // -- Profile queries ------------------------------------------------------

    /// Get player profile by address.
    pub async fn get_profile(&self, address: &str) -> Result<Value, RpcError> {
        self.call("polay_getProfile", json!([address])).await
    }

    /// Get achievements for a player.
    pub async fn get_achievements(&self, address: &str) -> Result<Value, RpcError> {
        self.call("polay_getAchievements", json!([address])).await
    }

    // -- Validator queries ----------------------------------------------------

    /// Get validator info by address.
    pub async fn get_validator(&self, address: &str) -> Result<Value, RpcError> {
        self.call("polay_getValidator", json!([address])).await
    }

    // -- Match queries --------------------------------------------------------

    /// Get match result by match ID.
    pub async fn get_match_result(&self, match_id: &str) -> Result<Value, RpcError> {
        self.call("polay_getMatchResult", json!([match_id])).await
    }

    // -- Transaction queries --------------------------------------------------

    /// Get a transaction from the mempool by hash.
    pub async fn get_transaction(&self, tx_hash: &str) -> Result<Value, RpcError> {
        self.call("polay_getTransaction", json!([tx_hash])).await
    }

    // -- Chain info -----------------------------------------------------------

    /// Get high-level chain info (height, latest hash, etc.).
    pub async fn get_chain_info(&self) -> Result<Value, RpcError> {
        self.call("polay_getChainInfo", json!([])).await
    }

    /// Get current mempool size.
    pub async fn get_mempool_size(&self) -> Result<Value, RpcError> {
        self.call("polay_getMempoolSize", json!([])).await
    }
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur when talking to the POLAY node RPC.
#[derive(Debug, thiserror::Error)]
pub enum RpcError {
    /// Network-level error.
    #[error("transport error: {0}")]
    Transport(String),

    /// Non-2xx HTTP status.
    #[error("HTTP {status}: {body}")]
    Http { status: u16, body: String },

    /// JSON-RPC error response.
    #[error("RPC error: {0}")]
    Rpc(String),

    /// Failed to parse the response body.
    #[error("parse error: {0}")]
    Parse(String),
}
