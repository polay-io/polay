# ---------------------------------------------------------------------------
# POLAY Testnet — Terraform Deployment
#
# Deploys a 4-validator POLAY testnet on AWS using EC2 instances.
# Each validator runs the Docker image with P2P networking enabled.
#
# Usage:
#   cd deploy/terraform
#   terraform init
#   terraform plan -var="ssh_key_name=my-key"
#   terraform apply -var="ssh_key_name=my-key"
# ---------------------------------------------------------------------------

terraform {
  required_version = ">= 1.5"
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.0"
    }
  }
}

provider "aws" {
  region = var.region
}

# ---------------------------------------------------------------------------
# Variables
# ---------------------------------------------------------------------------

variable "region" {
  description = "AWS region"
  default     = "us-east-1"
}

variable "instance_type" {
  description = "EC2 instance type for validator nodes"
  default     = "t3.medium"
}

variable "ssh_key_name" {
  description = "Name of the SSH key pair for EC2 access"
  type        = string
}

variable "validator_count" {
  description = "Number of validator nodes"
  default     = 4
}

variable "docker_image" {
  description = "Docker image for the POLAY node"
  default     = "ghcr.io/polaychain/polay:main"
}

variable "allowed_ssh_cidrs" {
  description = "CIDR blocks allowed to SSH into validators"
  type        = list(string)
  default     = ["0.0.0.0/0"]
}

# ---------------------------------------------------------------------------
# Networking
# ---------------------------------------------------------------------------

resource "aws_vpc" "polay" {
  cidr_block           = "10.0.0.0/16"
  enable_dns_hostnames = true
  enable_dns_support   = true

  tags = { Name = "polay-testnet-vpc" }
}

resource "aws_subnet" "polay" {
  vpc_id                  = aws_vpc.polay.id
  cidr_block              = "10.0.1.0/24"
  map_public_ip_on_launch = true
  availability_zone       = "${var.region}a"

  tags = { Name = "polay-testnet-subnet" }
}

resource "aws_internet_gateway" "polay" {
  vpc_id = aws_vpc.polay.id
  tags   = { Name = "polay-testnet-igw" }
}

resource "aws_route_table" "polay" {
  vpc_id = aws_vpc.polay.id

  route {
    cidr_block = "0.0.0.0/0"
    gateway_id = aws_internet_gateway.polay.id
  }

  tags = { Name = "polay-testnet-rt" }
}

resource "aws_route_table_association" "polay" {
  subnet_id      = aws_subnet.polay.id
  route_table_id = aws_route_table.polay.id
}

# ---------------------------------------------------------------------------
# Security Group
# ---------------------------------------------------------------------------

resource "aws_security_group" "validator" {
  name_prefix = "polay-validator-"
  vpc_id      = aws_vpc.polay.id

  # SSH
  ingress {
    from_port   = 22
    to_port     = 22
    protocol    = "tcp"
    cidr_blocks = var.allowed_ssh_cidrs
  }

  # JSON-RPC
  ingress {
    from_port   = 9944
    to_port     = 9944
    protocol    = "tcp"
    cidr_blocks = ["0.0.0.0/0"]
  }

  # P2P
  ingress {
    from_port   = 30333
    to_port     = 30333
    protocol    = "tcp"
    cidr_blocks = ["0.0.0.0/0"]
  }

  # Prometheus metrics
  ingress {
    from_port   = 9100
    to_port     = 9100
    protocol    = "tcp"
    cidr_blocks = ["10.0.0.0/16"]
  }

  # All outbound
  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }

  tags = { Name = "polay-validator-sg" }
}

# ---------------------------------------------------------------------------
# EC2 Instances
# ---------------------------------------------------------------------------

data "aws_ami" "ubuntu" {
  most_recent = true
  owners      = ["099720109477"] # Canonical

  filter {
    name   = "name"
    values = ["ubuntu/images/hvm-ssd/ubuntu-jammy-22.04-amd64-server-*"]
  }
}

resource "aws_instance" "validator" {
  count                  = var.validator_count
  ami                    = data.aws_ami.ubuntu.id
  instance_type          = var.instance_type
  key_name               = var.ssh_key_name
  subnet_id              = aws_subnet.polay.id
  vpc_security_group_ids = [aws_security_group.validator.id]

  root_block_device {
    volume_size = 50
    volume_type = "gp3"
  }

  user_data = <<-USERDATA
    #!/bin/bash
    set -e

    # Install Docker
    apt-get update -y
    apt-get install -y docker.io curl jq
    systemctl enable docker
    systemctl start docker

    # Pull POLAY image
    docker pull ${var.docker_image}

    # Create data directory
    mkdir -p /opt/polay/data /opt/polay/keys

    echo "POLAY validator ${count.index + 1} ready."
    echo "Upload genesis.json and validator key, then start with:"
    echo "  docker run -d --name polay-validator \\"
    echo "    -v /opt/polay:/data \\"
    echo "    -p 9944:9944 -p 30333:30333 \\"
    echo "    ${var.docker_image} run \\"
    echo "    --genesis /data/genesis.json \\"
    echo "    --data-dir /data/state \\"
    echo "    --rpc-addr 0.0.0.0:9944 \\"
    echo "    --validator-key /data/keys/validator.key \\"
    echo "    --p2p-addr /ip4/0.0.0.0/tcp/30333 \\"
    echo "    --boot-nodes <BOOT_NODE_MULTIADDR>"
  USERDATA

  tags = {
    Name    = "polay-validator-${count.index + 1}"
    Role    = "validator"
    Network = "polay-testnet"
  }
}

# ---------------------------------------------------------------------------
# Monitoring Instance
# ---------------------------------------------------------------------------

resource "aws_security_group" "monitoring" {
  name_prefix = "polay-monitoring-"
  vpc_id      = aws_vpc.polay.id

  # SSH
  ingress {
    from_port   = 22
    to_port     = 22
    protocol    = "tcp"
    cidr_blocks = var.allowed_ssh_cidrs
  }

  # Grafana
  ingress {
    from_port   = 3000
    to_port     = 3000
    protocol    = "tcp"
    cidr_blocks = ["0.0.0.0/0"]
  }

  # Prometheus
  ingress {
    from_port   = 9090
    to_port     = 9090
    protocol    = "tcp"
    cidr_blocks = ["10.0.0.0/16"]
  }

  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }

  tags = { Name = "polay-monitoring-sg" }
}

resource "aws_instance" "monitoring" {
  ami                    = data.aws_ami.ubuntu.id
  instance_type          = "t3.small"
  key_name               = var.ssh_key_name
  subnet_id              = aws_subnet.polay.id
  vpc_security_group_ids = [aws_security_group.monitoring.id]

  root_block_device {
    volume_size = 30
    volume_type = "gp3"
  }

  user_data = <<-USERDATA
    #!/bin/bash
    set -e
    apt-get update -y
    apt-get install -y docker.io docker-compose-plugin curl
    systemctl enable docker
    systemctl start docker

    echo "POLAY monitoring node ready."
    echo "Clone the repo and run:"
    echo "  docker compose -f monitoring/docker-compose.monitoring.yml up -d"
  USERDATA

  tags = {
    Name    = "polay-monitoring"
    Role    = "monitoring"
    Network = "polay-testnet"
  }
}

# ---------------------------------------------------------------------------
# Outputs
# ---------------------------------------------------------------------------

output "validator_public_ips" {
  description = "Public IPs of validator nodes"
  value       = aws_instance.validator[*].public_ip
}

output "validator_private_ips" {
  description = "Private IPs of validator nodes"
  value       = aws_instance.validator[*].private_ip
}

output "monitoring_public_ip" {
  description = "Public IP of monitoring node"
  value       = aws_instance.monitoring.public_ip
}

output "boot_node_multiaddr" {
  description = "Boot node multiaddr for validator-1 (use private IP within VPC)"
  value       = "/ip4/${aws_instance.validator[0].private_ip}/tcp/30333"
}

output "rpc_endpoints" {
  description = "RPC endpoints for all validators"
  value = [
    for i, inst in aws_instance.validator :
    "http://${inst.public_ip}:9944"
  ]
}

output "grafana_url" {
  description = "Grafana dashboard URL"
  value       = "http://${aws_instance.monitoring.public_ip}:3000"
}
