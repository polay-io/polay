# ---------------------------------------------------------------------------
# POLAY Testnet — Hetzner Cloud Deployment
#
# Deploys a 4-validator POLAY network across global Hetzner datacenters.
# Each validator runs the Docker image with P2P networking enabled.
#
# Usage:
#   cd deploy/hetzner
#   cp terraform.tfvars.example terraform.tfvars   # fill in your values
#   terraform init
#   terraform plan
#   terraform apply
#
# After apply, run:
#   ../scripts/deploy-validators.sh
# ---------------------------------------------------------------------------

terraform {
  required_version = ">= 1.5"
  required_providers {
    hcloud = {
      source  = "hetznercloud/hcloud"
      version = "~> 1.45"
    }
  }
}

provider "hcloud" {
  token = var.hcloud_token
}

# ---------------------------------------------------------------------------
# Variables
# ---------------------------------------------------------------------------

variable "hcloud_token" {
  description = "Hetzner Cloud API token (set via HCLOUD_TOKEN env var or TF_VAR_hcloud_token)"
  type        = string
  sensitive   = true
  default     = ""
}

variable "ssh_key_name" {
  description = "Name of the SSH key in Hetzner Cloud console"
  type        = string
}

variable "server_type" {
  description = "Hetzner server type (cpx21 = 3 vCPU, 4GB RAM, 80GB)"
  default     = "cpx21"
}

variable "docker_image" {
  description = "Docker image for the POLAY node"
  default     = "ghcr.io/polay-io/polay:main"
}

variable "validators" {
  description = "Validator definitions: name, location, and P2P port"
  type = list(object({
    name     = string
    location = string
  }))
  default = [
    { name = "validator-1", location = "nbg1" },  # Nuremberg, EU
    { name = "validator-2", location = "ash" },    # Ashburn, US-East
    { name = "validator-3", location = "hil" },    # Hillsboro, US-West
    { name = "validator-4", location = "sin" },    # Singapore, Asia
  ]
}

# ---------------------------------------------------------------------------
# SSH Key
# ---------------------------------------------------------------------------

data "hcloud_ssh_key" "deploy" {
  name = var.ssh_key_name
}

# ---------------------------------------------------------------------------
# Firewall
# ---------------------------------------------------------------------------

resource "hcloud_firewall" "validator" {
  name = "polay-validator-fw"

  # SSH
  rule {
    direction  = "in"
    protocol   = "tcp"
    port       = "22"
    source_ips = ["0.0.0.0/0", "::/0"]
  }

  # JSON-RPC
  rule {
    direction  = "in"
    protocol   = "tcp"
    port       = "9944"
    source_ips = ["0.0.0.0/0", "::/0"]
  }

  # P2P consensus
  rule {
    direction  = "in"
    protocol   = "tcp"
    port       = "30333"
    source_ips = ["0.0.0.0/0", "::/0"]
  }

  # Prometheus metrics (internal only — restrict in production)
  rule {
    direction  = "in"
    protocol   = "tcp"
    port       = "9100"
    source_ips = ["0.0.0.0/0", "::/0"]
  }
}

# ---------------------------------------------------------------------------
# Validator Servers
# ---------------------------------------------------------------------------

resource "hcloud_server" "validator" {
  count       = length(var.validators)
  name        = "polay-${var.validators[count.index].name}"
  image       = "ubuntu-22.04"
  server_type = var.server_type
  location    = var.validators[count.index].location
  ssh_keys    = [data.hcloud_ssh_key.deploy.id]

  firewall_ids = [hcloud_firewall.validator.id]

  user_data = <<-USERDATA
    #!/bin/bash
    set -e

    # Install Docker
    apt-get update -y
    apt-get install -y docker.io curl jq
    systemctl enable docker
    systemctl start docker

    # Create data directories
    mkdir -p /opt/polay/{data,keys,state}

    # Pull POLAY image
    docker pull ${var.docker_image} || true

    echo "POLAY ${var.validators[count.index].name} provisioned in ${var.validators[count.index].location}."
  USERDATA

  labels = {
    role    = "validator"
    network = "polay-testnet"
    index   = tostring(count.index + 1)
  }
}

# ---------------------------------------------------------------------------
# Outputs
# ---------------------------------------------------------------------------

output "validator_ips" {
  description = "Public IPs of all validators"
  value = {
    for i, server in hcloud_server.validator :
    var.validators[i].name => {
      ip       = server.ipv4_address
      location = var.validators[i].location
    }
  }
}

output "boot_node_ip" {
  description = "IP of validator-1 (boot node)"
  value       = hcloud_server.validator[0].ipv4_address
}

output "boot_node_multiaddr" {
  description = "Boot node multiaddr for other validators to connect to"
  value       = "/ip4/${hcloud_server.validator[0].ipv4_address}/tcp/30333"
}

output "rpc_endpoints" {
  description = "JSON-RPC endpoints for all validators"
  value = [
    for server in hcloud_server.validator :
    "http://${server.ipv4_address}:9944"
  ]
}

output "ssh_commands" {
  description = "SSH commands for each validator"
  value = [
    for i, server in hcloud_server.validator :
    "ssh root@${server.ipv4_address}  # ${var.validators[i].name} (${var.validators[i].location})"
  ]
}
