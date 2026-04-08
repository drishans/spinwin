"""Mystery Prize tests — verifies fallback behavior when all other prizes are exhausted.

Uses small stock quantities for speed. Spin now atomically creates tickets (merged spin+claim).
"""
import sys
import json
import urllib.request
import urllib.error

BASE = sys.argv[1] if len(sys.argv) > 1 else "http://localhost:3096"
PASSED = 0
FAILED = 0


def api_get(path):
    with urllib.request.urlopen(f"{BASE}{path}") as r:
        return json.loads(r.read())


def api_post(path, data):
    req = urllib.request.Request(
        f"{BASE}{path}",
        data=json.dumps(data).encode(),
        headers={"Content-Type": "application/json"},
    )
    try:
        with urllib.request.urlopen(req) as r:
            return r.status, json.loads(r.read())
    except urllib.error.HTTPError as e:
        body = e.read().decode()
        try:
            return e.code, json.loads(body)
        except json.JSONDecodeError:
            return e.code, {"error": body}


def assert_eq(desc, expected, actual):
    global PASSED, FAILED
    if str(expected) == str(actual):
        print(f"  PASS: {desc}")
        PASSED += 1
    else:
        print(f"  FAIL: {desc} (expected '{expected}', got '{actual}')")
        FAILED += 1


# ══════════════════════════════════════════════
print("── Test: Mystery Prize seeded correctly ──")
prizes = api_get("/api/prizes")
mystery = [p for p in prizes if p["name"] == "Mystery Prize"]
assert_eq("Mystery Prize exists", 1, len(mystery))
mystery_id = mystery[0]["id"]
mystery_stock = mystery[0]["remaining"]
assert_eq("Mystery Prize stock is 2 (small stock mode)", 2, mystery_stock)

# ══════════════════════════════════════════════
print()
print("── Test: Spin creates ticket with prize ──")
_, spin = api_post("/api/spin", {"email": "mystery_test@test.com"})
assert_eq("Spin returns ticket_id", True, "ticket_id" in spin)
assert_eq("Spin returns prize_name", True, "prize_name" in spin)
print(f"    (Won: {spin['prize_name']})")

# ══════════════════════════════════════════════
print()
print("── Test: Exhaust all prizes ──")
total_stock = sum(p["remaining"] for p in api_get("/api/prizes"))
claim_count = 0
for i in range(total_stock + 50):
    email = f"exhaust_{i}@test.com"
    status, data = api_post("/api/spin", {"email": email})
    if status == 200:
        claim_count += 1
    elif status == 410:
        remaining = sum(p["remaining"] for p in api_get("/api/prizes"))
        if remaining == 0:
            break

prizes_exhausted = api_get("/api/prizes")
all_remaining = sum(p["remaining"] for p in prizes_exhausted)
assert_eq("All prize stock is 0", 0, all_remaining)
print(f"    (Claimed {claim_count} prizes to exhaust stock)")

# ══════════════════════════════════════════════
print()
print("── Test: Spin falls back to Mystery Prize ──")
_, spin_fallback = api_post("/api/spin", {"email": "fallback1@test.com"})
assert_eq("Fallback spin returns Mystery Prize", "Mystery Prize", spin_fallback["prize"]["name"])
assert_eq("Fallback spin returns ticket", True, "ticket_id" in spin_fallback)

# ══════════════════════════════════════════════
print()
print("── Test: Unlimited Mystery Prize in fallback ──")
fallback_successes = 0
fallback_count = 5
for i in range(fallback_count):
    email = f"fallback_claim_{i}@test.com"
    status, spin_fb = api_post("/api/spin", {"email": email})
    assert_eq(f"Fallback spin {i+1} returns Mystery Prize", "Mystery Prize", spin_fb["prize"]["name"])
    if status == 200:
        fallback_successes += 1

assert_eq(f"All {fallback_count} fallback spins succeed", fallback_count, fallback_successes)

# Verify stock didn't go negative
prizes_final = api_get("/api/prizes")
mystery_final = [p for p in prizes_final if p["name"] == "Mystery Prize"][0]
assert_eq("Mystery Prize stock not negative", True, mystery_final["remaining"] >= 0)

# ── Results ──
print()
print("============================================")
print(f"  RESULTS: {PASSED} passed, {FAILED} failed")
print("============================================")
sys.exit(0 if FAILED == 0 else 1)
