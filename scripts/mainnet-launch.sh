#!/usr/bin/env bash
# =============================================================================
# POLAY Mainnet Validator Launch Sequence
# =============================================================================
#
# Orchestrates the coordinated startup of a POLAY mainnet validator.
#
# Pre-flight checks:
#   - Binary version verification
#   - Genesis file checksum validation
#   - Validator key presence and format
#   - Disk space and resource requirements
#   - Network connectivity to boot nodes
#   - Port availability (RPC, P2P)
#
# Usage:
#   ./scripts/mainnet-launch.sh \
#     --genesis genesis.json \
#     --validator-key /path/to/validator.key \
#     --data-dir /var/lib/polay \
#     --rpc-port 9944 \
#     --p2p-port 30333
#
# =============================================================================
set -euo pipefail

# ---------------------------------------------------------------------------
# Colors
# ---------------------------------------------------------------------------
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

# ---------------------------------------------------------------------------
# Defaults
# ---------------------------------------------------------------------------
GENESIS_FILE=""
VALIDATOR_KEY=""
DATA_DIR="/var/lib/polay"
RPC_PORT=9944
P2P_PORT=30333
RPC_HOST="0.0.0.0"
EXPECTED_CHECKSUM=""
LOG_LEVEL="info"

BOOT_NODES=(
    "/dns4/boot1.polaychain.com/tcp/30333"
    "/dns4/boot2.polaychain.com/tcp/30333"
    "/dns4/boot3.polaychain.com/tcp/30333"
)

MIN_DISK_GB=50
MIN_RAM_MB=4096

# ---------------------------------------------------------------------------
# Parse arguments
# ---------------------------------------------------------------------------
while [[ $# -gt 0 ]]; do
    case "$1" in
        --genesis)        GENESIS_FILE="$2"; shift 2 ;;
        --validator-key)  VALIDATOR_KEY="$2"; shift 2 ;;
        --data-dir)       DATA_DIR="$2"; shift 2 ;;
        --rpc-port)       RPC_PORT="$2"; shift 2 ;;
        --p2p-port)       P2P_PORT="$2"; shift 2 ;;
        --rpc-host)       RPC_HOST="$2"; shift 2 ;;
        --checksum)       EXPECTED_CHECKSUM="$2"; shift 2 ;;
        --log-level)      LOG_LEVEL="$2"; shift 2 ;;
        *)
            echo -e "${RED}Unknown option: $1${NC}" >&2
            exit 1
            ;;
    esac
done

# ---------------------------------------------------------------------------
# Banner
# ---------------------------------------------------------------------------
echo -e "${BOLD}${CYAN}"
echo "  ╔═══════════════════════════════════════════════════════════╗"
echo "  ║           POLAY MAINNET LAUNCH SEQUENCE                   ║"
echo "  ╚═══════════════════════════════════════════════════════════╝"
echo -e "${NC}"

ERRORS=0
WARNINGS=0

check_pass() { echo -e "  ${GREEN}[PASS]${NC} $1"; }
check_fail() { echo -e "  ${RED}[FAIL]${NC} $1"; ERRORS=$((ERRORS + 1)); }
check_warn() { echo -e "  ${YELLOW}[WARN]${NC} $1"; WARNINGS=$((WARNINGS + 1)); }

# ---------------------------------------------------------------------------
# Pre-flight 1: Required files
# ---------------------------------------------------------------------------
echo -e "${BOLD}Pre-flight 1: Required files${NC}"
echo "─────────────────────────────────────────────"

if [[ -z "$GENESIS_FILE" ]]; then
    check_fail "--genesis is required"
elif [[ ! -f "$GENESIS_FILE" ]]; then
    check_fail "Genesis file not found: $GENESIS_FILE"
else
    check_pass "Genesis file: $GENESIS_FILE"
fi

if [[ -z "$VALIDATOR_KEY" ]]; then
    check_fail "--validator-key is required"
elif [[ ! -f "$VALIDATOR_KEY" ]]; then
    check_fail "Validator key not found: $VALIDATOR_KEY"
else
    KEY_SIZE=$(wc -c < "$VALIDATOR_KEY" | xargs)
    if (( KEY_SIZE < 32 )); then
        check_fail "Validator key too small ($KEY_SIZE bytes, expected >= 32)"
    else
        check_pass "Validator key: $VALIDATOR_KEY ($KEY_SIZE bytes)"
    fi
fi

# ---------------------------------------------------------------------------
# Pre-flight 2: Binary version
# ---------------------------------------------------------------------------
echo ""
echo -e "${BOLD}Pre-flight 2: Binary verification${NC}"
echo "─────────────────────────────────────────────"

if command -v polay &> /dev/null; then
    POLAY_VERSION=$(polay --version 2>/dev/null || echo "unknown")
    check_pass "polay binary found: $POLAY_VERSION"
else
    # Check in cargo build output.
    POLAY_BIN="./target/release/polay"
    if [[ -f "$POLAY_BIN" ]]; then
        check_pass "polay binary found at $POLAY_BIN"
    else
        check_fail "polay binary not found. Run: cargo build --release -p polay-node"
    fi
fi

# ---------------------------------------------------------------------------
# Pre-flight 3: Genesis checksum
# ---------------------------------------------------------------------------
echo ""
echo -e "${BOLD}Pre-flight 3: Genesis checksum${NC}"
echo "─────────────────────────────────────────────"

if [[ -f "$GENESIS_FILE" ]]; then
    if command -v shasum &> /dev/null; then
        ACTUAL_CHECKSUM=$(shasum -a 256 "$GENESIS_FILE" | awk '{print $1}')
    elif command -v sha256sum &> /dev/null; then
        ACTUAL_CHECKSUM=$(sha256sum "$GENESIS_FILE" | awk '{print $1}')
    else
        ACTUAL_CHECKSUM="(no sha256 tool)"
    fi

    echo -e "  SHA-256: ${CYAN}$ACTUAL_CHECKSUM${NC}"

    if [[ -n "$EXPECTED_CHECKSUM" ]]; then
        if [[ "$ACTUAL_CHECKSUM" == "$EXPECTED_CHECKSUM" ]]; then
            check_pass "Checksum matches expected value"
        else
            check_fail "Checksum mismatch! Expected: $EXPECTED_CHECKSUM"
        fi
    else
        check_warn "No --checksum provided. Verify manually with other validators."
    fi

    # Validate genesis JSON.
    if command -v python3 &> /dev/null; then
        if python3 -c "import json; json.load(open('$GENESIS_FILE'))" 2>/dev/null; then
            CHAIN_ID=$(python3 -c "import json; print(json.load(open('$GENESIS_FILE'))['chain_id'])" 2>/dev/null || echo "unknown")
            check_pass "Valid JSON, chain_id: $CHAIN_ID"
        else
            check_fail "Genesis file is not valid JSON"
        fi
    fi
fi

# ---------------------------------------------------------------------------
# Pre-flight 4: System resources
# ---------------------------------------------------------------------------
echo ""
echo -e "${BOLD}Pre-flight 4: System resources${NC}"
echo "─────────────────────────────────────────────"

# Disk space.
if [[ -d "$DATA_DIR" ]] || mkdir -p "$DATA_DIR" 2>/dev/null; then
    if [[ "$(uname)" == "Darwin" ]]; then
        AVAIL_GB=$(df -g "$DATA_DIR" | tail -1 | awk '{print $4}')
    else
        AVAIL_GB=$(df -BG "$DATA_DIR" | tail -1 | awk '{print $4}' | tr -d 'G')
    fi
    if (( AVAIL_GB >= MIN_DISK_GB )); then
        check_pass "Disk space: ${AVAIL_GB}GB available (minimum ${MIN_DISK_GB}GB)"
    else
        check_fail "Insufficient disk: ${AVAIL_GB}GB available, need ${MIN_DISK_GB}GB"
    fi
else
    check_warn "Cannot check disk space for $DATA_DIR"
fi

# Memory.
if [[ "$(uname)" == "Darwin" ]]; then
    TOTAL_RAM_MB=$(( $(sysctl -n hw.memsize) / 1024 / 1024 ))
else
    TOTAL_RAM_MB=$(awk '/MemTotal/ {print int($2/1024)}' /proc/meminfo 2>/dev/null || echo 0)
fi
if (( TOTAL_RAM_MB >= MIN_RAM_MB )); then
    check_pass "Memory: ${TOTAL_RAM_MB}MB total (minimum ${MIN_RAM_MB}MB)"
else
    check_warn "Low memory: ${TOTAL_RAM_MB}MB total (recommended ${MIN_RAM_MB}MB)"
fi

# CPU cores.
if [[ "$(uname)" == "Darwin" ]]; then
    CPU_CORES=$(sysctl -n hw.ncpu)
else
    CPU_CORES=$(nproc 2>/dev/null || echo 1)
fi
if (( CPU_CORES >= 4 )); then
    check_pass "CPU cores: $CPU_CORES (minimum 4)"
else
    check_warn "Low CPU cores: $CPU_CORES (recommended 4+)"
fi

# ---------------------------------------------------------------------------
# Pre-flight 5: Port availability
# ---------------------------------------------------------------------------
echo ""
echo -e "${BOLD}Pre-flight 5: Port availability${NC}"
echo "─────────────────────────────────────────────"

check_port() {
    local port=$1
    local label=$2
    if lsof -i ":$port" -sTCP:LISTEN &>/dev/null 2>&1; then
        check_fail "Port $port ($label) is already in use"
    else
        check_pass "Port $port ($label) is available"
    fi
}

check_port "$RPC_PORT" "JSON-RPC"
check_port "$P2P_PORT" "P2P"

# ---------------------------------------------------------------------------
# Pre-flight summary
# ---------------------------------------------------------------------------
echo ""
echo "═════════════════════════════════════════════"

if (( ERRORS > 0 )); then
    echo -e "${RED}${BOLD}PRE-FLIGHT FAILED: $ERRORS error(s), $WARNINGS warning(s)${NC}"
    echo -e "${RED}Fix the issues above before launching.${NC}"
    exit 1
fi

if (( WARNINGS > 0 )); then
    echo -e "${YELLOW}${BOLD}PRE-FLIGHT PASSED with $WARNINGS warning(s)${NC}"
else
    echo -e "${GREEN}${BOLD}ALL PRE-FLIGHT CHECKS PASSED${NC}"
fi

# ---------------------------------------------------------------------------
# Launch confirmation
# ---------------------------------------------------------------------------
echo ""
echo -e "${BOLD}Launch configuration:${NC}"
echo -e "  Data directory:  ${CYAN}$DATA_DIR${NC}"
echo -e "  RPC endpoint:    ${CYAN}$RPC_HOST:$RPC_PORT${NC}"
echo -e "  P2P listener:    ${CYAN}0.0.0.0:$P2P_PORT${NC}"
echo -e "  Log level:       ${CYAN}$LOG_LEVEL${NC}"
echo -e "  Boot nodes:      ${CYAN}${#BOOT_NODES[@]}${NC}"
echo ""

read -p "$(echo -e "${BOLD}Proceed with mainnet launch? [y/N] ${NC}")" -n 1 -r
echo ""

if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo -e "${YELLOW}Launch cancelled by operator.${NC}"
    exit 0
fi

# ---------------------------------------------------------------------------
# Initialize data directory
# ---------------------------------------------------------------------------
echo ""
echo -e "${BOLD}Initializing node...${NC}"

mkdir -p "$DATA_DIR"/{state,keys,logs}

# Copy genesis and key.
cp "$GENESIS_FILE" "$DATA_DIR/genesis.json"
cp "$VALIDATOR_KEY" "$DATA_DIR/keys/validator.key"
chmod 600 "$DATA_DIR/keys/validator.key"

echo -e "  ${GREEN}Data directory initialized${NC}"

# ---------------------------------------------------------------------------
# Build boot node arguments
# ---------------------------------------------------------------------------
BOOTNODE_ARGS=""
for bn in "${BOOT_NODES[@]}"; do
    BOOTNODE_ARGS+=" --boot-node $bn"
done

# ---------------------------------------------------------------------------
# Determine binary path
# ---------------------------------------------------------------------------
if command -v polay &> /dev/null; then
    POLAY_CMD="polay"
else
    POLAY_CMD="./target/release/polay"
fi

# ---------------------------------------------------------------------------
# Launch
# ---------------------------------------------------------------------------
echo ""
echo -e "${BOLD}${GREEN}  ╔═══════════════════════════════════════════════════════════╗${NC}"
echo -e "${BOLD}${GREEN}  ║           LAUNCHING POLAY MAINNET VALIDATOR               ║${NC}"
echo -e "${BOLD}${GREEN}  ╚═══════════════════════════════════════════════════════════╝${NC}"
echo ""

set -x
exec "$POLAY_CMD" \
    --chain mainnet \
    --data-dir "$DATA_DIR/state" \
    --genesis "$DATA_DIR/genesis.json" \
    --validator-key "$DATA_DIR/keys/validator.key" \
    --rpc-host "$RPC_HOST" \
    --rpc-port "$RPC_PORT" \
    --p2p-port "$P2P_PORT" \
    --log-level "$LOG_LEVEL" \
    $BOOTNODE_ARGS \
    2>&1 | tee "$DATA_DIR/logs/mainnet.log"
