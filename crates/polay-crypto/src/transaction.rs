use polay_types::{SignedTransaction, Transaction};

use crate::error::{CryptoError, CryptoResult};
use crate::hash::hash_transaction;
use crate::keypair::PolayKeypair;
use crate::public_key::PolayPublicKey;

/// Build the deterministic signing payload for a transaction.
///
/// This is the canonical byte representation that gets signed -- produced by
/// Borsh-serializing the `Transaction` struct.
pub fn build_tx_signing_payload(tx: &Transaction) -> CryptoResult<Vec<u8>> {
    borsh::to_vec(tx).map_err(|e| CryptoError::SerializationError(e.to_string()))
}

/// Sign a transaction, producing a `SignedTransaction`.
///
/// 1. Borsh-serialize the transaction to produce the signing payload.
/// 2. Sign the payload with the keypair.
/// 3. Compute the transaction hash (SHA-256 of the Borsh bytes).
/// 4. Return a `SignedTransaction` bundling the original tx, signature, and hash.
pub fn sign_transaction(
    keypair: &PolayKeypair,
    tx: Transaction,
) -> CryptoResult<SignedTransaction> {
    let payload = build_tx_signing_payload(&tx)?;
    let signature = keypair.sign(&payload);
    let tx_hash = hash_transaction(&tx)?;
    let signer_pubkey = keypair.public_key().to_bytes().to_vec();

    Ok(SignedTransaction {
        transaction: tx,
        signature,
        tx_hash,
        signer_pubkey,
    })
}

/// Verify a signed transaction given the signer's public key.
///
/// Since a POLAY `Address` is a hash of the public key (one-way), we cannot
/// recover the public key from the address alone.  The caller must supply the
/// `PolayPublicKey` that corresponds to `signed_tx.transaction.signer`.
///
/// Checks performed:
/// 1. The public key's derived address matches `transaction.signer`.
/// 2. The stored `tx_hash` matches a fresh hash of the transaction.
/// 3. The Ed25519 signature verifies against the signing payload.
pub fn verify_transaction_with_key(
    signed_tx: &SignedTransaction,
    pubkey: &PolayPublicKey,
) -> CryptoResult<bool> {
    // 1. Confirm the public key corresponds to the claimed signer address.
    let derived_address = pubkey.address();
    if derived_address != signed_tx.transaction.signer {
        return Err(CryptoError::InvalidPublicKey(format!(
            "public key derives address {}, but transaction signer is {}",
            derived_address, signed_tx.transaction.signer,
        )));
    }

    // 2. Verify the transaction hash is correct.
    let expected_hash = hash_transaction(&signed_tx.transaction)?;
    if expected_hash != signed_tx.tx_hash {
        return Err(CryptoError::HashError(format!(
            "transaction hash mismatch: expected {}, stored {}",
            expected_hash, signed_tx.tx_hash,
        )));
    }

    // 3. Verify the Ed25519 signature over the signing payload.
    let payload = build_tx_signing_payload(&signed_tx.transaction)?;
    pubkey.verify(&payload, &signed_tx.signature)?;

    Ok(true)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use polay_types::{Address, Hash, TransactionAction};

    fn make_transfer_tx(signer: Address) -> Transaction {
        Transaction {
            chain_id: "polay-testnet".to_string(),
            nonce: 0,
            signer,
            action: TransactionAction::Transfer {
                to: Address::ZERO,
                amount: 42,
            },
            max_fee: 1000,
            timestamp: 1_700_000_000,
            session: None,
            sponsor: None,
        }
    }

    #[test]
    fn sign_and_verify() {
        let kp = PolayKeypair::generate();
        let tx = make_transfer_tx(kp.address());
        let signed = sign_transaction(&kp, tx).unwrap();

        let result = verify_transaction_with_key(&signed, &kp.public_key());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), true);
    }

    #[test]
    fn verify_rejects_wrong_key() {
        let kp1 = PolayKeypair::generate();
        let kp2 = PolayKeypair::generate();

        let tx = make_transfer_tx(kp1.address());
        let signed = sign_transaction(&kp1, tx).unwrap();

        // Verify with the wrong public key -- address mismatch.
        let result = verify_transaction_with_key(&signed, &kp2.public_key());
        assert!(result.is_err());
    }

    #[test]
    fn verify_rejects_tampered_hash() {
        let kp = PolayKeypair::generate();
        let tx = make_transfer_tx(kp.address());
        let mut signed = sign_transaction(&kp, tx).unwrap();

        // Tamper with the stored hash.
        signed.tx_hash = Hash::ZERO;

        let result = verify_transaction_with_key(&signed, &kp.public_key());
        assert!(result.is_err());
    }

    #[test]
    fn verify_rejects_tampered_transaction() {
        let kp = PolayKeypair::generate();
        let tx = make_transfer_tx(kp.address());
        let mut signed = sign_transaction(&kp, tx).unwrap();

        // Tamper with the transaction body (change the nonce).
        signed.transaction.nonce = 999;
        // Also fix the hash to match the tampered transaction,
        // so the hash check passes but the signature check should fail.
        signed.tx_hash = hash_transaction(&signed.transaction).unwrap();

        let result = verify_transaction_with_key(&signed, &kp.public_key());
        assert!(result.is_err());
    }

    #[test]
    fn signing_payload_is_deterministic() {
        let tx = make_transfer_tx(Address::ZERO);
        let p1 = build_tx_signing_payload(&tx).unwrap();
        let p2 = build_tx_signing_payload(&tx).unwrap();
        assert_eq!(p1, p2);
        assert!(!p1.is_empty());
    }

    #[test]
    fn different_transactions_produce_different_signatures() {
        let kp = PolayKeypair::generate();

        let tx1 = Transaction {
            chain_id: "polay-testnet".to_string(),
            nonce: 0,
            signer: kp.address(),
            action: TransactionAction::Transfer {
                to: Address::ZERO,
                amount: 10,
            },
            max_fee: 100,
            timestamp: 1_700_000_000,
            session: None,
            sponsor: None,
        };

        let tx2 = Transaction {
            chain_id: "polay-testnet".to_string(),
            nonce: 1,
            signer: kp.address(),
            action: TransactionAction::Transfer {
                to: Address::ZERO,
                amount: 10,
            },
            max_fee: 100,
            timestamp: 1_700_000_000,
            session: None,
            sponsor: None,
        };

        let s1 = sign_transaction(&kp, tx1).unwrap();
        let s2 = sign_transaction(&kp, tx2).unwrap();
        assert_ne!(s1.signature, s2.signature);
        assert_ne!(s1.tx_hash, s2.tx_hash);
    }
}
