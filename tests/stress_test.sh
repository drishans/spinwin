#!/usr/bin/env bash
# Stress test — verifies no overselling under concurrent load
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
SERVER_DIR="$PROJECT_DIR/server"
BINARY="$PROJECT_DIR/target/release/spinwin-server"
DB_FILE="$SERVER_DIR/test_stress.db"
PORT=3098
BASE="http://localhost:$PORT"

cleanup() {
    kill "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
    rm -f "$DB_FILE"
}
trap cleanup EXIT

echo "============================================"
echo "  CONCURRENT STRESS TEST"
echo "============================================"
echo ""
echo "  Scenario: 100 concurrent claims targeting"
echo "  a single prize with 50 stock. Verifies the"
echo "  atomic decrement prevents overselling."
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

python3 "$(dirname "$0")/windows/stress_test.py" "$BASE"
exit $?
