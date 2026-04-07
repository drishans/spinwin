#!/usr/bin/env bash
# Mystery Prize test — verifies fallback behavior when prizes are exhausted
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
SERVER_DIR="$PROJECT_DIR/server"
BINARY="$PROJECT_DIR/target/release/spinwin-server"
DB_FILE="$SERVER_DIR/test_mystery.db"
PORT=3096
BASE="http://localhost:$PORT"

cleanup() {
    kill "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
    rm -f "$DB_FILE"
}
trap cleanup EXIT

echo "============================================"
echo "  MYSTERY PRIZE TEST"
echo "============================================"
echo ""
echo "  Verifies Mystery Prize fallback behavior"
echo "  when all other prizes are exhausted."
echo ""

# Build
source "$HOME/.cargo/env"
echo "  Building release binary..."
cargo build --release --manifest-path "$PROJECT_DIR/Cargo.toml" 2>&1 | tail -1

# Start server
rm -f "$DB_FILE"
DATABASE_URL="sqlite:$DB_FILE?mode=rwc" BIND_ADDR="127.0.0.1:$PORT" GOOGLE_SHEET_ID="" SMTP_EMAIL="" SMTP_PASSWORD="" SPINWIN_SMALL_STOCK=1 "$BINARY" &
SERVER_PID=$!
sleep 2

if ! kill -0 "$SERVER_PID" 2>/dev/null; then
    echo "  FAIL: Server failed to start"
    exit 1
fi
echo "  Server running on port $PORT (PID $SERVER_PID)"
echo ""

python3 "$(dirname "$0")/windows/mystery_prize_test.py" "$BASE"
exit $?
