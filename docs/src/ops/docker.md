# Docker Deployment

POLAY provides Docker images and Compose files for containerized deployment of validators, infrastructure services, and supporting tools.

## Dockerfile

The multi-stage Dockerfile builds a minimal production image:

```dockerfile
# Build stage
FROM rust:1.77-bookworm AS builder
WORKDIR /build
COPY . .
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y librocksdb-dev ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/polay /usr/local/bin/polay
EXPOSE 9944 9945 26656
ENTRYPOINT ["polay"]
CMD ["run"]
```

Build and tag:

```bash
docker build -t polay/node:latest .
```

## Local Devnet (docker-compose.yml)

A 4-validator local devnet for development:

```yaml
# docker/docker-compose.yml
services:
  validator-0:
    image: polay/node:latest
    command: run --home /data --validator
    volumes:
      - ./devnet/validator-0:/data
    ports:
      - "9944:9944"
      - "26656:26656"
    networks:
      - polay-net

  validator-1:
    image: polay/node:latest
    command: run --home /data --validator
    volumes:
      - ./devnet/validator-1:/data
    ports:
      - "9945:9944"
      - "26657:26656"
    networks:
      - polay-net

  validator-2:
    image: polay/node:latest
    command: run --home /data --validator
    volumes:
      - ./devnet/validator-2:/data
    ports:
      - "9946:9944"
      - "26658:26656"
    networks:
      - polay-net

  validator-3:
    image: polay/node:latest
    command: run --home /data --validator
    volumes:
      - ./devnet/validator-3:/data
    ports:
      - "9947:9944"
      - "26659:26656"
    networks:
      - polay-net

networks:
  polay-net:
    driver: bridge
```

Initialize and start:

```bash
# Generate devnet configs
./scripts/init-devnet.sh --output docker/devnet

# Start all validators
cd docker
docker-compose up -d

# Check logs
docker-compose logs -f validator-0

# Stop
docker-compose down
```

## Testnet Deployment (docker-compose.testnet.yml)

For a more production-like setup with infrastructure services:

```yaml
# docker/docker-compose.testnet.yml
services:
  # --- Validators ---
  validator-0:
    image: polay/node:latest
    command: run --home /data --validator
    volumes:
      - validator-0-data:/data
    ports:
      - "9944:9944"
      - "26656:26656"
    networks:
      - polay-net
    restart: always

  validator-1:
    image: polay/node:latest
    command: run --home /data --validator
    volumes:
      - validator-1-data:/data
    ports:
      - "9945:9944"
      - "26657:26656"
    networks:
      - polay-net
    restart: always

  validator-2:
    image: polay/node:latest
    command: run --home /data --validator
    volumes:
      - validator-2-data:/data
    networks:
      - polay-net
    restart: always

  validator-3:
    image: polay/node:latest
    command: run --home /data --validator
    volumes:
      - validator-3-data:/data
    networks:
      - polay-net
    restart: always

  # --- Infrastructure ---
  postgres:
    image: postgres:16
    environment:
      POSTGRES_DB: polay_indexer
      POSTGRES_USER: polay
      POSTGRES_PASSWORD: changeme
    volumes:
      - postgres-data:/var/lib/postgresql/data
    networks:
      - polay-net

  indexer:
    image: polay/indexer:latest
    environment:
      RPC_URL: http://validator-0:9944
      DATABASE_URL: postgres://polay:changeme@postgres:5432/polay_indexer
    depends_on:
      - postgres
      - validator-0
    networks:
      - polay-net

  faucet:
    image: polay/faucet:latest
    environment:
      RPC_URL: http://validator-0:9944
      FAUCET_KEY: /keys/faucet_key.json
      AMOUNT: 10000000
      COOLDOWN_SECONDS: 60
    volumes:
      - ./keys:/keys:ro
    ports:
      - "8080:8080"
    networks:
      - polay-net

  explorer-api:
    image: polay/explorer-api:latest
    environment:
      DATABASE_URL: postgres://polay:changeme@postgres:5432/polay_indexer
    ports:
      - "3001:3001"
    depends_on:
      - indexer
    networks:
      - polay-net

volumes:
  validator-0-data:
  validator-1-data:
  validator-2-data:
  validator-3-data:
  postgres-data:

networks:
  polay-net:
    driver: bridge
```

### Infrastructure Services

| Service | Port | Description |
|---|---|---|
| **postgres** | 5432 (internal) | PostgreSQL database for the indexer |
| **indexer** | -- | Follows the chain and indexes data into PostgreSQL |
| **faucet** | 8080 | Web service to dispense testnet tokens |
| **explorer-api** | 3001 | REST API for the block explorer frontend |

## P2P Networking in Docker

Validators in the same Docker network discover each other via DNS. Each validator's `config.toml` should reference peers by Docker service name:

```toml
[network]
boot_nodes = [
    "/dns4/validator-0/tcp/26656/p2p/PEER_ID_0",
    "/dns4/validator-1/tcp/26656/p2p/PEER_ID_1",
]
```

For external access, publish the P2P port and set `external_addr` to the host's public IP.

## Useful Commands

```bash
# View running services
docker-compose ps

# Restart a single validator
docker-compose restart validator-0

# View resource usage
docker stats

# Exec into a container
docker-compose exec validator-0 polay keys show

# Wipe all state and restart fresh
docker-compose down -v
docker-compose up -d
```
