# Monitoring

POLAY ships with a Prometheus metrics exporter and a Grafana dashboard for observing node health, chain progress, and network performance.

## Architecture

```
POLAY Node (RPC)
      |
metrics-exporter.py  (polls RPC every 15s)
      |
Prometheus  (scrapes metrics-exporter on :9100)
      |
Grafana  (visualizes dashboards on :3000)
```

## Metrics Exporter

The exporter is a Python script at `scripts/metrics-exporter.py` that polls the node's JSON-RPC endpoint and exposes Prometheus-compatible metrics.

### Running the Exporter

```bash
pip install prometheus-client requests

python scripts/metrics-exporter.py \
  --rpc-url http://localhost:9944 \
  --port 9100 \
  --interval 15
```

Or with Docker:

```bash
docker run -d \
  --name polay-metrics \
  --network host \
  -e RPC_URL=http://localhost:9944 \
  -e PORT=9100 \
  polay/metrics-exporter:latest
```

## Exported Metrics

The exporter publishes 18 metrics:

| Metric | Type | Description |
|---|---|---|
| `polay_block_height` | Gauge | Current block height |
| `polay_block_time_ms` | Gauge | Time to produce last block |
| `polay_peer_count` | Gauge | Number of connected peers |
| `polay_mempool_size` | Gauge | Pending transactions in mempool |
| `polay_uptime_seconds` | Gauge | Node uptime |
| `polay_total_supply` | Gauge | Total POL supply |
| `polay_circulating_supply` | Gauge | Circulating POL supply |
| `polay_total_staked` | Gauge | Total staked POL |
| `polay_total_burned` | Gauge | Cumulative burned POL |
| `polay_treasury_balance` | Gauge | Treasury balance |
| `polay_total_minted` | Gauge | Cumulative minted POL (inflation) |
| `polay_fees_collected` | Counter | Total fees collected |
| `polay_active_validators` | Gauge | Number of active validators |
| `polay_total_transactions` | Counter | Total confirmed transactions |
| `polay_current_epoch` | Gauge | Current epoch number |
| `polay_inflation_rate_bps` | Gauge | Current annual inflation rate in basis points |
| `polay_epoch_reward` | Gauge | Reward distributed per epoch |
| `polay_is_syncing` | Gauge | 1 if node is syncing, 0 if caught up |

## Prometheus Configuration

Add the exporter as a scrape target in `prometheus.yml`:

```yaml
scrape_configs:
  - job_name: 'polay'
    scrape_interval: 15s
    static_configs:
      - targets: ['localhost:9100']
        labels:
          chain: 'polay-testnet-1'
          node: 'validator-0'
```

## Grafana Dashboard

Import the provided dashboard from `scripts/grafana-dashboard.json`:

1. Open Grafana at `http://localhost:3000`
2. Go to Dashboards > Import
3. Upload `scripts/grafana-dashboard.json`
4. Select the Prometheus data source

### Dashboard Panels

The dashboard includes the following panels:

**Chain Overview Row:**
- Block height (stat panel with sparkline)
- Block time (gauge, green < 2s, yellow < 3s, red > 3s)
- Transactions per block (time series)
- Mempool size (time series)

**Network Row:**
- Peer count (stat)
- Active validators (stat)
- Syncing status (stat, green = synced)

**Economics Row:**
- Total supply vs circulating supply (time series, dual axis)
- Staked POL (time series)
- Burned POL (counter, cumulative)
- Treasury balance (time series)
- Inflation rate (gauge)

**Node Health Row:**
- Uptime (stat)
- Resource usage (if node_exporter is also running: CPU, memory, disk)

## Alerting

Recommended Grafana alert rules:

| Alert | Condition | Severity |
|---|---|---|
| Node not producing blocks | `block_height` unchanged for > 30s | Critical |
| High block time | `block_time_ms` > 5000 for > 1 min | Warning |
| Low peer count | `peer_count` < 2 for > 1 min | Warning |
| Node syncing | `is_syncing` == 1 for > 5 min | Warning |
| Mempool backlog | `mempool_size` > 500 for > 2 min | Warning |
| Disk usage high | disk used > 80% | Warning |
| Disk usage critical | disk used > 95% | Critical |

Configure alert notifications via Grafana's notification channels (Slack, PagerDuty, email, etc.).

## Docker Compose Stack

Run the full monitoring stack with Docker Compose:

```yaml
# docker-compose.monitoring.yml
services:
  prometheus:
    image: prom/prometheus:latest
    volumes:
      - ./prometheus.yml:/etc/prometheus/prometheus.yml
    ports:
      - "9090:9090"

  grafana:
    image: grafana/grafana:latest
    volumes:
      - ./grafana-dashboard.json:/var/lib/grafana/dashboards/polay.json
    ports:
      - "3000:3000"
    environment:
      - GF_SECURITY_ADMIN_PASSWORD=admin

  metrics-exporter:
    image: polay/metrics-exporter:latest
    environment:
      - RPC_URL=http://host.docker.internal:9944
      - PORT=9100
    ports:
      - "9100:9100"
```

```bash
docker-compose -f docker-compose.monitoring.yml up -d
```

Access Grafana at `http://localhost:3000` (default: admin/admin).
