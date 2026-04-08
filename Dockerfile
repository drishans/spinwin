# Stage 1: Build the Rust binary
FROM rust:1.83-slim AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

# Copy workspace manifests first for dependency caching
COPY Cargo.toml ./
COPY core/Cargo.toml core/Cargo.toml
COPY server/Cargo.toml server/Cargo.toml
COPY scanner-wasm/Cargo.toml scanner-wasm/Cargo.toml

# Create dummy source files so cargo can resolve dependencies
RUN mkdir -p core/src server/src scanner-wasm/src && \
    echo "pub fn dummy() {}" > core/src/lib.rs && \
    echo "fn main() {}" > server/src/main.rs && \
    echo "pub fn dummy() {}" > scanner-wasm/src/lib.rs

# Build dependencies only (cached unless Cargo.toml changes)
RUN cargo build --release --package spinwin-server 2>/dev/null || true

# Copy actual source code
COPY core/ core/
COPY server/src/ server/src/

# Touch to invalidate cached build of our code (not deps)
RUN touch core/src/lib.rs server/src/main.rs

# Build the real binary
RUN cargo build --release --package spinwin-server

# Stage 2: Minimal runtime image
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the binary
COPY --from=builder /app/target/release/spinwin-server .

# Copy frontend files (HTML, JS, CSS, assets, WASM)
COPY server/frontend/ frontend/

# SQLite DB will live on the persistent volume at /data
ENV DATABASE_URL="sqlite:/data/spinwin.db?mode=rwc"
ENV BIND_ADDR="0.0.0.0:8080"

EXPOSE 8080

CMD ["./spinwin-server"]
