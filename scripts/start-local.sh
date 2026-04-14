#!/usr/bin/env bash
set -euo pipefail

# ---------------------------------------------------------------------------
# start-local.sh
#
# Starts a single POLAY validator node for local development.
# If devnet-data does not exist, runs init-devnet.sh first.
# ---------------------------------------------------------------------------

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
DEVNET_DIR="${PROJECT_ROOT}/devnet-data"
GENESIS_FILE="${DEVNET_DIR}/genesis.json"
VALIDATOR_KEY="${DEVNET_DIR}/keys/validator.key"
DATA_DIR="${DEVNET_DIR}/validator-1"
RPC_ADDR="127.0.0.1:9944"
BLOCK_TIME=2000

POLAY_BIN="${PROJECT_ROOT}/target/release/polay"

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

info() { printf "\033[1;34m[start-local]\033[0m %s\n" "$*"; }
ok()   { printf "\033[1;32m[start-local]\033[0m %s\n" "$*"; }
err()  { printf "\033[1;31m[start-local]\033[0m %s\n" "$*" >&2; }

# ---------------------------------------------------------------------------
# Parse optional arguments
# ---------------------------------------------------------------------------

while [[ $# -gt 0 ]]; do
    case "$1" in
        --rpc-addr)
            RPC_ADDR="$2"; shift 2 ;;
        --block-time)
            BLOCK_TIME="$2"; shift 2 ;;
        --validator)
            VALIDATOR_NUM="$2"
            VALIDATOR_KEY="${DEVNET_DIR}/keys/validator-${VALIDATOR_NUM}.key"
            DATA_DIR="${DEVNET_DIR}/validator-${VALIDATOR_NUM}"
            shift 2 ;;
        --data-dir)
            DATA_DIR="$2"; shift 2 ;;
        --help|-h)
            echo "Usage: start-local.sh [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --rpc-addr ADDR    RPC listen address (default: 127.0.0.1:9944)"
            echo "  --block-time MS    Block production interval in ms (default: 2000)"
            echo "  --validator NUM    Which validator to run, 1-4 (default: 1)"
            echo "  --data-dir PATH    Override data directory"
            echo "  -h, --help         Show this help"
            exit 0 ;;
        *)
            err "Unknown argument: $1"
            exit 1 ;;
    esac
done

# ---------------------------------------------------------------------------
# Build if necessary
# ---------------------------------------------------------------------------

if [[ ! -f "${POLAY_BIN}" ]]; then
    info "Binary not found. Building in release mode..."
    (cd "${PROJECT_ROOT}" && cargo build --release --bin polay)
fi

# ---------------------------------------------------------------------------
# Initialize devnet if necessary
# ---------------------------------------------------------------------------

if [[ ! -f "${GENESIS_FILE}" ]]; then
    info "No genesis file found. Running init-devnet.sh..."
    "${SCRIPT_DIR}/init-devnet.sh"
    echo ""
fi

if [[ ! -f "${VALIDATOR_KEY}" ]]; then
    err "Validator key not found at ${VALIDATOR_KEY}"
    err "Run ./scripts/init-devnet.sh first."
    exit 1
fi

# ---------------------------------------------------------------------------
# Start the node
# ---------------------------------------------------------------------------

info "Starting POLAY local validator node"
info "  Genesis:       ${GENESIS_FILE}"
info "  Data dir:      ${DATA_DIR}"
info "  Validator key: ${VALIDATOR_KEY}"
info "  RPC address:   ${RPC_ADDR}"
info "  Block time:    ${BLOCK_TIME}ms"
echo ""
ok "Node starting. Press Ctrl+C to stop."
echo ""

export RUST_LOG="${RUST_LOG:-info}"

exec "${POLAY_BIN}" run \
    --genesis "${GENESIS_FILE}" \
    --data-dir "${DATA_DIR}" \
    --rpc-addr "${RPC_ADDR}" \
    --validator-key "${VALIDATOR_KEY}" \
    --block-time "${BLOCK_TIME}"
