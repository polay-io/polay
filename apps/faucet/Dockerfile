FROM rust:1.85 AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/ src/

# Build
RUN cargo build --release --bin polay-faucet

# Runtime
FROM debian:bookworm-slim
RUN apt-get update && \
    apt-get install -y --no-install-recommends libssl3 ca-certificates && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/polay-faucet /usr/local/bin/polay-faucet

EXPOSE 8080
ENTRYPOINT ["polay-faucet"]
