# Hosting & cost

Spin & Win runs one week a year but was deployed like it runs every day. This
doc covers the two postures it can be in, how to switch, and where the money
goes.

## Where the money went

From the July 2026 invoice ($7.34):

| Line item | Quantity | Amount |
|-----------|----------|--------|
| Machines Shared CPU 1x (sjc) | 2,681,992 sec | $2.40 |
| Machines Shared 1x: Additional RAM (sjc) | 2,011,494 GB-sec | $4.63 |
| Volumes | 1,487 GB-hours | $0.31 |
| Volume snapshot storage | — | $0.02 |
| Bandwidth (all regions, egress + private) | 142M+ bytes | $0.00 |

Two things stand out:

**The RAM was the expensive part, not the machine.** `shared-cpu-1x` includes
256MB; everything above that bills separately at ~$0.0000023/GB-sec. The 1GB VM
meant 0.75GB of billable extra RAM for 31 days — $4.63, nearly two thirds of the
bill. The app's resident set is under 50MB, so that RAM was never used.

**The machine ran 24/7.** 2,681,992 seconds ÷ 86,400 = 31.04 days. That's
`min_machines_running = 1` billing every hour of the month for an event that
hadn't started.

Bandwidth was free at this traffic level, and there was no dedicated IPv4 charge
on the account.

## Two postures

### Demo (default in this repo — between events)

- `256mb` RAM instead of `1gb` — eliminates the $4.63 additional-RAM line
  outright, since 256MB is the included allotment.
- `min_machines_running = 0` with `auto_stop_machines = 'suspend'` — the machine
  snapshots to disk when idle and resumes on the next request, so CPU seconds
  bill only while someone is actually on the page.
- `SPINWIN_DEMO=1` — the frontend simulates the whole spin client-side. No stock
  decremented, no ticket rows, no email. `POST /api/spin` returns `403` so a
  stray request can't mint a real ticket either. It also skips the Google Sheet
  fetch at startup, which is a blocking HTTPS round-trip a scaled-to-zero app
  would otherwise pay on every wake.

Apply it:

```bash
fly deploy                       # picks up fly.toml
fly scale count 1 --region sjc   # one machine, allowed to suspend
```

Then drop what bills regardless of traffic:

```bash
# Retire staging — recreatable from fly.staging.toml. Removes its volume too.
fly apps destroy spinwin-staging

# Optional, saves another ~$0.155/mo. Demo mode never writes to the DB, so the
# volume only holds seeded prize rows. Remove the [mounts] block from fly.toml
# and redeploy first, then:
# fly volumes list
# fly volumes destroy <volume-id>
```

Expected steady state: **pennies per month.** Suspended machines still bill for
their root filesystem (~$0.15/GB/month, so a couple of cents for this image),
plus CPU seconds only while serving. Don't count on a literal $0.00 invoice —
Fly is widely reported not to collect balances under $5, but that threshold does
not appear in their official billing docs, so treat it as a bonus rather than
the plan.

### Live (the October event)

1. In `fly.toml`:
   - delete the `SPINWIN_DEMO = "1"` line
   - set `min_machines_running = 1` (no cold starts for attendees)
   - set both memory fields back to `1gb` / `1024`
   - optionally set `auto_stop_machines = 'stop'` — irrelevant while a machine
     is pinned running
2. Recreate the volume if you destroyed it, and restore the `[mounts]` block:
   `fly volumes create spinwin_data --region sjc --size 1`
3. Confirm secrets survived: `fly secrets list` should show `SPINWIN_SIGNING_KEY`,
   `GOOGLE_SHEET_ID`, `SMTP_EMAIL`, `SMTP_PASSWORD`, `ADMIN_USER`, `ADMIN_PASSWORD`.
   Secrets persist across deploys but not across `fly apps destroy`.
4. Deploy: GitHub → Actions → **Fly Deploy** → Run workflow → `production`.
5. Smoke test: `/api/config` should report `{"demo":false}`, and a spin from a
   test email should produce a scannable ticket.

Back at 1GB always-on this returns to roughly $7/month, which is the right price
to pay for the week it matters.

> Keep `SPINWIN_SIGNING_KEY` stable. Tickets issued under a different key fail
> verification at the venue.

## Cold starts

The server binary reaches first-response in ~50ms from process start, so app
boot is not the bottleneck — Fly's machine wake is. With `suspend` that's a few
hundred milliseconds; with `stop` it's a second or two. Only the first visitor
after an idle period pays it, and the machine stays awake for everyone
immediately behind them.

## Forcing a mode while testing

`?demo=1` forces demo mode, `?demo=0` forces the live flow, on any host:

```
https://spinwin.fly.dev/?demo=0
```

## Static hosting (not currently used)

The frontend is fully self-contained in demo mode — `server/frontend/demo.js`
mirrors the server's weighted prize draw and landing-angle math, and asset and
API paths are relative, so the directory can be dropped on any static host as-is
and it will detect the missing `/api/config` and switch itself into demo mode.
That would be free and instant, at the cost of serving from a different URL.
Not wired up; noted here in case the Fly bill ever becomes a problem again.
