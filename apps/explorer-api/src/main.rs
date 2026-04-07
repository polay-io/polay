//! POLAY Explorer API
//!
//! A REST API service designed for blockchain explorers. Acts as a
//! proxy/aggregator that calls the POLAY node JSON-RPC server and transforms
//! the responses into REST-friendly JSON.

mod rpc_client;

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json};
use axum::routing::get;
use axum::Router;
use clap::Parser;
use serde::Deserialize;
use serde_json::{json, Value};
use tower_http::cors::{Any, CorsLayer};
use tracing::info;
use tracing_subscriber::EnvFilter;

use rpc_client::RpcClient;

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

/// POLAY Explorer API server.
#[derive(Parser)]
#[command(name = "polay-explorer-api", about = "REST API for POLAY blockchain explorers")]
struct Cli {
    /// Address to listen on.
    #[arg(long, default_value = "0.0.0.0:8080")]
    listen: String,

    /// URL of the POLAY node JSON-RPC server.
    #[arg(long, default_value = "http://127.0.0.1:9944")]
    rpc_url: String,

    /// Log level filter.
    #[arg(long, default_value = "info")]
    log_level: String,
}

// ---------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------

/// Application state shared across all request handlers.
struct AppState {
    rpc: RpcClient,
}

// ---------------------------------------------------------------------------
// Error handling
// ---------------------------------------------------------------------------

/// A wrapper that turns RPC errors into appropriate HTTP responses.
enum ApiError {
    NotFound(String),
    BadRequest(String),
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match self {
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            ApiError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };
        let body = json!({ "error": message });
        (status, Json(body)).into_response()
    }
}

impl From<rpc_client::RpcError> for ApiError {
    fn from(e: rpc_client::RpcError) -> Self {
        ApiError::Internal(e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Pagination
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct PaginationParams {
    limit: Option<u64>,
    offset: Option<u64>,
}

impl PaginationParams {
    fn limit(&self) -> u64 {
        self.limit.unwrap_or(20).min(100)
    }

    fn offset(&self) -> u64 {
        self.offset.unwrap_or(0)
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let filter =
        EnvFilter::try_new(&cli.log_level).unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let state = Arc::new(AppState {
        rpc: RpcClient::new(&cli.rpc_url),
    });

    let cors = CorsLayer::new()
        .allow_methods(Any)
        .allow_origin(Any)
        .allow_headers(Any);

    let app = Router::new()
        // Blocks
        .route("/api/v1/blocks", get(get_blocks))
        .route("/api/v1/blocks/latest", get(get_latest_block))
        .route("/api/v1/blocks/{height}", get(get_block_by_height))
        // Transactions
        .route("/api/v1/transactions/{hash}", get(get_transaction))
        .route(
            "/api/v1/accounts/{address}/transactions",
            get(get_account_transactions),
        )
        // Accounts
        .route("/api/v1/accounts/{address}", get(get_account))
        .route("/api/v1/accounts/{address}/assets", get(get_account_assets))
        // Assets
        .route("/api/v1/assets", get(get_assets))
        .route("/api/v1/assets/{id}", get(get_asset))
        .route("/api/v1/assets/{id}/holders", get(get_asset_holders))
        // Marketplace
        .route("/api/v1/listings", get(get_listings))
        .route("/api/v1/listings/{id}", get(get_listing))
        // Validators
        .route("/api/v1/validators", get(get_validators))
        .route("/api/v1/validators/{address}", get(get_validator))
        // Identity
        .route("/api/v1/profiles/{address}", get(get_profile))
        .route(
            "/api/v1/profiles/{address}/achievements",
            get(get_achievements),
        )
        // Gaming
        .route("/api/v1/matches/{match_id}", get(get_match_result))
        .route("/api/v1/games/{game_id}/matches", get(get_game_matches))
        // Chain
        .route("/api/v1/chain/info", get(get_chain_info))
        .route("/api/v1/chain/search", get(search))
        .layer(cors)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&cli.listen).await?;
    info!(listen = %cli.listen, rpc_url = %cli.rpc_url, "explorer API server starting");
    axum::serve(listener, app).await?;
    Ok(())
}

// ===========================================================================
// Handlers -- Blocks
// ===========================================================================

/// `GET /api/v1/blocks` -- latest blocks (paginated).
async fn get_blocks(
    State(state): State<Arc<AppState>>,
    Query(params): Query<PaginationParams>,
) -> Result<impl IntoResponse, ApiError> {
    // Get current chain height to compute the range.
    let info = state.rpc.get_chain_info().await?;
    let height = info
        .get("height")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let limit = params.limit();
    let offset = params.offset();

    let mut blocks = Vec::new();
    if height > 0 {
        // Walk backwards from the tip.
        let start = height.saturating_sub(offset);
        let end = start.saturating_sub(limit).max(0);
        let mut h = start;
        while h > end && h > 0 {
            match state.rpc.get_block(h).await? {
                Value::Null => {}
                block => blocks.push(block),
            }
            h -= 1;
        }
    }

    Ok(Json(json!({
        "blocks": blocks,
        "total": height,
        "limit": limit,
        "offset": offset,
    })))
}

/// `GET /api/v1/blocks/latest` -- the latest block.
async fn get_latest_block(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, ApiError> {
    let block = state.rpc.get_latest_block().await?;
    if block.is_null() {
        return Err(ApiError::NotFound("no blocks yet".into()));
    }
    Ok(Json(block))
}

/// `GET /api/v1/blocks/:height` -- block by height.
async fn get_block_by_height(
    State(state): State<Arc<AppState>>,
    Path(height): Path<u64>,
) -> Result<impl IntoResponse, ApiError> {
    let block = state.rpc.get_block(height).await?;
    if block.is_null() {
        return Err(ApiError::NotFound(format!(
            "block at height {} not found",
            height
        )));
    }
    Ok(Json(block))
}

// ===========================================================================
// Handlers -- Transactions
// ===========================================================================

/// `GET /api/v1/transactions/:hash` -- transaction by hash.
async fn get_transaction(
    State(state): State<Arc<AppState>>,
    Path(hash): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    // First check the mempool.
    let tx = state.rpc.get_transaction(&hash).await?;
    if !tx.is_null() {
        return Ok(Json(json!({
            "transaction": tx,
            "status": "pending",
        })));
    }

    // Scan recent blocks for the transaction.
    // For MVP, search the last 100 blocks.
    let info = state.rpc.get_chain_info().await?;
    let height = info.get("height").and_then(|v| v.as_u64()).unwrap_or(0);
    let search_depth = 100u64.min(height);

    for h in (height.saturating_sub(search_depth)..=height).rev() {
        if h == 0 {
            continue;
        }
        let block = state.rpc.get_block(h).await?;
        if let Some(txs) = block.get("transactions").and_then(|t| t.as_array()) {
            for tx in txs {
                if let Some(tx_hash) = tx.get("tx_hash") {
                    let tx_hash_str = match tx_hash {
                        Value::String(s) => s.clone(),
                        Value::Object(obj) => {
                            // Hash might be serialized as { "bytes": "..." }
                            obj.get("bytes")
                                .or_else(|| obj.get("hex"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string()
                        }
                        _ => String::new(),
                    };
                    if tx_hash_str == hash {
                        return Ok(Json(json!({
                            "transaction": tx,
                            "block_height": h,
                            "status": "confirmed",
                        })));
                    }
                }
            }
        }
    }

    Err(ApiError::NotFound(format!(
        "transaction {} not found",
        hash
    )))
}

/// `GET /api/v1/accounts/:address/transactions` -- txs for an account.
async fn get_account_transactions(
    State(state): State<Arc<AppState>>,
    Path(address): Path<String>,
    Query(params): Query<PaginationParams>,
) -> Result<impl IntoResponse, ApiError> {
    let info = state.rpc.get_chain_info().await?;
    let height = info.get("height").and_then(|v| v.as_u64()).unwrap_or(0);
    let limit = params.limit();
    let offset = params.offset();

    // Scan recent blocks for transactions involving this address.
    let mut txs = Vec::new();
    let mut skipped = 0u64;
    let search_depth = 200u64.min(height);

    'outer: for h in (height.saturating_sub(search_depth)..=height).rev() {
        if h == 0 {
            continue;
        }
        let block = state.rpc.get_block(h).await?;
        if let Some(block_txs) = block.get("transactions").and_then(|t| t.as_array()) {
            for tx in block_txs {
                let involves_address = tx_involves_address(tx, &address);
                if involves_address {
                    if skipped < offset {
                        skipped += 1;
                        continue;
                    }
                    txs.push(json!({
                        "transaction": tx,
                        "block_height": h,
                    }));
                    if txs.len() as u64 >= limit {
                        break 'outer;
                    }
                }
            }
        }
    }

    Ok(Json(json!({
        "transactions": txs,
        "address": address,
        "limit": limit,
        "offset": offset,
    })))
}

/// Check if a transaction JSON object involves a given address.
fn tx_involves_address(tx: &Value, address: &str) -> bool {
    // Check signer.
    if let Some(inner) = tx.get("transaction") {
        if let Some(signer) = inner.get("signer") {
            if value_matches_hex(signer, address) {
                return true;
            }
        }
        // Check action fields for the address.
        if let Some(action) = inner.get("action") {
            let action_str = action.to_string();
            if action_str.contains(address) {
                return true;
            }
        }
    }
    false
}

/// Check if a JSON value (string or hex-encoded object) matches a hex address.
fn value_matches_hex(val: &Value, hex_str: &str) -> bool {
    match val {
        Value::String(s) => s == hex_str,
        Value::Object(obj) => obj
            .get("bytes")
            .or_else(|| obj.get("hex"))
            .and_then(|v| v.as_str())
            .map(|s| s == hex_str)
            .unwrap_or(false),
        _ => false,
    }
}

// ===========================================================================
// Handlers -- Accounts
// ===========================================================================

/// `GET /api/v1/accounts/:address` -- account info.
async fn get_account(
    State(state): State<Arc<AppState>>,
    Path(address): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let account = state.rpc.get_account(&address).await?;
    if account.is_null() {
        return Err(ApiError::NotFound(format!(
            "account {} not found",
            address
        )));
    }
    Ok(Json(account))
}

/// `GET /api/v1/accounts/:address/assets` -- asset balances.
///
/// MVP: Returns a placeholder. A full implementation would require prefix
/// iteration on the node side.
async fn get_account_assets(
    State(_state): State<Arc<AppState>>,
    Path(address): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    // The node RPC currently requires a specific asset_class_id to query
    // balances. A full implementation would need prefix iteration support.
    Ok(Json(json!({
        "address": address,
        "assets": [],
        "note": "prefix iteration not yet supported; query specific asset_class_id via /api/v1/assets/:id"
    })))
}

// ===========================================================================
// Handlers -- Assets
// ===========================================================================

/// `GET /api/v1/assets` -- list all asset classes (paginated).
///
/// MVP: The node does not support listing all assets. Returns empty.
async fn get_assets(
    Query(params): Query<PaginationParams>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(json!({
        "assets": [],
        "limit": params.limit(),
        "offset": params.offset(),
        "note": "prefix iteration not yet supported on the node"
    })))
}

/// `GET /api/v1/assets/:id` -- asset class details.
async fn get_asset(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let asset = state.rpc.get_asset_class(&id).await?;
    if asset.is_null() {
        return Err(ApiError::NotFound(format!(
            "asset class {} not found",
            id
        )));
    }
    Ok(Json(asset))
}

/// `GET /api/v1/assets/:id/holders` -- holders of an asset.
///
/// MVP: Returns a placeholder. Requires indexing support.
async fn get_asset_holders(
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(json!({
        "asset_class_id": id,
        "holders": [],
        "note": "holder enumeration not yet supported"
    })))
}

// ===========================================================================
// Handlers -- Marketplace
// ===========================================================================

/// Listing filter query parameters.
#[derive(Debug, Deserialize)]
struct ListingFilterParams {
    asset_class_id: Option<String>,
    seller: Option<String>,
    status: Option<String>,
    limit: Option<u64>,
    offset: Option<u64>,
}

/// `GET /api/v1/listings` -- active listings (paginated, filterable).
///
/// MVP: The node does not support listing enumeration. Returns empty.
async fn get_listings(
    Query(params): Query<ListingFilterParams>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(json!({
        "listings": [],
        "filters": {
            "asset_class_id": params.asset_class_id,
            "seller": params.seller,
            "status": params.status,
        },
        "limit": params.limit.unwrap_or(20),
        "offset": params.offset.unwrap_or(0),
        "note": "listing enumeration not yet supported; query specific listing IDs"
    })))
}

/// `GET /api/v1/listings/:id` -- listing details.
async fn get_listing(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let listing = state.rpc.get_listing(&id).await?;
    if listing.is_null() {
        return Err(ApiError::NotFound(format!(
            "listing {} not found",
            id
        )));
    }
    Ok(Json(listing))
}

// ===========================================================================
// Handlers -- Validators
// ===========================================================================

/// `GET /api/v1/validators` -- all validators.
///
/// MVP: The node does not support validator enumeration. Returns empty.
async fn get_validators() -> Result<impl IntoResponse, ApiError> {
    Ok(Json(json!({
        "validators": [],
        "note": "validator enumeration not yet supported; query specific addresses"
    })))
}

/// `GET /api/v1/validators/:address` -- validator details.
async fn get_validator(
    State(state): State<Arc<AppState>>,
    Path(address): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let validator = state.rpc.get_validator(&address).await?;
    if validator.is_null() {
        return Err(ApiError::NotFound(format!(
            "validator {} not found",
            address
        )));
    }
    Ok(Json(validator))
}

// ===========================================================================
// Handlers -- Identity
// ===========================================================================

/// `GET /api/v1/profiles/:address` -- player profile.
async fn get_profile(
    State(state): State<Arc<AppState>>,
    Path(address): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let profile = state.rpc.get_profile(&address).await?;
    if profile.is_null() {
        return Err(ApiError::NotFound(format!(
            "profile for {} not found",
            address
        )));
    }
    Ok(Json(profile))
}

/// `GET /api/v1/profiles/:address/achievements` -- achievements.
async fn get_achievements(
    State(state): State<Arc<AppState>>,
    Path(address): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let achievements = state.rpc.get_achievements(&address).await?;
    Ok(Json(json!({
        "address": address,
        "achievements": achievements,
    })))
}

// ===========================================================================
// Handlers -- Gaming
// ===========================================================================

/// `GET /api/v1/matches/:match_id` -- match result.
async fn get_match_result(
    State(state): State<Arc<AppState>>,
    Path(match_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let result = state.rpc.get_match_result(&match_id).await?;
    if result.is_null() {
        return Err(ApiError::NotFound(format!(
            "match {} not found",
            match_id
        )));
    }
    Ok(Json(result))
}

/// `GET /api/v1/games/:game_id/matches` -- matches for a game (paginated).
///
/// MVP: The node does not support match enumeration by game_id. Returns empty.
async fn get_game_matches(
    Path(game_id): Path<String>,
    Query(params): Query<PaginationParams>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(json!({
        "game_id": game_id,
        "matches": [],
        "limit": params.limit(),
        "offset": params.offset(),
        "note": "match enumeration by game_id not yet supported"
    })))
}

// ===========================================================================
// Handlers -- Chain
// ===========================================================================

/// `GET /api/v1/chain/info` -- chain stats.
async fn get_chain_info(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, ApiError> {
    let info = state.rpc.get_chain_info().await?;
    let mempool_size = state.rpc.get_mempool_size().await?;

    Ok(Json(json!({
        "chain_info": info,
        "mempool_size": mempool_size,
    })))
}

/// Search query parameter.
#[derive(Debug, Deserialize)]
struct SearchParams {
    q: Option<String>,
}

/// `GET /api/v1/chain/search?q=` -- search by block height, tx hash, address, or username.
async fn search(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SearchParams>,
) -> Result<impl IntoResponse, ApiError> {
    let query = params
        .q
        .unwrap_or_default()
        .trim()
        .to_string();

    if query.is_empty() {
        return Err(ApiError::BadRequest("search query 'q' is required".into()));
    }

    // 1. If the query is a number, try it as a block height.
    if let Ok(height) = query.parse::<u64>() {
        let block = state.rpc.get_block(height).await?;
        if !block.is_null() {
            return Ok(Json(json!({
                "type": "block",
                "result": block,
            })));
        }
    }

    // 2. If the query is 64-char hex, try as a transaction hash then block hash.
    if query.len() == 64 && query.chars().all(|c| c.is_ascii_hexdigit()) {
        // Try transaction.
        let tx = state.rpc.get_transaction(&query).await?;
        if !tx.is_null() {
            return Ok(Json(json!({
                "type": "transaction",
                "result": tx,
            })));
        }

        // Try as an account address.
        let account = state.rpc.get_account(&query).await?;
        if !account.is_null() {
            return Ok(Json(json!({
                "type": "account",
                "result": account,
            })));
        }

        // Try as a match ID.
        let match_result = state.rpc.get_match_result(&query).await?;
        if !match_result.is_null() {
            return Ok(Json(json!({
                "type": "match",
                "result": match_result,
            })));
        }
    }

    // 3. Try as a player profile (username-like query -- shorter strings).
    if query.len() < 64 {
        // Could be a username; we don't have a lookup-by-username RPC yet,
        // but we can try it as a partial address if it's valid hex.
        if query.len() == 64 && query.chars().all(|c| c.is_ascii_hexdigit()) {
            let profile = state.rpc.get_profile(&query).await?;
            if !profile.is_null() {
                return Ok(Json(json!({
                    "type": "profile",
                    "result": profile,
                })));
            }
        }
    }

    Err(ApiError::NotFound(format!(
        "no results found for '{}'",
        query
    )))
}
