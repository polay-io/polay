#!/usr/bin/env python3
"""
POLAY Metrics Exporter

Scrapes POLAY node RPC endpoints and exposes Prometheus-compatible metrics
on an HTTP endpoint. Bridges the JSON-RPC health/info/stats endpoints to
the Prometheus text exposition format.

Environment variables:
  VALIDATOR_URLS  — Comma-separated list of RPC URLs (default: http://localhost:9944)
  LISTEN_ADDR     — Address to listen on (default: 0.0.0.0:9100)
  POLL_INTERVAL   — Seconds between scrapes (default: 5)
"""

import json
import os
import time
import threading
from http.server import HTTPServer, BaseHTTPRequestHandler
from urllib.request import urlopen, Request
from urllib.error import URLError

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

VALIDATOR_URLS = os.environ.get(
    "VALIDATOR_URLS", "http://localhost:9944"
).split(",")
LISTEN_ADDR = os.environ.get("LISTEN_ADDR", "0.0.0.0:9100")
POLL_INTERVAL = int(os.environ.get("POLL_INTERVAL", "5"))

# ---------------------------------------------------------------------------
# Metrics state
# ---------------------------------------------------------------------------

metrics_lock = threading.Lock()
metrics_text = ""


def rpc_call(url, method, params=None):
    """Make a JSON-RPC call and return the result."""
    body = json.dumps({
        "jsonrpc": "2.0",
        "method": method,
        "params": params or {},
        "id": 1,
    }).encode()
    req = Request(url, data=body, headers={"Content-Type": "application/json"})
    try:
        with urlopen(req, timeout=5) as resp:
            data = json.loads(resp.read())
            return data.get("result")
    except (URLError, json.JSONDecodeError, Exception):
        return None


def scrape_validator(url, index):
    """Scrape one validator node and return metric lines."""
    lines = []
    label = f'validator="{index}",url="{url}"'

    # polay_health
    health = rpc_call(url, "polay_health")
    if health:
        lines.append(f'polay_node_up{{{label}}} 1')
        lines.append(f'polay_block_height{{{label}}} {health.get("height", 0)}')
        syncing = 1 if health.get("syncing", False) else 0
        lines.append(f'polay_node_syncing{{{label}}} {syncing}')
    else:
        lines.append(f'polay_node_up{{{label}}} 0')
        return lines

    # polay_getNodeInfo
    info = rpc_call(url, "polay_getNodeInfo")
    if info:
        lines.append(f'polay_mempool_size{{{label}}} {info.get("mempool_size", 0)}')
        lines.append(f'polay_uptime_seconds{{{label}}} {info.get("uptime_seconds", 0)}')
        lines.append(f'polay_peer_count{{{label}}} {info.get("peer_count", 0)}')
        lines.append(f'polay_block_time_ms{{{label}}} {info.get("block_time_ms", 0)}')

    # polay_getNetworkStats
    stats = rpc_call(url, "polay_getNetworkStats")
    if stats:
        lines.append(f'polay_total_transactions{{{label}}} {stats.get("total_transactions", 0)}')
        lines.append(f'polay_active_validators{{{label}}} {stats.get("active_validators", 0)}')
        lines.append(f'polay_total_staked{{{label}}} {stats.get("total_staked", 0)}')
        lines.append(f'polay_epoch{{{label}}} {stats.get("epoch", 0)}')

    # polay_getSupplyInfo
    supply = rpc_call(url, "polay_getSupplyInfo")
    if supply:
        lines.append(f'polay_total_supply{{{label}}} {supply.get("total_supply", 0)}')
        lines.append(f'polay_circulating_supply{{{label}}} {supply.get("circulating", 0)}')
        lines.append(f'polay_staked_supply{{{label}}} {supply.get("staked", 0)}')
        lines.append(f'polay_burned_supply{{{label}}} {supply.get("burned", 0)}')
        lines.append(f'polay_treasury_balance{{{label}}} {supply.get("treasury", 0)}')
        lines.append(f'polay_total_minted{{{label}}} {supply.get("minted", 0)}')
        lines.append(f'polay_fees_collected{{{label}}} {supply.get("fees_collected", 0)}')

    # polay_getMempoolSize
    mempool = rpc_call(url, "polay_getMempoolSize")
    if mempool is not None:
        lines.append(f'polay_mempool_pending{{{label}}} {mempool}')

    return lines


def scrape_all():
    """Scrape all validators and build Prometheus text."""
    all_lines = [
        "# HELP polay_node_up Whether the POLAY node is reachable (1=up, 0=down).",
        "# TYPE polay_node_up gauge",
        "# HELP polay_block_height Current block height.",
        "# TYPE polay_block_height gauge",
        "# HELP polay_node_syncing Whether the node is syncing (1=yes, 0=no).",
        "# TYPE polay_node_syncing gauge",
        "# HELP polay_mempool_size Number of transactions in the mempool.",
        "# TYPE polay_mempool_size gauge",
        "# HELP polay_uptime_seconds Node uptime in seconds.",
        "# TYPE polay_uptime_seconds gauge",
        "# HELP polay_peer_count Number of connected P2P peers.",
        "# TYPE polay_peer_count gauge",
        "# HELP polay_block_time_ms Target block production interval in ms.",
        "# TYPE polay_block_time_ms gauge",
        "# HELP polay_total_transactions Total transactions processed.",
        "# TYPE polay_total_transactions counter",
        "# HELP polay_active_validators Number of active validators.",
        "# TYPE polay_active_validators gauge",
        "# HELP polay_total_staked Total POL staked across all validators.",
        "# TYPE polay_total_staked gauge",
        "# HELP polay_epoch Current epoch number.",
        "# TYPE polay_epoch gauge",
        "# HELP polay_total_supply Total POL supply.",
        "# TYPE polay_total_supply gauge",
        "# HELP polay_circulating_supply Circulating POL supply.",
        "# TYPE polay_circulating_supply gauge",
        "# HELP polay_staked_supply Staked POL supply.",
        "# TYPE polay_staked_supply gauge",
        "# HELP polay_burned_supply Total burned POL.",
        "# TYPE polay_burned_supply counter",
        "# HELP polay_treasury_balance Treasury POL balance.",
        "# TYPE polay_treasury_balance gauge",
        "# HELP polay_total_minted Total minted POL (inflation rewards).",
        "# TYPE polay_total_minted counter",
        "# HELP polay_fees_collected Total fees collected.",
        "# TYPE polay_fees_collected counter",
        "# HELP polay_mempool_pending Pending transactions in mempool.",
        "# TYPE polay_mempool_pending gauge",
        "",
    ]

    for i, url in enumerate(VALIDATOR_URLS, start=1):
        url = url.strip()
        if url:
            all_lines.extend(scrape_validator(url, i))

    all_lines.append("")
    return "\n".join(all_lines)


def poll_loop():
    """Background loop that periodically scrapes all validators."""
    global metrics_text
    while True:
        try:
            text = scrape_all()
            with metrics_lock:
                metrics_text = text
        except Exception as e:
            print(f"[metrics-exporter] scrape error: {e}")
        time.sleep(POLL_INTERVAL)


# ---------------------------------------------------------------------------
# HTTP server
# ---------------------------------------------------------------------------

class MetricsHandler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/metrics":
            with metrics_lock:
                body = metrics_text.encode()
            self.send_response(200)
            self.send_header("Content-Type", "text/plain; version=0.0.4; charset=utf-8")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
        elif self.path == "/health":
            self.send_response(200)
            self.end_headers()
            self.wfile.write(b"ok")
        else:
            self.send_response(404)
            self.end_headers()

    def log_message(self, fmt, *args):
        pass  # Suppress access logs


def main():
    host, port = LISTEN_ADDR.rsplit(":", 1)
    port = int(port)

    print(f"[metrics-exporter] Scraping {len(VALIDATOR_URLS)} validators every {POLL_INTERVAL}s")
    print(f"[metrics-exporter] Listening on {host}:{port}/metrics")

    # Start background scraper.
    t = threading.Thread(target=poll_loop, daemon=True)
    t.start()

    # Serve metrics.
    server = HTTPServer((host, port), MetricsHandler)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
