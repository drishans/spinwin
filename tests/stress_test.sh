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
DATABASE_URL="sqlite:$DB_FILE?mode=rwc" BIND_ADDR="127.0.0.1:$PORT" "$BINARY" &
SERVER_PID=$!
sleep 2

if ! kill -0 "$SERVER_PID" 2>/dev/null; then
    echo "  FAIL: Server failed to start"
    exit 1
fi
echo "  Server running on port $PORT (PID $SERVER_PID)"
echo ""

# Verify initial stock
BANGLES_STOCK=$(curl -s "$BASE/api/prizes" | python3 -c "
import sys,json
for p in json.load(sys.stdin):
    if p['name'] == 'Bangles':
        print(p['remaining'])
")
echo "  Initial Bangles stock: $BANGLES_STOCK"
echo ""

# ── Stress test: 100 concurrent claims against Bangles (id=5, stock=50) ──
echo "  Launching 100 concurrent claims (50 workers)..."
echo ""

python3 -c "
import subprocess, concurrent.futures, json, sys

BASE = '$BASE'
results = {'claimed': 0, 'rejected_stock': 0, 'rejected_dupe': 0, 'errors': 0}

def claim(i):
    try:
        r = subprocess.run(
            ['curl', '-s', '-X', 'POST', f'{BASE}/api/claim',
             '-H', 'Content-Type: application/json',
             '-d', json.dumps({'name': f'Stress User {i}', 'email': f'stress{i}@test.com', 'prize_id': 5})],
            capture_output=True, text=True, timeout=10
        )
        data = json.loads(r.stdout)
        if 'ticket_id' in data:
            return 'claimed'
        elif 'no longer available' in data.get('error', ''):
            return 'rejected_stock'
        elif 'already been used' in data.get('error', ''):
            return 'rejected_dupe'
        else:
            return f'unexpected: {data}'
    except Exception as e:
        return f'error: {e}'

with concurrent.futures.ThreadPoolExecutor(max_workers=50) as pool:
    all_results = list(pool.map(claim, range(100)))

claimed = all_results.count('claimed')
rejected_stock = all_results.count('rejected_stock')
rejected_dupe = all_results.count('rejected_dupe')
unexpected = [r for r in all_results if r.startswith('unexpected')]
errors = [r for r in all_results if r.startswith('error')]

print(f'  Claimed:              {claimed}')
print(f'  Rejected (no stock):  {rejected_stock}')
print(f'  Rejected (dupe):      {rejected_dupe}')
print(f'  Unexpected:           {len(unexpected)}')
print(f'  Errors:               {len(errors)}')
print(f'  Total:                {len(all_results)}')
print()

failed = False

if claimed > 50:
    print(f'  FAIL: OVERSOLD by {claimed - 50}!')
    failed = True
elif claimed == 50:
    print(f'  PASS: Exactly 50 claimed — no overselling')
else:
    print(f'  PASS: {claimed} claimed (<=50) — no overselling')

if claimed + rejected_stock != 100:
    print(f'  FAIL: claimed + rejected_stock != 100 ({claimed} + {rejected_stock})')
    failed = True
else:
    print(f'  PASS: All 100 requests accounted for')

if len(errors) > 0:
    print(f'  FAIL: {len(errors)} errors:')
    for e in errors[:5]:
        print(f'    {e}')
    failed = True
else:
    print(f'  PASS: Zero errors')

if len(unexpected) > 0:
    print(f'  FAIL: {len(unexpected)} unexpected responses:')
    for u in unexpected[:5]:
        print(f'    {u}')
    failed = True

sys.exit(1 if failed else 0)
"

STRESS_RESULT=$?

echo ""

# Verify final stock from API
echo "── Verify final state ──"
FINAL_STOCK=$(curl -s "$BASE/api/prizes" | python3 -c "
import sys,json
for p in json.load(sys.stdin):
    if p['name'] == 'Bangles':
        print(p['remaining'])
")
echo "  Final Bangles stock: $FINAL_STOCK"

if [ "$FINAL_STOCK" -eq 0 ]; then
    echo "  PASS: Stock is exactly 0"
else
    echo "  FAIL: Stock should be 0, got $FINAL_STOCK"
    STRESS_RESULT=1
fi

# Verify other prizes untouched
OTHERS_OK=$(curl -s "$BASE/api/prizes" | python3 -c "
import sys,json
ok = True
for p in json.load(sys.stdin):
    if p['name'] != 'Bangles' and p['remaining'] != p['total_qty']:
        print(f\"  FAIL: {p['name']} stock changed unexpectedly: {p['remaining']}/{p['total_qty']}\")
        ok = False
if ok:
    print('  PASS: Other prizes untouched')
")
echo "$OTHERS_OK"

echo ""
echo "============================================"
if [ "$STRESS_RESULT" -eq 0 ]; then
    echo "  STRESS TEST PASSED"
else
    echo "  STRESS TEST FAILED"
fi
echo "============================================"

exit $STRESS_RESULT
