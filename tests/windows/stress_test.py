"""Concurrent stress test — verifies no overselling under concurrent load.
Spin now atomically creates tickets (merged spin+claim).
"""
import sys
import json
import urllib.request
import urllib.error
import concurrent.futures

BASE = sys.argv[1] if len(sys.argv) > 1 else "http://localhost:3098"


def get_total_stock():
    with urllib.request.urlopen(f"{BASE}/api/prizes") as r:
        return sum(p["remaining"] for p in json.loads(r.read()))


initial_stock = get_total_stock()
print(f"  Initial total stock: {initial_stock}")
print()
print("  Launching 100 concurrent spins...")
print()


def spin(i):
    try:
        data = json.dumps({"email": f"stress{i}@test.com"}).encode()
        req = urllib.request.Request(
            f"{BASE}/api/spin",
            data=data,
            headers={"Content-Type": "application/json"},
        )
        with urllib.request.urlopen(req) as r:
            resp = json.loads(r.read())
            return "claimed" if "ticket_id" in resp else f"unexpected: {resp}"
    except urllib.error.HTTPError as e:
        body = json.loads(e.read().decode())
        err = body.get("error", "")
        if "no longer available" in err or "All prizes" in err:
            return "rejected_stock"
        elif "already been used" in err:
            return "rejected_dupe"
        return f"unexpected: {body}"
    except Exception as e:
        return f"error: {e}"


with concurrent.futures.ThreadPoolExecutor(max_workers=50) as pool:
    all_results = list(pool.map(spin, range(100)))

claimed = all_results.count("claimed")
rejected_stock = all_results.count("rejected_stock")
rejected_dupe = all_results.count("rejected_dupe")
unexpected = [r for r in all_results if r.startswith("unexpected")]
errors = [r for r in all_results if r.startswith("error")]

print(f"  Claimed:              {claimed}")
print(f"  Rejected (no stock):  {rejected_stock}")
print(f"  Rejected (dupe):      {rejected_dupe}")
print(f"  Unexpected:           {len(unexpected)}")
print(f"  Errors:               {len(errors)}")
print(f"  Total:                {len(all_results)}")
print()

failed = False

if claimed > initial_stock:
    print(f"  FAIL: OVERSOLD by {claimed - initial_stock}!")
    failed = True
else:
    print(f"  PASS: {claimed} claimed (<={initial_stock}) — no overselling")

if claimed + rejected_stock != 100:
    print(f"  FAIL: claimed + rejected_stock != 100 ({claimed} + {rejected_stock})")
    failed = True
else:
    print(f"  PASS: All 100 requests accounted for")

if len(errors) > 0:
    print(f"  FAIL: {len(errors)} errors:")
    for e in errors[:5]:
        print(f"    {e}")
    failed = True
else:
    print(f"  PASS: Zero errors")

if len(unexpected) > 0:
    print(f"  FAIL: {len(unexpected)} unexpected responses:")
    for u in unexpected[:5]:
        print(f"    {u}")
    failed = True

# Verify final stock
print()
print("── Verify final state ──")
final_stock = get_total_stock()
expected_remaining = initial_stock - claimed
print(f"  Final total stock: {final_stock}")
print(f"  Expected remaining: {expected_remaining}")

if final_stock == expected_remaining:
    print(f"  PASS: Stock accounting correct")
else:
    print(f"  FAIL: Stock mismatch (expected {expected_remaining}, got {final_stock})")
    failed = True

sys.exit(1 if failed else 0)
