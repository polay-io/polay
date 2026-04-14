#!/usr/bin/env bash
set -euo pipefail

# ---------------------------------------------------------------------------
# init-devnet.sh
#
# Initializes a local POLAY devnet environment.
# Uses `polay init` to generate genesis, keys, and data directories in one
# step, then creates per-validator key copies for multi-node setups.
# ---------------------------------------------------------------------------

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
DEVNET_DIR="${PROJECT_ROOT}/devnet-data"
KEYS_DIR="${DEVNET_DIR}/keys"
GENESIS_FILE="${DEVNET_DIR}/genesis.json"
NUM_VALIDATORS=4

POLAY_BIN="${PROJECT_ROOT}/target/release/polay"

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

info() { printf "\033[1;34m[init-devnet]\033[0m %s\n" "$*"; }
ok()   { printf "\033[1;32m[init-devnet]\033[0m %s\n" "$*"; }
err()  { printf "\033[1;31m[init-devnet]\033[0m %s\n" "$*" >&2; }

# ---------------------------------------------------------------------------
# Pre-flight checks
# ---------------------------------------------------------------------------

if [[ ! -f "${POLAY_BIN}" ]]; then
    info "Binary not found at ${POLAY_BIN}. Building in release mode..."
    (cd "${PROJECT_ROOT}" && cargo build --release --bin polay)
fi

if [[ -d "${DEVNET_DIR}" ]]; then
    info "Existing devnet-data directory found."
    read -r -p "  Remove it and start fresh? [y/N] " answer
    case "${answer}" in
        [yY]|[yY][eE][sS])
            rm -rf "${DEVNET_DIR}"
            info "Removed ${DEVNET_DIR}."
            ;;
        *)
            err "Aborting. Remove devnet-data manually or answer yes."
            exit 1
            ;;
    esac
fi

# ---------------------------------------------------------------------------
# Initialize with polay init (generates genesis + keys + data dirs)
# ---------------------------------------------------------------------------

info "Initializing devnet with ${NUM_VALIDATORS} validators..."

"${POLAY_BIN}" init \
    --output "${GENESIS_FILE}" \
    --validators "${NUM_VALIDATORS}" \
    --data-dir "${DEVNET_DIR}" \
    --network devnet

ok "Genesis + keys generated"

# ---------------------------------------------------------------------------
# Create per-validator key copies for multi-node deployment
#
# `polay init` generates a single validator.key. For multi-node setups,
# each validator needs its own key. Generate additional keys with keygen.
# ---------------------------------------------------------------------------

info "Generating individual validator keys for multi-node deployment..."

mkdir -p "${KEYS_DIR}"

# Keep the existing validator.key as validator-1
if [[ -f "${KEYS_DIR}/validator.key" ]]; then
    cp "${KEYS_DIR}/validator.key" "${KEYS_DIR}/validator-1.key"
    ok "  Validator 1: using genesis validator key"
fi

# Generate additional keys for validators 2+
for i in $(seq 2 ${NUM_VALIDATORS}); do
    KEY_FILE="${KEYS_DIR}/validator-${i}.key"
    "${POLAY_BIN}" keygen --output "${KEY_FILE}" 2>/dev/null
    ok "  Validator ${i}: key generated"
done

# Create per-validator data directories
for i in $(seq 1 ${NUM_VALIDATORS}); do
    mkdir -p "${DEVNET_DIR}/validator-${i}"
done

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------

echo ""
echo "============================================================"
echo "  POLAY Local Devnet Initialized"
echo "============================================================"
echo ""
echo "  Chain ID:         polay-devnet-1"
echo "  Validators:       ${NUM_VALIDATORS}"
echo "  Block time:       2000ms"
echo ""
echo "  Directory layout:"
echo "    ${DEVNET_DIR}/"
echo "    +-- genesis.json            Shared genesis configuration"
echo "    +-- keys/"
echo "    |   +-- validator.key       Genesis validator (same as validator-1)"
for i in $(seq 1 ${NUM_VALIDATORS}); do
    echo "    |   +-- validator-${i}.key"
done
echo "    +-- validator-{1..${NUM_VALIDATORS}}/       Per-validator data dirs"
echo ""
echo "  Next steps:"
echo ""
echo "    # Start a single local node:"
echo "    ./scripts/start-local.sh"
echo ""
echo "    # Start full 4-node devnet with Docker:"
echo "    docker compose -f docker-compose.testnet.yml up --build"
echo ""
echo "    # Deploy to Hetzner:"
echo "    cd deploy/hetzner && terraform apply"
echo "    ./deploy-validators.sh"
echo ""
echo "    # Send sample transactions:"
echo "    ./scripts/sample-transactions.sh"
echo ""
echo "============================================================"
