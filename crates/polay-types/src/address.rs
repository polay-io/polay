use std::fmt;
use std::str::FromStr;

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

/// A 32-byte account address on the POLAY blockchain.
///
/// Displayed and serialized (via serde) as a hex-encoded string.
/// Borsh encoding writes the raw 32 bytes.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
pub struct Address(pub [u8; 32]);

impl Address {
    /// The all-zeros address, used as a sentinel / system address.
    pub const ZERO: Self = Self([0u8; 32]);

    /// Size of the address in bytes.
    pub const LEN: usize = 32;

    /// Create an address from a 32-byte array.
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

    /// Return the hex-encoded string (no `0x` prefix).
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Decode from a hex-encoded string (with or without `0x` prefix).
    pub fn from_hex(s: &str) -> Result<Self, AddressParseError> {
        let s = s.strip_prefix("0x").unwrap_or(s);
        let bytes = hex::decode(s).map_err(|e| AddressParseError(e.to_string()))?;
        if bytes.len() != 32 {
            return Err(AddressParseError(format!(
                "expected 32 bytes, got {}",
                bytes.len()
            )));
        }
        let mut buf = [0u8; 32];
        buf.copy_from_slice(&bytes);
        Ok(Self(buf))
    }

    /// Encode the address in Base58 (sometimes useful for user-facing display).
    pub fn to_bs58(&self) -> String {
        bs58::encode(self.0).into_string()
    }

    /// Decode from a Base58-encoded string.
    pub fn from_bs58(s: &str) -> Result<Self, AddressParseError> {
        let bytes = bs58::decode(s)
            .into_vec()
            .map_err(|e| AddressParseError(e.to_string()))?;
        if bytes.len() != 32 {
            return Err(AddressParseError(format!(
                "expected 32 bytes, got {}",
                bytes.len()
            )));
        }
        let mut buf = [0u8; 32];
        buf.copy_from_slice(&bytes);
        Ok(Self(buf))
    }

    /// Returns `true` if this is the zero address.
    pub fn is_zero(&self) -> bool {
        self == &Self::ZERO
    }
}

// ---------------------------------------------------------------------------
// Display / Debug / FromStr
// ---------------------------------------------------------------------------

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

impl fmt::Debug for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Address({})", self.to_hex())
    }
}

/// Error returned when parsing an address from a string fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddressParseError(pub String);

impl fmt::Display for AddressParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid address: {}", self.0)
    }
}

impl std::error::Error for AddressParseError {}

impl FromStr for Address {
    type Err = AddressParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_hex(s)
    }
}

// ---------------------------------------------------------------------------
// Serde — serialize as hex string
// ---------------------------------------------------------------------------

impl Serialize for Address {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Address {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = <String as Deserialize>::deserialize(deserializer)?;
        Address::from_hex(&s).map_err(de::Error::custom)
    }
}

// ---------------------------------------------------------------------------
// Conversions
// ---------------------------------------------------------------------------

impl From<[u8; 32]> for Address {
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl From<Address> for [u8; 32] {
    fn from(addr: Address) -> [u8; 32] {
        addr.0
    }
}

impl AsRef<[u8]> for Address {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl Default for Address {
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

    fn sample_address() -> Address {
        let mut bytes = [0u8; 32];
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = i as u8;
        }
        Address(bytes)
    }

    #[test]
    fn hex_round_trip() {
        let addr = sample_address();
        let hex_str = addr.to_hex();
        let parsed: Address = hex_str.parse().unwrap();
        assert_eq!(addr, parsed);
    }

    #[test]
    fn hex_0x_prefix() {
        let addr = sample_address();
        let hex_str = format!("0x{}", addr.to_hex());
        let parsed = Address::from_hex(&hex_str).unwrap();
        assert_eq!(addr, parsed);
    }

    #[test]
    fn bs58_round_trip() {
        let addr = sample_address();
        let encoded = addr.to_bs58();
        let decoded = Address::from_bs58(&encoded).unwrap();
        assert_eq!(addr, decoded);
    }

    #[test]
    fn serde_round_trip() {
        let addr = sample_address();
        let json = serde_json::to_string(&addr).unwrap();
        let parsed: Address = serde_json::from_str(&json).unwrap();
        assert_eq!(addr, parsed);
    }

    #[test]
    fn borsh_round_trip() {
        let addr = sample_address();
        let encoded = borsh::to_vec(&addr).unwrap();
        assert_eq!(encoded.len(), 32);
        let decoded = Address::try_from_slice(&encoded).unwrap();
        assert_eq!(addr, decoded);
    }

    #[test]
    fn zero_address() {
        assert!(Address::ZERO.is_zero());
        assert!(!sample_address().is_zero());
    }

    #[test]
    fn invalid_length_rejected() {
        let short = hex::encode([0u8; 16]);
        assert!(Address::from_hex(&short).is_err());
    }
}
