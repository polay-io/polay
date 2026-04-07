use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::Duration;

use libp2p::gossipsub::{self, IdentTopic, MessageAuthenticity};
use libp2p::swarm::SwarmEvent;
use libp2p::{mdns, noise, tcp, yamux, Multiaddr, PeerId, Swarm};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::behaviour::{PolayBehaviour, PolayBehaviourEvent};
use crate::error::NetworkError;
use crate::message::{ConsensusVoteMsg, MessageEnvelope, NetworkMessage};
use crate::peer_manager::{PeerManager, MAX_PEERS, MIN_PEERS};
use crate::rate_limiter::PeerRateLimiter;
use crate::topics::{TOPIC_BLOCKS, TOPIC_CONSENSUS, TOPIC_TRANSACTIONS};

use polay_types::block::Block;
use polay_types::transaction::SignedTransaction;

// ---------------------------------------------------------------------------
// P2P Configuration
// ---------------------------------------------------------------------------

/// Configuration for the P2P networking layer.
pub struct P2PConfig {
    /// libp2p listen multiaddr (e.g. "/ip4/0.0.0.0/tcp/30333").
    pub listen_addr: String,
    /// Multiaddr strings for initial boot nodes to connect to.
    pub boot_nodes: Vec<String>,
    /// Optional keypair for the libp2p node identity. If `None`, a random
    /// identity will be generated.
    pub node_keypair: Option<libp2p::identity::Keypair>,
    /// Maximum number of connected peers (default 50).
    pub max_peers: usize,
    /// Minimum desired connected peers (default 4).
    pub min_peers: usize,
    /// Default ban duration in seconds (default 3600 = 1 hour).
    pub ban_duration_secs: u64,
    /// Enable mDNS peer discovery (default true; disable for production).
    pub enable_mdns: bool,
    /// Gossipsub heartbeat interval in milliseconds (default 1000).
    pub gossipsub_heartbeat_ms: u64,
}

impl Default for P2PConfig {
    fn default() -> Self {
        Self {
            listen_addr: "/ip4/0.0.0.0/tcp/30333".to_string(),
            boot_nodes: Vec::new(),
            node_keypair: None,
            max_peers: MAX_PEERS,
            min_peers: MIN_PEERS,
            ban_duration_secs: 3600,
            enable_mdns: true,
            gossipsub_heartbeat_ms: 1000,
        }
    }
}

// ---------------------------------------------------------------------------
// Commands and Events
// ---------------------------------------------------------------------------

/// Commands sent to the P2P background task by the node/validator.
#[derive(Debug)]
pub enum P2PCommand {
    /// Broadcast a signed transaction to peers.
    BroadcastTransaction(SignedTransaction),
    /// Broadcast a block proposal to peers.
    BroadcastBlock(Block),
    /// Broadcast a consensus vote message to peers.
    BroadcastConsensusMessage(ConsensusVoteMsg),
    /// Request the current peer count.
    GetPeerCount,
    /// Gracefully shut down the P2P service.
    Shutdown,
}

/// Events emitted by the P2P background task to the node/validator.
#[derive(Debug, Clone)]
pub enum P2PEvent {
    /// A transaction was received from the network.
    TransactionReceived(SignedTransaction),
    /// A block was received from the network.
    BlockReceived(Block),
    /// A consensus vote message was received from the network.
    ConsensusMessageReceived(ConsensusVoteMsg),
    /// A peer connected.
    PeerConnected(String),
    /// A peer disconnected.
    PeerDisconnected(String),
    /// Response to a GetPeerCount command.
    PeerCount(usize),
}

// ---------------------------------------------------------------------------
// P2PService
// ---------------------------------------------------------------------------

/// Real libp2p-based P2P networking service for POLAY.
///
/// Wraps a libp2p Swarm running in a background tokio task. The node
/// communicates with the swarm through channels: `command_tx` to send
/// commands, `event_rx` to receive network events.
pub struct P2PService {
    command_tx: mpsc::Sender<P2PCommand>,
    event_rx: mpsc::Receiver<P2PEvent>,
}

impl P2PService {
    /// Create and start a new P2P service.
    ///
    /// This spawns a background tokio task that runs the libp2p swarm event
    /// loop. Returns a `P2PService` handle for the caller.
    pub async fn start(config: P2PConfig) -> Result<Self, NetworkError> {
        let (command_tx, command_rx) = mpsc::channel::<P2PCommand>(256);
        let (event_tx, event_rx) = mpsc::channel::<P2PEvent>(256);

        let max_peers = config.max_peers;
        let min_peers = config.min_peers;
        let enable_mdns = config.enable_mdns;
        let heartbeat_ms = config.gossipsub_heartbeat_ms;

        let keypair = config
            .node_keypair
            .unwrap_or_else(libp2p::identity::Keypair::generate_ed25519);

        let peer_id = keypair.public().to_peer_id();
        info!(%peer_id, max_peers, min_peers, enable_mdns, "starting P2P service");

        // Build the swarm.
        let mut swarm = libp2p::SwarmBuilder::with_existing_identity(keypair)
            .with_tokio()
            .with_tcp(
                tcp::Config::default(),
                noise::Config::new,
                yamux::Config::default,
            )
            .map_err(|e| NetworkError::TransportError(e.to_string()))?
            .with_behaviour(|key| {
                // Gossipsub configuration.
                let message_id_fn = |message: &gossipsub::Message| {
                    let mut hasher = DefaultHasher::new();
                    message.data.hash(&mut hasher);
                    message.topic.hash(&mut hasher);
                    gossipsub::MessageId::from(hasher.finish().to_string())
                };

                let gossipsub_config = gossipsub::ConfigBuilder::default()
                    .heartbeat_interval(Duration::from_millis(heartbeat_ms))
                    .validation_mode(gossipsub::ValidationMode::Strict)
                    .message_id_fn(message_id_fn)
                    .max_transmit_size(Self::MAX_BLOCK_MESSAGE_SIZE + 1024) // envelope overhead
                    .build()
                    .expect("valid gossipsub config");

                let gossipsub = gossipsub::Behaviour::new(
                    MessageAuthenticity::Signed(key.clone()),
                    gossipsub_config,
                )
                .expect("valid gossipsub behaviour");

                let mdns = mdns::tokio::Behaviour::new(
                    mdns::Config::default(),
                    key.public().to_peer_id(),
                )
                .expect("valid mDNS behaviour");

                PolayBehaviour { gossipsub, mdns }
            })
            .map_err(|e| NetworkError::TransportError(e.to_string()))?
            .build();

        // Subscribe to gossipsub topics.
        let tx_topic = IdentTopic::new(TOPIC_TRANSACTIONS);
        let block_topic = IdentTopic::new(TOPIC_BLOCKS);
        let consensus_topic = IdentTopic::new(TOPIC_CONSENSUS);

        swarm
            .behaviour_mut()
            .gossipsub
            .subscribe(&tx_topic)
            .map_err(|e| NetworkError::TransportError(format!("gossipsub subscribe: {e}")))?;
        swarm
            .behaviour_mut()
            .gossipsub
            .subscribe(&block_topic)
            .map_err(|e| NetworkError::TransportError(format!("gossipsub subscribe: {e}")))?;
        swarm
            .behaviour_mut()
            .gossipsub
            .subscribe(&consensus_topic)
            .map_err(|e| NetworkError::TransportError(format!("gossipsub subscribe: {e}")))?;

        // Start listening.
        let listen_addr: Multiaddr = config
            .listen_addr
            .parse()
            .map_err(|e| NetworkError::TransportError(format!("invalid listen addr: {e}")))?;
        swarm
            .listen_on(listen_addr)
            .map_err(|e| NetworkError::TransportError(format!("listen failed: {e}")))?;

        // Connect to boot nodes.
        for addr_str in &config.boot_nodes {
            match addr_str.parse::<Multiaddr>() {
                Ok(addr) => {
                    info!(%addr, "dialing boot node");
                    if let Err(e) = swarm.dial(addr) {
                        warn!(error = %e, "failed to dial boot node");
                    }
                }
                Err(e) => {
                    warn!(addr = %addr_str, error = %e, "invalid boot node multiaddr");
                }
            }
        }

        // Create the peer manager and rate limiter.
        let peer_manager = PeerManager::new(max_peers, min_peers);
        // 100 messages/sec, 20 MB/sec per peer -- generous for a blockchain.
        let rate_limiter = PeerRateLimiter::new(100, 20 * 1024 * 1024);

        // Spawn background event loop.
        tokio::spawn(Self::event_loop(
            swarm,
            command_rx,
            event_tx,
            peer_manager,
            rate_limiter,
            enable_mdns,
        ));

        Ok(P2PService {
            command_tx,
            event_rx,
        })
    }

    /// Broadcast a signed transaction to the network.
    pub async fn broadcast_tx(&self, tx: SignedTransaction) -> Result<(), NetworkError> {
        self.command_tx
            .send(P2PCommand::BroadcastTransaction(tx))
            .await
            .map_err(|_| NetworkError::ChannelClosed)
    }

    /// Broadcast a block to the network.
    pub async fn broadcast_block(&self, block: Block) -> Result<(), NetworkError> {
        self.command_tx
            .send(P2PCommand::BroadcastBlock(block))
            .await
            .map_err(|_| NetworkError::ChannelClosed)
    }

    /// Broadcast a consensus vote message to the network.
    pub async fn broadcast_consensus(&self, msg: ConsensusVoteMsg) -> Result<(), NetworkError> {
        self.command_tx
            .send(P2PCommand::BroadcastConsensusMessage(msg))
            .await
            .map_err(|_| NetworkError::ChannelClosed)
    }

    /// Receive the next event from the network.
    ///
    /// Returns `None` when the background task has stopped.
    pub async fn recv_event(&mut self) -> Option<P2PEvent> {
        self.event_rx.recv().await
    }

    /// Request the current peer count.
    pub async fn peer_count(&self) -> Result<(), NetworkError> {
        self.command_tx
            .send(P2PCommand::GetPeerCount)
            .await
            .map_err(|_| NetworkError::ChannelClosed)
    }

    /// Gracefully shut down the P2P service.
    ///
    /// Signals the background event loop to close all connections and
    /// terminate. The event loop will disconnect from all peers and drop
    /// the swarm.
    pub async fn shutdown(&self) -> Result<(), NetworkError> {
        self.command_tx
            .send(P2PCommand::Shutdown)
            .await
            .map_err(|_| NetworkError::ChannelClosed)
    }

    // -----------------------------------------------------------------
    // Background event loop
    // -----------------------------------------------------------------

    /// Interval at which expired bans are cleaned up.
    const BAN_CLEANUP_INTERVAL_SECS: u64 = 60;

    async fn event_loop(
        mut swarm: Swarm<PolayBehaviour>,
        mut command_rx: mpsc::Receiver<P2PCommand>,
        event_tx: mpsc::Sender<P2PEvent>,
        mut peer_manager: PeerManager,
        mut rate_limiter: PeerRateLimiter,
        enable_mdns: bool,
    ) {
        let tx_topic = IdentTopic::new(TOPIC_TRANSACTIONS);
        let block_topic = IdentTopic::new(TOPIC_BLOCKS);
        let consensus_topic = IdentTopic::new(TOPIC_CONSENSUS);

        let mut ban_cleanup_interval =
            tokio::time::interval(Duration::from_secs(Self::BAN_CLEANUP_INTERVAL_SECS));

        loop {
            tokio::select! {
                Some(cmd) = command_rx.recv() => {
                    let should_shutdown = matches!(cmd, P2PCommand::Shutdown);
                    Self::handle_command(&mut swarm, cmd, &tx_topic, &block_topic, &consensus_topic, &event_tx);
                    if should_shutdown {
                        info!("P2P service shutting down gracefully");
                        let connected: Vec<PeerId> = swarm.connected_peers().cloned().collect();
                        for pid in connected {
                            let _ = swarm.disconnect_peer_id(pid);
                        }
                        break;
                    }
                }
                event = swarm.select_next_some() => {
                    Self::handle_swarm_event(
                        event,
                        &mut swarm,
                        &event_tx,
                        &mut peer_manager,
                        &mut rate_limiter,
                        enable_mdns,
                    ).await;
                }
                _ = ban_cleanup_interval.tick() => {
                    let unbanned = peer_manager.cleanup_expired_bans();
                    if !unbanned.is_empty() {
                        debug!(count = unbanned.len(), "expired bans cleaned up");
                    }
                }
            }
        }

        info!("P2P event loop terminated");
    }

    /// Publish an envelope-wrapped payload to a gossipsub topic.
    fn publish_envelope(
        swarm: &mut Swarm<PolayBehaviour>,
        topic: &IdentTopic,
        payload: NetworkMessage,
        label: &str,
    ) {
        let envelope = MessageEnvelope::new(payload);
        match envelope.encode() {
            Ok(data) => {
                if let Err(e) = swarm.behaviour_mut().gossipsub.publish(topic.clone(), data) {
                    debug!(error = %e, label, "gossipsub publish failed (may have no peers)");
                }
            }
            Err(e) => error!(error = %e, label, "failed to encode envelope"),
        }
    }

    fn handle_command(
        swarm: &mut Swarm<PolayBehaviour>,
        cmd: P2PCommand,
        tx_topic: &IdentTopic,
        block_topic: &IdentTopic,
        consensus_topic: &IdentTopic,
        event_tx: &mpsc::Sender<P2PEvent>,
    ) {
        match cmd {
            P2PCommand::BroadcastTransaction(tx) => {
                Self::publish_envelope(
                    swarm,
                    tx_topic,
                    NetworkMessage::NewTransaction(tx),
                    "transaction",
                );
            }
            P2PCommand::BroadcastBlock(block) => {
                Self::publish_envelope(
                    swarm,
                    block_topic,
                    NetworkMessage::BlockProposal(block),
                    "block",
                );
            }
            P2PCommand::BroadcastConsensusMessage(msg) => {
                Self::publish_envelope(
                    swarm,
                    consensus_topic,
                    NetworkMessage::ConsensusVote(msg),
                    "consensus",
                );
            }
            P2PCommand::GetPeerCount => {
                let count = swarm.connected_peers().count();
                let _ = event_tx.try_send(P2PEvent::PeerCount(count));
            }
            P2PCommand::Shutdown => {
                // Handled by the caller in event_loop after this returns.
            }
        }
    }

    // -- Message size limits --------------------------------------------------

    /// Maximum size for a transaction gossipsub message (128 KB).
    const MAX_TX_MESSAGE_SIZE: usize = 128 * 1024;
    /// Maximum size for a block gossipsub message (10 MB).
    const MAX_BLOCK_MESSAGE_SIZE: usize = 10 * 1024 * 1024;
    /// Maximum size for a consensus gossipsub message (4 KB).
    const MAX_CONSENSUS_MESSAGE_SIZE: usize = 4 * 1024;

    /// Determine the per-topic size limit for a given topic string.
    fn max_size_for_topic(topic: &str) -> usize {
        if topic == TOPIC_TRANSACTIONS {
            Self::MAX_TX_MESSAGE_SIZE
        } else if topic == TOPIC_BLOCKS {
            Self::MAX_BLOCK_MESSAGE_SIZE
        } else if topic == TOPIC_CONSENSUS {
            Self::MAX_CONSENSUS_MESSAGE_SIZE
        } else {
            Self::MAX_TX_MESSAGE_SIZE
        }
    }

    async fn handle_swarm_event(
        event: SwarmEvent<PolayBehaviourEvent>,
        swarm: &mut Swarm<PolayBehaviour>,
        event_tx: &mpsc::Sender<P2PEvent>,
        peer_manager: &mut PeerManager,
        rate_limiter: &mut PeerRateLimiter,
        enable_mdns: bool,
    ) {
        match event {
            SwarmEvent::Behaviour(PolayBehaviourEvent::Gossipsub(
                gossipsub::Event::Message {
                    propagation_source,
                    message,
                    ..
                },
            )) => {
                let peer_id = propagation_source;
                let topic_str = message.topic.as_str();
                let data_len = message.data.len();

                // Reject messages from banned peers.
                if peer_manager.is_banned(&peer_id) {
                    debug!(%peer_id, "dropping message from banned peer");
                    return;
                }

                // Rate-limit check.
                if !rate_limiter.check_rate(&peer_id, data_len) {
                    warn!(%peer_id, "rate limit exceeded, dropping message");
                    let ban = peer_manager.record_bad_message(&peer_id);
                    if let Some(record) = ban {
                        warn!(%peer_id, reason = %record.reason, "peer banned (rate limiting)");
                        let _ = swarm.disconnect_peer_id(peer_id);
                    }
                    return;
                }

                // Per-topic size check.
                let max_size = Self::max_size_for_topic(topic_str);
                if data_len > max_size {
                    warn!(
                        %peer_id,
                        size = data_len,
                        max = max_size,
                        topic = topic_str,
                        "dropping oversized message"
                    );
                    let ban = peer_manager.record_bad_message(&peer_id);
                    if let Some(record) = ban {
                        warn!(%peer_id, reason = %record.reason, "peer banned (oversized messages)");
                        let _ = swarm.disconnect_peer_id(peer_id);
                    }
                    return;
                }

                // Decode the versioned envelope.
                let envelope = match MessageEnvelope::decode(&message.data) {
                    Ok(env) => env,
                    Err(e) => {
                        // Legacy fallback: try raw deserialization for peers
                        // that have not yet upgraded to the envelope format.
                        debug!(
                            %peer_id,
                            error = %e,
                            "envelope decode failed, attempting legacy fallback"
                        );
                        Self::handle_legacy_message(
                            &peer_id,
                            topic_str,
                            &message.data,
                            event_tx,
                            peer_manager,
                        )
                        .await;
                        return;
                    }
                };

                // Good envelope -- credit the peer.
                peer_manager.record_good_message(&peer_id);

                // Dispatch based on the inner payload.
                Self::dispatch_payload(
                    &peer_id,
                    topic_str,
                    envelope.payload,
                    event_tx,
                    peer_manager,
                )
                .await;
            }
            SwarmEvent::Behaviour(PolayBehaviourEvent::Mdns(mdns::Event::Discovered(peers))) => {
                if !enable_mdns {
                    return;
                }
                for (peer_id, addr) in peers {
                    info!(%peer_id, %addr, "mDNS discovered peer");
                    swarm
                        .behaviour_mut()
                        .gossipsub
                        .add_explicit_peer(&peer_id);
                    let _ = event_tx
                        .send(P2PEvent::PeerConnected(peer_id.to_string()))
                        .await;
                }
            }
            SwarmEvent::Behaviour(PolayBehaviourEvent::Mdns(mdns::Event::Expired(peers))) => {
                if !enable_mdns {
                    return;
                }
                for (peer_id, _addr) in peers {
                    info!(%peer_id, "mDNS peer expired");
                    swarm
                        .behaviour_mut()
                        .gossipsub
                        .remove_explicit_peer(&peer_id);
                    let _ = event_tx
                        .send(P2PEvent::PeerDisconnected(peer_id.to_string()))
                        .await;
                }
            }
            SwarmEvent::NewListenAddr { address, .. } => {
                info!(%address, "listening on");
            }
            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                if peer_manager.is_banned(&peer_id) {
                    warn!(%peer_id, "rejecting connection from banned peer");
                    let _ = swarm.disconnect_peer_id(peer_id);
                    return;
                }
                if !peer_manager.on_peer_connected(peer_id) {
                    if peer_manager.is_at_capacity() {
                        warn!(%peer_id, "at peer capacity, rejecting connection");
                    }
                    let _ = swarm.disconnect_peer_id(peer_id);
                    return;
                }
                debug!(%peer_id, peers = peer_manager.peer_count(), "connection established");
            }
            SwarmEvent::ConnectionClosed { peer_id, .. } => {
                peer_manager.on_peer_disconnected(&peer_id);
                rate_limiter.remove_peer(&peer_id);
                debug!(%peer_id, peers = peer_manager.peer_count(), "connection closed");
            }
            _ => {}
        }
    }

    /// Dispatch a successfully decoded envelope payload to the appropriate
    /// event channel.
    async fn dispatch_payload(
        peer_id: &PeerId,
        topic_str: &str,
        payload: NetworkMessage,
        event_tx: &mpsc::Sender<P2PEvent>,
        peer_manager: &mut PeerManager,
    ) {
        match payload {
            NetworkMessage::NewTransaction(tx) if topic_str == TOPIC_TRANSACTIONS => {
                let _ = event_tx.send(P2PEvent::TransactionReceived(tx)).await;
            }
            NetworkMessage::BlockProposal(block) if topic_str == TOPIC_BLOCKS => {
                let _ = event_tx.send(P2PEvent::BlockReceived(block)).await;
            }
            NetworkMessage::ConsensusVote(msg) if topic_str == TOPIC_CONSENSUS => {
                let _ = event_tx
                    .send(P2PEvent::ConsensusMessageReceived(msg))
                    .await;
            }
            _ => {
                // Payload type does not match the topic -- treat as bad message.
                warn!(
                    %peer_id,
                    topic = topic_str,
                    "payload type mismatch for topic"
                );
                peer_manager.record_bad_message(peer_id);
            }
        }
    }

    /// Backwards-compatible handler for legacy (non-envelope) messages.
    async fn handle_legacy_message(
        peer_id: &PeerId,
        topic_str: &str,
        data: &[u8],
        event_tx: &mpsc::Sender<P2PEvent>,
        peer_manager: &mut PeerManager,
    ) {
        if topic_str == TOPIC_TRANSACTIONS {
            match serde_json::from_slice::<SignedTransaction>(data) {
                Ok(tx) => {
                    peer_manager.record_good_message(peer_id);
                    let _ = event_tx.send(P2PEvent::TransactionReceived(tx)).await;
                }
                Err(e) => {
                    warn!(%peer_id, error = %e, "failed to deserialize legacy transaction");
                    peer_manager.record_bad_message(peer_id);
                }
            }
        } else if topic_str == TOPIC_BLOCKS {
            match serde_json::from_slice::<Block>(data) {
                Ok(block) => {
                    peer_manager.record_good_message(peer_id);
                    let _ = event_tx.send(P2PEvent::BlockReceived(block)).await;
                }
                Err(e) => {
                    warn!(%peer_id, error = %e, "failed to deserialize legacy block");
                    peer_manager.record_bad_message(peer_id);
                }
            }
        } else if topic_str == TOPIC_CONSENSUS {
            match serde_json::from_slice::<ConsensusVoteMsg>(data) {
                Ok(msg) => {
                    peer_manager.record_good_message(peer_id);
                    let _ = event_tx
                        .send(P2PEvent::ConsensusMessageReceived(msg))
                        .await;
                }
                Err(e) => {
                    warn!(%peer_id, error = %e, "failed to deserialize legacy consensus msg");
                    peer_manager.record_bad_message(peer_id);
                }
            }
        } else {
            warn!(%peer_id, topic = topic_str, "unknown topic in legacy message");
            peer_manager.record_bad_message(peer_id);
        }
    }
}

// Use the `futures` re-export from libp2p for `select_next_some`.
use libp2p::futures::StreamExt as _;

// ---------------------------------------------------------------------------
// NetworkService — the internal end of the channel pair (kept for compat)
// ---------------------------------------------------------------------------

/// The internal side of the networking layer.
///
/// `NetworkService` owns the send-side of the inbound channel (used to push
/// messages that the node should process) and the receive-side of the
/// outbound channel (used to drain messages the node wants to broadcast).
///
/// This is the channel-based interface used by `LocalNetwork` for in-process
/// testing.
pub struct NetworkService {
    /// Send messages into the node's inbound queue.
    inbound_tx: mpsc::Sender<NetworkMessage>,
    /// Receive messages the node wants to broadcast.
    outbound_rx: mpsc::Receiver<NetworkMessage>,
    /// Known peer addresses (for future use with real transports).
    pub peers: Vec<String>,
}

impl NetworkService {
    /// Create a paired `(NetworkService, NetworkHandle)` with the given
    /// channel buffer size.
    pub fn new(buffer_size: usize) -> (Self, NetworkHandle) {
        let (inbound_tx, inbound_rx) = mpsc::channel(buffer_size);
        let (outbound_tx, outbound_rx) = mpsc::channel(buffer_size);

        let service = NetworkService {
            inbound_tx,
            outbound_rx,
            peers: Vec::new(),
        };

        let handle = NetworkHandle {
            inbound_rx,
            outbound_tx,
        };

        (service, handle)
    }

    /// Push a message into the node's inbound queue (as if it arrived from
    /// the network).
    pub async fn inject_inbound(&self, msg: NetworkMessage) -> Result<(), NetworkError> {
        self.inbound_tx
            .send(msg)
            .await
            .map_err(|_| NetworkError::ChannelClosed)
    }

    /// Try to pull the next outbound message that the node wants to
    /// broadcast. Returns `None` if no message is available yet.
    pub async fn recv_outbound(&mut self) -> Option<NetworkMessage> {
        self.outbound_rx.recv().await
    }

    /// Run the service loop, draining outbound messages and logging them.
    pub async fn run(mut self) {
        info!(peers = self.peers.len(), "network service started (MVP stub)");
        while let Some(msg) = self.outbound_rx.recv().await {
            debug!(?msg, "outbound message (would broadcast to peers)");
        }
        info!("network service stopped");
    }
}

// ---------------------------------------------------------------------------
// NetworkHandle — the node-facing end of the channel pair
// ---------------------------------------------------------------------------

/// The node-facing half of the network channel pair.
///
/// Validator / node code uses this handle to broadcast messages and receive
/// messages that arrived from the network (or were injected by a test
/// harness).
pub struct NetworkHandle {
    /// Receive messages from the network (or test harness).
    inbound_rx: mpsc::Receiver<NetworkMessage>,
    /// Send messages to be broadcast to the network.
    outbound_tx: mpsc::Sender<NetworkMessage>,
}

impl NetworkHandle {
    /// Broadcast a message to the network.
    pub async fn broadcast(&self, msg: NetworkMessage) -> Result<(), NetworkError> {
        self.outbound_tx
            .send(msg)
            .await
            .map_err(|_| NetworkError::ChannelClosed)
    }

    /// Receive the next inbound message, waiting asynchronously.
    pub async fn recv(&mut self) -> Option<NetworkMessage> {
        self.inbound_rx.recv().await
    }

    /// Try to receive an inbound message without blocking.
    pub fn try_recv(&mut self) -> Option<NetworkMessage> {
        self.inbound_rx.try_recv().ok()
    }
}

// ---------------------------------------------------------------------------
// LocalNetwork — in-process multi-node test network (test-only)
// ---------------------------------------------------------------------------

/// An in-process network that connects multiple `NetworkService` instances
/// for local devnet testing.
///
/// Every message broadcast by one node is delivered to the inbound queues of
/// all other nodes. No real sockets are involved.
#[cfg(any(test, feature = "test-utils"))]
pub struct LocalNetwork {
    /// One entry per node: the `NetworkService` (internal end) for that node.
    services: Vec<NetworkService>,
}

#[cfg(any(test, feature = "test-utils"))]
impl LocalNetwork {
    /// Create a `LocalNetwork` with `n` nodes, returning the handles that
    /// each node should hold.
    pub fn new(n: usize, buffer_size: usize) -> (Self, Vec<NetworkHandle>) {
        let mut services = Vec::with_capacity(n);
        let mut handles = Vec::with_capacity(n);

        for _ in 0..n {
            let (svc, handle) = NetworkService::new(buffer_size);
            services.push(svc);
            handles.push(handle);
        }

        (LocalNetwork { services }, handles)
    }

    /// Run the local network relay loop.
    pub async fn run(mut self) {
        info!(
            nodes = self.services.len(),
            "local network relay started"
        );

        let inbound_senders: Vec<mpsc::Sender<NetworkMessage>> = self
            .services
            .iter()
            .map(|svc| svc.inbound_tx.clone())
            .collect();

        let (merged_tx, mut merged_rx) =
            mpsc::channel::<(usize, NetworkMessage)>(self.services.len() * 64);

        for (idx, mut svc) in self.services.drain(..).enumerate() {
            let merged_tx = merged_tx.clone();
            tokio::spawn(async move {
                while let Some(msg) = svc.outbound_rx.recv().await {
                    if merged_tx.send((idx, msg)).await.is_err() {
                        break;
                    }
                }
            });
        }

        drop(merged_tx);

        while let Some((sender_idx, msg)) = merged_rx.recv().await {
            for (idx, tx) in inbound_senders.iter().enumerate() {
                if idx == sender_idx {
                    continue;
                }
                if tx.send(msg.clone()).await.is_err() {
                    warn!(node = idx, "failed to deliver message to node (channel closed)");
                }
            }
        }

        info!("local network relay stopped");
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn service_and_handle_round_trip() {
        let (svc, mut handle) = NetworkService::new(16);

        svc.inject_inbound(NetworkMessage::Ping(1)).await.unwrap();

        let msg = handle.recv().await.unwrap();
        match msg {
            NetworkMessage::Ping(n) => assert_eq!(n, 1),
            _ => panic!("expected Ping"),
        }

        handle.broadcast(NetworkMessage::Pong(1)).await.unwrap();

        drop(handle);

        let mut svc = svc;
        let out = svc.recv_outbound().await.unwrap();
        match out {
            NetworkMessage::Pong(n) => assert_eq!(n, 1),
            _ => panic!("expected Pong"),
        }
    }

    #[tokio::test]
    async fn local_network_relays_messages() {
        let (local_net, mut handles) = LocalNetwork::new(3, 64);

        let relay_handle = tokio::spawn(local_net.run());

        handles[0].broadcast(NetworkMessage::Ping(42)).await.unwrap();

        let msg1 = handles[1].recv().await.unwrap();
        match msg1 {
            NetworkMessage::Ping(n) => assert_eq!(n, 42),
            _ => panic!("expected Ping on node 1"),
        }

        let msg2 = handles[2].recv().await.unwrap();
        match msg2 {
            NetworkMessage::Ping(n) => assert_eq!(n, 42),
            _ => panic!("expected Ping on node 2"),
        }

        drop(handles);
        let _ = relay_handle.await;
    }

    #[tokio::test]
    async fn try_recv_returns_none_when_empty() {
        let (_svc, mut handle) = NetworkService::new(16);
        assert!(handle.try_recv().is_none());
    }

    #[tokio::test]
    async fn p2p_service_can_be_created() {
        // Verify that P2PService::start does not panic.
        // We use a random port to avoid conflicts.
        let config = P2PConfig {
            listen_addr: "/ip4/127.0.0.1/tcp/0".to_string(),
            boot_nodes: Vec::new(),
            node_keypair: None,
            ..Default::default()
        };
        let service = P2PService::start(config).await;
        assert!(service.is_ok());
    }

    #[tokio::test]
    async fn p2p_service_shutdown() {
        let config = P2PConfig {
            listen_addr: "/ip4/127.0.0.1/tcp/0".to_string(),
            ..Default::default()
        };
        let service = P2PService::start(config).await.unwrap();
        assert!(service.shutdown().await.is_ok());
    }

    #[test]
    fn p2p_config_default() {
        let config = P2PConfig::default();
        assert_eq!(config.listen_addr, "/ip4/0.0.0.0/tcp/30333");
        assert!(config.boot_nodes.is_empty());
        assert!(config.node_keypair.is_none());
        assert_eq!(config.max_peers, 50);
        assert_eq!(config.min_peers, 4);
        assert_eq!(config.ban_duration_secs, 3600);
        assert!(config.enable_mdns);
        assert_eq!(config.gossipsub_heartbeat_ms, 1000);
    }

    #[test]
    fn network_message_serde_for_gossipsub() {
        // Test that all message types used over gossipsub can round-trip through JSON.
        use polay_types::address::Address;
        use polay_types::hash::Hash as PolayHash;
        use polay_types::signature::Signature;

        // Transaction
        let tx = polay_types::transaction::SignedTransaction::new(
            polay_types::transaction::Transaction {
                chain_id: "test".into(),
                nonce: 1,
                signer: Address::ZERO,
                action: polay_types::transaction::TransactionAction::Transfer {
                    to: Address::ZERO,
                    amount: 100,
                },
                max_fee: 10,
                timestamp: 12345,
                session: None,
                sponsor: None,
            },
            Signature::ZERO,
            PolayHash::ZERO,
            vec![0u8; 32],
        );
        let data = serde_json::to_vec(&tx).unwrap();
        let parsed: SignedTransaction = serde_json::from_slice(&data).unwrap();
        assert_eq!(parsed.tx_hash, tx.tx_hash);

        // ConsensusVoteMsg
        let vote = ConsensusVoteMsg {
            height: 5,
            round: 0,
            vote_type: "prevote".to_string(),
            block_hash: PolayHash::ZERO,
            voter: Address::ZERO,
            voter_signature: Signature::ZERO,
        };
        let data = serde_json::to_vec(&vote).unwrap();
        let parsed: ConsensusVoteMsg = serde_json::from_slice(&data).unwrap();
        assert_eq!(parsed.height, 5);
        assert_eq!(parsed.vote_type, "prevote");
    }
}
