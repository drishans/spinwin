#!/usr/bin/env bash
# Run all Spin & Win tests
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
TOTAL_PASSED=0
TOTAL_FAILED=0

echo ""
echo "╔════════════════════════════════════════════╗"
echo "║       SPIN & WIN — FULL TEST SUITE         ║"
echo "╚════════════════════════════════════════════╝"
echo ""

run_test() {
    local name="$1" script="$2"
    echo "▶ Running: $name"
    echo ""
    if bash "$script"; then
        TOTAL_PASSED=$((TOTAL_PASSED + 1))
        echo ""
    else
        TOTAL_FAILED=$((TOTAL_FAILED + 1))
        echo ""
        echo "  ⛔ $name FAILED"
        echo ""
    fi
}

run_test "Core Crypto Tests" "$SCRIPT_DIR/core_crypto_test.sh"
run_test "API Integration Tests" "$SCRIPT_DIR/api_integration_test.sh"
run_test "Concurrent Stress Test" "$SCRIPT_DIR/stress_test.sh"

echo "╔════════════════════════════════════════════╗"
echo "  Suites passed: $TOTAL_PASSED"
echo "  Suites failed: $TOTAL_FAILED"
if [ "$TOTAL_FAILED" -eq 0 ]; then
    echo "  ALL TESTS PASSED"
else
    echo "  SOME TESTS FAILED"
fi
echo "╚════════════════════════════════════════════╝"

[ "$TOTAL_FAILED" -eq 0 ] || exit 1
