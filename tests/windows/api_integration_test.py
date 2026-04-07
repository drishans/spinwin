"""API integration tests — called by api_integration_test.bat"""
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
assert_eq("5 prizes seeded", 5, len(prizes))
total_stock = sum(p["remaining"] for p in prizes)
assert_eq("Total stock is 450", 450, total_stock)

# ── Spin returns valid prize and angle ──
print()
print("── Test: Spin returns valid prize and angle ──")
_, spin = api_post("/api/spin", {"email": "alice@test.com"})
spin_prize_id = spin["prize"]["id"]
spin_prize_name = spin["prize"]["name"]
spin_angle = spin["angle"]
assert_eq("Spin returns angle with full rotations", True, spin_angle > 360)
print(f"    (Won: {spin_prize_name}, angle: {spin_angle:.1f})")

# ── Wheel-prize alignment ──
print()
print("── Test: Wheel-prize alignment ──")
alignment_pass = 0
alignment_total = 20
for i in range(1, alignment_total + 1):
    _, result = api_post("/api/spin", {"email": f"align{i}@test.com"})
    # Re-fetch prizes each time since stock changes
    current_prizes = api_get("/api/prizes")
    available = [p for p in current_prizes if p["remaining"] > 0]
    num_prizes = len(available)

    selected_id = result["prize"]["id"]
    angle = result["angle"] % 360
    pointer_pos = (360 - angle) % 360

    # Equal-sized segments: each segment is 360/N degrees
    segment_size = 360.0 / num_prizes
    landed_on = None
    for idx, p in enumerate(available):
        start = idx * segment_size
        end = start + segment_size
        if start <= pointer_pos < end:
            landed_on = p["id"]
            break

    if landed_on is None:
        landed_on = available[-1]["id"]

    if landed_on == selected_id:
        alignment_pass += 1
    else:
        print(f"  Spin {i}: prize={selected_id} but angle landed on {landed_on} (angle={angle:.1f}, pointer={pointer_pos:.1f})")

assert_eq(f"All {alignment_total} spins land on correct segment", alignment_total, alignment_pass)

# ── Claim flow ──
print()
print("── Test: Claim flow ──")
_, claim = api_post("/api/claim", {"name": "Alice Test", "email": "alice@test.com", "prize_id": spin_prize_id})
assert_eq("Claim returns ticket_id", True, "ticket_id" in claim)
assert_eq("Claim returns qr_data", True, "qr_data" in claim)
assert_eq("Claim returns correct name", "Alice Test", claim["attendee_name"])
qr_token = claim["qr_data"]

# ── Duplicate email rejection ──
print()
print("── Test: Duplicate email rejection ──")
dupe_spin_status = api_post_status("/api/spin", {"email": "alice@test.com"})
assert_eq("Duplicate email spin returns 409", "409", dupe_spin_status)

dupe_claim_status = api_post_status("/api/claim", {"name": "Alice Again", "email": "alice@test.com", "prize_id": 1})
assert_eq("Duplicate email claim returns 409", "409", dupe_claim_status)

_, bob_spin = api_post("/api/spin", {"email": "bob@test.com"})
bob_prize_id = bob_spin["prize"]["id"]
claim2_status = api_post_status("/api/claim", {"name": "Bob Test", "email": "bob@test.com", "prize_id": bob_prize_id})
assert_eq("Different email succeeds (200)", "200", claim2_status)

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
# 2 claimed (alice + bob) + 20 alignment spins that were not claimed won't decrement stock
# Actually spins don't decrement stock — only claims do
assert_eq("Total stock decremented by 2", 2, total_dec)

# ── Results ──
print()
print("============================================")
print(f"  RESULTS: {PASSED} passed, {FAILED} failed")
print("============================================")
sys.exit(0 if FAILED == 0 else 1)
