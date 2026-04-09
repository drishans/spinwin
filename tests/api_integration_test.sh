#!/usr/bin/env bash
# API integration tests — starts a fresh server, tests full flows, tears down
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
SERVER_DIR="$PROJECT_DIR/server"
BINARY="$PROJECT_DIR/target/release/spinwin-server"
DB_FILE="$SERVER_DIR/test_integration.db"
PORT=3099
BASE="http://localhost:$PORT"
PASSED=0
FAILED=0

cleanup() {
    kill "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
    rm -f "$DB_FILE"
}
trap cleanup EXIT

assert_eq() {
    local desc="$1" expected="$2" actual="$3"
    if [ "$expected" = "$actual" ]; then
        echo "  PASS: $desc"
        PASSED=$((PASSED + 1))
    else
        echo "  FAIL: $desc (expected '$expected', got '$actual')"
        FAILED=$((FAILED + 1))
    fi
}

assert_contains() {
    local desc="$1" needle="$2" haystack="$3"
    if echo "$haystack" | grep -q "$needle"; then
        echo "  PASS: $desc"
        PASSED=$((PASSED + 1))
    else
        echo "  FAIL: $desc (expected to contain '$needle')"
        FAILED=$((FAILED + 1))
    fi
}

echo "============================================"
echo "  API INTEGRATION TESTS"
echo "============================================"
echo ""

# Build
source "$HOME/.cargo/env"
echo "  Building release binary..."
cargo build --release --manifest-path "$PROJECT_DIR/Cargo.toml" 2>&1 | tail -1

# Start server
rm -f "$DB_FILE"
DATABASE_URL="sqlite:$DB_FILE?mode=rwc" BIND_ADDR="127.0.0.1:$PORT" GOOGLE_SHEET_ID="" SMTP_EMAIL="" SMTP_PASSWORD="" "$BINARY" &
SERVER_PID=$!
sleep 2

if ! kill -0 "$SERVER_PID" 2>/dev/null; then
    echo "  FAIL: Server failed to start"
    exit 1
fi
echo "  Server running on port $PORT (PID $SERVER_PID)"
echo ""

# ──────────────────────────────────────────────
echo "── Test: Prizes are seeded correctly ──"
PRIZES=$(curl -s "$BASE/api/prizes")
PRIZE_COUNT=$(echo "$PRIZES" | python3 -c "import sys,json; print(len(json.load(sys.stdin)))")
TOTAL_STOCK=$(echo "$PRIZES" | python3 -c "import sys,json; print(sum(p['remaining'] for p in json.load(sys.stdin)))")
assert_eq "6 prizes seeded" "6" "$PRIZE_COUNT"
assert_eq "Total stock is 500" "500" "$TOTAL_STOCK"

# ──────────────────────────────────────────────
echo ""
echo "── Test: Spin creates ticket atomically ──"
SPIN=$(curl -s -X POST "$BASE/api/spin" -H 'Content-Type: application/json' -d '{"email":"alice@test.com"}')
HAS_TICKET=$(echo "$SPIN" | python3 -c "import sys,json; d=json.load(sys.stdin); print('yes' if 'ticket_id' in d else 'no')")
HAS_QR=$(echo "$SPIN" | python3 -c "import sys,json; d=json.load(sys.stdin); print('yes' if 'qr_data' in d else 'no')")
HAS_ANGLE=$(echo "$SPIN" | python3 -c "import sys,json; d=json.load(sys.stdin); print('yes' if d['angle'] > 360 else 'no')")
SPIN_PRIZE_NAME=$(echo "$SPIN" | python3 -c "import sys,json; print(json.load(sys.stdin)['prize_name'])")
assert_eq "Spin returns ticket_id" "yes" "$HAS_TICKET"
assert_eq "Spin returns qr_data" "yes" "$HAS_QR"
assert_eq "Spin returns angle with full rotations" "yes" "$HAS_ANGLE"
echo "    (Won: $SPIN_PRIZE_NAME)"

QR_TOKEN=$(echo "$SPIN" | python3 -c "import sys,json; print(json.load(sys.stdin)['qr_data'])")

# ──────────────────────────────────────────────
echo ""
echo "── Test: Duplicate email rejection ──"
DUPE_SPIN=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$BASE/api/spin" \
    -H 'Content-Type: application/json' \
    -d '{"email":"alice@test.com"}')
assert_eq "Duplicate email spin returns 409" "409" "$DUPE_SPIN"

BOB_SPIN_STATUS=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$BASE/api/spin" \
    -H 'Content-Type: application/json' \
    -d '{"email":"bob@test.com"}')
assert_eq "Different email succeeds (200)" "200" "$BOB_SPIN_STATUS"

# ──────────────────────────────────────────────
echo ""
echo "── Test: Ticket verification ──"
VERIFY=$(curl -s "$BASE/api/verify/$QR_TOKEN")
VALID=$(echo "$VERIFY" | python3 -c "import sys,json; print(json.load(sys.stdin)['valid'])")
REDEEMED=$(echo "$VERIFY" | python3 -c "import sys,json; print(json.load(sys.stdin)['redeemed'])")
assert_eq "Valid ticket verifies as valid" "True" "$VALID"
assert_eq "Ticket not yet redeemed" "False" "$REDEEMED"

# ──────────────────────────────────────────────
echo ""
echo "── Test: Redemption flow ──"
REDEEM1=$(curl -s -X POST "$BASE/api/redeem/$QR_TOKEN")
SUCCESS1=$(echo "$REDEEM1" | python3 -c "import sys,json; print(json.load(sys.stdin)['success'])")
assert_eq "First redemption succeeds" "True" "$SUCCESS1"

REDEEM2=$(curl -s -X POST "$BASE/api/redeem/$QR_TOKEN")
SUCCESS2=$(echo "$REDEEM2" | python3 -c "import sys,json; print(json.load(sys.stdin)['success'])")
MSG2=$(echo "$REDEEM2" | python3 -c "import sys,json; print(json.load(sys.stdin)['message'])")
assert_eq "Second redemption fails" "False" "$SUCCESS2"
assert_contains "Reports already redeemed" "already redeemed" "$MSG2"

VERIFY_AFTER=$(curl -s "$BASE/api/verify/$QR_TOKEN")
REDEEMED_AFTER=$(echo "$VERIFY_AFTER" | python3 -c "import sys,json; print(json.load(sys.stdin)['redeemed'])")
assert_eq "Verify shows redeemed after redemption" "True" "$REDEEMED_AFTER"

# ──────────────────────────────────────────────
echo ""
echo "── Test: Invalid token rejection ──"
INVALID_VERIFY=$(curl -s "$BASE/api/verify/totally-invalid-token")
INVALID_VALID=$(echo "$INVALID_VERIFY" | python3 -c "import sys,json; print(json.load(sys.stdin)['valid'])")
assert_eq "Invalid token rejected" "False" "$INVALID_VALID"

INVALID_REDEEM_STATUS=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$BASE/api/redeem/totally-invalid-token")
assert_eq "Invalid token redeem returns 400" "400" "$INVALID_REDEEM_STATUS"

# ──────────────────────────────────────────────
echo ""
echo "── Test: Stock decremented correctly ──"
PRIZES_AFTER=$(curl -s "$BASE/api/prizes")
TOTAL_DEC=$(python3 -c "
import json
before = json.loads('''$PRIZES''')
after = json.loads('''$PRIZES_AFTER''')
before_map = {p['id']: p['remaining'] for p in before}
after_map = {p['id']: p['remaining'] for p in after}
total_decremented = sum(before_map[pid] - after_map[pid] for pid in before_map)
print(total_decremented)
")
# We claimed 2 tickets (alice and bob)
assert_eq "Total stock decremented by 2" "2" "$TOTAL_DEC"

# ──────────────────────────────────────────────
echo ""
echo "============================================"
echo "  RESULTS: $PASSED passed, $FAILED failed"
echo "============================================"

[ "$FAILED" -eq 0 ] || exit 1
