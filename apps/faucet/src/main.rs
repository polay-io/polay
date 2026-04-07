use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::{get, post};
use axum::Router;
use chrono::Utc;
use clap::Parser;
use dashmap::DashMap;
use ed25519_dalek::{Signer, SigningKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tower_http::cors::CorsLayer;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

/// POLAY Testnet Faucet — drips POL to requesting addresses.
#[derive(Parser)]
#[command(name = "polay-faucet")]
struct Cli {
    /// Address for the faucet HTTP server.
    #[arg(long, default_value = "0.0.0.0:8080")]
    listen: String,

    /// POLAY node JSON-RPC URL.
    #[arg(long, default_value = "http://127.0.0.1:9944")]
    rpc_url: String,

    /// Hex-encoded 32-byte faucet private key.
    #[arg(long, env = "FAUCET_SECRET_KEY")]
    secret_key: String,

    /// Amount of POL to drip per request (base units).
    #[arg(long, default_value_t = 10_000_000)]
    drip_amount: u64,

    /// Cooldown per address in seconds.
    #[arg(long, default_value_t = 86400)]
    cooldown_secs: u64,

    /// Maximum drips per address per day.
    #[arg(long, default_value_t = 3)]
    max_drips_per_day: u32,

    /// Log level.
    #[arg(long, default_value = "info")]
    log_level: String,
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

struct AppState {
    rpc_url: String,
    signing_key: SigningKey,
    faucet_address: String,
    drip_amount: u64,
    cooldown: Duration,
    max_drips_per_day: u32,
    /// address -> (last_drip_timestamp, drips_today, day_number)
    rate_limits: DashMap<String, (i64, u32, u32)>,
    http: reqwest::Client,
}

// ---------------------------------------------------------------------------
// API types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct DripRequest {
    address: String,
}

#[derive(Serialize)]
struct DripResponse {
    success: bool,
    tx_hash: Option<String>,
    amount: u64,
    message: String,
}

#[derive(Serialize)]
struct FaucetInfo {
    faucet_address: String,
    drip_amount: u64,
    cooldown_secs: u64,
    network: String,
}

#[derive(Serialize)]
struct HealthResponse {
    status: String,
    faucet_address: String,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn health(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".into(),
        faucet_address: state.faucet_address.clone(),
    })
}

async fn info_handler(State(state): State<Arc<AppState>>) -> Json<FaucetInfo> {
    Json(FaucetInfo {
        faucet_address: state.faucet_address.clone(),
        drip_amount: state.drip_amount,
        cooldown_secs: state.cooldown.as_secs(),
        network: "polay-testnet".into(),
    })
}

async fn drip(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DripRequest>,
) -> Result<Json<DripResponse>, (StatusCode, Json<DripResponse>)> {
    let addr = req.address.trim().to_lowercase();

    // Validate address format (64 hex chars).
    if addr.len() != 64 || hex::decode(&addr).is_err() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(DripResponse {
                success: false,
                tx_hash: None,
                amount: 0,
                message: "Invalid address: expected 64 hex characters".into(),
            }),
        ));
    }

    // Rate limiting.
    let now = Utc::now().timestamp();
    let today = (now / 86400) as u32;

    {
        if let Some(mut entry) = state.rate_limits.get_mut(&addr) {
            let (last_ts, drips, day) = entry.value_mut();

            // Reset daily counter if new day.
            if *day != today {
                *drips = 0;
                *day = today;
            }

            if *drips >= state.max_drips_per_day {
                return Err((
                    StatusCode::TOO_MANY_REQUESTS,
                    Json(DripResponse {
                        success: false,
                        tx_hash: None,
                        amount: 0,
                        message: format!(
                            "Daily limit reached ({} drips/day). Try again tomorrow.",
                            state.max_drips_per_day
                        ),
                    }),
                ));
            }

            let elapsed = Duration::from_secs((now - *last_ts).max(0) as u64);
            if elapsed < state.cooldown {
                let remaining = state.cooldown - elapsed;
                return Err((
                    StatusCode::TOO_MANY_REQUESTS,
                    Json(DripResponse {
                        success: false,
                        tx_hash: None,
                        amount: 0,
                        message: format!(
                            "Cooldown active. Try again in {} seconds.",
                            remaining.as_secs()
                        ),
                    }),
                ));
            }
        }
    }

    // Get the faucet account nonce.
    let nonce = match get_nonce(&state.http, &state.rpc_url, &state.faucet_address).await {
        Ok(n) => n,
        Err(e) => {
            error!(%e, "failed to get faucet nonce");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(DripResponse {
                    success: false,
                    tx_hash: None,
                    amount: 0,
                    message: "Internal error: could not query faucet nonce".into(),
                }),
            ));
        }
    };

    // Build and sign the transfer transaction.
    let tx_hash = match send_transfer(
        &state.http,
        &state.rpc_url,
        &state.signing_key,
        &state.faucet_address,
        &addr,
        state.drip_amount,
        nonce,
    )
    .await
    {
        Ok(hash) => hash,
        Err(e) => {
            error!(%e, to = %addr, "failed to send drip tx");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(DripResponse {
                    success: false,
                    tx_hash: None,
                    amount: 0,
                    message: format!("Transaction failed: {}", e),
                }),
            ));
        }
    };

    // Update rate limits.
    state
        .rate_limits
        .insert(addr.clone(), (now, {
            let prev = state.rate_limits.get(&addr)
                .map(|e| if e.2 == today { e.1 } else { 0 })
                .unwrap_or(0);
            prev + 1
        }, today));

    info!(to = %addr, amount = state.drip_amount, tx = %tx_hash, "drip sent");

    Ok(Json(DripResponse {
        success: true,
        tx_hash: Some(tx_hash),
        amount: state.drip_amount,
        message: format!("Sent {} POL to {}", state.drip_amount, addr),
    }))
}

// ---------------------------------------------------------------------------
// RPC helpers
// ---------------------------------------------------------------------------

async fn get_nonce(
    http: &reqwest::Client,
    rpc_url: &str,
    address: &str,
) -> Result<u64, String> {
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "polay_getAccount",
        "params": { "address": address },
        "id": 1
    });

    let resp: serde_json::Value = http
        .post(rpc_url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("RPC request failed: {}", e))?
        .json()
        .await
        .map_err(|e| format!("RPC response parse failed: {}", e))?;

    if let Some(err) = resp.get("error") {
        return Err(format!("RPC error: {}", err));
    }

    Ok(resp["result"]["nonce"].as_u64().unwrap_or(0))
}

async fn send_transfer(
    http: &reqwest::Client,
    rpc_url: &str,
    signing_key: &SigningKey,
    from: &str,
    to: &str,
    amount: u64,
    nonce: u64,
) -> Result<String, String> {
    // Build the canonical message to sign:
    // SHA-256(sender || nonce || "Transfer" || to || amount)
    let mut hasher = Sha256::new();
    hasher.update(hex::decode(from).map_err(|e| e.to_string())?);
    hasher.update(nonce.to_le_bytes());
    hasher.update(b"Transfer");
    hasher.update(hex::decode(to).map_err(|e| e.to_string())?);
    hasher.update(amount.to_le_bytes());
    let msg = hasher.finalize();

    let signature = signing_key.sign(&msg);
    let sig_hex = hex::encode(signature.to_bytes());

    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "polay_submitTransaction",
        "params": {
            "sender": from,
            "nonce": nonce,
            "action": {
                "type": "Transfer",
                "to": to,
                "amount": amount.to_string()
            },
            "max_fee": "10000",
            "signature": sig_hex
        },
        "id": 1
    });

    let resp: serde_json::Value = http
        .post(rpc_url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("RPC request failed: {}", e))?
        .json()
        .await
        .map_err(|e| format!("RPC response parse failed: {}", e))?;

    if let Some(err) = resp.get("error") {
        return Err(format!("RPC error: {}", err));
    }

    resp["result"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "No tx hash in response".into())
}

// ---------------------------------------------------------------------------
// Address derivation
// ---------------------------------------------------------------------------

fn address_from_signing_key(key: &SigningKey) -> String {
    let public = key.verifying_key();
    let mut hasher = Sha256::new();
    hasher.update(public.as_bytes());
    let hash = hasher.finalize();
    hex::encode(hash)
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_new(&cli.log_level).unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    // Parse signing key.
    let key_bytes = hex::decode(cli.secret_key.trim())
        .expect("FAUCET_SECRET_KEY must be valid hex");
    assert_eq!(key_bytes.len(), 32, "secret key must be 32 bytes");
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&key_bytes);
    let signing_key = SigningKey::from_bytes(&arr);

    let faucet_address = address_from_signing_key(&signing_key);
    info!(%faucet_address, "POLAY Faucet starting");

    let state = Arc::new(AppState {
        rpc_url: cli.rpc_url,
        signing_key,
        faucet_address: faucet_address.clone(),
        drip_amount: cli.drip_amount,
        cooldown: Duration::from_secs(cli.cooldown_secs),
        max_drips_per_day: cli.max_drips_per_day,
        rate_limits: DashMap::new(),
        http: reqwest::Client::new(),
    });

    let app = Router::new()
        .route("/health", get(health))
        .route("/info", get(info_handler))
        .route("/drip", post(drip))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr: SocketAddr = cli.listen.parse().expect("invalid listen address");
    info!(%addr, "faucet listening");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
