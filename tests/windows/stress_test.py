"""Concurrent stress test — called by stress_test.bat"""
import sys
import json
import urllib.request
import urllib.error
import concurrent.futures

BASE = sys.argv[1] if len(sys.argv) > 1 else "http://localhost:3098"


def get_bangles_stock():
    with urllib.request.urlopen(f"{BASE}/api/prizes") as r:
        for p in json.loads(r.read()):
            if p["name"] == "Bangles":
                return p["remaining"]
    return -1


print(f"  Initial Bangles stock: {get_bangles_stock()}")
print()
print("  Launching 100 concurrent claims (50 workers)...")
print()


def claim(i):
    try:
        data = json.dumps({"name": f"Stress User {i}", "email": f"stress{i}@test.com", "prize_id": 5}).encode()
        req = urllib.request.Request(
            f"{BASE}/api/claim",
            data=data,
            headers={"Content-Type": "application/json"},
        )
        with urllib.request.urlopen(req) as r:
            resp = json.loads(r.read())
            return "claimed" if "ticket_id" in resp else f"unexpected: {resp}"
    except urllib.error.HTTPError as e:
        body = json.loads(e.read().decode())
        err = body.get("error", "")
        if "no longer available" in err:
            return "rejected_stock"
        elif "already been used" in err:
            return "rejected_dupe"
        return f"unexpected: {body}"
    except Exception as e:
        return f"error: {e}"


with concurrent.futures.ThreadPoolExecutor(max_workers=50) as pool:
    all_results = list(pool.map(claim, range(100)))

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

if claimed > 50:
    print(f"  FAIL: OVERSOLD by {claimed - 50}!")
    failed = True
elif claimed == 50:
    print(f"  PASS: Exactly 50 claimed — no overselling")
else:
    print(f"  PASS: {claimed} claimed (<=50) — no overselling")

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
final_stock = get_bangles_stock()
print(f"  Final Bangles stock: {final_stock}")

if final_stock == 0:
    print(f"  PASS: Stock is exactly 0")
else:
    print(f"  FAIL: Stock should be 0, got {final_stock}")
    failed = True

# Check other prizes untouched
with urllib.request.urlopen(f"{BASE}/api/prizes") as r:
    for p in json.loads(r.read()):
        if p["name"] != "Bangles" and p["remaining"] != p["total_qty"]:
            print(f"  FAIL: {p['name']} stock changed unexpectedly: {p['remaining']}/{p['total_qty']}")
            failed = True
    if not failed:
        print("  PASS: Other prizes untouched")

sys.exit(1 if failed else 0)
