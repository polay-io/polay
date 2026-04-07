#!/usr/bin/env bash
set -euo pipefail

# ---------------------------------------------------------------------------
# init-devnet.sh
#
# Initializes a local POLAY devnet environment.
# Creates the data directory, generates validator keys, and produces the
# genesis.json that all four validators share.
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
# Create directory structure
# ---------------------------------------------------------------------------

info "Creating devnet directory structure..."
mkdir -p "${KEYS_DIR}"
for i in $(seq 1 ${NUM_VALIDATORS}); do
    mkdir -p "${DEVNET_DIR}/validator-${i}"
done

# ---------------------------------------------------------------------------
# Generate validator keys
# ---------------------------------------------------------------------------

info "Generating ${NUM_VALIDATORS} validator key pairs..."

VALIDATOR_ADDRESSES=()
for i in $(seq 1 ${NUM_VALIDATORS}); do
    KEY_FILE="${KEYS_DIR}/validator-${i}.key"
    "${POLAY_BIN}" keygen --output "${KEY_FILE}"
    # Extract the public address from the key file (second line is the public key)
    ADDRESS=$(sed -n '2p' "${KEY_FILE}")
    VALIDATOR_ADDRESSES+=("${ADDRESS}")
    ok "  Validator ${i}: ${ADDRESS:0:16}...${ADDRESS: -8}"
done

# ---------------------------------------------------------------------------
# Generate genesis
# ---------------------------------------------------------------------------

info "Generating genesis configuration..."

# Build the validator list argument
VALIDATOR_ARGS=""
for addr in "${VALIDATOR_ADDRESSES[@]}"; do
    VALIDATOR_ARGS="${VALIDATOR_ARGS} --validator ${addr}"
done

"${POLAY_BIN}" init \
    --chain-id "polay-devnet-1" \
    --output "${GENESIS_FILE}" \
    --initial-supply 100000000000000000 \
    --block-time 2000 \
    ${VALIDATOR_ARGS}

ok "Genesis written to ${GENESIS_FILE}"

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
echo "  Initial supply:   100,000,000 POL"
echo ""
echo "  Directory layout:"
echo "    ${DEVNET_DIR}/"
echo "    +-- genesis.json            Shared genesis configuration"
echo "    +-- keys/"
for i in $(seq 1 ${NUM_VALIDATORS}); do
    echo "    |   +-- validator-${i}.key     ${VALIDATOR_ADDRESSES[$((i-1))]:0:16}..."
done
echo "    +-- validator-1/            Data dir for validator 1"
echo "    +-- validator-2/            Data dir for validator 2"
echo "    +-- validator-3/            Data dir for validator 3"
echo "    +-- validator-4/            Data dir for validator 4"
echo ""
echo "  Next steps:"
echo ""
echo "    # Start the full 4-node devnet with Docker:"
echo "    docker compose up --build"
echo ""
echo "    # Or start a single local node:"
echo "    ./scripts/start-local.sh"
echo ""
echo "    # Send sample transactions:"
echo "    ./scripts/sample-transactions.sh"
echo ""
echo "============================================================"
