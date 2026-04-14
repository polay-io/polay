FROM rust:latest AS builder

WORKDIR /app

# Install build dependencies for C libraries (zstd-sys, etc.)
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        pkg-config \
        libssl-dev \
        cmake \
        make \
        clang && \
    rm -rf /var/lib/apt/lists/*

# Cache dependency builds by copying manifests first
COPY Cargo.toml Cargo.lock ./
COPY crates/polay-types/Cargo.toml crates/polay-types/Cargo.toml
COPY crates/polay-crypto/Cargo.toml crates/polay-crypto/Cargo.toml
COPY crates/polay-config/Cargo.toml crates/polay-config/Cargo.toml
COPY crates/polay-genesis/Cargo.toml crates/polay-genesis/Cargo.toml
COPY crates/polay-state/Cargo.toml crates/polay-state/Cargo.toml
COPY crates/polay-mempool/Cargo.toml crates/polay-mempool/Cargo.toml
COPY crates/polay-execution/Cargo.toml crates/polay-execution/Cargo.toml
COPY crates/polay-consensus/Cargo.toml crates/polay-consensus/Cargo.toml
COPY crates/polay-network/Cargo.toml crates/polay-network/Cargo.toml
COPY crates/polay-rpc/Cargo.toml crates/polay-rpc/Cargo.toml
COPY crates/polay-validator/Cargo.toml crates/polay-validator/Cargo.toml
COPY crates/polay-staking/Cargo.toml crates/polay-staking/Cargo.toml
COPY crates/polay-attestation/Cargo.toml crates/polay-attestation/Cargo.toml
COPY crates/polay-market/Cargo.toml crates/polay-market/Cargo.toml
COPY crates/polay-identity/Cargo.toml crates/polay-identity/Cargo.toml
COPY crates/polay-node/Cargo.toml crates/polay-node/Cargo.toml

# Create stub lib.rs files so cargo can resolve the workspace and cache deps
RUN for crate_dir in crates/*/; do \
        mkdir -p "${crate_dir}src" && \
        echo "" > "${crate_dir}src/lib.rs"; \
    done && \
    # Create a stub main.rs for the node binary
    mkdir -p crates/polay-node/src && \
    echo "fn main() {}" > crates/polay-node/src/main.rs && \
    cargo build --release --bin polay 2>/dev/null || true

# Copy actual source code and build for real
COPY crates/ crates/
RUN cargo build --release --bin polay

# ---------------------------------------------------------------------------
# Runtime image
# ---------------------------------------------------------------------------
FROM debian:trixie-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        libssl3 \
        ca-certificates \
        curl && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/polay /usr/local/bin/polay

# Default data directory
RUN mkdir -p /data
VOLUME ["/data"]

EXPOSE 9944
ENTRYPOINT ["polay"]
