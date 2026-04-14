use std::fmt;

use ed25519_dalek::Signer;
use rand::rngs::OsRng;
use sha2::{Digest, Sha256};

use polay_types::{Address, Signature};

use crate::error::CryptoResult;
use crate::public_key::PolayPublicKey;

/// An Ed25519 keypair for signing POLAY transactions and messages.
///
/// Wraps `ed25519_dalek::SigningKey`. The secret key bytes are never
/// exposed through Display or Debug — only the derived address is shown.
pub struct PolayKeypair {
    inner: ed25519_dalek::SigningKey,
}

impl PolayKeypair {
    /// Generate a new random keypair using the OS random number generator.
    pub fn generate() -> Self {
        let signing_key = ed25519_dalek::SigningKey::generate(&mut OsRng);
        Self { inner: signing_key }
    }

    /// Reconstruct a keypair from 32 secret-key bytes.
    pub fn from_bytes(secret: &[u8; 32]) -> CryptoResult<Self> {
        let signing_key = ed25519_dalek::SigningKey::from_bytes(secret);
        Ok(Self { inner: signing_key })
    }

    /// Return the public key half of this keypair.
    pub fn public_key(&self) -> PolayPublicKey {
        PolayPublicKey::from_verifying_key(self.inner.verifying_key())
    }

    /// Derive the POLAY address from the public key.
    ///
    /// The address is the first 32 bytes of SHA-256(public_key_bytes).
    /// Since SHA-256 outputs exactly 32 bytes, this is the full digest.
    pub fn address(&self) -> Address {
        let pubkey_bytes = self.inner.verifying_key().to_bytes();
        let digest = Sha256::digest(pubkey_bytes);
        let mut addr = [0u8; 32];
        addr.copy_from_slice(&digest[..32]);
        Address::new(addr)
    }

    /// Sign an arbitrary message, returning a POLAY Signature.
    pub fn sign(&self, message: &[u8]) -> Signature {
        let sig = self.inner.sign(message);
        Signature::new(sig.to_bytes())
    }

    /// Return the 32-byte secret key. Handle with care.
    pub fn to_bytes(&self) -> [u8; 32] {
        self.inner.to_bytes()
    }

    /// Borrow the inner `ed25519_dalek::SigningKey`.
    pub fn as_signing_key(&self) -> &ed25519_dalek::SigningKey {
        &self.inner
    }
}

// ---------------------------------------------------------------------------
// Clone (explicit — copying secret key material is deliberate)
// ---------------------------------------------------------------------------

impl Clone for PolayKeypair {
    fn clone(&self) -> Self {
        Self {
            inner: ed25519_dalek::SigningKey::from_bytes(&self.inner.to_bytes()),
        }
    }
}

// ---------------------------------------------------------------------------
// Display / Debug — NEVER show the secret key
// ---------------------------------------------------------------------------

impl fmt::Display for PolayKeypair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PolayKeypair(address={})", self.address())
    }
}

impl fmt::Debug for PolayKeypair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PolayKeypair")
            .field("address", &self.address().to_hex())
            .finish_non_exhaustive()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_produces_valid_keypair() {
        let kp = PolayKeypair::generate();
        let msg = b"hello polay";
        let sig = kp.sign(msg);

        // Verify through the public key wrapper.
        let pk = kp.public_key();
        assert!(pk.verify(msg, &sig).is_ok());
    }

    #[test]
    fn from_bytes_round_trip() {
        let kp1 = PolayKeypair::generate();
        let secret = kp1.to_bytes();
        let kp2 = PolayKeypair::from_bytes(&secret).unwrap();

        assert_eq!(kp1.address(), kp2.address());
        assert_eq!(kp1.public_key().to_bytes(), kp2.public_key().to_bytes(),);
    }

    #[test]
    fn address_is_sha256_of_pubkey() {
        let kp = PolayKeypair::generate();
        let pubkey_bytes = kp.public_key().to_bytes();
        let digest = Sha256::digest(pubkey_bytes);
        let expected = Address::new(digest.into());
        assert_eq!(kp.address(), expected);
    }

    #[test]
    fn display_does_not_leak_secret() {
        let kp = PolayKeypair::generate();
        let display = format!("{}", kp);
        let debug = format!("{:?}", kp);
        let secret_hex = hex::encode(kp.to_bytes());

        assert!(!display.contains(&secret_hex));
        assert!(!debug.contains(&secret_hex));
        assert!(display.contains("PolayKeypair"));
    }

    #[test]
    fn wrong_message_fails_verification() {
        let kp = PolayKeypair::generate();
        let sig = kp.sign(b"correct message");
        let pk = kp.public_key();
        assert!(pk.verify(b"wrong message", &sig).is_err());
    }
}
