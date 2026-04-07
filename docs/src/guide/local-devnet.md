# Local Devnet

This guide covers running a multi-validator POLAY network on your local machine for development and testing.

## Single-Validator (Quickstart)

The fastest path to a running devnet:

```bash
# Initialize with default devnet config
./target/release/polay init --chain devnet

# Run the node
./target/release/polay run
```

This starts a single validator that produces blocks immediately. Useful for basic development, but does not exercise consensus or networking.

## Multi-Validator with init-devnet.sh

The repository includes a script that sets up a 4-validator local network:

```bash
# Generate config for 4 validators
./scripts/init-devnet.sh

# This creates:
# /tmp/polay-devnet/validator-0/
# /tmp/polay-devnet/validator-1/
# /tmp/polay-devnet/validator-2/
# /tmp/polay-devnet/validator-3/
```

Each validator directory contains its own `config.toml`, keys, and genesis file. The genesis file is shared and includes all four validators in the initial set.

Start each validator in a separate terminal:

```bash
# Terminal 1
./target/release/polay run --home /tmp/polay-devnet/validator-0

# Terminal 2
./target/release/polay run --home /tmp/polay-devnet/validator-1

# Terminal 3
./target/release/polay run --home /tmp/polay-devnet/validator-2

# Terminal 4
./target/release/polay run --home /tmp/polay-devnet/validator-3
```

The validators discover each other via mDNS and begin producing blocks through BFT consensus. RPC is available on ports 9944-9947.

## Docker Compose

For a containerized devnet that handles startup orchestration:

```bash
cd docker
docker-compose up -d
```

The `docker-compose.yml` launches:

| Service | Port | Description |
|---|---|---|
| `validator-0` | 9944, 26656 | RPC + P2P |
| `validator-1` | 9945, 26657 | RPC + P2P |
| `validator-2` | 9946, 26658 | RPC + P2P |
| `validator-3` | 9947, 26659 | RPC + P2P |

```bash
# View logs
docker-compose logs -f validator-0

# Stop the network
docker-compose down

# Stop and wipe state
docker-compose down -v
```

## Genesis Configuration

The devnet genesis allocates test accounts with funds:

```json
{
  "chain_id": "polay-devnet-1",
  "genesis_time": "2025-01-01T00:00:00Z",
  "accounts": [
    {
      "address": "polay1alice...",
      "balance": 10000000000
    },
    {
      "address": "polay1bob...",
      "balance": 10000000000
    }
  ],
  "validators": [
    {
      "address": "polay1val0...",
      "pub_key": "ed25519:...",
      "stake": 1000000000
    }
  ]
}
```

To customize the genesis, edit the template in `scripts/genesis-template.json` before running `init-devnet.sh`.

## Faucet

The devnet includes a faucet account with a large balance. Request funds via RPC:

```bash
curl -s http://localhost:9944 \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "faucet_fund",
    "params": ["polay1youraddress...", 1000000000]
  }'
```

This is only available on devnet (`chain_id` starting with `polay-devnet`).

## Resetting State

To reset the devnet and start fresh:

```bash
# Script-based devnet
rm -rf /tmp/polay-devnet
./scripts/init-devnet.sh

# Docker devnet
docker-compose down -v
docker-compose up -d
```

## Troubleshooting

| Problem | Solution |
|---|---|
| Validators not finding each other | Ensure mDNS is not blocked by firewall; check that all nodes use the same `chain_id` |
| Blocks not being produced | Need at least 3 of 4 validators online for 2/3+ quorum |
| Port already in use | Stop other services on ports 9944-9947 and 26656-26659 |
| State corruption after crash | Reset state with `--reset-state` flag or delete the `data/` directory |
