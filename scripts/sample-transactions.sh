#!/usr/bin/env bash
set -euo pipefail

# ---------------------------------------------------------------------------
# sample-transactions.sh
#
# Sends a series of sample JSON-RPC requests to a running POLAY node.
# Demonstrates the full range of RPC methods: chain queries, transfers,
# asset management, marketplace operations, and player profiles.
#
# Usage:
#   ./scripts/sample-transactions.sh [RPC_URL]
#
# Default RPC_URL: http://127.0.0.1:9944
# ---------------------------------------------------------------------------

RPC_URL="${1:-http://127.0.0.1:9944}"
REQUEST_ID=0

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

next_id() { REQUEST_ID=$((REQUEST_ID + 1)); echo "${REQUEST_ID}"; }

section() {
    echo ""
    echo "================================================================"
    echo "  $1"
    echo "================================================================"
    echo ""
}

info() { printf "\033[1;34m>>>\033[0m %s\n" "$*"; }
desc() { printf "\033[0;37m    %s\033[0m\n" "$*"; }

rpc_call() {
    local method="$1"
    local params="$2"
    local id
    id=$(next_id)

    local payload
    payload=$(cat <<ENDJSON
{
  "jsonrpc": "2.0",
  "method": "${method}",
  "params": ${params},
  "id": ${id}
}
ENDJSON
)

    info "POST ${method} (id=${id})"
    echo "    Request:"
    echo "${payload}" | sed 's/^/      /'
    echo ""

    local response
    response=$(curl -s -X POST \
        -H "Content-Type: application/json" \
        -d "${payload}" \
        "${RPC_URL}" 2>&1) || true

    echo "    Response:"
    if command -v jq &>/dev/null; then
        echo "${response}" | jq '.' 2>/dev/null | sed 's/^/      /' || echo "      ${response}"
    else
        echo "      ${response}"
    fi
    echo ""
}

# ---------------------------------------------------------------------------
# Pre-flight: check that the node is reachable
# ---------------------------------------------------------------------------

section "Pre-flight Check"
info "Testing connection to ${RPC_URL}..."

if ! curl -s -o /dev/null -w "" "${RPC_URL}" 2>/dev/null; then
    echo ""
    echo "  Could not connect to ${RPC_URL}."
    echo "  Make sure a POLAY node is running:"
    echo ""
    echo "    ./scripts/start-local.sh"
    echo "    # or"
    echo "    docker compose up"
    echo ""
    exit 1
fi

echo "  Connected."

# ===========================================================================
# 1. Chain Info
# ===========================================================================

section "1. Query Chain Info"
desc "Retrieve the current chain status: height, epoch, supply, validators."

rpc_call "polay_getChainInfo" "{}"

# ===========================================================================
# 2. Get Latest Block
# ===========================================================================

section "2. Get Latest Block"
desc "Fetch the most recently committed block."

rpc_call "polay_getLatestBlock" "{}"

# ===========================================================================
# 3. Get Block by Height
# ===========================================================================

section "3. Get Block by Height"
desc "Fetch the genesis block (height 0)."

rpc_call "polay_getBlock" '{"height": 0}'

# ===========================================================================
# 4. Query Genesis Account Balance
# ===========================================================================

section "4. Query Genesis Account Balance"
desc "Check the POL balance of the genesis account."
desc "The genesis account holds the initial supply minus validator stakes."

# This is a well-known devnet genesis address (first validator).
# In a real devnet, replace this with the address from init-devnet.sh output.
GENESIS_ADDR="a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2"

rpc_call "polay_getBalance" "{\"address\": \"${GENESIS_ADDR}\"}"

# ===========================================================================
# 5. Get Full Account Info
# ===========================================================================

section "5. Get Full Account Info"
desc "Retrieve the full account record (balance + nonce) for the genesis address."

rpc_call "polay_getAccount" "{\"address\": \"${GENESIS_ADDR}\"}"

# ===========================================================================
# 6. Submit a Transfer Transaction
# ===========================================================================

section "6. Submit a Transfer Transaction"
desc "Send 1,000,000 base-unit POL from the genesis account to another address."
desc "This uses a pre-constructed example. In production, the signature would"
desc "be computed from the sender's private key over the transaction payload."

RECIPIENT="ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"

rpc_call "polay_submitTransaction" "{
  \"sender\": \"${GENESIS_ADDR}\",
  \"nonce\": 0,
  \"action\": {
    \"type\": \"Transfer\",
    \"to\": \"${RECIPIENT}\",
    \"amount\": \"1000000\"
  },
  \"max_fee\": \"100\",
  \"signature\": \"00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000\"
}"

desc "Note: The above signature is a placeholder. On a real devnet the"
desc "signature must be valid Ed25519 over the canonical transaction bytes."
desc "Use the TypeScript SDK or 'polay tx' CLI to sign transactions properly."

# ===========================================================================
# 7. Query Balance After Transfer
# ===========================================================================

section "7. Query Balance After Transfer"
desc "Check the recipient's balance to confirm the transfer landed."

rpc_call "polay_getBalance" "{\"address\": \"${RECIPIENT}\"}"

# ===========================================================================
# 8. Create an Asset Class
# ===========================================================================

section "8. Create an Asset Class"
desc "Register a new game asset class: 'Legendary Sword' with a max supply of 1000."
desc "Only the creator address can mint new instances of this asset."

rpc_call "polay_submitTransaction" "{
  \"sender\": \"${GENESIS_ADDR}\",
  \"nonce\": 1,
  \"action\": {
    \"type\": \"CreateAssetClass\",
    \"name\": \"Legendary Sword\",
    \"max_supply\": 1000,
    \"metadata\": \"{\\\"image\\\": \\\"https://assets.example.io/sword.png\\\", \\\"rarity\\\": \\\"legendary\\\", \\\"game\\\": \\\"battle-royale-v1\\\"}\"
  },
  \"max_fee\": \"500\",
  \"signature\": \"00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000\"
}"

# ===========================================================================
# 9. Mint Assets
# ===========================================================================

section "9. Mint Assets"
desc "Mint 5 Legendary Swords to the recipient address."
desc "The asset_class_id would be returned from the CreateAssetClass response."

ASSET_CLASS_ID="aaa111222333444555666777888999000111222333444555666777888999000aaa"

rpc_call "polay_submitTransaction" "{
  \"sender\": \"${GENESIS_ADDR}\",
  \"nonce\": 2,
  \"action\": {
    \"type\": \"MintAsset\",
    \"asset_class_id\": \"${ASSET_CLASS_ID}\",
    \"to\": \"${RECIPIENT}\",
    \"amount\": 5
  },
  \"max_fee\": \"200\",
  \"signature\": \"00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000\"
}"

# ===========================================================================
# 10. Query Asset Balance
# ===========================================================================

section "10. Query Asset Balance"
desc "Check how many Legendary Swords the recipient holds."

rpc_call "polay_getAssetBalance" "{
  \"address\": \"${RECIPIENT}\",
  \"asset_class_id\": \"${ASSET_CLASS_ID}\"
}"

# ===========================================================================
# 11. Query Asset Class Info
# ===========================================================================

section "11. Query Asset Class Info"
desc "Get the full definition of the Legendary Sword asset class."

rpc_call "polay_getAssetClass" "{\"asset_class_id\": \"${ASSET_CLASS_ID}\"}"

# ===========================================================================
# 12. Create a Marketplace Listing
# ===========================================================================

section "12. Create a Marketplace Listing"
desc "List 2 Legendary Swords for sale at 50,000 POL each."

rpc_call "polay_submitTransaction" "{
  \"sender\": \"${RECIPIENT}\",
  \"nonce\": 0,
  \"action\": {
    \"type\": \"CreateListing\",
    \"asset_class_id\": \"${ASSET_CLASS_ID}\",
    \"quantity\": 2,
    \"price_per_unit\": \"50000\"
  },
  \"max_fee\": \"300\",
  \"signature\": \"00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000\"
}"

# ===========================================================================
# 13. Browse Marketplace Listings
# ===========================================================================

section "13. Browse Marketplace Listings"
desc "Query active marketplace listings for the Legendary Sword asset class."

rpc_call "polay_getListings" "{
  \"asset_class_id\": \"${ASSET_CLASS_ID}\",
  \"is_active\": true,
  \"limit\": 10
}"

# ===========================================================================
# 14. Create a Player Profile
# ===========================================================================

section "14. Create a Player Profile"
desc "Register an onchain player profile with a display name and metadata."

rpc_call "polay_submitTransaction" "{
  \"sender\": \"${RECIPIENT}\",
  \"nonce\": 1,
  \"action\": {
    \"type\": \"CreateProfile\",
    \"display_name\": \"DragonSlayer99\",
    \"metadata\": \"{\\\"avatar\\\": \\\"https://avatars.example.io/dragon.png\\\", \\\"bio\\\": \\\"Slaying dragons since 2024\\\"}\"
  },
  \"max_fee\": \"200\",
  \"signature\": \"00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000\"
}"

# ===========================================================================
# 15. Query Player Profile
# ===========================================================================

section "15. Query Player Profile"
desc "Fetch the player profile for the recipient address."

rpc_call "polay_getProfile" "{\"address\": \"${RECIPIENT}\"}"

# ===========================================================================
# 16. Get Validator Set
# ===========================================================================

section "16. Get Validator Set"
desc "Query the current active validator set and total stake."

rpc_call "polay_getValidatorSet" "{}"

# ===========================================================================
# Done
# ===========================================================================

section "Done"
echo "  All sample transactions submitted."
echo ""
echo "  Notes:"
echo "  - Signatures above are placeholders. Real transactions require valid"
echo "    Ed25519 signatures computed over the canonical transaction bytes."
echo "  - Use the TypeScript SDK for proper transaction construction:"
echo ""
echo "      import { PolayClient } from '@polay/sdk';"
echo "      const client = new PolayClient('${RPC_URL}');"
echo "      await client.transfer(keypair, recipient, 1000000n);"
echo ""
echo "  - See docs/rpc.md for the full RPC specification."
echo ""
