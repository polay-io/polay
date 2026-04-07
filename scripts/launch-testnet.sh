#!/usr/bin/env bash
set -euo pipefail

# ---------------------------------------------------------------------------
# launch-testnet.sh
#
# Generates validator keys, creates a shared genesis, and boots a 4-node
# P2P testnet using docker-compose.testnet.yml.
#
# Usage:
#   ./scripts/launch-testnet.sh [--reset] [--no-docker]
#
# Flags:
#   --reset      Remove existing testnet data and start fresh.
#   --no-docker  Only generate keys/genesis; don't start Docker.
# ---------------------------------------------------------------------------

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
TESTNET_DIR="${PROJECT_ROOT}/testnet-data"
KEYS_DIR="${TESTNET_DIR}/keys"
GENESIS_FILE="${TESTNET_DIR}/genesis.json"
POLAY_BIN="${POLAY_BIN:-${PROJECT_ROOT}/target/release/polay}"
NUM_VALIDATORS=4
RESET=false
NO_DOCKER=false

# ---------------------------------------------------------------------------
# Parse flags
# ---------------------------------------------------------------------------

for arg in "$@"; do
    case "${arg}" in
        --reset)    RESET=true ;;
        --no-docker) NO_DOCKER=true ;;
        *)          echo "Unknown flag: ${arg}"; exit 1 ;;
    esac
done

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

info()  { printf "\033[1;34m[testnet]\033[0m %s\n" "$*"; }
ok()    { printf "\033[1;32m[testnet]\033[0m %s\n" "$*"; }
err()   { printf "\033[1;31m[testnet]\033[0m %s\n" "$*" >&2; }
banner() {
    echo ""
    echo "  ██████╗  ██████╗ ██╗      █████╗ ██╗   ██╗"
    echo "  ██╔══██╗██╔═══██╗██║     ██╔══██╗╚██╗ ██╔╝"
    echo "  ██████╔╝██║   ██║██║     ███████║ ╚████╔╝ "
    echo "  ██╔═══╝ ██║   ██║██║     ██╔══██║  ╚██╔╝  "
    echo "  ██║     ╚██████╔╝███████╗██║  ██║   ██║   "
    echo "  ╚═╝      ╚═════╝ ╚══════╝╚═╝  ╚═╝   ╚═╝   "
    echo ""
    echo "  Testnet Launcher — ${NUM_VALIDATORS}-Node P2P BFT Network"
    echo ""
}

# ---------------------------------------------------------------------------
# Pre-flight
# ---------------------------------------------------------------------------

banner

# Build binary if needed.
if [[ ! -f "${POLAY_BIN}" ]]; then
    info "Binary not found. Building release..."
    (cd "${PROJECT_ROOT}" && export PATH="$HOME/.cargo/bin:$PATH" && cargo build --release --bin polay)
fi

# Handle existing data.
if [[ -d "${TESTNET_DIR}" ]]; then
    if [[ "${RESET}" == "true" ]]; then
        info "Removing existing testnet data..."
        rm -rf "${TESTNET_DIR}"
    else
        ok "Testnet data already exists at ${TESTNET_DIR}."
        ok "Use --reset to regenerate, or starting with existing data."
        if [[ "${NO_DOCKER}" == "false" ]]; then
            info "Starting Docker services..."
            cd "${PROJECT_ROOT}"
            docker compose -f docker-compose.testnet.yml up --build -d
            ok "Testnet is running! See endpoints below."
            echo ""
            echo "  Validator 1 RPC:  http://localhost:9944"
            echo "  Validator 2 RPC:  http://localhost:9945"
            echo "  Validator 3 RPC:  http://localhost:9946"
            echo "  Validator 4 RPC:  http://localhost:9947"
            echo "  Faucet:           http://localhost:8080"
            echo "  Explorer API:     http://localhost:3001"
            echo "  Prometheus:       http://localhost:9090"
            echo "  Grafana:          http://localhost:3000  (admin/polay)"
            echo ""
        fi
        exit 0
    fi
fi

# ---------------------------------------------------------------------------
# Step 1: Generate validator keys
# ---------------------------------------------------------------------------

info "Step 1/4: Generating ${NUM_VALIDATORS} validator keypairs..."

mkdir -p "${KEYS_DIR}"
ADDRESSES=()
PUBKEYS=()

for i in $(seq 1 ${NUM_VALIDATORS}); do
    KEY_FILE="${KEYS_DIR}/validator-${i}.key"
    "${POLAY_BIN}" keygen --output "${KEY_FILE}" 2>/dev/null

    # Read the key and derive address/pubkey.
    KEY_HEX=$(cat "${KEY_FILE}" | tr -d '[:space:]')

    # Use the polay binary to show info — extract from stdout.
    KEY_INFO=$("${POLAY_BIN}" keygen --output "/tmp/polay-tmpkey-${i}" 2>&1 || true)
    ADDRESS=$(echo "${KEY_INFO}" | grep -oP 'address=\K[a-f0-9]+' || echo "")

    # If we can't extract from binary output, compute from the key.
    if [[ -z "${ADDRESS}" ]]; then
        # The address is SHA-256(public_key). Since we can't easily compute
        # Ed25519 pubkey from CLI, we'll use a Python helper.
        ADDRESS=$(python3 -c "
import hashlib
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
key = Ed25519PrivateKey.from_private_bytes(bytes.fromhex('${KEY_HEX}'))
pub = key.public_key().public_bytes_raw()
addr = hashlib.sha256(pub).hexdigest()
print(addr)
" 2>/dev/null || echo "unknown-${i}")
        PUBKEY=$(python3 -c "
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
key = Ed25519PrivateKey.from_private_bytes(bytes.fromhex('${KEY_HEX}'))
pub = key.public_key().public_bytes_raw().hex()
print(pub)
" 2>/dev/null || echo "unknown-${i}")
    fi

    ADDRESSES+=("${ADDRESS}")
    PUBKEYS+=("${PUBKEY:-unknown}")
    ok "  Validator ${i}: ${ADDRESS:0:16}..."
    rm -f "/tmp/polay-tmpkey-${i}"
done

# ---------------------------------------------------------------------------
# Step 2: Generate genesis with all validators
# ---------------------------------------------------------------------------

info "Step 2/4: Generating testnet genesis..."

# Use the first validator key to run init (it injects itself into genesis).
"${POLAY_BIN}" init \
    --output "${GENESIS_FILE}" \
    --data-dir "${TESTNET_DIR}/validator-1" \
    --validators "${NUM_VALIDATORS}" \
    --network testnet 2>&1 | grep -v "^$" | head -10

# The init command generates its own key. We need to overwrite with our
# pre-generated key so validator-1 matches.
cp "${KEYS_DIR}/validator-1.key" "${TESTNET_DIR}/validator-1/keys/validator.key" 2>/dev/null || true

ok "Genesis written to ${GENESIS_FILE}"

# ---------------------------------------------------------------------------
# Step 3: Create validator data directories
# ---------------------------------------------------------------------------

info "Step 3/4: Creating validator data directories..."

for i in $(seq 1 ${NUM_VALIDATORS}); do
    mkdir -p "${TESTNET_DIR}/validator-${i}/state"
    ok "  Created validator-${i}/state"
done

# ---------------------------------------------------------------------------
# Step 4: Launch
# ---------------------------------------------------------------------------

if [[ "${NO_DOCKER}" == "true" ]]; then
    ok "Testnet data generated. Skipping Docker launch (--no-docker)."
    echo ""
    echo "  To start manually:"
    echo "    docker compose -f docker-compose.testnet.yml up --build"
    echo ""
    exit 0
fi

info "Step 4/4: Starting Docker services..."

cd "${PROJECT_ROOT}"
docker compose -f docker-compose.testnet.yml up --build -d

# Wait for validator-1 health.
info "Waiting for validator-1 to become healthy..."
RETRIES=30
while [[ ${RETRIES} -gt 0 ]]; do
    if curl -sf http://localhost:9944 >/dev/null 2>&1; then
        break
    fi
    sleep 2
    RETRIES=$((RETRIES - 1))
done

if [[ ${RETRIES} -eq 0 ]]; then
    err "Validator-1 failed to start within 60s."
    docker compose -f docker-compose.testnet.yml logs validator-1 | tail -20
    exit 1
fi

# Quick health check.
CHAIN_INFO=$(curl -s -X POST -H "Content-Type: application/json" \
    -d '{"jsonrpc":"2.0","method":"polay_getChainInfo","params":{},"id":1}' \
    http://localhost:9944 | python3 -m json.tool 2>/dev/null || echo "{}")

echo ""
echo "================================================================"
echo "  POLAY Testnet Launched Successfully!"
echo "================================================================"
echo ""
echo "  Chain Info:"
echo "${CHAIN_INFO}" | sed 's/^/    /'
echo ""
echo "  Endpoints:"
echo "    Validator 1 RPC:  http://localhost:9944"
echo "    Validator 2 RPC:  http://localhost:9945"
echo "    Validator 3 RPC:  http://localhost:9946"
echo "    Validator 4 RPC:  http://localhost:9947"
echo "    Faucet:           http://localhost:8080"
echo "    Explorer API:     http://localhost:3001"
echo "    Prometheus:       http://localhost:9090"
echo "    Grafana:          http://localhost:3000  (admin/polay)"
echo ""
echo "  Useful commands:"
echo "    docker compose -f docker-compose.testnet.yml logs -f       # Follow logs"
echo "    docker compose -f docker-compose.testnet.yml down           # Stop testnet"
echo "    docker compose -f docker-compose.testnet.yml down -v        # Stop + wipe volumes"
echo "    ./scripts/launch-testnet.sh --reset                         # Full reset"
echo ""
echo "================================================================"
