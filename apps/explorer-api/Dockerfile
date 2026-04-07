FROM rust:1.82-slim-bookworm AS builder

WORKDIR /build

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY src/ ./src/

RUN cargo build --release

# ---------- runtime image ----------
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/polay-explorer-api /usr/local/bin/polay-explorer-api

EXPOSE 3001
ENTRYPOINT ["polay-explorer-api"]
