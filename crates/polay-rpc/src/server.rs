//! JSON-RPC server for the POLAY gaming blockchain.
//!
//! Exposes all read-only state queries and transaction submission over a
//! jsonrpsee HTTP+WS server with CORS enabled so browser-based game clients
//! can interact with the chain directly.
//!
//! WebSocket connections can subscribe to real-time chain events via the
//! `polay_subscribeNewBlocks`, `polay_subscribeNewTransactions`, and
//! `polay_subscribeEvents` subscription methods.

use std::net::SocketAddr;
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use jsonrpsee::server::{RpcModule, Server, ServerHandle};
use jsonrpsee::types::ErrorObjectOwned;
use jsonrpsee::SubscriptionMessage;
use tower_http::cors::{Any, CorsLayer};
use tracing::{error, info};

use polay_config::ChainConfig;
use polay_crypto::PolayPublicKey;
use polay_execution::gas::GasSchedule;
use polay_mempool::Mempool;
use polay_state::{StateStore, StateView};
use polay_types::{Address, Attestor, Hash, SignedTransaction, Transaction};

use crate::error::RpcError;
use crate::event_bus::{ChainEvent, EventBus};
use crate::rate_limiter::{IpRateLimiter, SubmissionThrottle};
use crate::types::{
    AccountResponse, AssetBalanceResponse, AssetClassResponse, BlockResponse, ChainInfoResponse,
    EpochInfoResponse, EventResponse, GasEstimateResponse, HealthResponse, InflationRateResponse,
    ListingResponse, MatchResultResponse, NetworkStatsResponse, NodeInfoResponse, ProfileResponse,
    ProposalResponse, ReceiptResponse, SubmitTransactionResponse, SupplyInfoResponse,
    TransactionWithStatus, UnbondingEntryResponse, ValidatorResponse,
};

// ---------------------------------------------------------------------------
// Server start time (for uptime tracking)
// ---------------------------------------------------------------------------

/// Returns the `Instant` at which the RPC module was first initialized.
fn start_time() -> Instant {
    static START: OnceLock<Instant> = OnceLock::new();
    *START.get_or_init(Instant::now)
}

// ---------------------------------------------------------------------------
// RpcServer context
// ---------------------------------------------------------------------------

/// Shared state threaded through every RPC handler.
pub struct RpcServer {
    pub store: Arc<dyn StateStore>,
    pub mempool: Arc<Mempool>,
    pub chain_config: ChainConfig,
    pub event_bus: Arc<EventBus>,
    /// Rate limiter for transaction submissions.
    pub submission_throttle: Arc<SubmissionThrottle>,
    /// Per-IP rate limiter for all RPC requests.
    pub ip_rate_limiter: Arc<IpRateLimiter>,
}

impl RpcServer {
    /// Create a new `RpcServer` context.
    pub fn new(
        store: Arc<dyn StateStore>,
        mempool: Arc<Mempool>,
        chain_config: ChainConfig,
        event_bus: Arc<EventBus>,
    ) -> Self {
        let max_per_sec = chain_config.rpc_max_submissions_per_second;
        Self {
            store,
            mempool,
            chain_config,
            event_bus,
            submission_throttle: Arc::new(SubmissionThrottle::new(max_per_sec)),
            ip_rate_limiter: Arc::new(IpRateLimiter::new(
                200, // 200 requests/sec per IP
                50,  // 50 concurrent connections per IP
            )),
        }
    }
}

// ---------------------------------------------------------------------------
// Build RPC module
// ---------------------------------------------------------------------------

/// Helper to produce a typed `Ok` so the compiler can infer `ErrorObjectOwned`
/// on the `Err` side of every async RPC handler.
#[inline]
fn ok<T>(val: T) -> Result<T, ErrorObjectOwned> {
    Ok(val)
}

/// Register all `polay_*` RPC methods on a [`RpcModule`].
pub fn build_rpc_module(ctx: Arc<RpcServer>) -> Result<RpcModule<()>, RpcError> {
    let mut module = RpcModule::new(());

    // -----------------------------------------------------------------------
    // polay_submitTransaction
    // -----------------------------------------------------------------------
    {
        let ctx = Arc::clone(&ctx);
        module
            .register_async_method("polay_submitTransaction", move |params, _, _| {
                let ctx = Arc::clone(&ctx);
                async move {
                    // -- Rate limiting check -----------------------------------
                    if !ctx.submission_throttle.check_and_increment() {
                        return Err(ErrorObjectOwned::owned(
                            -32005,
                            "rate limit exceeded: too many submissions per second",
                            None::<()>,
                        ));
                    }

                    let signed_tx: SignedTransaction = params.one()?;
                    let tx_hash = signed_tx.tx_hash;

                    // -- Ed25519 signature verification -------------------------

                    // 1. Verify the signer_pubkey is exactly 32 bytes.
                    if signed_tx.signer_pubkey.len() != 32 {
                        return Err(ErrorObjectOwned::owned(
                            -32000,
                            format!(
                                "signer_pubkey must be 32 bytes, got {}",
                                signed_tx.signer_pubkey.len()
                            ),
                            None::<()>,
                        ));
                    }

                    let pubkey_bytes: [u8; 32] = signed_tx.signer_pubkey[..32]
                        .try_into()
                        .expect("length already checked");
                    let pubkey = PolayPublicKey::from_bytes(&pubkey_bytes).map_err(|e| {
                        ErrorObjectOwned::owned(
                            -32000,
                            format!("invalid signer public key: {e}"),
                            None::<()>,
                        )
                    })?;

                    // 2. Derive address from pubkey and verify it matches the
                    //    expected identity. For session-signed txs, the pubkey
                    //    must derive to the session address (not the signer).
                    let derived_address = pubkey.address();
                    if let Some(session_addr) = &signed_tx.transaction.session {
                        if derived_address != *session_addr {
                            return Err(ErrorObjectOwned::owned(
                                -32000,
                                format!(
                                    "session pubkey derives address {}, but transaction.session is {}",
                                    derived_address, session_addr,
                                ),
                                None::<()>,
                            ));
                        }
                    } else if derived_address != signed_tx.transaction.signer {
                        return Err(ErrorObjectOwned::owned(
                            -32000,
                            format!(
                                "signer_pubkey derives address {}, but transaction signer is {}",
                                derived_address, signed_tx.transaction.signer
                            ),
                            None::<()>,
                        ));
                    }

                    // 3. Recompute tx_hash and verify it matches.
                    let expected_hash =
                        polay_crypto::hash_transaction(&signed_tx.transaction).map_err(|e| {
                            ErrorObjectOwned::owned(
                                -32000,
                                format!("failed to compute tx hash: {e}"),
                                None::<()>,
                            )
                        })?;
                    if expected_hash != signed_tx.tx_hash {
                        return Err(ErrorObjectOwned::owned(
                            -32000,
                            format!(
                                "tx_hash mismatch: expected {}, got {}",
                                expected_hash, signed_tx.tx_hash
                            ),
                            None::<()>,
                        ));
                    }

                    // 4. Verify the Ed25519 signature against the session or
                    //    account pubkey. For session txs, we verify the
                    //    signature directly (not via verify_transaction_with_key
                    //    which enforces pubkey->signer match).
                    if signed_tx.transaction.session.is_some() {
                        let payload = polay_crypto::build_tx_signing_payload(
                            &signed_tx.transaction,
                        )
                        .map_err(|e| {
                            ErrorObjectOwned::owned(
                                -32000,
                                format!("failed to build signing payload: {e}"),
                                None::<()>,
                            )
                        })?;
                        pubkey.verify(&payload, &signed_tx.signature).map_err(
                            |e| {
                                ErrorObjectOwned::owned(
                                    -32000,
                                    format!("session signature verification failed: {e}"),
                                    None::<()>,
                                )
                            },
                        )?;
                    } else {
                        polay_crypto::verify_transaction_with_key(&signed_tx, &pubkey)
                            .map_err(|e| {
                                ErrorObjectOwned::owned(
                                    -32000,
                                    format!("signature verification failed: {e}"),
                                    None::<()>,
                                )
                            })?;
                    }

                    // -- Insert into mempool ------------------------------------

                    ctx.mempool.insert(signed_tx).map_err(|e| {
                        ErrorObjectOwned::owned(
                            -32000,
                            format!("mempool rejected transaction: {e}"),
                            None::<()>,
                        )
                    })?;

                    info!(tx_hash = %tx_hash, "transaction submitted via RPC");

                    ok(SubmitTransactionResponse {
                        tx_hash: tx_hash.to_hex(),
                    })
                }
            })
            .map_err(|e| RpcError::RegistrationError(e.to_string()))?;
    }

    // -----------------------------------------------------------------------
    // polay_getBlock
    // -----------------------------------------------------------------------
    {
        let ctx = Arc::clone(&ctx);
        module
            .register_async_method("polay_getBlock", move |params, _, _| {
                let ctx = Arc::clone(&ctx);
                async move {
                    let height: u64 = params.one()?;
                    let view = StateView::new(ctx.store.as_ref());
                    let block = view.get_block(height).map_err(state_err)?;
                    ok(block.map(BlockResponse::from))
                }
            })
            .map_err(|e| RpcError::RegistrationError(e.to_string()))?;
    }

    // -----------------------------------------------------------------------
    // polay_getLatestBlock
    // -----------------------------------------------------------------------
    {
        let ctx = Arc::clone(&ctx);
        module
            .register_async_method("polay_getLatestBlock", move |_params, _, _| {
                let ctx = Arc::clone(&ctx);
                async move {
                    let view = StateView::new(ctx.store.as_ref());
                    let height = view.get_chain_height().map_err(state_err)?;
                    if height == 0 {
                        return ok(None::<BlockResponse>);
                    }
                    let block = view.get_block(height).map_err(state_err)?;
                    ok(block.map(BlockResponse::from))
                }
            })
            .map_err(|e| RpcError::RegistrationError(e.to_string()))?;
    }

    // -----------------------------------------------------------------------
    // polay_getAccount
    // -----------------------------------------------------------------------
    {
        let ctx = Arc::clone(&ctx);
        module
            .register_async_method("polay_getAccount", move |params, _, _| {
                let ctx = Arc::clone(&ctx);
                async move {
                    let address_hex: String = params.one()?;
                    let address = parse_address(&address_hex)?;
                    let view = StateView::new(ctx.store.as_ref());
                    let acct = view.get_account(&address).map_err(state_err)?;
                    ok(acct.map(AccountResponse::from))
                }
            })
            .map_err(|e| RpcError::RegistrationError(e.to_string()))?;
    }

    // -----------------------------------------------------------------------
    // polay_getBalance
    // -----------------------------------------------------------------------
    {
        let ctx = Arc::clone(&ctx);
        module
            .register_async_method("polay_getBalance", move |params, _, _| {
                let ctx = Arc::clone(&ctx);
                async move {
                    let address_hex: String = params.one()?;
                    let address = parse_address(&address_hex)?;
                    let view = StateView::new(ctx.store.as_ref());
                    let balance = view.get_balance(&address).map_err(state_err)?;
                    ok(balance)
                }
            })
            .map_err(|e| RpcError::RegistrationError(e.to_string()))?;
    }

    // -----------------------------------------------------------------------
    // polay_getAssetClass
    // -----------------------------------------------------------------------
    {
        let ctx = Arc::clone(&ctx);
        module
            .register_async_method("polay_getAssetClass", move |params, _, _| {
                let ctx = Arc::clone(&ctx);
                async move {
                    let id_hex: String = params.one()?;
                    let id = parse_hash(&id_hex)?;
                    let view = StateView::new(ctx.store.as_ref());
                    let asset = view.get_asset_class(&id).map_err(state_err)?;
                    ok(asset.map(AssetClassResponse::from))
                }
            })
            .map_err(|e| RpcError::RegistrationError(e.to_string()))?;
    }

    // -----------------------------------------------------------------------
    // polay_getAssetBalance
    // -----------------------------------------------------------------------
    {
        let ctx = Arc::clone(&ctx);
        module
            .register_async_method("polay_getAssetBalance", move |params, _, _| {
                let ctx = Arc::clone(&ctx);
                async move {
                    let (asset_class_id_hex, owner_hex): (String, String) = params.parse()?;
                    let asset_class_id = parse_hash(&asset_class_id_hex)?;
                    let owner = parse_address(&owner_hex)?;
                    let view = StateView::new(ctx.store.as_ref());
                    let amount = view
                        .get_asset_balance(&asset_class_id, &owner)
                        .map_err(state_err)?;
                    ok(AssetBalanceResponse {
                        owner: owner.to_hex(),
                        asset_class_id: asset_class_id.to_hex(),
                        amount,
                    })
                }
            })
            .map_err(|e| RpcError::RegistrationError(e.to_string()))?;
    }

    // -----------------------------------------------------------------------
    // polay_getListing
    // -----------------------------------------------------------------------
    {
        let ctx = Arc::clone(&ctx);
        module
            .register_async_method("polay_getListing", move |params, _, _| {
                let ctx = Arc::clone(&ctx);
                async move {
                    let id_hex: String = params.one()?;
                    let id = parse_hash(&id_hex)?;
                    let view = StateView::new(ctx.store.as_ref());
                    let listing = view.get_listing(&id).map_err(state_err)?;
                    ok(listing.map(ListingResponse::from))
                }
            })
            .map_err(|e| RpcError::RegistrationError(e.to_string()))?;
    }

    // -----------------------------------------------------------------------
    // polay_getProfile
    // -----------------------------------------------------------------------
    {
        let ctx = Arc::clone(&ctx);
        module
            .register_async_method("polay_getProfile", move |params, _, _| {
                let ctx = Arc::clone(&ctx);
                async move {
                    let address_hex: String = params.one()?;
                    let address = parse_address(&address_hex)?;
                    let view = StateView::new(ctx.store.as_ref());
                    let profile = view.get_profile(&address).map_err(state_err)?;
                    ok(profile.map(ProfileResponse::from))
                }
            })
            .map_err(|e| RpcError::RegistrationError(e.to_string()))?;
    }

    // -----------------------------------------------------------------------
    // polay_getAchievements
    //
    // MVP: The state store does not support prefix iteration, so we return an
    // empty vec. A future version will add a prefix-scan capability.
    // -----------------------------------------------------------------------
    {
        module
            .register_async_method("polay_getAchievements", move |_params, _, _| async move {
                // TODO: implement prefix iteration over achievements
                ok(Vec::<serde_json::Value>::new())
            })
            .map_err(|e| RpcError::RegistrationError(e.to_string()))?;
    }

    // -----------------------------------------------------------------------
    // polay_getValidator
    // -----------------------------------------------------------------------
    {
        let ctx = Arc::clone(&ctx);
        module
            .register_async_method("polay_getValidator", move |params, _, _| {
                let ctx = Arc::clone(&ctx);
                async move {
                    let address_hex: String = params.one()?;
                    let address = parse_address(&address_hex)?;
                    let view = StateView::new(ctx.store.as_ref());
                    let validator = view.get_validator(&address).map_err(state_err)?;
                    ok(validator.map(ValidatorResponse::from))
                }
            })
            .map_err(|e| RpcError::RegistrationError(e.to_string()))?;
    }

    // -----------------------------------------------------------------------
    // polay_getAttestor
    // -----------------------------------------------------------------------
    {
        let ctx = Arc::clone(&ctx);
        module
            .register_async_method("polay_getAttestor", move |params, _, _| {
                let ctx = Arc::clone(&ctx);
                async move {
                    let address_hex: String = params.one()?;
                    let address = parse_address(&address_hex)?;
                    let view = StateView::new(ctx.store.as_ref());
                    let attestor: Option<Attestor> =
                        view.get_attestor(&address).map_err(state_err)?;
                    ok(attestor)
                }
            })
            .map_err(|e| RpcError::RegistrationError(e.to_string()))?;
    }

    // -----------------------------------------------------------------------
    // polay_getMatchResult
    // -----------------------------------------------------------------------
    {
        let ctx = Arc::clone(&ctx);
        module
            .register_async_method("polay_getMatchResult", move |params, _, _| {
                let ctx = Arc::clone(&ctx);
                async move {
                    let match_id_hex: String = params.one()?;
                    let match_id = parse_hash(&match_id_hex)?;
                    let view = StateView::new(ctx.store.as_ref());
                    let result = view.get_match_result(&match_id).map_err(state_err)?;
                    ok(result.map(MatchResultResponse::from))
                }
            })
            .map_err(|e| RpcError::RegistrationError(e.to_string()))?;
    }

    // -----------------------------------------------------------------------
    // polay_getChainInfo
    // -----------------------------------------------------------------------
    {
        let ctx = Arc::clone(&ctx);
        module
            .register_async_method("polay_getChainInfo", move |_params, _, _| {
                let ctx = Arc::clone(&ctx);
                async move {
                    let view = StateView::new(ctx.store.as_ref());
                    let height = view.get_chain_height().map_err(state_err)?;
                    let latest_hash = view.get_latest_hash().map_err(state_err)?;

                    // Fetch the latest block to get its timestamp and chain_id.
                    let (chain_id, block_time) = if height > 0 {
                        match view.get_block(height).map_err(state_err)? {
                            Some(block) => (block.header.chain_id.clone(), block.header.timestamp),
                            None => ("polay".to_string(), 0),
                        }
                    } else {
                        ("polay".to_string(), 0)
                    };

                    // Compute the current state root.
                    let state_root = polay_state::compute_state_root(ctx.store.as_ref())
                        .map_err(state_err)?
                        .root
                        .to_hex();

                    ok(ChainInfoResponse {
                        chain_id,
                        height,
                        latest_hash: latest_hash.to_hex(),
                        state_root,
                        block_time,
                    })
                }
            })
            .map_err(|e| RpcError::RegistrationError(e.to_string()))?;
    }

    // -----------------------------------------------------------------------
    // polay_getMempoolSize
    // -----------------------------------------------------------------------
    {
        let ctx = Arc::clone(&ctx);
        module
            .register_async_method("polay_getMempoolSize", move |_params, _, _| {
                let ctx = Arc::clone(&ctx);
                async move { ok(ctx.mempool.size()) }
            })
            .map_err(|e| RpcError::RegistrationError(e.to_string()))?;
    }

    // -----------------------------------------------------------------------
    // polay_getTransaction  (check mempool first, then block index)
    // -----------------------------------------------------------------------
    {
        let ctx = Arc::clone(&ctx);
        module
            .register_async_method("polay_getTransaction", move |params, _, _| {
                let ctx = Arc::clone(&ctx);
                async move {
                    let tx_hash_hex: String = params.one()?;
                    let tx_hash = parse_hash(&tx_hash_hex)?;

                    // Check mempool first for pending transactions.
                    if let Some(tx) = ctx.mempool.get(&tx_hash) {
                        return ok(Some(TransactionWithStatus {
                            transaction: serde_json::to_value(&tx).unwrap_or_default(),
                            status: "pending".to_string(),
                            receipt: None,
                            block_height: None,
                        }));
                    }

                    // Check confirmed transactions in the block index.
                    let view = StateView::new(ctx.store.as_ref());
                    if let Some(location) = view.get_tx_location(&tx_hash).map_err(state_err)? {
                        let receipt = view.get_receipt(&tx_hash).map_err(state_err)?;
                        let block = view.get_block(location.block_height).map_err(state_err)?;

                        // Find the transaction in the block.
                        let tx_value = block
                            .and_then(|b| {
                                b.transactions
                                    .get(location.tx_index as usize)
                                    .map(|tx| serde_json::to_value(tx).unwrap_or_default())
                            })
                            .unwrap_or_default();

                        let receipt_response =
                            receipt.map(|r| ReceiptResponse::from_receipt(&r, location.tx_index));

                        return ok(Some(TransactionWithStatus {
                            transaction: tx_value,
                            status: "confirmed".to_string(),
                            receipt: receipt_response,
                            block_height: Some(location.block_height),
                        }));
                    }

                    ok(None::<TransactionWithStatus>)
                }
            })
            .map_err(|e| RpcError::RegistrationError(e.to_string()))?;
    }

    // -----------------------------------------------------------------------
    // polay_getTransactionReceipt
    // -----------------------------------------------------------------------
    {
        let ctx = Arc::clone(&ctx);
        module
            .register_async_method("polay_getTransactionReceipt", move |params, _, _| {
                let ctx = Arc::clone(&ctx);
                async move {
                    let tx_hash_hex: String = params.one()?;
                    let tx_hash = parse_hash(&tx_hash_hex)?;
                    let view = StateView::new(ctx.store.as_ref());
                    let receipt = view.get_receipt(&tx_hash).map_err(state_err)?;
                    let location = view.get_tx_location(&tx_hash).map_err(state_err)?;

                    let response = match (receipt, location) {
                        (Some(r), Some(loc)) => {
                            Some(ReceiptResponse::from_receipt(&r, loc.tx_index))
                        }
                        (Some(r), None) => Some(ReceiptResponse::from_receipt(&r, 0)),
                        _ => None,
                    };
                    ok(response)
                }
            })
            .map_err(|e| RpcError::RegistrationError(e.to_string()))?;
    }

    // -----------------------------------------------------------------------
    // polay_getBlockReceipts
    // -----------------------------------------------------------------------
    {
        let ctx = Arc::clone(&ctx);
        module
            .register_async_method("polay_getBlockReceipts", move |params, _, _| {
                let ctx = Arc::clone(&ctx);
                async move {
                    let height: u64 = params.one()?;
                    let view = StateView::new(ctx.store.as_ref());
                    let block = view.get_block(height).map_err(state_err)?;

                    let receipts: Vec<ReceiptResponse> = match block {
                        Some(b) => {
                            let mut result = Vec::with_capacity(b.transactions.len());
                            for (idx, tx) in b.transactions.iter().enumerate() {
                                if let Some(receipt) =
                                    view.get_receipt(&tx.tx_hash).map_err(state_err)?
                                {
                                    result
                                        .push(ReceiptResponse::from_receipt(&receipt, idx as u32));
                                }
                            }
                            result
                        }
                        None => Vec::new(),
                    };
                    ok(receipts)
                }
            })
            .map_err(|e| RpcError::RegistrationError(e.to_string()))?;
    }

    // -----------------------------------------------------------------------
    // polay_getBlockEvents
    // -----------------------------------------------------------------------
    {
        let ctx = Arc::clone(&ctx);
        module
            .register_async_method("polay_getBlockEvents", move |params, _, _| {
                let ctx = Arc::clone(&ctx);
                async move {
                    let height: u64 = params.one()?;
                    let view = StateView::new(ctx.store.as_ref());
                    let events = view.get_block_events(height).map_err(state_err)?;
                    let response: Vec<EventResponse> = events
                        .unwrap_or_default()
                        .into_iter()
                        .map(EventResponse::from)
                        .collect();
                    ok(response)
                }
            })
            .map_err(|e| RpcError::RegistrationError(e.to_string()))?;
    }

    // -----------------------------------------------------------------------
    // polay_getUnbondingEntries
    // -----------------------------------------------------------------------
    {
        let ctx = Arc::clone(&ctx);
        module
            .register_async_method("polay_getUnbondingEntries", move |params, _, _| {
                let ctx = Arc::clone(&ctx);
                async move {
                    let address_hex: String = params.one()?;
                    let address = parse_address(&address_hex)?;
                    let view = StateView::new(ctx.store.as_ref());
                    let entries = view.get_unbonding_entries(&address).map_err(state_err)?;
                    let response: Vec<UnbondingEntryResponse> = entries
                        .into_iter()
                        .map(UnbondingEntryResponse::from)
                        .collect();
                    ok(response)
                }
            })
            .map_err(|e| RpcError::RegistrationError(e.to_string()))?;
    }

    // -----------------------------------------------------------------------
    // polay_estimateGas
    // -----------------------------------------------------------------------
    {
        let ctx = Arc::clone(&ctx);
        module
            .register_async_method("polay_estimateGas", move |params, _, _| {
                let ctx = Arc::clone(&ctx);
                async move {
                    let tx: Transaction = params.one()?;
                    let gas = GasSchedule::total_gas(
                        &tx,
                        ctx.chain_config.base_gas,
                        ctx.chain_config.gas_per_byte,
                    );
                    let gas_price = ctx.chain_config.min_gas_price;
                    let estimated_fee = GasSchedule::fee(gas, gas_price);
                    ok(GasEstimateResponse {
                        gas,
                        estimated_fee,
                        gas_price,
                    })
                }
            })
            .map_err(|e| RpcError::RegistrationError(e.to_string()))?;
    }

    // -----------------------------------------------------------------------
    // polay_getProposal
    // -----------------------------------------------------------------------
    {
        let ctx = Arc::clone(&ctx);
        module
            .register_async_method("polay_getProposal", move |params, _, _| {
                let ctx = Arc::clone(&ctx);
                async move {
                    let id_hex: String = params.one()?;
                    let id = parse_hash(&id_hex)?;
                    let view = StateView::new(ctx.store.as_ref());
                    let proposal = view.get_proposal(&id).map_err(state_err)?;
                    ok(proposal.map(ProposalResponse::from))
                }
            })
            .map_err(|e| RpcError::RegistrationError(e.to_string()))?;
    }

    // -----------------------------------------------------------------------
    // polay_getProposals
    // -----------------------------------------------------------------------
    {
        let ctx = Arc::clone(&ctx);
        module
            .register_async_method("polay_getProposals", move |_params, _, _| {
                let ctx = Arc::clone(&ctx);
                async move {
                    let view = StateView::new(ctx.store.as_ref());
                    let ids = view.get_proposal_list().map_err(state_err)?;
                    let mut proposals = Vec::with_capacity(ids.len());
                    for id in &ids {
                        if let Some(p) = view.get_proposal(id).map_err(state_err)? {
                            proposals.push(ProposalResponse::from(p));
                        }
                    }
                    ok(proposals)
                }
            })
            .map_err(|e| RpcError::RegistrationError(e.to_string()))?;
    }

    // -----------------------------------------------------------------------
    // polay_getSession
    // -----------------------------------------------------------------------
    {
        let ctx = Arc::clone(&ctx);
        module
            .register_async_method("polay_getSession", move |params, _, _| {
                let ctx = Arc::clone(&ctx);
                async move {
                    let (granter_hex, session_hex): (String, String) = params.parse()?;
                    let granter = parse_address(&granter_hex)?;
                    let session_address = parse_address(&session_hex)?;
                    let view = StateView::new(ctx.store.as_ref());
                    let session = view
                        .get_session(&granter, &session_address)
                        .map_err(state_err)?;
                    ok(session)
                }
            })
            .map_err(|e| RpcError::RegistrationError(e.to_string()))?;
    }

    // -----------------------------------------------------------------------
    // polay_getActiveSessions
    // -----------------------------------------------------------------------
    {
        let ctx = Arc::clone(&ctx);
        module
            .register_async_method("polay_getActiveSessions", move |params, _, _| {
                let ctx = Arc::clone(&ctx);
                async move {
                    let granter_hex: String = params.one()?;
                    let granter = parse_address(&granter_hex)?;
                    let view = StateView::new(ctx.store.as_ref());
                    let all_sessions =
                        view.get_sessions_for_granter(&granter).map_err(state_err)?;
                    let height = view.get_chain_height().map_err(state_err)?;
                    // Filter to active (not revoked, not expired) sessions.
                    let active: Vec<_> = all_sessions
                        .into_iter()
                        .filter(|g| g.is_valid(height))
                        .collect();
                    ok(active)
                }
            })
            .map_err(|e| RpcError::RegistrationError(e.to_string()))?;
    }

    // -----------------------------------------------------------------------
    // polay_getEpochInfo
    // -----------------------------------------------------------------------
    {
        let ctx = Arc::clone(&ctx);
        module
            .register_async_method("polay_getEpochInfo", move |params, _, _| {
                let ctx = Arc::clone(&ctx);
                async move {
                    let epoch: u64 = params.one()?;
                    let view = StateView::new(ctx.store.as_ref());
                    let info = view.get_epoch_info(epoch).map_err(state_err)?;
                    ok(info.map(EpochInfoResponse::from))
                }
            })
            .map_err(|e| RpcError::RegistrationError(e.to_string()))?;
    }

    // -----------------------------------------------------------------------
    // polay_getCurrentEpoch
    // -----------------------------------------------------------------------
    {
        let ctx = Arc::clone(&ctx);
        module
            .register_async_method("polay_getCurrentEpoch", move |_params, _, _| {
                let ctx = Arc::clone(&ctx);
                async move {
                    let view = StateView::new(ctx.store.as_ref());
                    let epoch = view
                        .get_current_epoch(ctx.chain_config.epoch_length)
                        .map_err(state_err)?;
                    ok(epoch)
                }
            })
            .map_err(|e| RpcError::RegistrationError(e.to_string()))?;
    }

    // -----------------------------------------------------------------------
    // polay_getActiveValidatorSet
    // -----------------------------------------------------------------------
    {
        let ctx = Arc::clone(&ctx);
        module
            .register_async_method("polay_getActiveValidatorSet", move |_params, _, _| {
                let ctx = Arc::clone(&ctx);
                async move {
                    let view = StateView::new(ctx.store.as_ref());
                    let addrs = view
                        .get_active_validator_set()
                        .map_err(state_err)?
                        .unwrap_or_default();

                    // For each address, look up the full validator info.
                    let mut responses = Vec::with_capacity(addrs.len());
                    for addr in &addrs {
                        if let Some(v) = view.get_validator(addr).map_err(state_err)? {
                            responses.push(ValidatorResponse::from(v));
                        }
                    }
                    ok(responses)
                }
            })
            .map_err(|e| RpcError::RegistrationError(e.to_string()))?;
    }

    // =======================================================================
    // Economics / Supply
    // =======================================================================

    // -----------------------------------------------------------------------
    // polay_getSupplyInfo
    // -----------------------------------------------------------------------
    {
        let ctx = Arc::clone(&ctx);
        module
            .register_async_method("polay_getSupplyInfo", move |_params, _, _| {
                let ctx = Arc::clone(&ctx);
                async move {
                    let view = StateView::new(ctx.store.as_ref());
                    let supply = view.get_supply_info().map_err(state_err)?;
                    ok(supply.map(SupplyInfoResponse::from))
                }
            })
            .map_err(|e| RpcError::RegistrationError(e.to_string()))?;
    }

    // -----------------------------------------------------------------------
    // polay_getInflationRate
    // -----------------------------------------------------------------------
    {
        let ctx = Arc::clone(&ctx);
        module
            .register_async_method("polay_getInflationRate", move |_params, _, _| {
                let ctx = Arc::clone(&ctx);
                async move {
                    let view = StateView::new(ctx.store.as_ref());
                    let supply = view
                        .get_supply_info()
                        .map_err(state_err)?
                        .unwrap_or_default();
                    let total_staked = supply.total_staked;
                    let inflation = &ctx.chain_config.inflation_params;
                    let rate = inflation.initial_rate_bps.max(inflation.min_rate_bps);
                    let epoch_reward = polay_staking::StakingModule::calculate_epoch_rewards(
                        supply.total_supply,
                        total_staked,
                        inflation,
                        ctx.chain_config.epoch_length,
                    );
                    ok(InflationRateResponse {
                        rate_bps: rate,
                        epoch_reward,
                    })
                }
            })
            .map_err(|e| RpcError::RegistrationError(e.to_string()))?;
    }

    // -----------------------------------------------------------------------
    // polay_getBlockReward
    // -----------------------------------------------------------------------
    {
        let ctx = Arc::clone(&ctx);
        module
            .register_async_method("polay_getBlockReward", move |_params, _, _| {
                let ctx = Arc::clone(&ctx);
                async move {
                    let view = StateView::new(ctx.store.as_ref());
                    let supply = view
                        .get_supply_info()
                        .map_err(state_err)?
                        .unwrap_or_default();
                    let total_staked = supply.total_staked;
                    let inflation = &ctx.chain_config.inflation_params;
                    let epoch_reward = polay_staking::StakingModule::calculate_epoch_rewards(
                        supply.total_supply,
                        total_staked,
                        inflation,
                        ctx.chain_config.epoch_length,
                    );
                    // Block reward = epoch_reward / epoch_length
                    let block_reward = if ctx.chain_config.epoch_length > 0 {
                        epoch_reward / ctx.chain_config.epoch_length
                    } else {
                        0
                    };
                    ok(block_reward)
                }
            })
            .map_err(|e| RpcError::RegistrationError(e.to_string()))?;
    }

    // =======================================================================
    // Health & Node Info
    // =======================================================================

    // -----------------------------------------------------------------------
    // polay_health — lightweight health check
    // -----------------------------------------------------------------------
    {
        let ctx = Arc::clone(&ctx);
        module
            .register_async_method("polay_health", move |_params, _, _| {
                let ctx = Arc::clone(&ctx);
                async move {
                    let view = StateView::new(ctx.store.as_ref());
                    let height = view.get_chain_height().map_err(state_err)?;
                    ok(HealthResponse {
                        status: "healthy".to_string(),
                        height,
                        syncing: false,
                    })
                }
            })
            .map_err(|e| RpcError::RegistrationError(e.to_string()))?;
    }

    // -----------------------------------------------------------------------
    // polay_getNodeInfo — detailed node information
    // -----------------------------------------------------------------------
    {
        let ctx = Arc::clone(&ctx);
        module
            .register_async_method("polay_getNodeInfo", move |_params, _, _| {
                let ctx = Arc::clone(&ctx);
                async move {
                    let view = StateView::new(ctx.store.as_ref());
                    let height = view.get_chain_height().map_err(state_err)?;
                    let latest_hash = view.get_latest_hash().map_err(state_err)?;
                    let state_root = polay_state::compute_state_root(ctx.store.as_ref())
                        .map_err(state_err)?
                        .root
                        .to_hex();
                    let uptime_seconds = start_time().elapsed().as_secs();

                    ok(NodeInfoResponse {
                        chain_id: ctx.chain_config.chain_id.clone(),
                        node_version: env!("CARGO_PKG_VERSION").to_string(),
                        height,
                        latest_hash: latest_hash.to_hex(),
                        state_root,
                        peer_count: 0, // P2P peer count not available at RPC layer
                        mempool_size: ctx.mempool.size(),
                        uptime_seconds,
                        block_time_ms: ctx.chain_config.block_time_ms,
                    })
                }
            })
            .map_err(|e| RpcError::RegistrationError(e.to_string()))?;
    }

    // -----------------------------------------------------------------------
    // polay_getNetworkStats — network-level statistics
    // -----------------------------------------------------------------------
    {
        let ctx = Arc::clone(&ctx);
        module
            .register_async_method("polay_getNetworkStats", move |_params, _, _| {
                let ctx = Arc::clone(&ctx);
                async move {
                    let view = StateView::new(ctx.store.as_ref());
                    let height = view.get_chain_height().map_err(state_err)?;
                    let epoch = view
                        .get_current_epoch(ctx.chain_config.epoch_length)
                        .map_err(state_err)?;

                    // Count active validators and their total stake.
                    let active_addrs = view
                        .get_active_validator_set()
                        .map_err(state_err)?
                        .unwrap_or_default();
                    let active_validators = active_addrs.len();
                    let mut total_staked: u64 = 0;
                    for addr in &active_addrs {
                        if let Some(v) = view.get_validator(addr).map_err(state_err)? {
                            total_staked = total_staked.saturating_add(v.stake);
                        }
                    }

                    // Approximate total transactions from block history.
                    // A full implementation would track this in state; here we
                    // use a best-effort approach reading recent blocks.
                    let total_transactions = {
                        let sample_size = height.min(100);
                        let mut tx_count: u64 = 0;
                        for h in (height.saturating_sub(sample_size) + 1)..=height {
                            if let Some(block) = view.get_block(h).map_err(state_err)? {
                                tx_count += block.transactions.len() as u64;
                            }
                        }
                        if sample_size > 0 && height > sample_size {
                            // Extrapolate from sample
                            (tx_count as f64 / sample_size as f64 * height as f64) as u64
                        } else {
                            tx_count
                        }
                    };

                    ok(NetworkStatsResponse {
                        height,
                        total_transactions,
                        active_validators,
                        total_staked,
                        epoch,
                        block_time_ms: ctx.chain_config.block_time_ms,
                    })
                }
            })
            .map_err(|e| RpcError::RegistrationError(e.to_string()))?;
    }

    // =======================================================================
    // WebSocket Subscriptions
    // =======================================================================

    // -----------------------------------------------------------------------
    // polay_subscribeNewBlocks / polay_newBlock / polay_unsubscribeNewBlocks
    // -----------------------------------------------------------------------
    {
        let ctx = Arc::clone(&ctx);
        module
            .register_subscription(
                "polay_subscribeNewBlocks",
                "polay_newBlock",
                "polay_unsubscribeNewBlocks",
                move |_params, pending, _ctx, _extensions| {
                    let event_bus = Arc::clone(&ctx.event_bus);
                    async move {
                        let sink: jsonrpsee::SubscriptionSink = pending.accept().await?;
                        let mut rx = event_bus.subscribe();

                        while let Ok(event) = rx.recv().await {
                            if matches!(event, ChainEvent::NewBlock { .. }) {
                                let msg = SubscriptionMessage::from_json(&event)?;
                                if sink.send(msg).await.is_err() {
                                    break;
                                }
                            }
                        }
                        Ok(())
                    }
                },
            )
            .map_err(|e| RpcError::RegistrationError(e.to_string()))?;
    }

    // -----------------------------------------------------------------------
    // polay_subscribeNewTransactions / polay_newTransaction /
    // polay_unsubscribeNewTransactions
    // -----------------------------------------------------------------------
    {
        let ctx = Arc::clone(&ctx);
        module
            .register_subscription(
                "polay_subscribeNewTransactions",
                "polay_newTransaction",
                "polay_unsubscribeNewTransactions",
                move |_params, pending, _ctx, _extensions| {
                    let event_bus = Arc::clone(&ctx.event_bus);
                    async move {
                        let sink: jsonrpsee::SubscriptionSink = pending.accept().await?;
                        let mut rx = event_bus.subscribe();

                        while let Ok(event) = rx.recv().await {
                            let is_tx = matches!(
                                event,
                                ChainEvent::NewTransaction { .. }
                                    | ChainEvent::TransactionConfirmed { .. }
                            );
                            if is_tx {
                                let msg = SubscriptionMessage::from_json(&event)?;
                                if sink.send(msg).await.is_err() {
                                    break;
                                }
                            }
                        }
                        Ok(())
                    }
                },
            )
            .map_err(|e| RpcError::RegistrationError(e.to_string()))?;
    }

    // -----------------------------------------------------------------------
    // polay_subscribeEvents / polay_event / polay_unsubscribeEvents
    // -----------------------------------------------------------------------
    {
        let ctx = Arc::clone(&ctx);
        module
            .register_subscription(
                "polay_subscribeEvents",
                "polay_event",
                "polay_unsubscribeEvents",
                move |_params, pending, _ctx, _extensions| {
                    let event_bus = Arc::clone(&ctx.event_bus);
                    async move {
                        let sink: jsonrpsee::SubscriptionSink = pending.accept().await?;
                        let mut rx = event_bus.subscribe();

                        while let Ok(event) = rx.recv().await {
                            let msg = SubscriptionMessage::from_json(&event)?;
                            if sink.send(msg).await.is_err() {
                                break;
                            }
                        }
                        Ok(())
                    }
                },
            )
            .map_err(|e| RpcError::RegistrationError(e.to_string()))?;
    }

    Ok(module)
}

// ---------------------------------------------------------------------------
// Server start
// ---------------------------------------------------------------------------

/// Start the JSON-RPC HTTP+WebSocket server.
///
/// Returns a [`ServerHandle`] that can be used to shut the server down
/// gracefully.
pub async fn start_rpc_server(
    addr: &str,
    store: Arc<dyn StateStore>,
    mempool: Arc<Mempool>,
    chain_config: ChainConfig,
    event_bus: Arc<EventBus>,
) -> Result<ServerHandle, RpcError> {
    let cors = CorsLayer::new()
        .allow_methods(Any)
        .allow_origin(Any)
        .allow_headers(Any);

    let middleware = tower_04::ServiceBuilder::new().layer(cors);

    let socket_addr: SocketAddr = addr.parse().map_err(|e: std::net::AddrParseError| {
        RpcError::ServerStartError(format!("invalid listen address '{addr}': {e}"))
    })?;

    let server = Server::builder()
        .set_http_middleware(middleware)
        .max_request_body_size(2 * 1024 * 1024) // 2 MB max request
        .max_response_body_size(10 * 1024 * 1024) // 10 MB max response
        .max_connections(512) // 512 concurrent connections
        .build(socket_addr)
        .await
        .map_err(|e| RpcError::ServerStartError(format!("failed to bind RPC server: {e}")))?;

    let ctx = Arc::new(RpcServer::new(store, mempool, chain_config, event_bus));
    let module = build_rpc_module(ctx)?;

    let handle = server.start(module);
    info!(%socket_addr, "JSON-RPC HTTP+WS server listening");
    Ok(handle)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse a hex string into an [`Address`], returning a jsonrpsee error on
/// failure.
fn parse_address(hex: &str) -> Result<Address, ErrorObjectOwned> {
    Address::from_hex(hex)
        .map_err(|e| ErrorObjectOwned::owned(-32602, format!("invalid address: {e}"), None::<()>))
}

/// Parse a hex string into a [`Hash`], returning a jsonrpsee error on failure.
fn parse_hash(hex: &str) -> Result<Hash, ErrorObjectOwned> {
    Hash::from_hex(hex)
        .map_err(|e| ErrorObjectOwned::owned(-32602, format!("invalid hash: {e}"), None::<()>))
}

/// Convert a [`polay_state::StateError`] into a jsonrpsee error object.
fn state_err(e: polay_state::StateError) -> ErrorObjectOwned {
    error!(error = %e, "state access error in RPC handler");
    ErrorObjectOwned::owned(-32001, format!("state error: {e}"), None::<()>)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use polay_mempool::MempoolConfig;
    use polay_state::{MemoryStore, StateWriter};
    use polay_types::block::{Block, BlockHeader};
    use polay_types::transaction::TxLocation;
    use polay_types::{Event, Signature, Transaction, TransactionAction, TransactionReceipt};

    fn test_addr(byte: u8) -> Address {
        Address::new([byte; 32])
    }

    fn test_hash(byte: u8) -> Hash {
        Hash::new([byte; 32])
    }

    fn make_rpc_module(store: Arc<dyn StateStore>, mempool: Arc<Mempool>) -> RpcModule<()> {
        let config = ChainConfig::default();
        let event_bus = Arc::new(EventBus::new(64));
        let ctx = Arc::new(RpcServer::new(store, mempool, config, event_bus));
        build_rpc_module(ctx).unwrap()
    }

    fn seed_receipt(
        store: &dyn StateStore,
        tx_hash: Hash,
        height: u64,
        tx_index: u32,
        success: bool,
    ) {
        let writer = StateWriter::new(store);
        let receipt = if success {
            TransactionReceipt::success(
                tx_hash,
                height,
                500,
                21000,
                test_addr(1),
                vec![Event::new(
                    "bank",
                    "transfer",
                    vec![
                        ("from".into(), "alice".into()),
                        ("to".into(), "bob".into()),
                        ("amount".into(), "1000".into()),
                    ],
                )],
            )
        } else {
            TransactionReceipt::failure(
                tx_hash,
                height,
                200,
                10000,
                test_addr(1),
                "execution failed".into(),
            )
        };
        writer.set_receipt(&receipt).unwrap();
        writer
            .set_tx_location(
                &tx_hash,
                &TxLocation {
                    block_height: height,
                    tx_index,
                },
            )
            .unwrap();
    }

    fn seed_block(store: &dyn StateStore, height: u64, tx_hashes: &[Hash]) {
        let writer = StateWriter::new(store);
        let txs: Vec<SignedTransaction> = tx_hashes
            .iter()
            .map(|h| {
                SignedTransaction::new(
                    Transaction {
                        chain_id: "polay-devnet-1".into(),
                        nonce: 0,
                        signer: test_addr(1),
                        action: TransactionAction::Transfer {
                            to: test_addr(2),
                            amount: 100,
                        },
                        max_fee: 1000,
                        timestamp: 1_700_000_000,
                        session: None,
                        sponsor: None,
                    },
                    Signature::ZERO,
                    *h,
                    vec![0u8; 32],
                )
            })
            .collect();

        let header = BlockHeader {
            height,
            timestamp: 1_700_000_000,
            parent_hash: Hash::ZERO,
            state_root: Hash::ZERO,
            transactions_root: Hash::ZERO,
            proposer: test_addr(0xAA),
            chain_id: "polay-devnet-1".into(),
            hash: test_hash(0xFF),
        };
        let block = Block::new(header, txs);
        writer.store_block(&block).unwrap();
        writer.set_chain_height(height).unwrap();
    }

    #[tokio::test]
    async fn rpc_get_transaction_receipt_found() {
        let store: Arc<dyn StateStore> = Arc::new(MemoryStore::new());
        let mempool = Arc::new(Mempool::new(MempoolConfig::default()));
        let tx_hash = test_hash(0x11);
        seed_receipt(store.as_ref(), tx_hash, 10, 0, true);

        let module = make_rpc_module(store, mempool);
        let response: Option<ReceiptResponse> = module
            .call("polay_getTransactionReceipt", [tx_hash.to_hex()])
            .await
            .unwrap();

        let r = response.unwrap();
        assert_eq!(r.tx_hash, tx_hash.to_hex());
        assert_eq!(r.block_height, 10);
        assert_eq!(r.tx_index, 0);
        assert!(r.success);
        assert_eq!(r.gas_used, 21000);
        assert_eq!(r.events.len(), 1);
        assert!(r.error.is_none());
    }

    #[tokio::test]
    async fn rpc_get_transaction_receipt_not_found() {
        let store: Arc<dyn StateStore> = Arc::new(MemoryStore::new());
        let mempool = Arc::new(Mempool::new(MempoolConfig::default()));

        let module = make_rpc_module(store, mempool);
        let response: Option<ReceiptResponse> = module
            .call("polay_getTransactionReceipt", [test_hash(0xFF).to_hex()])
            .await
            .unwrap();

        assert!(response.is_none());
    }

    #[tokio::test]
    async fn rpc_get_block_receipts() {
        let store: Arc<dyn StateStore> = Arc::new(MemoryStore::new());
        let mempool = Arc::new(Mempool::new(MempoolConfig::default()));

        let tx1 = test_hash(0x11);
        let tx2 = test_hash(0x22);
        seed_receipt(store.as_ref(), tx1, 5, 0, true);
        seed_receipt(store.as_ref(), tx2, 5, 1, false);
        seed_block(store.as_ref(), 5, &[tx1, tx2]);

        let module = make_rpc_module(store, mempool);
        let response: Vec<ReceiptResponse> =
            module.call("polay_getBlockReceipts", [5u64]).await.unwrap();

        assert_eq!(response.len(), 2);
        assert!(response[0].success);
        assert!(!response[1].success);
        assert_eq!(response[0].tx_index, 0);
        assert_eq!(response[1].tx_index, 1);
    }

    #[tokio::test]
    async fn rpc_get_block_events() {
        let store: Arc<dyn StateStore> = Arc::new(MemoryStore::new());
        let mempool = Arc::new(Mempool::new(MempoolConfig::default()));

        let events = vec![
            Event::new("bank", "transfer", vec![("amount".into(), "100".into())]),
            Event::new("asset", "mint", vec![("amount".into(), "50".into())]),
        ];
        StateWriter::new(store.as_ref())
            .set_block_events(7, &events)
            .unwrap();

        let module = make_rpc_module(store, mempool);
        let response: Vec<EventResponse> =
            module.call("polay_getBlockEvents", [7u64]).await.unwrap();

        assert_eq!(response.len(), 2);
        assert_eq!(response[0].module, "bank");
        assert_eq!(response[0].action, "transfer");
        assert_eq!(response[1].module, "asset");
        assert_eq!(response[1].action, "mint");
    }

    #[tokio::test]
    async fn rpc_get_block_events_empty() {
        let store: Arc<dyn StateStore> = Arc::new(MemoryStore::new());
        let mempool = Arc::new(Mempool::new(MempoolConfig::default()));

        let module = make_rpc_module(store, mempool);
        let response: Vec<EventResponse> =
            module.call("polay_getBlockEvents", [999u64]).await.unwrap();

        assert!(response.is_empty());
    }

    #[tokio::test]
    async fn rpc_get_transaction_pending_from_mempool() {
        let store: Arc<dyn StateStore> = Arc::new(MemoryStore::new());
        let mempool = Arc::new(Mempool::new(MempoolConfig {
            verify_signature: false,
            min_fee: 0,
            ..MempoolConfig::default()
        }));

        let tx_hash = test_hash(0x11);
        let stx = SignedTransaction::new(
            Transaction {
                chain_id: "polay-devnet-1".into(),
                nonce: 0,
                signer: test_addr(1),
                action: TransactionAction::Transfer {
                    to: test_addr(2),
                    amount: 100,
                },
                max_fee: 1000,
                timestamp: 1_700_000_000,
                session: None,
                sponsor: None,
            },
            Signature::ZERO,
            tx_hash,
            vec![0u8; 32],
        );
        mempool.insert(stx).unwrap();

        let module = make_rpc_module(store, mempool);
        let response: Option<TransactionWithStatus> = module
            .call("polay_getTransaction", [tx_hash.to_hex()])
            .await
            .unwrap();

        let tx_status = response.unwrap();
        assert_eq!(tx_status.status, "pending");
        assert!(tx_status.receipt.is_none());
        assert!(tx_status.block_height.is_none());
    }

    #[tokio::test]
    async fn rpc_get_transaction_confirmed_with_receipt() {
        let store: Arc<dyn StateStore> = Arc::new(MemoryStore::new());
        let mempool = Arc::new(Mempool::new(MempoolConfig::default()));

        let tx_hash = test_hash(0x33);
        seed_receipt(store.as_ref(), tx_hash, 10, 2, true);
        seed_block(
            store.as_ref(),
            10,
            &[test_hash(0x11), test_hash(0x22), tx_hash],
        );

        let module = make_rpc_module(store, mempool);
        let response: Option<TransactionWithStatus> = module
            .call("polay_getTransaction", [tx_hash.to_hex()])
            .await
            .unwrap();

        let tx_status = response.unwrap();
        assert_eq!(tx_status.status, "confirmed");
        assert_eq!(tx_status.block_height, Some(10));
        let receipt = tx_status.receipt.unwrap();
        assert!(receipt.success);
        assert_eq!(receipt.tx_index, 2);
    }

    #[tokio::test]
    async fn rpc_get_transaction_not_found() {
        let store: Arc<dyn StateStore> = Arc::new(MemoryStore::new());
        let mempool = Arc::new(Mempool::new(MempoolConfig::default()));

        let module = make_rpc_module(store, mempool);
        let response: Option<TransactionWithStatus> = module
            .call("polay_getTransaction", [test_hash(0xFF).to_hex()])
            .await
            .unwrap();

        assert!(response.is_none());
    }

    // -- Health & node info tests --------------------------------------------

    #[tokio::test]
    async fn rpc_health_returns_expected_structure() {
        let store: Arc<dyn StateStore> = Arc::new(MemoryStore::new());
        let mempool = Arc::new(Mempool::new(MempoolConfig::default()));

        let module = make_rpc_module(store, mempool);
        let response: crate::types::HealthResponse =
            module.call("polay_health", Vec::<()>::new()).await.unwrap();

        assert_eq!(response.status, "healthy");
        assert_eq!(response.height, 0);
        assert!(!response.syncing);
    }

    #[tokio::test]
    async fn rpc_get_node_info_includes_version_and_chain_id() {
        let store: Arc<dyn StateStore> = Arc::new(MemoryStore::new());
        let mempool = Arc::new(Mempool::new(MempoolConfig::default()));

        let module = make_rpc_module(store, mempool);
        let response: crate::types::NodeInfoResponse = module
            .call("polay_getNodeInfo", Vec::<()>::new())
            .await
            .unwrap();

        assert_eq!(response.chain_id, "polay-devnet-1");
        assert!(!response.node_version.is_empty());
        assert_eq!(response.block_time_ms, 2000);
    }

    #[tokio::test]
    async fn rpc_get_network_stats_returns_expected_structure() {
        let store: Arc<dyn StateStore> = Arc::new(MemoryStore::new());
        let mempool = Arc::new(Mempool::new(MempoolConfig::default()));

        let module = make_rpc_module(store, mempool);
        let response: crate::types::NetworkStatsResponse = module
            .call("polay_getNetworkStats", Vec::<()>::new())
            .await
            .unwrap();

        assert_eq!(response.height, 0);
        assert_eq!(response.block_time_ms, 2000);
    }
}
