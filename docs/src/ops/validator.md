# Running a Validator

This guide covers the requirements and steps for running a POLAY validator node in production.

## Hardware Requirements

| Resource | Minimum | Recommended |
|---|---|---|
| CPU | 4 cores | 8 cores |
| RAM | 8 GB | 16 GB |
| Storage | 100 GB SSD | 500 GB NVMe SSD |
| Network | 100 Mbps | 1 Gbps |
| OS | Ubuntu 22.04+ / Debian 12+ | Ubuntu 24.04 |

SSD storage is critical -- RocksDB performance degrades significantly on spinning disks. NVMe is preferred for production validators.

## Key Generation

Generate a validator keypair:

```bash
./target/release/polay keys generate --type validator
```

Output:

```
Validator address:  polay1abc...def
Public key:         ed25519:7f3a...
Private key file:   ~/.polay/validator_key.json
```

Keep the private key file secure. Back it up offline. If lost, the validator cannot sign blocks and will be jailed for downtime.

Also generate a node identity key:

```bash
./target/release/polay keys generate --type node
```

This produces `~/.polay/node_key.json` used for libp2p peer identity.

## Genesis Participation

For a new network, validators must be included in the genesis file:

1. Generate your validator key
2. Share your public key and desired initial stake with the genesis coordinator
3. The coordinator creates the genesis file with all validators
4. Distribute the finalized `genesis.json` to all validators

For joining an existing network, submit a `RegisterValidator` transaction after syncing.

## Configuration

Edit `~/.polay/config.toml`:

```toml
[chain]
chain_id = "polay-testnet-1"

[node]
moniker = "my-validator"
data_dir = "/data/polay"

[network]
listen_addr = "/ip4/0.0.0.0/tcp/26656"
external_addr = "/ip4/YOUR_PUBLIC_IP/tcp/26656"
boot_nodes = [
    "/ip4/34.120.55.10/tcp/26656/p2p/12D3KooWAbC...",
]
max_peers = 50

[rpc]
listen_addr = "127.0.0.1:9944"  # bind to localhost only for security
ws_addr = "127.0.0.1:9945"

[consensus]
# Timeouts in milliseconds
propose_timeout = 3000
prevote_timeout = 1000
precommit_timeout = 1000

[staking]
validator_key = "/root/.polay/validator_key.json"
```

### Security Notes

- **Bind RPC to localhost** unless you need external access. Use a reverse proxy (nginx) with TLS for external RPC.
- **Set `external_addr`** to your public IP so peers can connect to you.
- **Open port 26656** (P2P) in your firewall. Keep port 9944 (RPC) restricted.

## Starting the Validator

```bash
./target/release/polay run \
  --home /data/polay \
  --validator
```

With systemd for automatic restarts:

```ini
# /etc/systemd/system/polay.service
[Unit]
Description=POLAY Validator
After=network-online.target

[Service]
Type=simple
User=polay
ExecStart=/usr/local/bin/polay run --home /data/polay --validator
Restart=always
RestartSec=5
LimitNOFILE=65535

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable polay
sudo systemctl start polay
```

## Verifying Your Validator

Check that your validator is in the active set:

```bash
curl -s http://localhost:9944 -d '{
  "jsonrpc":"2.0","id":1,"method":"state_getValidator",
  "params":["polay1yourvalidator..."]
}' | jq
```

Look for `"status": "Active"` in the response.

Check that blocks are being proposed:

```bash
journalctl -u polay -f | grep "proposed block"
```

## Monitoring

Set up monitoring to detect issues before they lead to jailing:

- **Block height** -- should increment every ~1.5 seconds
- **Missed blocks** -- track consecutive missed proposals
- **Peer count** -- should be > 0 at all times
- **Disk usage** -- RocksDB grows over time; plan for compaction

See the [Monitoring](./monitoring.md) page for Prometheus + Grafana setup.

## Operational Practices

- **Keep software updated.** Follow the release channel for chain upgrades.
- **Monitor disk space.** Set alerts at 80% usage.
- **Maintain backups.** Snapshot the data directory periodically.
- **Secure your keys.** Use file permissions (`chmod 600`) and consider hardware security modules for mainnet.
- **Set up alerting.** Get notified on missed blocks, peer drops, or high resource usage.
- **Test upgrades on devnet first.** Never upgrade production without testing.
