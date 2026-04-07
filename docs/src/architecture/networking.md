# Networking

The `polay-network` crate implements POLAY's peer-to-peer networking layer using [libp2p](https://libp2p.io/). It handles peer discovery, message propagation, and connection management.

## Transport Stack

```
Application (gossipsub messages)
        |
    Gossipsub (pub/sub)
        |
    Yamux (stream multiplexing)
        |
    Noise (encryption + authentication)
        |
    TCP (transport)
```

- **TCP** -- reliable, ordered byte streams
- **Noise** -- authenticated encryption using Ed25519 node keys (XX handshake pattern)
- **Yamux** -- multiplexes multiple logical streams over a single TCP connection
- **Gossipsub** -- publish/subscribe message routing with mesh-based propagation

## Gossipsub Topics

Nodes subscribe to three gossipsub topics:

| Topic | Messages | Publishers |
|---|---|---|
| `transactions` | Signed transactions submitted by users | Any node with an RPC port |
| `blocks` | Finalized blocks with commit signatures | Block proposers |
| `consensus` | Proposals, prevotes, precommits | Active validators |

Messages are serialized with Borsh and wrapped in a `MessageEnvelope`:

```rust
pub struct MessageEnvelope {
    pub version: u32,         // protocol version for forward compat
    pub sender: PeerId,
    pub topic: String,
    pub payload: Vec<u8>,     // borsh-encoded message
    pub signature: Signature,
}
```

## Peer Discovery

POLAY supports two discovery mechanisms:

### mDNS (Local)

Multicast DNS automatically discovers peers on the local network. This is enabled by default for devnet and testnet configurations. It requires no configuration -- nodes find each other within seconds on the same LAN.

### Static Peers (Boot Nodes)

For production deployments, boot node addresses are specified in `config.toml`:

```toml
[network]
boot_nodes = [
    "/ip4/34.120.55.10/tcp/26656/p2p/12D3KooWAbC...",
    "/ip4/35.200.88.22/tcp/26656/p2p/12D3KooWXyZ...",
]
```

On startup, the node dials all boot nodes and requests their peer lists via the Identify protocol. This bootstraps the peer-to-peer mesh.

## Peer Scoring

Gossipsub uses a scoring function to evaluate peer behavior. Well-behaved peers are preferred for mesh membership; badly-behaved peers are pruned or banned.

| Behavior | Score Impact |
|---|---|
| Delivers valid messages promptly | +1 per message |
| First to deliver a new message | +5 |
| Delivers invalid messages | -10 per message |
| Sends duplicate messages excessively | -5 per burst |
| Fails to respond to protocol requests | -2 per timeout |
| Detected equivocation evidence | -50 |

**Score thresholds:**

- Below **0**: peer is deprioritized in mesh
- Below **-50**: peer is pruned from mesh
- Below **-100**: peer is banned (connection dropped, IP blocked temporarily)

## Rate Limiting

To prevent DoS attacks, the networking layer enforces rate limits:

| Limit | Value |
|---|---|
| Messages per second per peer | 100 |
| Bandwidth per peer | 10 MB/s |
| Max concurrent connections | 50 |
| Max pending connections | 20 |
| Connection timeout | 10 s |

Peers exceeding rate limits have their score penalized. Persistent offenders are banned.

## Connection Management

The connection manager maintains a target peer count and handles churn:

```toml
[network]
listen_addr = "/ip4/0.0.0.0/tcp/26656"
max_peers = 50
min_peers = 5
target_peers = 25
```

- If connected peers drop below `min_peers`, the node aggressively seeks new connections.
- If connected peers exceed `max_peers`, the lowest-scored peers are disconnected.
- Persistent peers (validators in the active set) are never disconnected.

## Protocol Versioning

The `MessageEnvelope.version` field enables protocol evolution. Nodes reject messages with an incompatible major version but accept messages with a higher minor version (ignoring unknown fields). This allows rolling upgrades across the network.

Current protocol version: **1.0**
