# Cloud Deployment

This guide covers deploying a POLAY testnet on AWS using Terraform. The infrastructure includes validator nodes, a monitoring node, and networking.

## Architecture

```
                  Internet
                     |
               [Load Balancer]
                  /       \
            [RPC Node]  [Explorer API]
                |
          [VPC: 10.0.0.0/16]
          /    |    |    \
       val-0 val-1 val-2 val-3    (private subnet)
                     |
               [Monitor Node]     (private subnet)
                     |
               [Boot Node]        (public subnet)
```

## Prerequisites

- [Terraform](https://www.terraform.io/) >= 1.5
- AWS CLI configured with appropriate credentials
- An S3 bucket for Terraform state (recommended)
- An EC2 key pair for SSH access

## Terraform Configuration

### Provider and State

```hcl
# terraform/main.tf

terraform {
  required_version = ">= 1.5"
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.0"
    }
  }
  backend "s3" {
    bucket = "polay-terraform-state"
    key    = "testnet/terraform.tfstate"
    region = "us-east-1"
  }
}

provider "aws" {
  region = var.region
}
```

### Variables

```hcl
# terraform/variables.tf

variable "region" {
  default = "us-east-1"
}

variable "validator_count" {
  default = 4
}

variable "validator_instance_type" {
  default = "c6i.xlarge"  # 4 vCPU, 8 GB RAM
}

variable "monitor_instance_type" {
  default = "t3.large"  # 2 vCPU, 8 GB RAM
}

variable "key_name" {
  description = "EC2 key pair name for SSH access"
}

variable "chain_id" {
  default = "polay-testnet-1"
}
```

### VPC and Networking

```hcl
# terraform/vpc.tf

resource "aws_vpc" "polay" {
  cidr_block           = "10.0.0.0/16"
  enable_dns_hostnames = true
  tags = { Name = "polay-testnet" }
}

resource "aws_subnet" "public" {
  vpc_id                  = aws_vpc.polay.id
  cidr_block              = "10.0.1.0/24"
  map_public_ip_on_launch = true
  availability_zone       = "${var.region}a"
  tags = { Name = "polay-public" }
}

resource "aws_subnet" "private" {
  vpc_id            = aws_vpc.polay.id
  cidr_block        = "10.0.2.0/24"
  availability_zone = "${var.region}a"
  tags = { Name = "polay-private" }
}

resource "aws_internet_gateway" "igw" {
  vpc_id = aws_vpc.polay.id
}

resource "aws_nat_gateway" "nat" {
  allocation_id = aws_eip.nat.id
  subnet_id     = aws_subnet.public.id
}

resource "aws_eip" "nat" {
  domain = "vpc"
}
```

### Security Groups

```hcl
# terraform/security.tf

resource "aws_security_group" "validator" {
  name   = "polay-validator"
  vpc_id = aws_vpc.polay.id

  # P2P between validators
  ingress {
    from_port   = 26656
    to_port     = 26659
    protocol    = "tcp"
    self        = true
  }

  # RPC from internal only
  ingress {
    from_port   = 9944
    to_port     = 9945
    protocol    = "tcp"
    cidr_blocks = ["10.0.0.0/16"]
  }

  # SSH from bastion
  ingress {
    from_port   = 22
    to_port     = 22
    protocol    = "tcp"
    cidr_blocks = ["10.0.1.0/24"]
  }

  # All outbound
  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }
}

resource "aws_security_group" "boot_node" {
  name   = "polay-boot-node"
  vpc_id = aws_vpc.polay.id

  # P2P from internet
  ingress {
    from_port   = 26656
    to_port     = 26656
    protocol    = "tcp"
    cidr_blocks = ["0.0.0.0/0"]
  }

  # SSH
  ingress {
    from_port   = 22
    to_port     = 22
    protocol    = "tcp"
    cidr_blocks = ["0.0.0.0/0"]  # restrict to your IP in production
  }

  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }
}
```

### Validator Instances

```hcl
# terraform/validators.tf

resource "aws_instance" "validator" {
  count         = var.validator_count
  ami           = data.aws_ami.ubuntu.id
  instance_type = var.validator_instance_type
  key_name      = var.key_name
  subnet_id     = aws_subnet.private.id

  vpc_security_group_ids = [aws_security_group.validator.id]

  root_block_device {
    volume_size = 100
    volume_type = "gp3"
    iops        = 3000
    throughput  = 125
  }

  user_data = templatefile("${path.module}/scripts/validator-init.sh", {
    chain_id        = var.chain_id
    validator_index = count.index
    boot_node_ip    = aws_instance.boot_node.private_ip
  })

  tags = { Name = "polay-validator-${count.index}" }
}

data "aws_ami" "ubuntu" {
  most_recent = true
  owners      = ["099720109477"]
  filter {
    name   = "name"
    values = ["ubuntu/images/hvm-ssd/ubuntu-*-24.04-amd64-server-*"]
  }
}
```

### Boot Node and Monitor

```hcl
# terraform/boot-node.tf

resource "aws_instance" "boot_node" {
  ami                    = data.aws_ami.ubuntu.id
  instance_type          = "t3.medium"
  key_name               = var.key_name
  subnet_id              = aws_subnet.public.id
  vpc_security_group_ids = [aws_security_group.boot_node.id]

  root_block_device {
    volume_size = 50
    volume_type = "gp3"
  }

  tags = { Name = "polay-boot-node" }
}

resource "aws_instance" "monitor" {
  ami                    = data.aws_ami.ubuntu.id
  instance_type          = var.monitor_instance_type
  key_name               = var.key_name
  subnet_id              = aws_subnet.private.id
  vpc_security_group_ids = [aws_security_group.validator.id]

  root_block_device {
    volume_size = 50
    volume_type = "gp3"
  }

  tags = { Name = "polay-monitor" }
}
```

## Deployment Steps

```bash
cd terraform

# Initialize Terraform
terraform init

# Review the plan
terraform plan -var="key_name=my-keypair"

# Apply
terraform apply -var="key_name=my-keypair"

# Get outputs
terraform output
```

## Post-Deployment

1. **SSH to boot node** and verify it is listening on port 26656
2. **Check validator logs** via SSH through the bastion/boot node
3. **Deploy monitoring** (Prometheus + Grafana) on the monitor node
4. **Point DNS** to the boot node's public IP for stable peer addresses
5. **Set up backups** -- snapshot EBS volumes on a schedule

## Cleanup

```bash
terraform destroy -var="key_name=my-keypair"
```

This destroys all resources. EBS volumes with data will be deleted unless you have snapshots.

## Cost Estimate

| Resource | Qty | Monthly Cost (approx) |
|---|---|---|
| c6i.xlarge (validators) | 4 | $480 |
| t3.medium (boot node) | 1 | $30 |
| t3.large (monitor) | 1 | $60 |
| gp3 storage (100 GB x 4 + 50 GB x 2) | 500 GB | $40 |
| NAT Gateway | 1 | $32 |
| **Total** | | **~$642/month** |

Costs vary by region and can be reduced with reserved instances or spot instances for non-validator workloads.
