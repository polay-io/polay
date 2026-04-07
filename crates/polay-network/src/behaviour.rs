use libp2p::{gossipsub, mdns, swarm::NetworkBehaviour};

/// Custom libp2p NetworkBehaviour combining gossipsub for message
/// propagation and mDNS for local peer discovery.
#[derive(NetworkBehaviour)]
pub struct PolayBehaviour {
    /// Gossipsub for broadcasting transactions, blocks, and consensus votes.
    pub gossipsub: gossipsub::Behaviour,
    /// mDNS for automatic local peer discovery (devnet).
    pub mdns: mdns::tokio::Behaviour,
}
