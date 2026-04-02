#!/usr/bin/env bash
# Core cryptographic tests — runs Rust unit tests in the core crate
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

echo "============================================"
echo "  CORE CRYPTO TESTS"
echo "============================================"
echo ""
echo "  Testing: Ed25519 signing, verification,"
echo "  tamper detection, wrong-key rejection,"
echo "  key serialization round-trip"
echo ""

cd "$PROJECT_DIR"
source "$HOME/.cargo/env"

cargo test -p spinwin-core -- --nocapture 2>&1

echo ""
echo "  All core crypto tests passed."
