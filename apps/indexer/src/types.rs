//! Lightweight types that mirror the POLAY RPC JSON responses.
//!
//! The indexer is a separate service that communicates with the chain over
//! JSON-RPC, so it defines its own serde-compatible types rather than depending
//! on `polay-types` directly.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// JSON-RPC envelope
// ---------------------------------------------------------------------------

/// A generic JSON-RPC 2.0 request.
#[derive(Debug, Serialize)]
pub struct JsonRpcRequest<'a, P: Serialize> {
    pub jsonrpc: &'a str,
    pub method: &'a str,
    pub params: P,
    pub id: u64,
}

/// A generic JSON-RPC 2.0 response.
#[derive(Debug, Deserialize)]
pub struct JsonRpcResponse<T> {
    #[allow(dead_code)]
    pub jsonrpc: String,
    pub result: Option<T>,
    pub error: Option<JsonRpcError>,
    #[allow(dead_code)]
    pub id: u64,
}

/// JSON-RPC error payload.
#[derive(Debug, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
}

// ---------------------------------------------------------------------------
// Chain info
// ---------------------------------------------------------------------------

/// Corresponds to `ChainInfoResponse` from polay-rpc.
#[derive(Debug, Clone, Deserialize)]
pub struct ChainInfo {
    pub chain_id: String,
    pub height: u64,
    pub latest_hash: String,
    pub state_root: String,
    pub block_time: u64,
}

// ---------------------------------------------------------------------------
// Block
// ---------------------------------------------------------------------------

/// Corresponds to `BlockResponse` from polay-rpc.
#[derive(Debug, Clone, Deserialize)]
pub struct BlockData {
    pub height: u64,
    pub timestamp: u64,
    pub hash: String,
    pub parent_hash: String,
    pub state_root: String,
    pub transactions_root: String,
    pub proposer: String,
    pub chain_id: String,
    pub tx_count: usize,
    pub transactions: Vec<SignedTransactionData>,
}

// ---------------------------------------------------------------------------
// Transaction
// ---------------------------------------------------------------------------

/// Mirrors `SignedTransaction` as returned by the RPC in block responses.
#[derive(Debug, Clone, Deserialize)]
pub struct SignedTransactionData {
    pub transaction: TransactionData,
    #[allow(dead_code)]
    pub signature: serde_json::Value,
    pub tx_hash: String,
    #[allow(dead_code)]
    pub signer_pubkey: serde_json::Value,
}

/// The inner transaction body.
#[derive(Debug, Clone, Deserialize)]
pub struct TransactionData {
    #[allow(dead_code)]
    pub chain_id: String,
    pub nonce: u64,
    pub signer: String,
    pub action: serde_json::Value,
    pub max_fee: u64,
    pub timestamp: u64,
}

// ---------------------------------------------------------------------------
// Account
// ---------------------------------------------------------------------------

/// Corresponds to `AccountResponse` from polay-rpc.
#[derive(Debug, Clone, Deserialize)]
pub struct AccountData {
    pub address: String,
    pub nonce: u64,
    pub balance: u64,
    #[allow(dead_code)]
    pub created_at: u64,
}

// ---------------------------------------------------------------------------
// Event
// ---------------------------------------------------------------------------

/// An event emitted during transaction execution.
#[derive(Debug, Clone, Deserialize)]
pub struct EventData {
    pub module: String,
    pub action: String,
    pub attributes: Vec<(String, String)>,
}

// ---------------------------------------------------------------------------
// Transaction receipt
// ---------------------------------------------------------------------------

/// Corresponds to `TransactionReceipt` from polay-types (returned inline in
/// some RPC responses).
#[derive(Debug, Clone, Deserialize)]
pub struct TransactionReceiptData {
    pub tx_hash: String,
    pub block_height: u64,
    pub success: bool,
    pub fee_used: u64,
    pub gas_used: u64,
    pub events: Vec<EventData>,
    pub error: Option<String>,
}

// ---------------------------------------------------------------------------
// Helpers for extracting action type from the serde_json::Value
// ---------------------------------------------------------------------------

/// Extract the action type label from a TransactionAction JSON value.
///
/// The polay-types `TransactionAction` enum is serialized by serde as:
///   - `{"Transfer": {...}}` for struct variants
///   - `"SomeUnit"` for unit variants (none exist currently)
///
/// We return the variant name lowercased with underscores (matching
/// `TransactionAction::label()`).
pub fn action_type_from_value(action: &serde_json::Value) -> String {
    match action {
        serde_json::Value::Object(map) => {
            if let Some(key) = map.keys().next() {
                camel_to_snake(key)
            } else {
                "unknown".to_string()
            }
        }
        serde_json::Value::String(s) => camel_to_snake(s),
        _ => "unknown".to_string(),
    }
}

/// Convert CamelCase to snake_case.
fn camel_to_snake(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 4);
    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(ch.to_ascii_lowercase());
        } else {
            result.push(ch);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_camel_to_snake() {
        assert_eq!(camel_to_snake("Transfer"), "transfer");
        assert_eq!(camel_to_snake("CreateAssetClass"), "create_asset_class");
        assert_eq!(camel_to_snake("BuyListing"), "buy_listing");
        assert_eq!(camel_to_snake("SubmitMatchResult"), "submit_match_result");
    }

    #[test]
    fn test_action_type_from_value() {
        let v: serde_json::Value =
            serde_json::json!({"Transfer": {"to": "abc", "amount": 100}});
        assert_eq!(action_type_from_value(&v), "transfer");

        let v2: serde_json::Value =
            serde_json::json!({"CreateAssetClass": {"name": "Gold"}});
        assert_eq!(action_type_from_value(&v2), "create_asset_class");
    }
}
