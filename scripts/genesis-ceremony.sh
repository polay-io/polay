#!/usr/bin/env bash
# ============================================================================
# POLAY Genesis Ceremony Script
#
# Generates a genesis.json from validator and account CSV files.
# Usage:
#   ./genesis-ceremony.sh \
#     --validators-file validators.csv \
#     --accounts-file accounts.csv \
#     --chain-id polay-testnet-1 \
#     --network testnet
# ============================================================================

set -euo pipefail

# ---------- Defaults ----------
VALIDATORS_FILE=""
ACCOUNTS_FILE=""
CHAIN_ID=""
NETWORK=""
OUTPUT="genesis.json"
MIN_STAKE=1000000          # minimum stake in base units (1 POL = 1_000_000 units)
GENESIS_TIME=""

# ---------- Colors ----------
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

# ---------- Helpers ----------
log_info()  { echo -e "${CYAN}[INFO]${NC}  $*"; }
log_ok()    { echo -e "${GREEN}[OK]${NC}    $*"; }
log_warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; }
log_error() { echo -e "${RED}[ERROR]${NC} $*" >&2; }
die()       { log_error "$@"; exit 1; }

usage() {
    cat <<EOF
Usage: $(basename "$0") [OPTIONS]

Options:
  --validators-file FILE   CSV file with columns: name,address,pubkey,stake
  --accounts-file FILE     CSV file with columns: address,balance
  --chain-id ID            Chain identifier (e.g. polay-testnet-1)
  --network NETWORK        Network type: testnet or mainnet
  --output FILE            Output path (default: genesis.json)
  --min-stake AMOUNT       Minimum validator stake in base units (default: 1000000)
  -h, --help               Show this help
EOF
    exit 0
}

is_hex64() {
    [[ "$1" =~ ^[0-9a-fA-F]{64}$ ]]
}

# ---------- Parse args ----------
while [[ $# -gt 0 ]]; do
    case "$1" in
        --validators-file) VALIDATORS_FILE="$2"; shift 2 ;;
        --accounts-file)   ACCOUNTS_FILE="$2";   shift 2 ;;
        --chain-id)        CHAIN_ID="$2";        shift 2 ;;
        --network)         NETWORK="$2";         shift 2 ;;
        --output)          OUTPUT="$2";           shift 2 ;;
        --min-stake)       MIN_STAKE="$2";        shift 2 ;;
        -h|--help)         usage ;;
        *) die "Unknown option: $1. Use --help for usage." ;;
    esac
done

# ---------- Validate required args ----------
[[ -z "$VALIDATORS_FILE" ]] && die "Missing --validators-file"
[[ -z "$ACCOUNTS_FILE" ]]   && die "Missing --accounts-file"
[[ -z "$CHAIN_ID" ]]        && die "Missing --chain-id"
[[ -z "$NETWORK" ]]         && die "Missing --network"

[[ "$NETWORK" != "testnet" && "$NETWORK" != "mainnet" ]] && \
    die "Invalid --network '$NETWORK'. Must be 'testnet' or 'mainnet'."

[[ ! -f "$VALIDATORS_FILE" ]] && die "Validators file not found: $VALIDATORS_FILE"
[[ ! -f "$ACCOUNTS_FILE" ]]   && die "Accounts file not found: $ACCOUNTS_FILE"

GENESIS_TIME=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

log_info "POLAY Genesis Ceremony"
log_info "Chain ID : $CHAIN_ID"
log_info "Network  : $NETWORK"
log_info "Time     : $GENESIS_TIME"
echo ""

# ---------- Process validators ----------
log_info "Processing validators from $VALIDATORS_FILE ..."

VALIDATOR_JSON="["
VALIDATOR_COUNT=0
TOTAL_STAKE=0
FIRST_V=true
LINE_NUM=0

while IFS=',' read -r name address pubkey stake; do
    LINE_NUM=$((LINE_NUM + 1))

    # Skip header
    if [[ $LINE_NUM -eq 1 ]]; then
        # Check if first line looks like a header
        if [[ "$name" == "name" || "$name" == "Name" ]]; then
            continue
        fi
    fi

    # Trim whitespace
    name=$(echo "$name" | xargs)
    address=$(echo "$address" | xargs)
    pubkey=$(echo "$pubkey" | xargs)
    stake=$(echo "$stake" | xargs)

    # Skip empty lines
    [[ -z "$name" && -z "$address" ]] && continue

    # Validate address
    if ! is_hex64 "$address"; then
        die "Line $LINE_NUM: Invalid validator address (not 64 hex chars): '$address'"
    fi

    # Validate pubkey
    if ! is_hex64 "$pubkey"; then
        die "Line $LINE_NUM: Invalid validator pubkey (not 64 hex chars): '$pubkey'"
    fi

    # Validate stake
    if ! [[ "$stake" =~ ^[0-9]+$ ]]; then
        die "Line $LINE_NUM: Invalid stake amount (not a number): '$stake'"
    fi

    if [[ "$stake" -lt "$MIN_STAKE" ]]; then
        die "Line $LINE_NUM: Validator '$name' stake ($stake) is below minimum ($MIN_STAKE)"
    fi

    if [[ "$FIRST_V" == "false" ]]; then
        VALIDATOR_JSON+=","
    fi
    FIRST_V=false

    VALIDATOR_JSON+="
    {
      \"name\": \"$name\",
      \"address\": \"$address\",
      \"pubkey\": \"$pubkey\",
      \"stake\": \"$stake\"
    }"

    VALIDATOR_COUNT=$((VALIDATOR_COUNT + 1))
    TOTAL_STAKE=$((TOTAL_STAKE + stake))

    log_ok "  Validator: $name (stake: $stake)"

done < "$VALIDATORS_FILE"

VALIDATOR_JSON+="
  ]"

[[ $VALIDATOR_COUNT -eq 0 ]] && die "No validators found in $VALIDATORS_FILE"

echo ""
log_info "Processing accounts from $ACCOUNTS_FILE ..."

# ---------- Process accounts ----------
ACCOUNT_JSON="["
ACCOUNT_COUNT=0
TOTAL_BALANCE=0
FIRST_A=true
LINE_NUM=0

while IFS=',' read -r address balance; do
    LINE_NUM=$((LINE_NUM + 1))

    # Skip header
    if [[ $LINE_NUM -eq 1 ]]; then
        if [[ "$address" == "address" || "$address" == "Address" ]]; then
            continue
        fi
    fi

    # Trim whitespace
    address=$(echo "$address" | xargs)
    balance=$(echo "$balance" | xargs)

    # Skip empty lines
    [[ -z "$address" && -z "$balance" ]] && continue

    # Validate address
    if ! is_hex64 "$address"; then
        die "Line $LINE_NUM: Invalid account address (not 64 hex chars): '$address'"
    fi

    # Validate balance
    if ! [[ "$balance" =~ ^[0-9]+$ ]]; then
        die "Line $LINE_NUM: Invalid balance (not a number): '$balance'"
    fi

    if [[ "$FIRST_A" == "false" ]]; then
        ACCOUNT_JSON+=","
    fi
    FIRST_A=false

    ACCOUNT_JSON+="
    {
      \"address\": \"$address\",
      \"balance\": \"$balance\"
    }"

    ACCOUNT_COUNT=$((ACCOUNT_COUNT + 1))
    TOTAL_BALANCE=$((TOTAL_BALANCE + balance))

done < "$ACCOUNTS_FILE"

ACCOUNT_JSON+="
  ]"

[[ $ACCOUNT_COUNT -eq 0 ]] && die "No accounts found in $ACCOUNTS_FILE"

# ---------- Compute total supply ----------
TOTAL_SUPPLY=$((TOTAL_STAKE + TOTAL_BALANCE))

# ---------- Write genesis.json ----------
log_info "Writing genesis to $OUTPUT ..."

cat > "$OUTPUT" <<GENESIS_EOF
{
  "chain_id": "$CHAIN_ID",
  "network": "$NETWORK",
  "genesis_time": "$GENESIS_TIME",
  "consensus": {
    "algorithm": "polay-bft",
    "epoch_length": 100,
    "block_time_ms": 2000,
    "min_validators": 3,
    "max_validators": 100
  },
  "economics": {
    "total_supply": "$TOTAL_SUPPLY",
    "staking_reward_rate": "0.05",
    "min_validator_stake": "$MIN_STAKE",
    "tx_fee_burn_rate": "0.50",
    "treasury_rate": "0.10"
  },
  "validators": $VALIDATOR_JSON,
  "accounts": $ACCOUNT_JSON
}
GENESIS_EOF

log_ok "Genesis file written: $OUTPUT"
echo ""

# ---------- Summary ----------
echo -e "${BOLD}============================================${NC}"
echo -e "${BOLD}  POLAY Genesis Ceremony - Summary${NC}"
echo -e "${BOLD}============================================${NC}"
echo ""
echo -e "  Chain ID        : ${CYAN}$CHAIN_ID${NC}"
echo -e "  Network         : ${CYAN}$NETWORK${NC}"
echo -e "  Genesis Time    : ${CYAN}$GENESIS_TIME${NC}"
echo ""
echo -e "  Validators      : ${GREEN}$VALIDATOR_COUNT${NC}"
echo -e "  Total Stake     : ${GREEN}$TOTAL_STAKE${NC}"
echo ""
echo -e "  Accounts        : ${GREEN}$ACCOUNT_COUNT${NC}"
echo -e "  Total Balances  : ${GREEN}$TOTAL_BALANCE${NC}"
echo ""
echo -e "  Total Supply    : ${YELLOW}$TOTAL_SUPPLY${NC}"
echo ""
echo -e "  Output File     : ${CYAN}$OUTPUT${NC}"
echo ""
echo -e "${BOLD}============================================${NC}"
echo ""
log_ok "Genesis ceremony complete."
