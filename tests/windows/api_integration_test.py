"""API integration tests — called by api_integration_test.bat
Spin now atomically creates the ticket (merged spin+claim).
"""
import sys
import json
import urllib.request
import urllib.error

BASE = sys.argv[1] if len(sys.argv) > 1 else "http://localhost:3099"
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


def api_post_status(path, data):
    code, _ = api_post(path, data)
    return str(code)


def assert_eq(desc, expected, actual):
    global PASSED, FAILED
    if str(expected) == str(actual):
        print(f"  PASS: {desc}")
        PASSED += 1
    else:
        print(f"  FAIL: {desc} (expected '{expected}', got '{actual}')")
        FAILED += 1


def assert_contains(desc, needle, haystack):
    global PASSED, FAILED
    if needle in str(haystack):
        print(f"  PASS: {desc}")
        PASSED += 1
    else:
        print(f"  FAIL: {desc} (expected to contain '{needle}')")
        FAILED += 1


# ── Prizes seeded correctly ──
print("── Test: Prizes are seeded correctly ──")
prizes = api_get("/api/prizes")
assert_eq("6 prizes seeded", 6, len(prizes))
total_stock = sum(p["remaining"] for p in prizes)
assert_eq("Total stock is 460", 460, total_stock)
mystery = [p for p in prizes if p["name"] == "Mystery Prize"]
assert_eq("Mystery Prize exists", 1, len(mystery))
assert_eq("Mystery Prize initial stock is 10", 10, mystery[0]["remaining"])

# ── Spin creates ticket atomically ──
print()
print("── Test: Spin creates ticket atomically ──")
_, spin = api_post("/api/spin", {"email": "alice@test.com"})
assert_eq("Spin returns ticket_id", True, "ticket_id" in spin)
assert_eq("Spin returns qr_data", True, "qr_data" in spin)
assert_eq("Spin returns prize_name", True, "prize_name" in spin)
assert_eq("Spin returns attendee_name", True, "attendee_name" in spin)
assert_eq("Spin returns angle with full rotations", True, spin["angle"] > 360)
qr_token = spin["qr_data"]
print(f"    (Won: {spin['prize_name']}, angle: {spin['angle']:.1f})")

# ── Duplicate email rejection ──
print()
print("── Test: Duplicate email rejection ──")
dupe_status = api_post_status("/api/spin", {"email": "alice@test.com"})
assert_eq("Duplicate email spin returns 409", "409", dupe_status)

_, bob_spin = api_post("/api/spin", {"email": "bob@test.com"})
assert_eq("Different email succeeds", True, "ticket_id" in bob_spin)

# ── Ticket verification ──
print()
print("── Test: Ticket verification ──")
verify = api_get(f"/api/verify/{qr_token}")
assert_eq("Valid ticket verifies as valid", True, verify["valid"])
assert_eq("Ticket not yet redeemed", False, verify["redeemed"])

# ── Redemption flow ──
print()
print("── Test: Redemption flow ──")
_, redeem1 = api_post(f"/api/redeem/{qr_token}", {})
assert_eq("First redemption succeeds", True, redeem1["success"])

_, redeem2 = api_post(f"/api/redeem/{qr_token}", {})
assert_eq("Second redemption fails", False, redeem2["success"])
assert_contains("Reports already redeemed", "already redeemed", redeem2["message"])

verify_after = api_get(f"/api/verify/{qr_token}")
assert_eq("Verify shows redeemed after redemption", True, verify_after["redeemed"])

# ── Invalid token rejection ──
print()
print("── Test: Invalid token rejection ──")
invalid_verify = api_get("/api/verify/totally-invalid-token")
assert_eq("Invalid token rejected", False, invalid_verify["valid"])

invalid_status = api_post_status("/api/redeem/totally-invalid-token", {})
assert_eq("Invalid token redeem returns 400", "400", invalid_status)

# ── Stock decremented correctly ──
print()
print("── Test: Stock decremented correctly ──")
prizes_after = api_get("/api/prizes")
before_map = {p["id"]: p["remaining"] for p in prizes}
after_map = {p["id"]: p["remaining"] for p in prizes_after}
total_dec = sum(before_map[pid] - after_map[pid] for pid in before_map)
# Every spin now decrements stock (merged spin+claim): alice + bob = 2
assert_eq("Total stock decremented by 2", 2, total_dec)

# ── Results ──
print()
print("============================================")
print(f"  RESULTS: {PASSED} passed, {FAILED} failed")
print("============================================")
sys.exit(0 if FAILED == 0 else 1)
