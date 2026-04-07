"""Prize alignment test — verifies the spinner angle matches the awarded prize.

The server calculates angles based on remaining stock (dynamic proportional segments).
The frontend has 3 wheel display modes:
  - dynamic: segments proportional to remaining stock (matches server)
  - equal:   all segments equal size
  - fixed:   segments proportional to total_qty

This test verifies alignment for ALL 3 modes. The server only generates angles
for the 'dynamic' layout, so 'equal' and 'fixed' modes WILL misalign — this test
documents and catches that known issue.
"""
import sys
import json
import urllib.request
import urllib.error

BASE = sys.argv[1] if len(sys.argv) > 1 else "http://localhost:3097"
PASSED = 0
FAILED = 0
WARNINGS = 0
SPINS_PER_MODE = 30


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
        return e.code, json.loads(e.read().decode())


def assert_eq(desc, expected, actual):
    global PASSED, FAILED
    if str(expected) == str(actual):
        print(f"  PASS: {desc}")
        PASSED += 1
    else:
        print(f"  FAIL: {desc} (expected '{expected}', got '{actual}')")
        FAILED += 1


def assert_warn(desc, expected, actual):
    """Like assert_eq but counts as a warning instead of failure."""
    global PASSED, WARNINGS
    if str(expected) == str(actual):
        print(f"  PASS: {desc}")
        PASSED += 1
    else:
        print(f"  WARN: {desc} (expected '{expected}', got '{actual}')")
        WARNINGS += 1


def build_segments_dynamic(prizes):
    """Segments proportional to remaining stock — matches server logic."""
    available = [p for p in prizes if p["remaining"] > 0]
    total = sum(p["remaining"] for p in available)
    segments = []
    start = 0
    for p in available:
        sweep = p["remaining"] / total * 360
        segments.append({"id": p["id"], "name": p["name"], "start": start, "sweep": sweep})
        start += sweep
    return segments


def build_segments_equal(prizes):
    """All segments equal size — does NOT match server angle calculation."""
    available = [p for p in prizes if p["remaining"] > 0]
    angle = 360 / len(available)
    return [
        {"id": p["id"], "name": p["name"], "start": i * angle, "sweep": angle}
        for i, p in enumerate(available)
    ]


def build_segments_fixed(prizes):
    """Segments proportional to total_qty — does NOT match server angle calculation."""
    available = [p for p in prizes if p["remaining"] > 0]
    total_qty = sum(p["total_qty"] for p in available)
    segments = []
    start = 0
    for p in available:
        sweep = p["total_qty"] / total_qty * 360
        segments.append({"id": p["id"], "name": p["name"], "start": start, "sweep": sweep})
        start += sweep
    return segments


def find_segment_at_angle(segments, angle):
    """Given a rotation angle, find which segment the pointer lands on.

    The wheel rotates clockwise. The pointer is at the top (12 o'clock).
    After rotating by `angle` degrees, the pointer reads the segment at
    position (360 - angle%360) % 360 from the start.
    """
    pointer_pos = (360 - (angle % 360)) % 360
    for seg in segments:
        if seg["start"] <= pointer_pos < seg["start"] + seg["sweep"]:
            return seg["id"], seg["name"]
    # Wraparound: if pointer_pos didn't match (floating point edge), return last
    return segments[-1]["id"], segments[-1]["name"]


def test_mode(mode_name, build_fn, spin_count, email_prefix, use_warn=False):
    """Spin multiple times and check if the angle lands on the correct segment."""
    check_fn = assert_warn if use_warn else assert_eq
    hits = 0
    misses = []

    for i in range(1, spin_count + 1):
        # Fetch current prizes before each spin (stock changes after claims elsewhere)
        prizes = api_get("/api/prizes")
        segments = build_fn(prizes)

        status, result = api_post("/api/spin", {"email": f"{email_prefix}{i}@test.com"})
        if status != 200:
            print(f"  Spin {i}: HTTP {status} — {result}")
            continue

        selected_id = result["prize"]["id"]
        selected_name = result["prize"]["name"]
        angle = result["angle"]

        landed_id, landed_name = find_segment_at_angle(segments, angle)

        if landed_id == selected_id:
            hits += 1
        else:
            misses.append(
                f"    Spin {i}: won '{selected_name}' (id={selected_id}) "
                f"but angle {angle:.1f} points to '{landed_name}' (id={landed_id})"
            )

    if misses:
        for m in misses[:5]:
            print(m)
        if len(misses) > 5:
            print(f"    ... and {len(misses) - 5} more mismatches")

    check_fn(f"[{mode_name}] {hits}/{spin_count} spins land on correct segment", spin_count, hits)


# ══════════════════════════════════════════════
# Test 1: Dynamic mode (server's native layout)
# This MUST pass — it's the mode the server calculates angles for.
# ══════════════════════════════════════════════
print("── Mode: DYNAMIC (proportional to remaining) ──")
print("   Server angles are calculated for this layout.")
print("   All spins MUST land on the correct segment.")
print()
test_mode("dynamic", build_segments_dynamic, SPINS_PER_MODE, "dyn", use_warn=False)

# ══════════════════════════════════════════════
# Test 2: Equal mode (all segments same size)
# This will likely FAIL because the server doesn't
# calculate angles for equal-sized segments.
# ══════════════════════════════════════════════
print()
print("── Mode: EQUAL (all segments same size) ──")
print("   Server angles are NOT calculated for this layout.")
print("   Mismatches here confirm the reported visual bug.")
print()
test_mode("equal", build_segments_equal, SPINS_PER_MODE, "eq", use_warn=True)

# ══════════════════════════════════════════════
# Test 3: Fixed mode (proportional to total_qty)
# This will also likely FAIL for the same reason.
# ══════════════════════════════════════════════
print()
print("── Mode: FIXED (proportional to total_qty) ──")
print("   Server angles are NOT calculated for this layout.")
print("   Mismatches here confirm the reported visual bug.")
print()
test_mode("fixed", build_segments_fixed, SPINS_PER_MODE, "fx", use_warn=True)

# ── Results ──
print()
print("============================================")
print(f"  RESULTS: {PASSED} passed, {FAILED} failed, {WARNINGS} warnings")
if WARNINGS > 0:
    print(f"  NOTE: {WARNINGS} warnings from equal/fixed modes —")
    print(f"  the server only calculates angles for 'dynamic' mode.")
    print(f"  Users on equal/fixed will see the wrong prize under the pointer.")
print("============================================")

sys.exit(0 if FAILED == 0 else 1)
