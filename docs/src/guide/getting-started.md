# Getting Started

This guide walks you through building POLAY from source and running a local single-validator node.

## Prerequisites

| Dependency | Minimum Version | Notes |
|---|---|---|
| Rust | 1.77+ | Install via [rustup](https://rustup.rs/) |
| RocksDB | 8.0+ | Usually built from source by the `rocksdb` crate |
| clang / LLVM | 14+ | Required to compile RocksDB |
| pkg-config | any | For system library discovery |
| Git | any | To clone the repository |

### Platform-Specific Setup

**macOS:**

```bash
brew install llvm pkg-config
```

**Ubuntu / Debian:**

```bash
sudo apt update
sudo apt install -y build-essential clang libclang-dev pkg-config librocksdb-dev
```

**Arch Linux:**

```bash
sudo pacman -S base-devel clang rocksdb
```

## Build from Source

```bash
# Clone the repository
git clone https://github.com/polay-chain/polay.git
cd polay

# Build in release mode
cargo build --release

# Run tests
cargo test --workspace
```

The binary is produced at `./target/release/polay`.

## Initialize a Devnet

The `init` command creates a genesis file, validator keys, and default configuration:

```bash
./target/release/polay init --chain devnet
```

This creates the data directory at `~/.polay/` with:

```
~/.polay/
  config.toml          # node configuration
  genesis.json         # genesis state (accounts, validators)
  node_key.json        # libp2p node identity
  validator_key.json   # validator signing key
  data/                # RocksDB state directory
```

## Run the Node

```bash
./target/release/polay run
```

You should see log output like:

```
INFO  polay_node  > Starting POLAY node v0.1.0
INFO  polay_node  > Chain ID: polay-devnet-1
INFO  polay_node  > Validator: polay1abc...def
INFO  polay_network > Listening on /ip4/0.0.0.0/tcp/26656
INFO  polay_rpc     > JSON-RPC server listening on 0.0.0.0:9944
INFO  polay_rpc     > WebSocket server listening on 0.0.0.0:9945
INFO  polay_consensus > Block #1 committed (0 txs) in 1.2s
INFO  polay_consensus > Block #2 committed (0 txs) in 1.4s
```

## Verify the Node

Query the node health endpoint:

```bash
curl -s http://localhost:9944 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"system_health","params":[]}' | jq
```

Expected response:

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "is_syncing": false,
    "peers": 0,
    "should_have_peers": false
  }
}
```

Query the latest block:

```bash
curl -s http://localhost:9944 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"chain_getBlock","params":["latest"]}' | jq
```

## Next Steps

- [Local Devnet](./local-devnet.md) -- run a multi-validator local network
- [TypeScript SDK](./sdk.md) -- interact with the chain from JavaScript/TypeScript
- [Transaction Types](./transaction-types.md) -- reference for all 40 action types
