FROM rust:1.85-slim-bookworm AS builder

WORKDIR /build

# Install build dependencies for linking against system libs.
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml ./
COPY src/ ./src/
COPY migrations/ ./migrations/

RUN cargo build --release

# ---------- runtime image ----------
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/polay-indexer /usr/local/bin/polay-indexer

ENTRYPOINT ["polay-indexer"]
