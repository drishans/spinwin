# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Spin & Win is a prize wheel web app for a WomenNow live event (~600-700 attendees). Attendees spin a wheel online, win a prize, and receive a cryptographically signed QR ticket to present at the venue. Staff scan QR codes to verify and redeem tickets.

**Stack:** Rust workspace (Axum server + SQLite) with vanilla JS frontend and WASM for client-side ticket verification.

## Build & Run Commands

```bash
# Build and run the server (starts at http://localhost:3000)
cd server && cargo run

# Build WASM scanner module (must run before scan.html works)
wasm-pack build scanner-wasm --target web --out-dir ../server/frontend/wasm

# Run all tests (Linux/macOS)
cd tests && ./run_all.sh

# Run all tests (Windows)
tests\windows\run_all.bat

# Run individual test suites (Linux/macOS)
./tests/core_crypto_test.sh       # Ed25519 unit tests (cargo test in core/)
./tests/api_integration_test.sh   # Full API flow (starts fresh server + clean DB)
./tests/prize_alignment_test.sh   # Wheel-prize visual alignment across all modes
./tests/stress_test.sh            # 100 concurrent claims against 50-stock prize

# Run individual test suites (Windows)
tests\windows\core_crypto_test.bat
tests\windows\api_integration_test.bat
tests\windows\prize_alignment_test.bat
tests\windows\stress_test.bat

# Run core Rust unit tests directly
cd core && cargo test
```

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `SPINWIN_SIGNING_KEY` | Dev key | 64-char hex string for Ed25519 signing |
| `DATABASE_URL` | `sqlite:spinwin.db?mode=rwc` | SQLite connection |
| `BIND_ADDR` | `0.0.0.0:3000` | Server bind address |
| `GOOGLE_SHEET_ID` | *(none)* | Google Sheet ID for registered email validation (publish sheet with "anyone with link") |
| `SMTP_EMAIL` | *(none)* | Gmail address for sending ticket emails |
| `SMTP_PASSWORD` | *(none)* | Gmail app password (requires 2FA enabled) |

Generate a production key: `openssl rand -hex 32`

## Architecture

**Workspace crates:**
- **core/** — Shared Ed25519 signing/verification and ticket codec. Compiles to both native (server) and WASM (scanner). Single crypto implementation, two runtimes.
- **server/** — Axum web server with SQLite (via sqlx). Serves API + static frontend files from `server/frontend/`.
- **scanner-wasm/** — Thin WASM wrapper around core, exports `verify_ticket_wasm()` for browser-side verification.

**Frontend (server/frontend/):**
- `index.html` — Attendee page: email gate → spin wheel → claim prize → QR ticket display
- `scan.html` — Staff scanner: camera QR scan → WASM signature verify (offline capable) → server redeem

**API endpoints (server/src/main.rs):**
- `GET /api/prizes` — List prizes with stock
- `GET /api/check-email/{email}` — Pre-spin duplicate check
- `POST /api/spin` — Server selects prize by weighted random (remaining stock), returns prize + landing angle
- `POST /api/claim` — Creates signed ticket, returns QR data
- `GET /api/verify/{token}` — Verify ticket signature
- `POST /api/redeem/{token}` — One-time redemption (sets redeemed=true)
- `GET /api/public-key` — Public key for client-side WASM verification

**Key design decisions:**
- Prize selection is server-side only; wheel animation is cosmetic (client cannot influence outcome)
- Stock management uses atomic `UPDATE ... WHERE remaining > 0` with row count check to prevent overselling
- Email uniqueness enforced at DB level (`UNIQUE` constraint on tickets.email)
- Two-tier QR verification: instant client-side WASM check (works offline) + server-side redemption status check
- Tickets are Ed25519-signed — forgery requires the server's private key

## Database Schema

Two tables: `prizes` (id, name, image_url, total_qty, remaining) and `tickets` (id UUID, email UNIQUE, name, prize_id, token, redeemed, created_at). Prizes are seeded on first run (450 total across 5 prize types).

## Test Strategy

Tests are bash-based integration tests that spin up a fresh server instance with a clean database. The stress test validates concurrent claim safety (100 simultaneous requests, expects exactly 50 successes matching stock). Core crate has standard Rust unit tests for crypto operations.

## Workflow

When committing changes, spawn a subagent to review what changed and update README.md to reflect the current state of the project.
