# Stage 1: Build the Rust binary
FROM rust:1.86-slim AS builder

WORKDIR /app

# Install build dependencies (curl is needed to install wasm-pack)
RUN apt-get update && apt-get install -y pkg-config libssl-dev curl && rm -rf /var/lib/apt/lists/*

# Copy workspace manifests and lockfile first for dependency caching
COPY Cargo.toml Cargo.lock ./
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

# Copy actual source code and frontend (needed for include_str! at compile time)
COPY core/ core/
COPY server/src/ server/src/
COPY server/frontend/ server/frontend/
COPY scanner-wasm/ scanner-wasm/

# Touch to invalidate cached build of our code (not deps)
RUN touch core/src/lib.rs server/src/main.rs

# Build the real binary
RUN cargo build --release --package spinwin-server

# Build the WASM scanner module so /wasm/* is available for client-side
# verification. Without this the scanner page can only use server-side verify.
RUN curl -sSf https://rustwasm.github.io/wasm-pack/installer/init.sh | sh && \
    wasm-pack build scanner-wasm --target web --out-dir ../server/frontend/wasm

# Stage 2: Minimal runtime image
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the binary
COPY --from=builder /app/target/release/spinwin-server .

# Copy frontend files (HTML, JS, CSS, assets) plus the WASM built in the builder
# stage. Copy from the builder so the generated /wasm/* artifacts are included.
COPY --from=builder /app/server/frontend/ frontend/

# SQLite DB will live on the persistent volume at /data
ENV DATABASE_URL="sqlite:/data/spinwin.db?mode=rwc"
ENV BIND_ADDR="0.0.0.0:8080"

EXPOSE 8080

CMD ["./spinwin-server"]
