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
| `GOOGLE_SHEET_ID` | *(none)* | Google Sheet ID for registered email validation (column B = email, column C = name) |
| `SMTP_EMAIL` | *(none)* | Gmail address for sending ticket emails |
| `SMTP_PASSWORD` | *(none)* | Gmail app password (requires 2FA enabled) |
| `ADMIN_USER` | *(none)* | Username for admin dashboard Basic Auth |
| `ADMIN_PASSWORD` | *(none)* | Password for admin dashboard Basic Auth |
| `SPINWIN_SMALL_STOCK` | *(none)* | When set to `1`, seeds prizes with small quantities (test mode) |

Generate a production key: `openssl rand -hex 32`

## Architecture

**Workspace crates:**
- **core/** — Shared Ed25519 signing/verification and ticket codec. Compiles to both native (server) and WASM (scanner). Single crypto implementation, two runtimes.
- **server/** — Axum web server with SQLite (via sqlx). Serves API + static frontend files from `server/frontend/`.
- **scanner-wasm/** — Thin WASM wrapper around core, exports `verify_ticket_wasm()` for browser-side verification.

**Frontend (server/frontend/):**
- `index.html` — Attendee page: email gate → spin wheel (atomically creates ticket) → QR ticket display
- `scan.html` — Staff scanner: camera QR scan → WASM signature verify (offline capable) → server redeem

**API endpoints (server/src/main.rs):**
- `GET /api/prizes` — List prizes with stock
- `GET /api/check-email/{email}` — Pre-spin duplicate check + returns attendee name from Google Sheet
- `POST /api/spin` — Atomic spin+claim: selects prize, decrements stock, creates signed ticket, sends email, returns prize + angle + ticket data
- `POST /api/resend/{email}` — Resend ticket confirmation email
- `GET /admin` — Admin dashboard (protected by Basic Auth via `ADMIN_USER`/`ADMIN_PASSWORD`)
- `GET /api/admin/stats` — Prize inventory + redemption stats (Basic Auth)
- `POST /api/admin/prizes/{id}/stock` — Adjust prize stock (Basic Auth)
- `GET /api/admin/tickets` — Recent tickets list (Basic Auth)
- `GET /api/verify/{token}` — Verify ticket signature
- `POST /api/redeem/{token}` — One-time redemption (sets redeemed=true)
- `GET /api/public-key` — Public key for client-side WASM verification

**Key design decisions:**
- Spin and claim are merged into a single atomic operation — no window for users to refresh and respin
- Attendee names come from Google Sheet (column C), no manual name input needed
- Prize selection is server-side only; wheel animation is cosmetic (client cannot influence outcome)
- Stock management uses atomic `UPDATE ... WHERE remaining > 0` with row count check to prevent overselling
- Email uniqueness enforced at DB level (`UNIQUE` constraint on tickets.email)
- Two-tier QR verification: instant client-side WASM check (works offline) + server-side redemption status check
- Tickets are Ed25519-signed — forgery requires the server's private key

## Database Schema

Two tables: `prizes` (id, name, image_url, total_qty, remaining) and `tickets` (id UUID, email UNIQUE, name, prize_id, token, redeemed, created_at). Prizes are seeded on first run (500 total across 6 prize types including Mystery Prize as the unlimited fallback).

## Test Strategy

Tests are bash-based integration tests that spin up a fresh server instance with a clean database. Tests override `GOOGLE_SHEET_ID=none` and unset SMTP to avoid external dependencies. The stress test validates concurrent spin safety (100 simultaneous requests, no overselling). The mystery prize test uses `SPINWIN_SMALL_STOCK=1` for reduced stock quantities. Core crate has standard Rust unit tests for crypto operations.

## Workflow

When committing changes, spawn a subagent to review what changed and update README.md to reflect the current state of the project.
