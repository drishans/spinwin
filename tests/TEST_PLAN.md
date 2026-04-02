# Spin & Win — Test Plan

## What we're testing and why

This app handles real inventory (physical prizes) and real people (600-700 attendees). Bugs here aren't just UI glitches — they mean someone shows up at an event and can't get their prize, or we give away 60 necklaces when we only have 50. Every test below maps to a concrete failure mode.

---

### 1. Cryptographic Ticket Integrity

**What:** Ed25519 signing and verification of ticket tokens.

**Why:** The QR code is the only proof an attendee has. If we can't verify it reliably, either legitimate tickets get rejected (attendee is upset) or forged tickets get accepted (inventory loss). These tests validate:

- A signed ticket verifies correctly (happy path)
- A tampered token is rejected (someone edits the base64)
- A token signed with the wrong key is rejected (attacker generates their own keypair)
- Key serialization round-trips correctly (important for WASM — the public key is sent to the browser as base64)

**Test file:** `core_crypto_test.sh`

---

### 2. One Spin Per Email

**What:** The UNIQUE constraint on email prevents duplicate claims.

**Why:** Without this, one person could spin repeatedly and hoard prizes. We test:

- First claim succeeds
- Second claim with the same email returns 409 Conflict
- Different email works fine (not over-restricting)

**Test file:** `api_integration_test.sh`

---

### 3. Prize Stock Never Goes Negative (Concurrent Stress Test)

**What:** 100 simultaneous claims against a prize with 50 stock.

**Why:** This is the most critical invariant. If the atomic decrement fails under concurrency, we oversell physical inventory. The event team orders based on these numbers — overselling means someone wins a prize that doesn't exist. We test:

- Exactly N claims succeed when stock is N (not N-1, not N+1)
- All excess claims are cleanly rejected
- Final `remaining` count in the DB matches expectations
- Zero errors under load (no 500s, no deadlocks)

**Test file:** `stress_test.sh`

---

### 4. Redemption Flow (Single-Use Tickets)

**What:** A ticket can only be redeemed once.

**Why:** If screenshots of QR codes work, one winning ticket becomes unlimited prizes. We test:

- A valid ticket verifies with `redeemed: false`
- First redemption succeeds
- Second redemption returns `success: false, "Ticket already redeemed"`
- Post-redemption verification shows `redeemed: true`
- Invalid tokens are rejected outright

**Test file:** `api_integration_test.sh`

---

### 5. Wheel-Prize Alignment

**What:** The angle returned by `/api/spin` lands within the correct prize's segment.

**Why:** If the visual wheel lands on "Ring" but the modal says "Bangles", user trust is destroyed. The server computes the landing angle — we verify the math is correct by checking the angle falls within the selected prize's segment arc.

**Test file:** `api_integration_test.sh`

---

## Running the tests

```bash
# Run all tests
cd spinwin/tests
./run_all.sh

# Run individually
./core_crypto_test.sh      # Unit tests (Rust, fast)
./api_integration_test.sh  # API flow tests (starts server)
./stress_test.sh           # Concurrent load test (starts server)
```

All integration and stress tests start a fresh server with a clean DB and tear it down after. No external dependencies required.
