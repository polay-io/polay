//! `polay-crypto` — cryptographic primitives for the POLAY gaming blockchain.
//!
//! This crate provides:
//! - **Keypair management** — Ed25519 key generation, signing, and address derivation.
//! - **Public key operations** — signature verification and hex (de)serialization.
//! - **Hashing utilities** — SHA-256, transaction hashing, block header hashing, and Merkle trees.
//! - **Transaction signing** — deterministic payload construction, signing, and verification.

pub mod error;
pub mod hash;
pub mod keypair;
pub mod public_key;
pub mod transaction;

// Re-export the primary public API at crate root for convenience.
pub use error::{CryptoError, CryptoResult};
pub use hash::{hash_block_header, hash_transaction, merkle_root, sha256};
pub use keypair::PolayKeypair;
pub use public_key::PolayPublicKey;
pub use transaction::{build_tx_signing_payload, sign_transaction, verify_transaction_with_key};
