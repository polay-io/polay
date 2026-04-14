#!/usr/bin/env bash
set -euo pipefail

# ---------------------------------------------------------------------------
# deploy-validators.sh
#
# Deploys POLAY validators to Hetzner servers provisioned by Terraform.
# Run this AFTER `terraform apply` completes.
#
# Prerequisites:
#   - terraform apply has completed (servers are up)
#   - devnet-data/ has genesis.json and keys/validator-{1..4}.key
#   - Docker image is pushed to ghcr.io/polaychain/polay:main
#
# Usage:
#   cd deploy/hetzner
#   ./deploy-validators.sh
#
# Options:
#   --genesis-only   Only upload genesis.json + keys (skip Docker start)
#   --restart        Restart validators without re-uploading files
#   --status         Check status of all validators
# ---------------------------------------------------------------------------

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
DEVNET_DIR="${PROJECT_ROOT}/devnet-data"
GENESIS_FILE="${DEVNET_DIR}/genesis.json"
DOCKER_IMAGE="ghcr.io/polaychain/polay:main"
BLOCK_TIME=2000

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

info()  { printf "\033[1;34m[deploy]\033[0m %s\n" "$*"; }
ok()    { printf "\033[1;32m[deploy]\033[0m %s\n" "$*"; }
err()   { printf "\033[1;31m[deploy]\033[0m %s\n" "$*" >&2; }
warn()  { printf "\033[1;33m[deploy]\033[0m %s\n" "$*"; }

# ---------------------------------------------------------------------------
# Read Terraform outputs
# ---------------------------------------------------------------------------

get_validator_ips() {
    cd "${SCRIPT_DIR}"
    terraform output -json validator_ips | jq -r 'to_entries | sort_by(.key) | .[].value.ip'
}

get_boot_node_ip() {
    cd "${SCRIPT_DIR}"
    terraform output -raw boot_node_ip
}

# ---------------------------------------------------------------------------
# Pre-flight checks
# ---------------------------------------------------------------------------

preflight() {
    if ! command -v terraform &>/dev/null; then
        err "terraform not found. Install it: brew install terraform"
        exit 1
    fi

    if ! command -v jq &>/dev/null; then
        err "jq not found. Install it: brew install jq"
        exit 1
    fi

    if [[ ! -f "${GENESIS_FILE}" ]]; then
        err "Genesis file not found at ${GENESIS_FILE}"
        err "Run ./scripts/init-devnet.sh first."
        exit 1
    fi

    # Check terraform state exists
    cd "${SCRIPT_DIR}"
    if ! terraform output -json validator_ips &>/dev/null; then
        err "Terraform state not found. Run 'terraform apply' first."
        exit 1
    fi
}

# ---------------------------------------------------------------------------
# Wait for SSH
# ---------------------------------------------------------------------------

wait_for_ssh() {
    local ip="$1"
    local max_attempts=30
    local attempt=0

    while ! ssh -o ConnectTimeout=5 -o StrictHostKeyChecking=no -o BatchMode=yes "root@${ip}" true 2>/dev/null; do
        attempt=$((attempt + 1))
        if [[ ${attempt} -ge ${max_attempts} ]]; then
            err "Timed out waiting for SSH on ${ip}"
            return 1
        fi
        printf "."
        sleep 5
    done
    echo ""
}

# ---------------------------------------------------------------------------
# Upload genesis + key to a single validator
# ---------------------------------------------------------------------------

upload_files() {
    local ip="$1"
    local validator_num="$2"
    local key_file="${DEVNET_DIR}/keys/validator-${validator_num}.key"

    if [[ ! -f "${key_file}" ]]; then
        err "Key file not found: ${key_file}"
        return 1
    fi

    info "Uploading genesis + key to validator-${validator_num} (${ip})..."

    ssh -o StrictHostKeyChecking=no "root@${ip}" "mkdir -p /opt/polay/{data,keys,state}"
    scp -o StrictHostKeyChecking=no "${GENESIS_FILE}" "root@${ip}:/opt/polay/data/genesis.json"
    scp -o StrictHostKeyChecking=no "${key_file}" "root@${ip}:/opt/polay/keys/validator.key"

    # Secure the key file
    ssh -o StrictHostKeyChecking=no "root@${ip}" "chmod 600 /opt/polay/keys/validator.key"

    ok "  Files uploaded to validator-${validator_num}"
}

# ---------------------------------------------------------------------------
# Start a validator via Docker
# ---------------------------------------------------------------------------

start_validator() {
    local ip="$1"
    local validator_num="$2"
    local boot_node_ip="$3"

    info "Starting validator-${validator_num} on ${ip}..."

    # Stop existing container if running
    ssh -o StrictHostKeyChecking=no "root@${ip}" \
        "docker rm -f polay-validator 2>/dev/null || true"

    # Pull latest image
    ssh -o StrictHostKeyChecking=no "root@${ip}" \
        "docker pull ${DOCKER_IMAGE}"

    # Build boot-nodes argument (empty for validator-1)
    local boot_args=""
    if [[ ${validator_num} -gt 1 && -n "${boot_node_ip}" ]]; then
        boot_args="--boot-nodes /ip4/${boot_node_ip}/tcp/30333"
    fi

    # Start the container
    ssh -o StrictHostKeyChecking=no "root@${ip}" \
        "docker run -d \
            --name polay-validator \
            --restart unless-stopped \
            -v /opt/polay/data:/data/data \
            -v /opt/polay/keys:/data/keys \
            -v /opt/polay/state:/data/state \
            -p 9944:9944 \
            -p 30333:30333 \
            -e RUST_LOG=info \
            ${DOCKER_IMAGE} run \
                --genesis /data/data/genesis.json \
                --data-dir /data/state \
                --rpc-addr 0.0.0.0:9944 \
                --validator-key /data/keys/validator.key \
                --block-time ${BLOCK_TIME} \
                --p2p-addr /ip4/0.0.0.0/tcp/30333 \
                ${boot_args}"

    ok "  Validator-${validator_num} started"
}

# ---------------------------------------------------------------------------
# Check status of all validators
# ---------------------------------------------------------------------------

check_status() {
    local ips
    ips=$(get_validator_ips)
    local i=1

    echo ""
    echo "============================================================"
    echo "  POLAY Validator Status"
    echo "============================================================"
    echo ""

    while IFS= read -r ip; do
        printf "  validator-%-2d (%s): " "${i}" "${ip}"

        # Check if Docker container is running
        local status
        status=$(ssh -o ConnectTimeout=5 -o StrictHostKeyChecking=no "root@${ip}" \
            "docker inspect -f '{{.State.Status}}' polay-validator 2>/dev/null" 2>/dev/null || echo "not-found")

        if [[ "${status}" == "running" ]]; then
            # Check RPC health
            local health
            health=$(ssh -o ConnectTimeout=5 -o StrictHostKeyChecking=no "root@${ip}" \
                "curl -sf http://localhost:9944/health 2>/dev/null" || echo "")
            if [[ -n "${health}" ]]; then
                printf "\033[1;32mHEALTHY\033[0m (container running, RPC responding)\n"
            else
                printf "\033[1;33mSTARTING\033[0m (container running, RPC not ready)\n"
            fi
        elif [[ "${status}" == "not-found" ]]; then
            printf "\033[1;31mNOT DEPLOYED\033[0m\n"
        else
            printf "\033[1;31m${status}\033[0m\n"
        fi

        i=$((i + 1))
    done <<< "${ips}"

    echo ""
    echo "============================================================"
}

# ---------------------------------------------------------------------------
# View logs from a validator
# ---------------------------------------------------------------------------

view_logs() {
    local validator_num="${1:-1}"
    local ips
    ips=$(get_validator_ips)
    local ip
    ip=$(echo "${ips}" | sed -n "${validator_num}p")

    if [[ -z "${ip}" ]]; then
        err "Validator ${validator_num} not found"
        exit 1
    fi

    info "Streaming logs from validator-${validator_num} (${ip})..."
    ssh -o StrictHostKeyChecking=no "root@${ip}" "docker logs -f --tail 50 polay-validator"
}

# ---------------------------------------------------------------------------
# Full deploy
# ---------------------------------------------------------------------------

deploy_all() {
    preflight

    local ips
    ips=$(get_validator_ips)
    local boot_ip
    boot_ip=$(get_boot_node_ip)

    echo ""
    echo "============================================================"
    echo "  POLAY Global Deployment"
    echo "============================================================"
    echo ""
    info "Boot node IP: ${boot_ip}"
    echo ""

    # Wait for all servers to be reachable
    local i=1
    while IFS= read -r ip; do
        info "Waiting for validator-${i} (${ip}) to accept SSH..."
        wait_for_ssh "${ip}"
        ok "  validator-${i} reachable"
        i=$((i + 1))
    done <<< "${ips}"

    echo ""

    # Upload genesis + keys to all validators
    i=1
    while IFS= read -r ip; do
        upload_files "${ip}" "${i}"
        i=$((i + 1))
    done <<< "${ips}"

    echo ""

    # Start validator-1 first (boot node)
    local first_ip
    first_ip=$(echo "${ips}" | head -1)
    start_validator "${first_ip}" 1 ""

    # Give boot node a few seconds to start P2P listener
    info "Waiting for boot node to initialize..."
    sleep 10

    # Start validators 2-4
    i=2
    while IFS= read -r ip; do
        if [[ ${i} -le 4 ]]; then
            start_validator "${ip}" "${i}" "${boot_ip}"
        fi
        i=$((i + 1))
    done <<< "$(echo "${ips}" | tail -n +2)"

    echo ""

    # Wait and check status
    info "Waiting 15s for network to stabilize..."
    sleep 15
    check_status

    echo "  RPC Endpoints:"
    i=1
    while IFS= read -r ip; do
        echo "    validator-${i}: http://${ip}:9944"
        i=$((i + 1))
    done <<< "${ips}"

    echo ""
    echo "  Quick checks:"
    echo "    curl http://${boot_ip}:9944/health"
    echo "    ./deploy-validators.sh --status"
    echo "    ./deploy-validators.sh --logs 1"
    echo ""
    echo "============================================================"
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

case "${1:-}" in
    --genesis-only)
        preflight
        ips=$(get_validator_ips)
        i=1
        while IFS= read -r ip; do
            upload_files "${ip}" "${i}"
            i=$((i + 1))
        done <<< "${ips}"
        ;;
    --restart)
        preflight
        ips=$(get_validator_ips)
        boot_ip=$(get_boot_node_ip)
        first_ip=$(echo "${ips}" | head -1)
        start_validator "${first_ip}" 1 ""
        sleep 10
        i=2
        while IFS= read -r ip; do
            start_validator "${ip}" "${i}" "${boot_ip}"
            i=$((i + 1))
        done <<< "$(echo "${ips}" | tail -n +2)"
        ;;
    --status)
        preflight
        check_status
        ;;
    --logs)
        preflight
        view_logs "${2:-1}"
        ;;
    --help|-h)
        echo "Usage: deploy-validators.sh [OPTION]"
        echo ""
        echo "Options:"
        echo "  (none)           Full deploy: upload files + start all validators"
        echo "  --genesis-only   Only upload genesis.json and keys"
        echo "  --restart        Restart all validators"
        echo "  --status         Check status of all validators"
        echo "  --logs [N]       Stream logs from validator N (default: 1)"
        echo "  -h, --help       Show this help"
        ;;
    *)
        deploy_all
        ;;
esac
