#!/usr/bin/env bash
# =============================================================================
# POLAY Mainnet Genesis Ceremony
# =============================================================================
#
# Multi-party genesis coordination for POLAY mainnet launch.
#
# Inputs:
#   --validators-csv <file>   CSV: name,address,pubkey,stake,commission_bps
#   --allocations-csv <file>  CSV: label,address,amount,vesting_months
#   --chain-id <string>       Chain identifier (default: polay-mainnet-1)
#   --output <file>            Output genesis file (default: genesis.json)
#
# The ceremony:
#   1. Validates all addresses and public keys (64-char hex)
#   2. Verifies stake >= min_stake (100M POL)
#   3. Verifies commission <= 20% (2000 bps)
#   4. Ensures total allocations == declared total supply
#   5. Generates deterministic genesis JSON
#   6. Computes SHA-256 checksum for multi-party verification
#   7. Each participant verifies the checksum independently
#
# Usage:
#   ./scripts/mainnet-genesis.sh \
#     --validators-csv validators.csv \
#     --allocations-csv allocations.csv \
#     --chain-id polay-mainnet-1 \
#     --output genesis.json
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
CHAIN_ID="polay-mainnet-1"
TOTAL_SUPPLY=1000000000
MIN_STAKE=100000000
MAX_COMMISSION_BPS=2000
OUTPUT="genesis.json"
VALIDATORS_CSV=""
ALLOCATIONS_CSV=""

# ---------------------------------------------------------------------------
# Parse arguments
# ---------------------------------------------------------------------------
while [[ $# -gt 0 ]]; do
    case "$1" in
        --validators-csv)  VALIDATORS_CSV="$2"; shift 2 ;;
        --allocations-csv) ALLOCATIONS_CSV="$2"; shift 2 ;;
        --chain-id)        CHAIN_ID="$2"; shift 2 ;;
        --output)          OUTPUT="$2"; shift 2 ;;
        --total-supply)    TOTAL_SUPPLY="$2"; shift 2 ;;
        *)
            echo -e "${RED}Unknown option: $1${NC}" >&2
            exit 1
            ;;
    esac
done

# ---------------------------------------------------------------------------
# Validate inputs
# ---------------------------------------------------------------------------
echo -e "${BOLD}${CYAN}"
echo "  ╔═══════════════════════════════════════════════════════════╗"
echo "  ║           POLAY MAINNET GENESIS CEREMONY                  ║"
echo "  ╚═══════════════════════════════════════════════════════════╝"
echo -e "${NC}"

if [[ -z "$VALIDATORS_CSV" ]]; then
    echo -e "${RED}Error: --validators-csv is required${NC}" >&2
    exit 1
fi
if [[ -z "$ALLOCATIONS_CSV" ]]; then
    echo -e "${RED}Error: --allocations-csv is required${NC}" >&2
    exit 1
fi
if [[ ! -f "$VALIDATORS_CSV" ]]; then
    echo -e "${RED}Error: Validators file not found: $VALIDATORS_CSV${NC}" >&2
    exit 1
fi
if [[ ! -f "$ALLOCATIONS_CSV" ]]; then
    echo -e "${RED}Error: Allocations file not found: $ALLOCATIONS_CSV${NC}" >&2
    exit 1
fi

ERRORS=0
GENESIS_TIME=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

echo -e "${BLUE}Chain ID:       ${BOLD}$CHAIN_ID${NC}"
echo -e "${BLUE}Total supply:   ${BOLD}$TOTAL_SUPPLY POL${NC}"
echo -e "${BLUE}Genesis time:   ${BOLD}$GENESIS_TIME${NC}"
echo -e "${BLUE}Output:         ${BOLD}$OUTPUT${NC}"
echo ""

# ---------------------------------------------------------------------------
# Validate hex address (64 characters)
# ---------------------------------------------------------------------------
validate_hex64() {
    local value="$1"
    local label="$2"
    if [[ ! "$value" =~ ^[0-9a-fA-F]{64}$ ]]; then
        echo -e "${RED}  ERROR: $label is not a valid 64-char hex: $value${NC}" >&2
        return 1
    fi
    return 0
}

# ---------------------------------------------------------------------------
# Step 1: Parse and validate validators
# ---------------------------------------------------------------------------
echo -e "${BOLD}Step 1: Validating validators${NC}"
echo "─────────────────────────────────────────────"

VALIDATORS_JSON="["
VALIDATOR_COUNT=0
TOTAL_STAKED=0
FIRST_V=true

while IFS=, read -r name address pubkey stake commission_bps; do
    # Skip header row.
    if [[ "$name" == "name" ]] || [[ -z "$name" ]]; then
        continue
    fi

    # Trim whitespace.
    name=$(echo "$name" | xargs)
    address=$(echo "$address" | xargs)
    pubkey=$(echo "$pubkey" | xargs)
    stake=$(echo "$stake" | xargs)
    commission_bps=$(echo "$commission_bps" | xargs)

    echo -e "  ${CYAN}Validator: ${BOLD}$name${NC}"

    # Validate address.
    if ! validate_hex64 "$address" "address"; then
        ERRORS=$((ERRORS + 1))
    fi

    # Validate pubkey.
    if ! validate_hex64 "$pubkey" "pubkey"; then
        ERRORS=$((ERRORS + 1))
    fi

    # Validate stake.
    if ! [[ "$stake" =~ ^[0-9]+$ ]]; then
        echo -e "${RED}  ERROR: stake is not numeric: $stake${NC}" >&2
        ERRORS=$((ERRORS + 1))
    elif (( stake < MIN_STAKE )); then
        echo -e "${RED}  ERROR: stake $stake < minimum $MIN_STAKE${NC}" >&2
        ERRORS=$((ERRORS + 1))
    else
        echo -e "    ${GREEN}Stake: $stake POL ✓${NC}"
    fi

    # Validate commission.
    if ! [[ "$commission_bps" =~ ^[0-9]+$ ]]; then
        echo -e "${RED}  ERROR: commission_bps is not numeric: $commission_bps${NC}" >&2
        ERRORS=$((ERRORS + 1))
    elif (( commission_bps > MAX_COMMISSION_BPS )); then
        echo -e "${RED}  ERROR: commission $commission_bps bps > max $MAX_COMMISSION_BPS bps${NC}" >&2
        ERRORS=$((ERRORS + 1))
    else
        echo -e "    ${GREEN}Commission: $commission_bps bps ✓${NC}"
    fi

    TOTAL_STAKED=$((TOTAL_STAKED + stake))
    VALIDATOR_COUNT=$((VALIDATOR_COUNT + 1))

    # Append to JSON array.
    if [[ "$FIRST_V" == "true" ]]; then
        FIRST_V=false
    else
        VALIDATORS_JSON+=","
    fi
    VALIDATORS_JSON+=$(cat <<VJSON

    {
      "name": "$name",
      "address": "$address",
      "pubkey": "$pubkey",
      "stake": $stake,
      "commission_bps": $commission_bps,
      "status": "active"
    }
VJSON
)
done < "$VALIDATORS_CSV"

VALIDATORS_JSON+=$'\n  ]'

echo ""
echo -e "  ${GREEN}Validators: $VALIDATOR_COUNT${NC}"
echo -e "  ${GREEN}Total staked: $TOTAL_STAKED POL${NC}"

if (( VALIDATOR_COUNT < 4 )); then
    echo -e "${RED}  ERROR: Minimum 4 validators required for mainnet (got $VALIDATOR_COUNT)${NC}" >&2
    ERRORS=$((ERRORS + 1))
fi

# ---------------------------------------------------------------------------
# Step 2: Parse and validate allocations
# ---------------------------------------------------------------------------
echo ""
echo -e "${BOLD}Step 2: Validating token allocations${NC}"
echo "─────────────────────────────────────────────"

ACCOUNTS_JSON="["
ALLOCATION_TOTAL=0
FIRST_A=true

while IFS=, read -r label address amount vesting_months; do
    # Skip header.
    if [[ "$label" == "label" ]] || [[ -z "$label" ]]; then
        continue
    fi

    label=$(echo "$label" | xargs)
    address=$(echo "$address" | xargs)
    amount=$(echo "$amount" | xargs)
    vesting_months=$(echo "$vesting_months" | xargs)

    echo -e "  ${CYAN}$label${NC}"

    if ! validate_hex64 "$address" "address"; then
        ERRORS=$((ERRORS + 1))
    fi

    if ! [[ "$amount" =~ ^[0-9]+$ ]]; then
        echo -e "${RED}  ERROR: amount is not numeric: $amount${NC}" >&2
        ERRORS=$((ERRORS + 1))
    else
        echo -e "    ${GREEN}Amount: $amount POL${NC}"
    fi

    if ! [[ "$vesting_months" =~ ^[0-9]+$ ]]; then
        echo -e "${RED}  ERROR: vesting_months is not numeric: $vesting_months${NC}" >&2
        ERRORS=$((ERRORS + 1))
    elif (( vesting_months > 0 )); then
        echo -e "    ${YELLOW}Vesting: $vesting_months months${NC}"
    fi

    ALLOCATION_TOTAL=$((ALLOCATION_TOTAL + amount))

    if [[ "$FIRST_A" == "true" ]]; then
        FIRST_A=false
    else
        ACCOUNTS_JSON+=","
    fi
    ACCOUNTS_JSON+=$(cat <<AJSON

    {
      "label": "$label",
      "address": "$address",
      "balance": $amount,
      "vesting_months": $vesting_months
    }
AJSON
)
done < "$ALLOCATIONS_CSV"

ACCOUNTS_JSON+=$'\n  ]'

echo ""
echo -e "  ${GREEN}Total allocated: $ALLOCATION_TOTAL POL${NC}"

# ---------------------------------------------------------------------------
# Step 3: Verify supply invariant
# ---------------------------------------------------------------------------
echo ""
echo -e "${BOLD}Step 3: Supply verification${NC}"
echo "─────────────────────────────────────────────"

COMPUTED_SUPPLY=$((TOTAL_STAKED + ALLOCATION_TOTAL))
echo -e "  Validator stakes:  $TOTAL_STAKED"
echo -e "  Account balances:  $ALLOCATION_TOTAL"
echo -e "  Computed total:    $COMPUTED_SUPPLY"
echo -e "  Declared supply:   $TOTAL_SUPPLY"

if (( COMPUTED_SUPPLY != TOTAL_SUPPLY )); then
    echo -e "${RED}  ERROR: Computed supply ($COMPUTED_SUPPLY) != declared supply ($TOTAL_SUPPLY)${NC}" >&2
    ERRORS=$((ERRORS + 1))
else
    echo -e "  ${GREEN}Supply invariant holds ✓${NC}"
fi

# ---------------------------------------------------------------------------
# Abort on errors
# ---------------------------------------------------------------------------
if (( ERRORS > 0 )); then
    echo ""
    echo -e "${RED}${BOLD}CEREMONY ABORTED: $ERRORS error(s) found.${NC}"
    echo -e "${RED}Fix the issues above and re-run.${NC}"
    exit 1
fi

# ---------------------------------------------------------------------------
# Step 4: Generate genesis.json
# ---------------------------------------------------------------------------
echo ""
echo -e "${BOLD}Step 4: Generating genesis file${NC}"
echo "─────────────────────────────────────────────"

cat > "$OUTPUT" <<GENESIS
{
  "chain_id": "$CHAIN_ID",
  "genesis_time": "$GENESIS_TIME",
  "initial_supply": $TOTAL_SUPPLY,
  "consensus": {
    "algorithm": "polay-bft",
    "epoch_length": 43200,
    "block_time_ms": 2000,
    "min_validators": 4,
    "max_validators": 100,
    "quorum_threshold_bps": 6667
  },
  "staking": {
    "min_stake": $MIN_STAKE,
    "max_commission_bps": $MAX_COMMISSION_BPS,
    "unbonding_period_blocks": 604800,
    "slash_fraction_downtime_bps": 100,
    "slash_fraction_double_sign_bps": 1000
  },
  "economics": {
    "fee_distribution": {
      "burn_bps": 5000,
      "treasury_bps": 2000,
      "validator_bps": 3000
    },
    "inflation": {
      "initial_rate_bps": 800,
      "min_rate_bps": 200,
      "decay_rate_bps": 500
    }
  },
  "governance": {
    "min_proposal_deposit": 1000000,
    "voting_period_blocks": 302400,
    "quorum_bps": 3333,
    "pass_threshold_bps": 5000
  },
  "validators": $VALIDATORS_JSON,
  "accounts": $ACCOUNTS_JSON
}
GENESIS

echo -e "  ${GREEN}Written: $OUTPUT${NC}"

# ---------------------------------------------------------------------------
# Step 5: Compute checksum
# ---------------------------------------------------------------------------
echo ""
echo -e "${BOLD}Step 5: Multi-party verification checksum${NC}"
echo "─────────────────────────────────────────────"

if command -v shasum &> /dev/null; then
    CHECKSUM=$(shasum -a 256 "$OUTPUT" | awk '{print $1}')
elif command -v sha256sum &> /dev/null; then
    CHECKSUM=$(sha256sum "$OUTPUT" | awk '{print $1}')
else
    echo -e "${YELLOW}Warning: No SHA-256 tool found. Skipping checksum.${NC}"
    CHECKSUM="(unavailable)"
fi

echo -e "  ${BOLD}SHA-256: ${CYAN}$CHECKSUM${NC}"
echo ""
echo -e "${BOLD}${GREEN}  ╔═══════════════════════════════════════════════════════════╗${NC}"
echo -e "${BOLD}${GREEN}  ║           GENESIS CEREMONY COMPLETE                       ║${NC}"
echo -e "${BOLD}${GREEN}  ╚═══════════════════════════════════════════════════════════╝${NC}"
echo ""
echo -e "  Chain:        ${BOLD}$CHAIN_ID${NC}"
echo -e "  Validators:   ${BOLD}$VALIDATOR_COUNT${NC}"
echo -e "  Total supply: ${BOLD}$TOTAL_SUPPLY POL${NC}"
echo -e "  Total staked: ${BOLD}$TOTAL_STAKED POL${NC}"
echo -e "  Genesis file: ${BOLD}$OUTPUT${NC}"
echo -e "  Checksum:     ${BOLD}$CHECKSUM${NC}"
echo ""
echo -e "${YELLOW}  NEXT STEPS:${NC}"
echo -e "  1. Each validator participant independently computes:"
echo -e "     ${CYAN}shasum -a 256 $OUTPUT${NC}"
echo -e "  2. All participants must confirm the same checksum:"
echo -e "     ${CYAN}$CHECKSUM${NC}"
echo -e "  3. Once confirmed, distribute genesis.json to all validators."
echo -e "  4. Run the launch sequence:"
echo -e "     ${CYAN}./scripts/mainnet-launch.sh --genesis $OUTPUT${NC}"
echo ""
