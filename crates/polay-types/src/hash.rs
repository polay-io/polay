use std::fmt;
use std::str::FromStr;

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

/// A 32-byte cryptographic hash used for block hashes, transaction hashes,
/// Merkle roots, and content-addressed identifiers throughout the POLAY chain.
///
/// Displayed and serialized (via serde) as a hex-encoded string.
/// Borsh encoding writes the raw 32 bytes.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
pub struct Hash(pub [u8; 32]);

impl Hash {
    /// The all-zeros hash, used as a sentinel (e.g., the parent hash of the
    /// genesis block).
    pub const ZERO: Self = Self([0u8; 32]);

    /// Size of the hash in bytes.
    pub const LEN: usize = 32;

    /// Create a hash from a 32-byte array.
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Return a reference to the underlying bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Return the raw byte array.
    pub fn to_bytes(self) -> [u8; 32] {
        self.0
    }

    /// Return the hex-encoded string (no prefix).
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Decode from a hex-encoded string (with or without `0x` prefix).
    pub fn from_hex(s: &str) -> Result<Self, HashParseError> {
        let s = s.strip_prefix("0x").unwrap_or(s);
        let bytes = hex::decode(s).map_err(|e| HashParseError(e.to_string()))?;
        if bytes.len() != 32 {
            return Err(HashParseError(format!(
                "expected 32 bytes, got {}",
                bytes.len()
            )));
        }
        let mut buf = [0u8; 32];
        buf.copy_from_slice(&bytes);
        Ok(Self(buf))
    }

    /// Returns `true` if this is the zero hash.
    pub fn is_zero(&self) -> bool {
        self == &Self::ZERO
    }
}

// ---------------------------------------------------------------------------
// Display / Debug / FromStr
// ---------------------------------------------------------------------------

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

impl fmt::Debug for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Hash({})", self.to_hex())
    }
}

/// Error returned when parsing a hash from a string fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HashParseError(pub String);

impl fmt::Display for HashParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid hash: {}", self.0)
    }
}

impl std::error::Error for HashParseError {}

impl FromStr for Hash {
    type Err = HashParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_hex(s)
    }
}

// ---------------------------------------------------------------------------
// Serde — serialize as hex string
// ---------------------------------------------------------------------------

impl Serialize for Hash {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Hash {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = <String as Deserialize>::deserialize(deserializer)?;
        Hash::from_hex(&s).map_err(de::Error::custom)
    }
}

// ---------------------------------------------------------------------------
// Conversions
// ---------------------------------------------------------------------------

impl From<[u8; 32]> for Hash {
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl From<Hash> for [u8; 32] {
    fn from(hash: Hash) -> [u8; 32] {
        hash.0
    }
}

impl AsRef<[u8]> for Hash {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl Default for Hash {
    fn default() -> Self {
        Self::ZERO
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_hash() -> Hash {
        let mut bytes = [0u8; 32];
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = (255 - i) as u8;
        }
        Hash(bytes)
    }

    #[test]
    fn hex_round_trip() {
        let h = sample_hash();
        let hex_str = h.to_hex();
        let parsed: Hash = hex_str.parse().unwrap();
        assert_eq!(h, parsed);
    }

    #[test]
    fn hex_0x_prefix() {
        let h = sample_hash();
        let hex_str = format!("0x{}", h.to_hex());
        let parsed = Hash::from_hex(&hex_str).unwrap();
        assert_eq!(h, parsed);
    }

    #[test]
    fn serde_round_trip() {
        let h = sample_hash();
        let json = serde_json::to_string(&h).unwrap();
        let parsed: Hash = serde_json::from_str(&json).unwrap();
        assert_eq!(h, parsed);
    }

    #[test]
    fn borsh_round_trip() {
        let h = sample_hash();
        let encoded = borsh::to_vec(&h).unwrap();
        assert_eq!(encoded.len(), 32);
        let decoded = Hash::try_from_slice(&encoded).unwrap();
        assert_eq!(h, decoded);
    }

    #[test]
    fn zero_hash() {
        assert!(Hash::ZERO.is_zero());
        assert!(!sample_hash().is_zero());
    }

    #[test]
    fn invalid_length_rejected() {
        assert!(Hash::from_hex(&hex::encode([0u8; 16])).is_err());
    }
}
