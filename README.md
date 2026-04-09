# Spin & Win — WomenNowTV Event Giveaway

A prize wheel web app for a live event with 600-700 attendees. Attendees spin a wheel before the event, win a prize, and receive a cryptographically signed QR ticket (displayed on-screen and emailed) they present at the venue to claim it. The spin atomically selects a prize, decrements stock, creates the ticket, and sends the confirmation email — no separate claim step, no respin on refresh.

## Architecture

```
spinwin/
├── core/              # Shared Rust crate: Ed25519 signing, verification, ticket codec
├── server/            # Axum web server: API, SQLite DB, static file serving
├── scanner-wasm/      # WASM wrapper around core for staff scanner page
└── server/frontend/   # Static HTML/CSS/JS frontend
    ├── index.html     # Attendee: animated wheel → flip card (congrats → QR ticket) + email
    ├── scan.html      # Staff: camera QR scanner → verify → redeem
    ├── admin.html     # Admin: prize inventory, redemption stats, stock adjustment
    ├── wasm/          # Compiled WASM module (generated, gitignored)
    └── .env           # Environment variables (loaded via dotenvy from project root)
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
| Unregistered attendee | Email validated against a published Google Sheet (column B) before spin is allowed; attendee name pulled from column C (no manual name input) |
| Prize overselling | Atomic `UPDATE ... WHERE remaining > 0`, check affected rows |
| All prizes exhausted | Mystery Prize acts as unlimited fallback when all other prizes run out of stock |
| Forged QR code | Ed25519 signature — can't produce valid tickets without server's private key |
| Screenshot shared to friend | One-time redemption flag: second scan returns "already redeemed" |
| Tampered ticket data | Signature verification fails if any payload byte changes |
| Lost ticket | Ticket recovery: re-entering an existing email re-displays the QR and allows resending the confirmation email |

## Wheel Design

The wheel displays equal-sized segments for all 6 prizes (including Mystery Prize). Prize selection is handled entirely server-side using weighted random selection based on remaining stock — the wheel animation is cosmetic. The client cannot influence which prize is awarded. Prize images are JPG (except Mystery Prize which uses SVG).

## Setup

### Prerequisites
- Rust 1.70+ and cargo
- wasm-pack (`cargo install wasm-pack`)

### Build & Run

```bash
# Build WASM scanner module
wasm-pack build scanner-wasm --target web --out-dir ../server/frontend/wasm

# Run the server (dev mode — loads .env from project root via dotenvy)
cd server
cargo run

# Server starts at http://localhost:3000
# Staff scanner at http://localhost:3000/scan.html
```

### Run Tests

```bash
# Linux/macOS
cd tests && ./run_all.sh

# Windows
tests\windows\run_all.bat
```

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `SPINWIN_SIGNING_KEY` | dev key | 64-char hex string (32 bytes) for Ed25519 signing |
| `DATABASE_URL` | `sqlite:spinwin.db?mode=rwc` | SQLite connection string |
| `BIND_ADDR` | `0.0.0.0:3000` | Server bind address |
| `GOOGLE_SHEET_ID` | *(none)* | Published Google Sheet ID — column B emails and column C names are used for registration validation (cached as email→name HashMap with 5-min refresh) |
| `SMTP_EMAIL` | *(none)* | Gmail address for sending QR ticket confirmation emails |
| `SMTP_PASSWORD` | *(none)* | Gmail app password ([create one here](https://myaccount.google.com/apppasswords)) |
| `ADMIN_USER` | *(none)* | Username for admin dashboard Basic Auth (at `/admin`) |
| `ADMIN_PASSWORD` | *(none)* | Password for admin dashboard Basic Auth |
| `SPINWIN_SMALL_STOCK` | *(none)* | When set to `1`, seeds prizes with small stock quantities (used by mystery prize tests) |

Environment variables are loaded from a `.env` file in the project root via **dotenvy**. For production, generate a signing key:
```bash
openssl rand -hex 32
```

## Hosting (Fly.io)

The app is deployed on [Fly.io](https://fly.io) with two environments:

| Environment | Config | URL | Min machines |
|-------------|--------|-----|-------------|
| **Production** | `fly.toml` | https://spinwin.fly.dev | 1 (always on) |
| **Staging** | `fly.staging.toml` | https://spinwin-staging.fly.dev | 0 (sleeps when idle) |

### First-time setup

```bash
# Create production app and volume
fly launch --yes --no-deploy
fly volumes create spinwin_data --region sjc --size 1

# Create staging app and volume
fly launch --yes --no-deploy --config fly.staging.toml --name spinwin-staging
fly volumes create spinwin_staging_data --region sjc --size 1 --app spinwin-staging
```

### Set secrets

Secrets are set per-app and never stored in config files:

```bash
# Production
fly secrets set SPINWIN_SIGNING_KEY="<your-prod-key>" GOOGLE_SHEET_ID="<sheet-id>" SMTP_EMAIL="<gmail>" SMTP_PASSWORD="<app-password>" ADMIN_USER="<user>" ADMIN_PASSWORD="<pass>"

# Staging
fly secrets set SPINWIN_SIGNING_KEY="<your-test-key>" GOOGLE_SHEET_ID="<sheet-id>" SMTP_EMAIL="<gmail>" SMTP_PASSWORD="<app-password>" ADMIN_USER="<user>" ADMIN_PASSWORD="<pass>" --app spinwin-staging
```

### Deploy

```bash
# Deploy to production
fly deploy

# Deploy to staging
fly deploy --config fly.staging.toml
```

### Custom domain

```bash
fly certs add yourdomain.com
# Then add a CNAME record: yourdomain.com → spinwin.fly.dev
```

### Useful commands

```bash
fly logs                          # live production logs
fly logs --app spinwin-staging    # live staging logs
fly status                        # machine health
fly ssh console                   # SSH into the container
fly secrets list                  # see which secrets are set
```

### How it works

- **Dockerfile**: Two-stage build — compiles Rust in a full image, copies the binary + frontend into a ~32MB runtime image. `Cargo.lock` is included in the build for reproducible builds.
- **Remote builds**: Fly uses [Depot](https://depot.dev) for remote Docker builds — no local Docker installation needed.
- **Persistent volume**: SQLite DB lives at `/data/spinwin.db` on a mounted volume that survives deploys and restarts.
- **HTTPS**: Enforced automatically, free TLS certificate included.
- **Auto-scaling**: Production keeps 1 machine always running (no cold starts). Staging sleeps when idle to save costs.

## Stretch Goals

- [ ] Apple Wallet `.pkpass` ticket generation (endpoint stubbed)
- [x] Admin dashboard showing prize inventory, redemption stats, recent tickets, and stock adjustment (`/admin`)
- [x] Email confirmation with ticket QR attached (Gmail SMTP via `SMTP_EMAIL` / `SMTP_PASSWORD`)
