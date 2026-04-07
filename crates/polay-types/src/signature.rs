use std::fmt;
use std::str::FromStr;

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

/// A 64-byte cryptographic signature (e.g., Ed25519).
///
/// Displayed and serialized (via serde) as a hex-encoded string.
/// Borsh encoding writes the raw 64 bytes.
#[derive(Copy, Clone, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct Signature(pub [u8; 64]);

impl Signature {
    /// The all-zeros signature (sentinel / placeholder).
    pub const ZERO: Self = Self([0u8; 64]);

    /// Size of the signature in bytes.
    pub const LEN: usize = 64;

    /// Create a signature from a 64-byte array.
    pub const fn new(bytes: [u8; 64]) -> Self {
        Self(bytes)
    }

    /// Return a reference to the underlying bytes.
    pub fn as_bytes(&self) -> &[u8; 64] {
        &self.0
    }

    /// Return the raw byte array.
    pub fn to_bytes(self) -> [u8; 64] {
        self.0
    }

    /// Return the hex-encoded string.
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Decode from a hex-encoded string (with or without `0x` prefix).
    pub fn from_hex(s: &str) -> Result<Self, SignatureParseError> {
        let s = s.strip_prefix("0x").unwrap_or(s);
        let bytes = hex::decode(s).map_err(|e| SignatureParseError(e.to_string()))?;
        if bytes.len() != 64 {
            return Err(SignatureParseError(format!(
                "expected 64 bytes, got {}",
                bytes.len()
            )));
        }
        let mut buf = [0u8; 64];
        buf.copy_from_slice(&bytes);
        Ok(Self(buf))
    }

    /// Returns `true` if this is the zero signature.
    pub fn is_zero(&self) -> bool {
        self == &Self::ZERO
    }
}

// ---------------------------------------------------------------------------
// Display / Debug / FromStr
// ---------------------------------------------------------------------------

impl fmt::Display for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

impl fmt::Debug for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Show only first and last 8 hex chars for readability.
        let h = self.to_hex();
        write!(f, "Signature({}...{})", &h[..8], &h[120..])
    }
}

/// Error returned when parsing a signature from a string fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureParseError(pub String);

impl fmt::Display for SignatureParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid signature: {}", self.0)
    }
}

impl std::error::Error for SignatureParseError {}

impl FromStr for Signature {
    type Err = SignatureParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_hex(s)
    }
}

// ---------------------------------------------------------------------------
// Serde — serialize as hex string
// ---------------------------------------------------------------------------

impl Serialize for Signature {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Signature {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = <String as Deserialize>::deserialize(deserializer)?;
        Signature::from_hex(&s).map_err(de::Error::custom)
    }
}

// ---------------------------------------------------------------------------
// Conversions
// ---------------------------------------------------------------------------

impl From<[u8; 64]> for Signature {
    fn from(bytes: [u8; 64]) -> Self {
        Self(bytes)
    }
}

impl From<Signature> for [u8; 64] {
    fn from(sig: Signature) -> [u8; 64] {
        sig.0
    }
}

impl AsRef<[u8]> for Signature {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl Default for Signature {
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

    fn sample_sig() -> Signature {
        let mut bytes = [0u8; 64];
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = (i * 3) as u8;
        }
        Signature(bytes)
    }

    #[test]
    fn hex_round_trip() {
        let sig = sample_sig();
        let hex_str = sig.to_hex();
        let parsed: Signature = hex_str.parse().unwrap();
        assert_eq!(sig, parsed);
    }

    #[test]
    fn hex_0x_prefix() {
        let sig = sample_sig();
        let hex_str = format!("0x{}", sig.to_hex());
        let parsed = Signature::from_hex(&hex_str).unwrap();
        assert_eq!(sig, parsed);
    }

    #[test]
    fn serde_round_trip() {
        let sig = sample_sig();
        let json = serde_json::to_string(&sig).unwrap();
        let parsed: Signature = serde_json::from_str(&json).unwrap();
        assert_eq!(sig, parsed);
    }

    #[test]
    fn borsh_round_trip() {
        let sig = sample_sig();
        let encoded = borsh::to_vec(&sig).unwrap();
        assert_eq!(encoded.len(), 64);
        let decoded = Signature::try_from_slice(&encoded).unwrap();
        assert_eq!(sig, decoded);
    }

    #[test]
    fn invalid_length_rejected() {
        let short = hex::encode([0u8; 32]);
        assert!(Signature::from_hex(&short).is_err());
    }
}
