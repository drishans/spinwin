# Spin & Win — WomenNowTV Event Giveaway

A prize wheel web app for a live event with 600-700 attendees. Attendees spin a wheel before the event, win a prize, and receive a cryptographically signed QR ticket they present at the venue to claim it.

## Architecture

```
spinwin/
├── core/              # Shared Rust crate: Ed25519 signing, verification, ticket codec
├── server/            # Axum web server: API, SQLite DB, static file serving
├── scanner-wasm/      # WASM wrapper around core for staff scanner page
└── server/frontend/   # Static HTML/CSS/JS frontend
    ├── index.html     # Attendee: animated wheel → claim form → QR ticket
    ├── scan.html      # Staff: camera QR scanner → verify → redeem
    └── wasm/          # Compiled WASM module (generated, gitignored)
```

## Why Rust?

This project uses Rust in three meaningful ways — not as a wrapper, but where it genuinely outperforms alternatives:

### 1. Shared cryptographic core (`core/`)
A single Rust crate handles Ed25519 ticket signing and verification. It compiles to **two targets**:
- **Native** (used by the Axum server to sign tickets)
- **WebAssembly** (used by the staff scanner to verify signatures client-side)

One implementation, two runtimes, zero divergence. A JavaScript implementation would require maintaining two separate crypto libraries (Node.js and browser) with subtle compatibility risks.

### 2. Axum API server (`server/`)
The server handles atomic prize allocation under concurrent access using SQLite's `BEGIN IMMEDIATE` semantics. Rust's type system ensures every database result is checked — the "forgot to check rows_affected" class of bugs that plagues dynamic languages is a compile error here.

### 3. Offline-capable staff verification (`scanner-wasm/`)
The WASM scanner verifies ticket signatures **entirely client-side**. At a venue with unreliable Wi-Fi, staff get instant green/red feedback without a network round-trip. The server is only needed for the final redemption step. This is only practical because the same Rust verification code compiles to WASM — reimplementing Ed25519 verification in JavaScript would be error-prone and unauditable.

## Anti-Fraud Design

| Threat | Mitigation |
|--------|------------|
| Same person spins twice | `UNIQUE(email)` constraint on tickets table |
| Prize overselling | Atomic `UPDATE ... WHERE remaining > 0`, check affected rows |
| Forged QR code | Ed25519 signature — can't produce valid tickets without server's private key |
| Screenshot shared to friend | One-time redemption flag: second scan returns "already redeemed" |
| Tampered ticket data | Signature verification fails if any payload byte changes |

## Wheel Modes

Three wheel visualization modes (selectable via UI toggle, useful for A/B testing):

1. **Dynamic Segments** — Segment sizes proportional to remaining stock. Visually honest: what you see reflects actual odds. Segments resize as prizes are claimed.
2. **Equal (Weighted)** — 5 equal visual segments. Server-side weighted random selection. Visually clean but hides actual probabilities.
3. **Fixed Proportional** — Segments proportional to original total stock. Never changes visually, regardless of remaining inventory.

All three modes use server-side prize selection — the wheel animation is cosmetic. The client cannot influence which prize is awarded.

## Setup

### Prerequisites
- Rust 1.70+ and cargo
- wasm-pack (`cargo install wasm-pack`)

### Build & Run

```bash
# Build WASM scanner module
wasm-pack build scanner-wasm --target web --out-dir ../server/frontend/wasm

# Run the server (dev mode)
cd server
cargo run

# Server starts at http://localhost:3000
# Staff scanner at http://localhost:3000/scan.html
```

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `SPINWIN_SIGNING_KEY` | dev key | 64-char hex string (32 bytes) for Ed25519 signing |
| `DATABASE_URL` | `sqlite:spinwin.db?mode=rwc` | SQLite connection string |
| `BIND_ADDR` | `0.0.0.0:3000` | Server bind address |

For production, generate a signing key:
```bash
openssl rand -hex 32
```

## Hosting

**Recommended: Shuttle.rs** (free tier)
- Rust-native deployment (`cargo shuttle deploy`)
- Built-in persistent SQLite storage
- Custom domains, HTTPS included
- No Docker/Dockerfile needed

**Fallback: Fly.io** (free tier)
- Requires a minimal Dockerfile
- Persistent volumes for SQLite

## Stretch Goals

- [ ] Apple Wallet `.pkpass` ticket generation (endpoint stubbed)
- [ ] Admin dashboard showing prize inventory and redemption stats
- [ ] Email confirmation with ticket QR attached
