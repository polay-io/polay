#!/usr/bin/env bash
# =============================================================================
# POLAY Load Test Script
#
# Sends a burst of transfer transactions to a running POLAY node via JSON-RPC
# and measures throughput.
#
# Usage:
#   ./scripts/load-test.sh [--rpc URL] [--accounts N] [--txs N]
#
# Defaults:
#   RPC:      http://localhost:9944
#   Accounts: 100
#   Txs:      1000
# =============================================================================

set -euo pipefail

# ---------------------------------------------------------------------------
# Defaults
# ---------------------------------------------------------------------------

RPC_URL="${RPC_URL:-http://localhost:9944}"
NUM_ACCOUNTS=100
NUM_TXS=1000

# ---------------------------------------------------------------------------
# Parse arguments
# ---------------------------------------------------------------------------

while [[ $# -gt 0 ]]; do
    case "$1" in
        --rpc)
            RPC_URL="$2"
            shift 2
            ;;
        --accounts)
            NUM_ACCOUNTS="$2"
            shift 2
            ;;
        --txs)
            NUM_TXS="$2"
            shift 2
            ;;
        -h|--help)
            echo "Usage: $0 [--rpc URL] [--accounts N] [--txs N]"
            echo ""
            echo "Options:"
            echo "  --rpc URL       JSON-RPC endpoint (default: http://localhost:9944)"
            echo "  --accounts N    Number of accounts to generate (default: 100)"
            echo "  --txs N         Number of transactions to send (default: 1000)"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

rpc_call() {
    local method="$1"
    local params="$2"
    curl -s -X POST "$RPC_URL" \
        -H "Content-Type: application/json" \
        -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"$method\",\"params\":$params}"
}

# ---------------------------------------------------------------------------
# Health check
# ---------------------------------------------------------------------------

echo "=== POLAY Load Test ==="
echo "RPC:        $RPC_URL"
echo "Accounts:   $NUM_ACCOUNTS"
echo "Txs:        $NUM_TXS"
echo "---"

echo "Checking node health..."
HEALTH=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$RPC_URL" \
    -H "Content-Type: application/json" \
    -d '{"jsonrpc":"2.0","id":1,"method":"polay_getBlockHeight","params":[]}' 2>/dev/null || echo "000")

if [ "$HEALTH" != "200" ]; then
    echo "ERROR: Node at $RPC_URL is not responding (HTTP $HEALTH)."
    echo "Make sure a POLAY node is running with: polay run"
    exit 1
fi

BLOCK_HEIGHT_RESP=$(rpc_call "polay_getBlockHeight" "[]")
INITIAL_HEIGHT=$(echo "$BLOCK_HEIGHT_RESP" | grep -o '"result":[0-9]*' | cut -d: -f2 || echo "unknown")
echo "Node is live. Current block height: $INITIAL_HEIGHT"

# ---------------------------------------------------------------------------
# Generate accounts (just random hex addresses for load testing)
# ---------------------------------------------------------------------------

echo ""
echo "Generating $NUM_ACCOUNTS test accounts..."

ACCOUNTS=()
for i in $(seq 1 "$NUM_ACCOUNTS"); do
    # Generate a deterministic address from the index.
    ADDR=$(printf '%064x' "$i" 2>/dev/null || printf '%064d' "$i")
    ACCOUNTS+=("$ADDR")
done

echo "Generated ${#ACCOUNTS[@]} accounts."

# ---------------------------------------------------------------------------
# Submit transactions
# ---------------------------------------------------------------------------

echo ""
echo "Submitting $NUM_TXS transactions..."

SUBMITTED=0
ERRORS=0
START_TIME=$(date +%s%N 2>/dev/null || python3 -c "import time; print(int(time.time()*1e9))")

for i in $(seq 1 "$NUM_TXS"); do
    # Round-robin sender/receiver from the account pool.
    SENDER_IDX=$(( (i - 1) % NUM_ACCOUNTS ))
    RECEIVER_IDX=$(( i % NUM_ACCOUNTS ))
    SENDER="${ACCOUNTS[$SENDER_IDX]}"
    RECEIVER="${ACCOUNTS[$RECEIVER_IDX]}"
    AMOUNT=$(( (i % 1000) + 1 ))
    NONCE=$(( (i - 1) / NUM_ACCOUNTS ))

    # Build the JSON-RPC request for submitting a raw transaction.
    # In a real scenario this would be a properly signed transaction;
    # for load testing we use the sendTransaction RPC which may accept
    # pre-signed payloads.
    RESULT=$(rpc_call "polay_sendTransaction" "[{
        \"chain_id\": \"polay-devnet-1\",
        \"nonce\": $NONCE,
        \"signer\": \"$SENDER\",
        \"action\": {\"Transfer\": {\"to\": \"$RECEIVER\", \"amount\": $AMOUNT}},
        \"max_fee\": 500000,
        \"timestamp\": $(date +%s)
    }]" 2>/dev/null || echo '{"error":"curl failed"}')

    if echo "$RESULT" | grep -q '"error"'; then
        ERRORS=$((ERRORS + 1))
    else
        SUBMITTED=$((SUBMITTED + 1))
    fi

    # Progress indicator every 100 txs.
    if [ $((i % 100)) -eq 0 ]; then
        echo "  ... sent $i / $NUM_TXS"
    fi
done

END_TIME=$(date +%s%N 2>/dev/null || python3 -c "import time; print(int(time.time()*1e9))")

# ---------------------------------------------------------------------------
# Calculate results
# ---------------------------------------------------------------------------

DURATION_NS=$((END_TIME - START_TIME))
DURATION_MS=$((DURATION_NS / 1000000))
DURATION_S=$(echo "scale=2; $DURATION_MS / 1000" | bc 2>/dev/null || echo "$((DURATION_MS / 1000))")

if [ "$DURATION_MS" -gt 0 ]; then
    TPS=$(echo "scale=0; $SUBMITTED * 1000 / $DURATION_MS" | bc 2>/dev/null || echo "$((SUBMITTED * 1000 / DURATION_MS))")
else
    TPS="inf"
fi

echo ""
echo "=== Results ==="
echo "Submitted:    $SUBMITTED transactions"
echo "Errors:       $ERRORS"
echo "Duration:     ${DURATION_S}s (${DURATION_MS}ms)"
echo "Submit TPS:   $TPS tx/s"

# ---------------------------------------------------------------------------
# Wait and check block production
# ---------------------------------------------------------------------------

echo ""
echo "Waiting 10s for block production..."
sleep 10

FINAL_HEIGHT_RESP=$(rpc_call "polay_getBlockHeight" "[]")
FINAL_HEIGHT=$(echo "$FINAL_HEIGHT_RESP" | grep -o '"result":[0-9]*' | cut -d: -f2 || echo "unknown")

echo "Initial height: $INITIAL_HEIGHT"
echo "Final height:   $FINAL_HEIGHT"

if [ "$INITIAL_HEIGHT" != "unknown" ] && [ "$FINAL_HEIGHT" != "unknown" ]; then
    BLOCKS_PRODUCED=$((FINAL_HEIGHT - INITIAL_HEIGHT))
    echo "Blocks produced: $BLOCKS_PRODUCED"
    if [ "$BLOCKS_PRODUCED" -gt 0 ]; then
        AVG_BLOCK_TIME=$(echo "scale=1; 10000 / $BLOCKS_PRODUCED" | bc 2>/dev/null || echo "N/A")
        echo "Avg block time:  ${AVG_BLOCK_TIME}ms"
    fi
fi

echo ""
echo "=== Load test complete ==="
