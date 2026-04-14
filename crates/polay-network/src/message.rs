use serde::{Deserialize, Serialize};

use crate::error::NetworkError;
use polay_state::snapshot::{SnapshotChunk, StateSnapshot};
use polay_types::address::Address;
use polay_types::block::Block;
use polay_types::hash::Hash;
use polay_types::signature::Signature;
use polay_types::transaction::SignedTransaction;

// ---------------------------------------------------------------------------
// Protocol versioning
// ---------------------------------------------------------------------------

/// Current protocol version. Peers running incompatible versions will have
/// their messages rejected.
pub const PROTOCOL_VERSION: u32 = 1;

/// Versioned message envelope. All gossipsub payloads are wrapped in this
/// structure so that peers can detect version mismatches early.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageEnvelope {
    pub version: u32,
    pub payload: NetworkMessage,
}

impl MessageEnvelope {
    /// Create a new envelope wrapping the given payload with the current
    /// protocol version.
    pub fn new(payload: NetworkMessage) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            payload,
        }
    }

    /// Decode an envelope from bytes, checking protocol version compatibility.
    pub fn decode(data: &[u8]) -> Result<Self, NetworkError> {
        let envelope: Self = serde_json::from_slice(data)
            .map_err(|e| NetworkError::DeserializationError(e.to_string()))?;
        if envelope.version != PROTOCOL_VERSION {
            return Err(NetworkError::IncompatibleVersion {
                ours: PROTOCOL_VERSION,
                theirs: envelope.version,
            });
        }
        Ok(envelope)
    }

    /// Encode this envelope to bytes.
    pub fn encode(&self) -> Result<Vec<u8>, NetworkError> {
        serde_json::to_vec(self).map_err(|e| NetworkError::SerializationError(e.to_string()))
    }
}

/// Messages exchanged between POLAY nodes over the network.
///
/// Each variant carries enough data to be independently actionable by the
/// receiving node. The enum is `Serialize` / `Deserialize` so it can be
/// encoded to JSON (or later a compact binary format) for the wire.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkMessage {
    /// A new signed transaction to propagate to peers.
    NewTransaction(SignedTransaction),

    /// A block proposal from the designated proposer.
    /// Includes the consensus round and proposer address so that
    /// re-proposals of the same block in later rounds produce distinct
    /// gossipsub message IDs and receivers can identify the actual proposer.
    BlockProposal { block: Block, round: u32, proposer: Address },

    /// A consensus vote (prevote or precommit) from a validator.
    ConsensusVote(ConsensusVoteMsg),

    /// Request the block at a specific height from a peer.
    RequestBlock(u64),

    /// Response carrying the requested block (or `None` if the peer does
    /// not have it).
    BlockResponse(Option<Block>),

    /// Liveness ping carrying a nonce.
    Ping(u64),

    /// Liveness pong echoing the nonce from a prior ping.
    Pong(u64),

    // -- State sync messages ------------------------------------------------
    /// Request a state snapshot at the given block height.
    RequestSnapshot { height: u64 },

    /// Response carrying snapshot metadata (total chunks, chunk hashes, etc.).
    SnapshotMetadata(StateSnapshot),

    /// Request a specific chunk of a snapshot.
    RequestChunk { height: u64, chunk_index: u32 },

    /// Response carrying a single snapshot chunk.
    SnapshotChunkData(SnapshotChunk),
}

/// A consensus vote message sent over the network.
///
/// This is a self-contained representation that carries the voter identity
/// and signature so a receiving node can verify it independently.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusVoteMsg {
    /// The block height this vote applies to.
    pub height: u64,
    /// The consensus round within the height.
    pub round: u32,
    /// Vote type: `"prevote"` or `"precommit"`.
    pub vote_type: String,
    /// Hash of the block being voted on (`Hash::ZERO` for a nil vote).
    pub block_hash: Hash,
    /// Address of the validator casting this vote.
    pub voter: Address,
    /// Cryptographic signature over the vote payload.
    pub voter_signature: Signature,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_message_serde_round_trip_ping() {
        let msg = NetworkMessage::Ping(42);
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: NetworkMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            NetworkMessage::Ping(n) => assert_eq!(n, 42),
            _ => panic!("expected Ping"),
        }
    }

    #[test]
    fn network_message_serde_round_trip_pong() {
        let msg = NetworkMessage::Pong(99);
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: NetworkMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            NetworkMessage::Pong(n) => assert_eq!(n, 99),
            _ => panic!("expected Pong"),
        }
    }

    #[test]
    fn consensus_vote_msg_serde_round_trip() {
        let vote = ConsensusVoteMsg {
            height: 10,
            round: 0,
            vote_type: "prevote".to_string(),
            block_hash: Hash::ZERO,
            voter: Address::ZERO,
            voter_signature: Signature::ZERO,
        };
        let json = serde_json::to_string(&vote).unwrap();
        let parsed: ConsensusVoteMsg = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.height, 10);
        assert_eq!(parsed.vote_type, "prevote");
    }

    #[test]
    fn request_block_serde() {
        let msg = NetworkMessage::RequestBlock(100);
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: NetworkMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            NetworkMessage::RequestBlock(h) => assert_eq!(h, 100),
            _ => panic!("expected RequestBlock"),
        }
    }

    #[test]
    fn block_response_none_serde() {
        let msg = NetworkMessage::BlockResponse(None);
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: NetworkMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            NetworkMessage::BlockResponse(opt) => assert!(opt.is_none()),
            _ => panic!("expected BlockResponse"),
        }
    }

    // -- MessageEnvelope tests ------------------------------------------------

    #[test]
    fn envelope_encode_decode_round_trip() {
        let msg = NetworkMessage::Ping(42);
        let envelope = MessageEnvelope::new(msg);
        let bytes = envelope.encode().unwrap();
        let decoded = MessageEnvelope::decode(&bytes).unwrap();

        assert_eq!(decoded.version, PROTOCOL_VERSION);
        match decoded.payload {
            NetworkMessage::Ping(n) => assert_eq!(n, 42),
            _ => panic!("expected Ping"),
        }
    }

    #[test]
    fn envelope_rejects_incompatible_version() {
        let mut envelope = MessageEnvelope::new(NetworkMessage::Pong(1));
        envelope.version = 999;
        let bytes = serde_json::to_vec(&envelope).unwrap();

        let result = MessageEnvelope::decode(&bytes);
        assert!(result.is_err());
        let err_str = result.unwrap_err().to_string();
        assert!(err_str.contains("incompatible"), "error: {err_str}");
    }

    #[test]
    fn envelope_rejects_garbage_bytes() {
        let result = MessageEnvelope::decode(b"not valid json");
        assert!(result.is_err());
    }
}
