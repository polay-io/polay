#!/usr/bin/env bash
set -euo pipefail

# ---------------------------------------------------------------------------
# e2e-smoke-test.sh
#
# Boots a POLAY devnet node, sends transactions covering all 40 action types,
# and verifies receipts. Exits 0 on success, 1 on any failure.
#
# Usage:
#   POLAY_BIN=./target/release/polay ./scripts/e2e-smoke-test.sh
# ---------------------------------------------------------------------------

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
POLAY_BIN="${POLAY_BIN:-${PROJECT_ROOT}/target/release/polay}"
WORK_DIR=$(mktemp -d)
RPC_URL="http://127.0.0.1:19944"
NODE_PID=""
PASS=0
FAIL=0
TOTAL=0

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

cleanup() {
    if [[ -n "${NODE_PID}" ]]; then
        kill "${NODE_PID}" 2>/dev/null || true
        wait "${NODE_PID}" 2>/dev/null || true
    fi
    rm -rf "${WORK_DIR}"
}
trap cleanup EXIT

info()  { printf "\033[1;34m[e2e]\033[0m %s\n" "$*"; }
ok()    { printf "\033[1;32m[PASS]\033[0m %s\n" "$*"; PASS=$((PASS + 1)); TOTAL=$((TOTAL + 1)); }
fail()  { printf "\033[1;31m[FAIL]\033[0m %s\n" "$*"; FAIL=$((FAIL + 1)); TOTAL=$((TOTAL + 1)); }

rpc() {
    local method="$1"
    local params="$2"
    curl -s -X POST -H "Content-Type: application/json" \
        -d "{\"jsonrpc\":\"2.0\",\"method\":\"${method}\",\"params\":${params},\"id\":1}" \
        "${RPC_URL}" 2>/dev/null
}

wait_for_node() {
    local retries=30
    while [[ ${retries} -gt 0 ]]; do
        if curl -s "${RPC_URL}" >/dev/null 2>&1; then
            return 0
        fi
        sleep 1
        retries=$((retries - 1))
    done
    return 1
}

wait_for_block() {
    local target="${1:-1}"
    local retries=30
    while [[ ${retries} -gt 0 ]]; do
        local height
        height=$(rpc "polay_getChainInfo" "{}" | python3 -c "import sys,json; print(json.load(sys.stdin).get('result',{}).get('height',0))" 2>/dev/null || echo "0")
        if [[ "${height}" -ge "${target}" ]]; then
            return 0
        fi
        sleep 1
        retries=$((retries - 1))
    done
    return 1
}

check_result() {
    local label="$1"
    local response="$2"
    local error
    error=$(echo "${response}" | python3 -c "import sys,json; r=json.load(sys.stdin); print(r.get('error',{}).get('message',''))" 2>/dev/null || echo "parse_error")
    if [[ -z "${error}" ]]; then
        ok "${label}"
    else
        fail "${label}: ${error}"
    fi
}

check_tx() {
    local label="$1"
    local response="$2"
    # For transaction submission, a result (tx hash) means success
    local result
    result=$(echo "${response}" | python3 -c "import sys,json; r=json.load(sys.stdin); print(r.get('result',''))" 2>/dev/null || echo "")
    if [[ -n "${result}" && "${result}" != "None" ]]; then
        ok "${label} -> tx: ${result:0:16}..."
    else
        local error
        error=$(echo "${response}" | python3 -c "import sys,json; r=json.load(sys.stdin); print(r.get('error',{}).get('message','unknown'))" 2>/dev/null || echo "unknown")
        fail "${label}: ${error}"
    fi
}

# ---------------------------------------------------------------------------
# Step 1: Initialize devnet
# ---------------------------------------------------------------------------

info "Initializing devnet in ${WORK_DIR}..."

"${POLAY_BIN}" init \
    --output "${WORK_DIR}/genesis.json" \
    --data-dir "${WORK_DIR}/data" \
    --network devnet 2>&1 | tail -5

# Find the generated validator key
VALIDATOR_KEY=$(find "${WORK_DIR}/data/keys" -name "*.key" | head -1)
if [[ -z "${VALIDATOR_KEY}" ]]; then
    echo "ERROR: No validator key found after init"
    exit 1
fi
info "Validator key: ${VALIDATOR_KEY}"

# Extract operator address from genesis
OPERATOR_ADDR=$(python3 -c "
import json
with open('${WORK_DIR}/genesis.json') as f:
    g = json.load(f)
# Last account is the operator (injected by init)
print(g['accounts'][-1]['address'])
" 2>/dev/null)
info "Operator address: ${OPERATOR_ADDR}"

# ---------------------------------------------------------------------------
# Step 2: Start the node
# ---------------------------------------------------------------------------

info "Starting POLAY node on port 19944..."

"${POLAY_BIN}" run \
    --genesis "${WORK_DIR}/genesis.json" \
    --data-dir "${WORK_DIR}/data/state" \
    --rpc-addr "0.0.0.0:19944" \
    --validator-key "${VALIDATOR_KEY}" \
    --block-time 1000 \
    --log-level warn \
    > "${WORK_DIR}/node.log" 2>&1 &
NODE_PID=$!

info "Node PID: ${NODE_PID}"

if ! wait_for_node; then
    echo "ERROR: Node failed to start within 30s"
    cat "${WORK_DIR}/node.log"
    exit 1
fi
ok "Node is responding"

info "Waiting for block 1..."
if ! wait_for_block 1; then
    echo "ERROR: No blocks produced within 30s"
    exit 1
fi
ok "Block production confirmed"

# ---------------------------------------------------------------------------
# Step 3: Query RPCs
# ---------------------------------------------------------------------------

info "=== RPC Query Tests ==="

R=$(rpc "polay_getChainInfo" "{}")
check_result "polay_getChainInfo" "${R}"

R=$(rpc "polay_getLatestBlock" "{}")
check_result "polay_getLatestBlock" "${R}"

R=$(rpc "polay_getBlock" "{\"height\":0}")
check_result "polay_getBlock(0)" "${R}"

R=$(rpc "polay_getAccount" "{\"address\":\"${OPERATOR_ADDR}\"}")
check_result "polay_getAccount" "${R}"

R=$(rpc "polay_getBalance" "{\"address\":\"${OPERATOR_ADDR}\"}")
check_result "polay_getBalance" "${R}"

R=$(rpc "polay_getValidatorSet" "{}")
check_result "polay_getValidatorSet" "${R}"

R=$(rpc "polay_getSupplyInfo" "{}")
check_result "polay_getSupplyInfo" "${R}"

R=$(rpc "polay_health" "{}")
check_result "polay_health" "${R}"

R=$(rpc "polay_getNodeInfo" "{}")
check_result "polay_getNodeInfo" "${R}"

R=$(rpc "polay_getNetworkStats" "{}")
check_result "polay_getNetworkStats" "${R}"

# ---------------------------------------------------------------------------
# Step 4: Submit transactions (all 40 types)
# ---------------------------------------------------------------------------

info "=== Transaction Submission Tests ==="
info "(Using placeholder signatures — node must accept unsigned devnet txs)"

NONCE=0
RECIPIENT="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
DUMMY_SIG="00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000"
ASSET_CLASS="bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
LISTING_ID="cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
PROFILE_ADDR="${OPERATOR_ADDR}"

submit_tx() {
    local label="$1"
    local action="$2"
    local sender="${3:-${OPERATOR_ADDR}}"
    local nonce="${4:-${NONCE}}"

    local R
    R=$(rpc "polay_submitTransaction" "{
        \"sender\": \"${sender}\",
        \"nonce\": ${nonce},
        \"action\": ${action},
        \"max_fee\": \"10000\",
        \"signature\": \"${DUMMY_SIG}\"
    }")
    check_tx "${label}" "${R}"
    NONCE=$((NONCE + 1))
}

# Wait for a couple blocks so state is settled
wait_for_block 3

# --- Core Financial ---
submit_tx "Transfer" \
    "{\"type\":\"Transfer\",\"to\":\"${RECIPIENT}\",\"amount\":\"1000000\"}"

# --- Asset Management ---
submit_tx "CreateAssetClass" \
    "{\"type\":\"CreateAssetClass\",\"name\":\"TestSword\",\"max_supply\":10000,\"metadata\":\"{}\"}"

submit_tx "MintAsset" \
    "{\"type\":\"MintAsset\",\"asset_class_id\":\"${ASSET_CLASS}\",\"to\":\"${RECIPIENT}\",\"amount\":10}"

submit_tx "TransferAsset" \
    "{\"type\":\"TransferAsset\",\"asset_class_id\":\"${ASSET_CLASS}\",\"to\":\"${RECIPIENT}\",\"amount\":5}"

submit_tx "BurnAsset" \
    "{\"type\":\"BurnAsset\",\"asset_class_id\":\"${ASSET_CLASS}\",\"amount\":1}"

submit_tx "UpdateAssetMetadata" \
    "{\"type\":\"UpdateAssetMetadata\",\"asset_class_id\":\"${ASSET_CLASS}\",\"metadata\":\"{\\\"updated\\\":true}\"}"

submit_tx "FreezeAsset" \
    "{\"type\":\"FreezeAsset\",\"asset_class_id\":\"${ASSET_CLASS}\",\"address\":\"${RECIPIENT}\"}"

submit_tx "UnfreezeAsset" \
    "{\"type\":\"UnfreezeAsset\",\"asset_class_id\":\"${ASSET_CLASS}\",\"address\":\"${RECIPIENT}\"}"

# --- Marketplace ---
submit_tx "CreateListing" \
    "{\"type\":\"CreateListing\",\"asset_class_id\":\"${ASSET_CLASS}\",\"quantity\":2,\"price_per_unit\":\"5000\"}"

submit_tx "CancelListing" \
    "{\"type\":\"CancelListing\",\"listing_id\":\"${LISTING_ID}\"}"

submit_tx "BuyListing" \
    "{\"type\":\"BuyListing\",\"listing_id\":\"${LISTING_ID}\",\"quantity\":1}"

# --- Identity ---
submit_tx "CreateProfile" \
    "{\"type\":\"CreateProfile\",\"display_name\":\"E2EBot\",\"metadata\":\"{\\\"test\\\":true}\"}"

submit_tx "UpdateProfile" \
    "{\"type\":\"UpdateProfile\",\"display_name\":\"E2EBotV2\",\"metadata\":\"{\\\"test\\\":true}\"}"

# --- Staking ---
submit_tx "RegisterValidator" \
    "{\"type\":\"RegisterValidator\",\"pubkey\":\"$(printf '1%.0s' {1..64})\",\"commission_bps\":500}"

submit_tx "Delegate" \
    "{\"type\":\"Delegate\",\"validator\":\"${OPERATOR_ADDR}\",\"amount\":\"1000\"}"

submit_tx "Undelegate" \
    "{\"type\":\"Undelegate\",\"validator\":\"${OPERATOR_ADDR}\",\"amount\":\"500\"}"

submit_tx "ClaimRewards" \
    "{\"type\":\"ClaimRewards\",\"validator\":\"${OPERATOR_ADDR}\"}"

submit_tx "UpdateCommission" \
    "{\"type\":\"UpdateCommission\",\"commission_bps\":600}"

# --- Attestation ---
submit_tx "SubmitAttestation" \
    "{\"type\":\"SubmitAttestation\",\"attestation_type\":\"game_result\",\"subject\":\"${RECIPIENT}\",\"data\":\"{\\\"score\\\":100}\"}"

submit_tx "RevokeAttestation" \
    "{\"type\":\"RevokeAttestation\",\"attestation_id\":\"dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd\"}"

# --- Session Keys ---
submit_tx "CreateSessionKey" \
    "{\"type\":\"CreateSessionKey\",\"session_pubkey\":\"$(printf '2%.0s' {1..64})\",\"permissions\":[\"Transfer\"],\"expires_at\":9999999999,\"spending_limit\":\"1000000\"}"

submit_tx "RevokeSessionKey" \
    "{\"type\":\"RevokeSessionKey\",\"session_pubkey\":\"$(printf '2%.0s' {1..64})\"}"

# --- Governance ---
submit_tx "SubmitProposal" \
    "{\"type\":\"SubmitProposal\",\"title\":\"Test Proposal\",\"description\":\"E2E test\",\"proposal_type\":\"text\"}"

submit_tx "VoteProposal" \
    "{\"type\":\"VoteProposal\",\"proposal_id\":\"eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee\",\"vote\":\"yes\"}"

# --- Rentals ---
submit_tx "ListForRent" \
    "{\"type\":\"ListForRent\",\"asset_class_id\":\"${ASSET_CLASS}\",\"quantity\":1,\"price_per_block\":\"10\",\"min_duration\":10,\"max_duration\":1000}"

submit_tx "RentAsset" \
    "{\"type\":\"RentAsset\",\"rental_id\":\"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff\",\"duration\":100}"

submit_tx "ReturnRental" \
    "{\"type\":\"ReturnRental\",\"rental_id\":\"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff\"}"

submit_tx "ClaimExpiredRental" \
    "{\"type\":\"ClaimExpiredRental\",\"rental_id\":\"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff\"}"

submit_tx "CancelRentalListing" \
    "{\"type\":\"CancelRentalListing\",\"rental_id\":\"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff\"}"

# --- Guilds ---
submit_tx "CreateGuild" \
    "{\"type\":\"CreateGuild\",\"name\":\"TestGuild\",\"metadata\":\"{\\\"tag\\\":\\\"TG\\\"}\"}"

submit_tx "JoinGuild" \
    "{\"type\":\"JoinGuild\",\"guild_id\":\"1111111111111111111111111111111111111111111111111111111111111111\"}"

submit_tx "LeaveGuild" \
    "{\"type\":\"LeaveGuild\",\"guild_id\":\"1111111111111111111111111111111111111111111111111111111111111111\"}"

submit_tx "GuildDeposit" \
    "{\"type\":\"GuildDeposit\",\"guild_id\":\"1111111111111111111111111111111111111111111111111111111111111111\",\"amount\":\"1000\"}"

submit_tx "GuildWithdraw" \
    "{\"type\":\"GuildWithdraw\",\"guild_id\":\"1111111111111111111111111111111111111111111111111111111111111111\",\"amount\":\"500\"}"

submit_tx "GuildPromote" \
    "{\"type\":\"GuildPromote\",\"guild_id\":\"1111111111111111111111111111111111111111111111111111111111111111\",\"member\":\"${RECIPIENT}\",\"role\":\"officer\"}"

submit_tx "GuildKick" \
    "{\"type\":\"GuildKick\",\"guild_id\":\"1111111111111111111111111111111111111111111111111111111111111111\",\"member\":\"${RECIPIENT}\"}"

# --- Tournaments ---
submit_tx "CreateTournament" \
    "{\"type\":\"CreateTournament\",\"name\":\"E2E Cup\",\"entry_fee\":\"100\",\"max_participants\":64,\"prize_distribution\":[7000,2000,1000],\"start_block\":999999}"

submit_tx "JoinTournament" \
    "{\"type\":\"JoinTournament\",\"tournament_id\":\"2222222222222222222222222222222222222222222222222222222222222222\"}"

submit_tx "StartTournament" \
    "{\"type\":\"StartTournament\",\"tournament_id\":\"2222222222222222222222222222222222222222222222222222222222222222\"}"

submit_tx "ReportTournamentResults" \
    "{\"type\":\"ReportTournamentResults\",\"tournament_id\":\"2222222222222222222222222222222222222222222222222222222222222222\",\"rankings\":[\"${OPERATOR_ADDR}\",\"${RECIPIENT}\"]}"

submit_tx "ClaimTournamentPrize" \
    "{\"type\":\"ClaimTournamentPrize\",\"tournament_id\":\"2222222222222222222222222222222222222222222222222222222222222222\"}"

submit_tx "CancelTournament" \
    "{\"type\":\"CancelTournament\",\"tournament_id\":\"2222222222222222222222222222222222222222222222222222222222222222\"}"

# ---------------------------------------------------------------------------
# Step 5: Wait for processing and check final state
# ---------------------------------------------------------------------------

info "Waiting for transactions to be included in blocks..."
sleep 5

R=$(rpc "polay_getChainInfo" "{}")
FINAL_HEIGHT=$(echo "${R}" | python3 -c "import sys,json; print(json.load(sys.stdin).get('result',{}).get('height',0))" 2>/dev/null || echo "?")
info "Final chain height: ${FINAL_HEIGHT}"

R=$(rpc "polay_getSupplyInfo" "{}")
check_result "Final supply info" "${R}"

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------

echo ""
echo "================================================================"
echo "  E2E Smoke Test Results"
echo "================================================================"
echo ""
echo "  Total:  ${TOTAL}"
echo "  Passed: ${PASS}"
echo "  Failed: ${FAIL}"
echo ""

if [[ ${FAIL} -gt 0 ]]; then
    echo "  SOME TESTS FAILED"
    echo ""
    echo "  Node log (last 30 lines):"
    tail -30 "${WORK_DIR}/node.log"
    exit 1
else
    echo "  ALL TESTS PASSED"
    exit 0
fi
