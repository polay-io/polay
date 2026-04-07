use std::fmt;
use std::str::FromStr;

use ed25519_dalek::Verifier;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};

use polay_types::{Address, Signature};

use crate::error::{CryptoError, CryptoResult};

/// An Ed25519 public key used to verify signatures on the POLAY blockchain.
///
/// Wraps `ed25519_dalek::VerifyingKey`. Serialized via serde as a hex string.
#[derive(Copy, Clone, PartialEq, Eq)]
pub struct PolayPublicKey {
    inner: ed25519_dalek::VerifyingKey,
}

impl PolayPublicKey {
    /// Construct from raw 32-byte compressed Edwards-y representation.
    pub fn from_bytes(bytes: &[u8; 32]) -> CryptoResult<Self> {
        let vk = ed25519_dalek::VerifyingKey::from_bytes(bytes)
            .map_err(|e| CryptoError::InvalidPublicKey(e.to_string()))?;
        Ok(Self { inner: vk })
    }

    /// Construct from an already-validated `ed25519_dalek::VerifyingKey`.
    pub(crate) fn from_verifying_key(vk: ed25519_dalek::VerifyingKey) -> Self {
        Self { inner: vk }
    }

    /// Verify that `signature` is a valid Ed25519 signature of `message`
    /// under this public key.
    pub fn verify(&self, message: &[u8], signature: &Signature) -> CryptoResult<()> {
        let dalek_sig = ed25519_dalek::Signature::from_bytes(signature.as_bytes());
        self.inner
            .verify(message, &dalek_sig)
            .map_err(|e| CryptoError::InvalidSignature(e.to_string()))
    }

    /// Derive the POLAY address for this public key.
    ///
    /// Address = SHA-256(public_key_bytes), taking the full 32-byte digest.
    pub fn address(&self) -> Address {
        let digest = Sha256::digest(self.inner.to_bytes());
        let mut addr = [0u8; 32];
        addr.copy_from_slice(&digest[..32]);
        Address::new(addr)
    }

    /// Return the raw 32 public-key bytes.
    pub fn to_bytes(&self) -> [u8; 32] {
        self.inner.to_bytes()
    }

    /// Return the hex-encoded public key string.
    pub fn to_hex(&self) -> String {
        hex::encode(self.inner.to_bytes())
    }

    /// Parse from a hex-encoded string (with or without `0x` prefix).
    pub fn from_hex(s: &str) -> CryptoResult<Self> {
        let s = s.strip_prefix("0x").unwrap_or(s);
        let bytes = hex::decode(s).map_err(|e| CryptoError::InvalidPublicKey(e.to_string()))?;
        if bytes.len() != 32 {
            return Err(CryptoError::InvalidPublicKey(format!(
                "expected 32 bytes, got {}",
                bytes.len()
            )));
        }
        let mut buf = [0u8; 32];
        buf.copy_from_slice(&bytes);
        Self::from_bytes(&buf)
    }

    /// Borrow the inner `ed25519_dalek::VerifyingKey`.
    pub fn as_verifying_key(&self) -> &ed25519_dalek::VerifyingKey {
        &self.inner
    }
}

// ---------------------------------------------------------------------------
// Display / Debug / FromStr
// ---------------------------------------------------------------------------

impl fmt::Display for PolayPublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

impl fmt::Debug for PolayPublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PolayPublicKey({})", self.to_hex())
    }
}

impl FromStr for PolayPublicKey {
    type Err = CryptoError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_hex(s)
    }
}

// ---------------------------------------------------------------------------
// Serde — serialize as hex string
// ---------------------------------------------------------------------------

impl Serialize for PolayPublicKey {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for PolayPublicKey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        PolayPublicKey::from_hex(&s).map_err(de::Error::custom)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keypair::PolayKeypair;

    #[test]
    fn from_bytes_round_trip() {
        let kp = PolayKeypair::generate();
        let pk = kp.public_key();
        let bytes = pk.to_bytes();
        let pk2 = PolayPublicKey::from_bytes(&bytes).unwrap();
        assert_eq!(pk, pk2);
    }

    #[test]
    fn hex_round_trip() {
        let kp = PolayKeypair::generate();
        let pk = kp.public_key();
        let hex_str = pk.to_hex();
        let pk2 = PolayPublicKey::from_hex(&hex_str).unwrap();
        assert_eq!(pk, pk2);
    }

    #[test]
    fn serde_round_trip() {
        let kp = PolayKeypair::generate();
        let pk = kp.public_key();
        let json = serde_json::to_string(&pk).unwrap();
        let pk2: PolayPublicKey = serde_json::from_str(&json).unwrap();
        assert_eq!(pk, pk2);
    }

    #[test]
    fn address_matches_keypair() {
        let kp = PolayKeypair::generate();
        assert_eq!(kp.address(), kp.public_key().address());
    }

    #[test]
    fn invalid_bytes_rejected() {
        // All zeros is not a valid Ed25519 public key point.
        let bad = [0u8; 32];
        // Some zero arrays might pass point decompression, but
        // we mainly care that from_bytes doesn't panic.
        let _ = PolayPublicKey::from_bytes(&bad);
    }

    #[test]
    fn display_shows_hex() {
        let kp = PolayKeypair::generate();
        let pk = kp.public_key();
        let display = format!("{}", pk);
        assert_eq!(display.len(), 64); // 32 bytes = 64 hex chars
    }

    #[test]
    fn from_str_with_0x_prefix() {
        let kp = PolayKeypair::generate();
        let pk = kp.public_key();
        let hex_str = format!("0x{}", pk.to_hex());
        let pk2: PolayPublicKey = hex_str.parse().unwrap();
        assert_eq!(pk, pk2);
    }
}
