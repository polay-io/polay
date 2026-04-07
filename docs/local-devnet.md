# Local Devnet Guide

This guide covers setting up and running a POLAY local development network for testing and development.

## Prerequisites

### Required

- **Rust** (1.75+ with `cargo`): [https://rustup.rs](https://rustup.rs)
- **Docker** and **Docker Compose** (for containerized multi-node setup): [https://docs.docker.com/get-docker](https://docs.docker.com/get-docker)

### Optional

- **jq**: For formatting JSON-RPC responses in the terminal. Install with `brew install jq` (macOS) or `apt install jq` (Linux).
- **curl**: For sending RPC requests (usually pre-installed on macOS/Linux).

### Verify installation

```bash
rustc --version    # should be 1.75.0 or higher
cargo --version
docker --version
docker compose version
```

## Quick Start (Single Node)

The fastest way to run a POLAY devnet:

```bash
# Clone the repository
git clone https://github.com/polay-chain/polay.git
cd polay

# Build all crates
cargo build --release

# Generate a genesis configuration with 1 validator
./target/release/polay-cli genesis init \
  --chain-id polay-devnet-1 \
  --validators 1 \
  --initial-balance 1000000000000 \
  --output ./genesis.json

# Start the node
./target/release/polay-node \
  --genesis ./genesis.json \
  --data-dir ./data/node0 \
  --rpc-port 9944 \
  --p2p-port 30333
```

The node will begin producing blocks immediately (single-validator mode does not require BFT rounds). You should see log output like:

```
[INFO] POLAY node starting...
[INFO] Chain ID: polay-devnet-1
[INFO] Genesis loaded: 1 validators, height 0
[INFO] RPC server listening on 0.0.0.0:9944
[INFO] P2P server listening on 0.0.0.0:30333
[INFO] Block 1 committed (0 txs) proposer=validator0 time=1ms
[INFO] Block 2 committed (0 txs) proposer=validator0 time=1ms
```

## Docker Compose Setup (Multi-Node)

For a more realistic network with multiple validators:

### docker-compose.yml

The repository includes a Docker Compose file for a 4-node devnet:

```bash
# Build the Docker image
docker build -t polay-node .

# Generate genesis for 4 validators
./target/release/polay-cli genesis init \
  --chain-id polay-devnet-1 \
  --validators 4 \
  --initial-balance 1000000000000 \
  --output ./devnet/genesis.json

# Start the network
docker compose up -d
```

This starts 4 nodes:

| Node | RPC Port | P2P Port | Role |
|---|---|---|---|
| `node0` | 9944 | 30333 | Validator 0 |
| `node1` | 9945 | 30334 | Validator 1 |
| `node2` | 9946 | 30335 | Validator 2 |
| `node3` | 9947 | 30336 | Validator 3 |

### Verify the network

```bash
# Check chain info on each node
curl -s -X POST http://localhost:9944 \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","method":"polay_getChainInfo","params":{},"id":1}' | jq

# All nodes should report the same height (within 1-2 blocks)
for port in 9944 9945 9946 9947; do
  echo "Node on port $port:"
  curl -s -X POST http://localhost:$port \
    -H 'Content-Type: application/json' \
    -d '{"jsonrpc":"2.0","method":"polay_getChainInfo","params":{},"id":1}' | jq '.result.current_height'
done
```

### Stop the network

```bash
docker compose down

# To also remove data volumes:
docker compose down -v
```

## Manual Multi-Node Setup (Without Docker)

For development and debugging, you may want to run nodes directly:

```bash
# Generate genesis for 4 validators
./target/release/polay-cli genesis init \
  --chain-id polay-devnet-1 \
  --validators 4 \
  --initial-balance 1000000000000 \
  --output ./genesis.json

# The genesis command outputs validator keys to ./keys/validator{0..3}.json

# Terminal 1: Node 0
./target/release/polay-node \
  --genesis ./genesis.json \
  --data-dir ./data/node0 \
  --validator-key ./keys/validator0.json \
  --rpc-port 9944 \
  --p2p-port 30333

# Terminal 2: Node 1
./target/release/polay-node \
  --genesis ./genesis.json \
  --data-dir ./data/node1 \
  --validator-key ./keys/validator1.json \
  --rpc-port 9945 \
  --p2p-port 30334 \
  --bootnodes /ip4/127.0.0.1/tcp/30333

# Terminal 3: Node 2
./target/release/polay-node \
  --genesis ./genesis.json \
  --data-dir ./data/node2 \
  --validator-key ./keys/validator2.json \
  --rpc-port 9946 \
  --p2p-port 30335 \
  --bootnodes /ip4/127.0.0.1/tcp/30333

# Terminal 4: Node 3
./target/release/polay-node \
  --genesis ./genesis.json \
  --data-dir ./data/node3 \
  --validator-key ./keys/validator3.json \
  --rpc-port 9947 \
  --p2p-port 30336 \
  --bootnodes /ip4/127.0.0.1/tcp/30333
```

Nodes 1-3 use Node 0 as a bootnode for initial peer discovery. After connection, gossipsub handles peer-to-peer propagation.

## Genesis Configuration

The `genesis init` command generates a `genesis.json` file:

```json
{
  "chain_id": "polay-devnet-1",
  "timestamp": 1700000000,
  "params": {
    "epoch_length": 100,
    "max_validators": 21,
    "min_self_stake": 100000,
    "block_reward": 10,
    "quarantine_threshold": 30
  },
  "accounts": [
    {
      "address": "a1b2c3d4...",
      "balance": 1000000000000
    },
    {
      "address": "e5f6a7b8...",
      "balance": 1000000000000
    }
  ],
  "validators": [
    {
      "address": "a1b2c3d4...",
      "public_key": "...",
      "stake": 100000000,
      "commission_rate": 1000
    }
  ]
}
```

### Custom genesis

You can create a custom genesis by editing the JSON directly or using CLI flags:

```bash
# More validators, different chain ID, custom epoch length
./target/release/polay-cli genesis init \
  --chain-id my-test-chain \
  --validators 7 \
  --initial-balance 5000000000000 \
  --epoch-length 50 \
  --output ./my-genesis.json
```

### Adding extra accounts

To add non-validator accounts with pre-funded balances (useful for testing):

```bash
# Generate a key pair
./target/release/polay-cli keys generate --output ./test-account.json

# Add the account to genesis before starting the network
./target/release/polay-cli genesis add-account \
  --genesis ./genesis.json \
  --address $(cat test-account.json | jq -r '.address') \
  --balance 1000000000000
```

## Sample Transaction Walkthrough

This walkthrough demonstrates the core transaction types using `curl` and the JSON-RPC interface.

### Step 1: Check initial balances

```bash
# Get Alice's account (first genesis account)
curl -s -X POST http://localhost:9944 \
  -H 'Content-Type: application/json' \
  -d '{
    "jsonrpc": "2.0",
    "method": "polay_getAccount",
    "params": { "address": "ALICE_ADDRESS" },
    "id": 1
  }' | jq
```

### Step 2: Transfer POL

```bash
# Use the CLI to construct and sign a transfer
./target/release/polay-cli tx transfer \
  --from ./keys/alice.json \
  --to BOB_ADDRESS \
  --amount 1000000 \
  --rpc http://localhost:9944

# Expected output:
# Transaction submitted: tx_hash=abc123...
# Status: accepted
```

### Step 3: Create an asset class

```bash
./target/release/polay-cli tx create-asset-class \
  --from ./keys/alice.json \
  --name "Legendary Sword" \
  --max-supply 1000 \
  --metadata '{"rarity": "legendary", "damage": 50}' \
  --rpc http://localhost:9944

# Expected output:
# Asset class created: id=aaa111...
```

### Step 4: Mint assets

```bash
./target/release/polay-cli tx mint-asset \
  --from ./keys/alice.json \
  --asset-class-id aaa111... \
  --to BOB_ADDRESS \
  --quantity 5 \
  --rpc http://localhost:9944
```

### Step 5: List an asset for sale

```bash
./target/release/polay-cli tx list-asset \
  --from ./keys/bob.json \
  --asset-class-id aaa111... \
  --quantity 2 \
  --price 50000 \
  --rpc http://localhost:9944

# Expected output:
# Listing created: id=bbb222...
```

### Step 6: Buy the listed asset

```bash
./target/release/polay-cli tx buy-asset \
  --from ./keys/charlie.json \
  --listing-id bbb222... \
  --rpc http://localhost:9944
```

### Step 7: Verify the results

```bash
# Check Bob's asset balance (should be 3: 5 minted - 2 listed and sold)
curl -s -X POST http://localhost:9944 \
  -H 'Content-Type: application/json' \
  -d '{
    "jsonrpc": "2.0",
    "method": "polay_getAssetBalance",
    "params": { "address": "BOB_ADDRESS", "asset_class_id": "aaa111..." },
    "id": 1
  }' | jq

# Check Charlie's asset balance (should be 2: purchased from listing)
curl -s -X POST http://localhost:9944 \
  -H 'Content-Type: application/json' \
  -d '{
    "jsonrpc": "2.0",
    "method": "polay_getAssetBalance",
    "params": { "address": "CHARLIE_ADDRESS", "asset_class_id": "aaa111..." },
    "id": 1
  }' | jq

# Check the listing (should be inactive after purchase)
curl -s -X POST http://localhost:9944 \
  -H 'Content-Type: application/json' \
  -d '{
    "jsonrpc": "2.0",
    "method": "polay_getListing",
    "params": { "listing_id": "bbb222..." },
    "id": 1
  }' | jq
```

## Useful RPC Queries

### Chain status
```bash
curl -s -X POST http://localhost:9944 \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","method":"polay_getChainInfo","params":{},"id":1}' | jq
```

### Latest block
```bash
curl -s -X POST http://localhost:9944 \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","method":"polay_getLatestBlock","params":{},"id":1}' | jq
```

### Validator set
```bash
curl -s -X POST http://localhost:9944 \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","method":"polay_getValidatorSet","params":{},"id":1}' | jq
```

### Active marketplace listings
```bash
curl -s -X POST http://localhost:9944 \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","method":"polay_getListings","params":{"is_active":true,"limit":20},"id":1}' | jq
```

## Troubleshooting

### Node fails to start: "genesis file not found"

Ensure the `--genesis` path is correct and the file exists. Use an absolute path if running from a different directory than where the genesis file was generated.

### Nodes not connecting to each other

- Check that P2P ports are not blocked by a firewall.
- Verify the `--bootnodes` multiaddr is correct: `/ip4/127.0.0.1/tcp/PORT`.
- Check logs for `Peer connected` or `Connection refused` messages.
- Ensure all nodes are using the same `genesis.json` (same chain ID). Nodes on different chain IDs will refuse to connect.

### Blocks not being produced

- Single-node setup: blocks should produce immediately. Check logs for errors.
- Multi-node setup: ensure at least 3 of 4 validators are running (BFT requires 2/3+).
- Check that each node has a unique `--data-dir`. Sharing a data directory between nodes causes corruption.

### Transaction stuck in mempool

- Check the nonce: use `polay_getAccount` to see the current nonce, and ensure your transaction uses exactly that nonce.
- Check the balance: ensure the sender has enough POL for the fee and the transaction amount.
- Check the signature: the signature must cover the exact transaction payload. Use the CLI to construct transactions to avoid manual signing errors.

### RPC returns "Internal error"

Check the node logs for the full error message. Common causes:
- RocksDB data directory has insufficient permissions.
- State corruption from unclean shutdown. Delete the data directory and restart from genesis.
- Incompatible genesis file (generated with a different version of the CLI).

### Resetting the devnet

To start fresh, stop all nodes and delete data directories:

```bash
# Docker
docker compose down -v

# Manual
rm -rf ./data/node0 ./data/node1 ./data/node2 ./data/node3
```

Then regenerate genesis and restart nodes.
