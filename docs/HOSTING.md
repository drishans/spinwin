# Hosting & cost

Spin & Win runs one week a year but was deployed like it runs every day. This
doc covers the two postures it can be in, how to switch, and where the money
goes.

## Where the ~$8/month went

| Line item | Cost | Why |
|-----------|------|-----|
| `spinwin` machine, 1GB RAM, always on | ~$5.70/mo | `min_machines_running = 1` billed ~720h/month |
| Dedicated IPv4 | $2.00/mo | Allocated at setup; not needed for a `*.fly.dev` hostname |
| Volumes (`spinwin_data` + `spinwin_staging_data`) | ~$0.30/mo | $0.15/GB/mo, billed even while the machine is stopped |
| `spinwin-staging` machine | ~$0 | Already scaled to zero |

Fly bills per second of machine uptime, so an idle always-on machine is the
expensive part. Fly also does not collect invoices under $5/month, which means
a scaled-to-zero app with a small volume lands at an effective $0.

## Two postures

### Demo (default in this repo — between events)

What the committed config does:

- `SPINWIN_DEMO=1` — the frontend simulates the whole spin client-side. No stock
  is decremented, no ticket rows are written, no email is sent. `POST /api/spin`
  returns `403` so a stray request can't mint a real ticket either.
- `min_machines_running = 0` — the machine sleeps when idle and wakes on the
  first request. Visitors see a ~1-2s cold start; nobody at the venue is
  depending on it.
- `256mb` RAM instead of `1gb` — Axum + SQLite sits well under 50MB.

Apply it:

```bash
fly deploy                       # picks up fly.toml
fly scale count 1 --region sjc   # one machine, allowed to sleep
```

Then drop the extras that bill regardless of traffic:

```bash
# 1. Release the dedicated IPv4 ($2/mo) and take a free shared one.
#    Safe for spinwin.fly.dev. If you point a custom domain at Fly with an
#    A record you need the dedicated IP — use a CNAME instead, or skip this.
fly ips list
fly ips release <the-v4-address>
fly ips allocate-v4 --shared

# 2. Retire staging entirely — it can be recreated from fly.staging.toml.
fly apps destroy spinwin-staging

# 3. Optional, saves the last $0.15/mo. Demo mode never writes to the DB, so
#    the volume only holds seeded prize rows. Detach by removing the [mounts]
#    block from fly.toml first, then:
# fly volumes list
# fly volumes destroy <volume-id>
```

Expected steady state afterwards: a few cents of compute per month, which Fly
does not invoice.

### Live (the October event)

1. In `fly.toml`:
   - delete the `SPINWIN_DEMO = "1"` line
   - set `min_machines_running = 1` (no cold starts for attendees)
   - set both memory fields back to `1gb` / `1024`
2. Re-allocate a dedicated IPv4 if you're using a custom domain with an A record:
   `fly ips allocate-v4`
3. Confirm secrets survived: `fly secrets list` should show `SPINWIN_SIGNING_KEY`,
   `GOOGLE_SHEET_ID`, `SMTP_EMAIL`, `SMTP_PASSWORD`, `ADMIN_USER`, `ADMIN_PASSWORD`.
   Secrets persist across deploys but not across `fly apps destroy`.
4. Deploy: GitHub → Actions → **Fly Deploy** → Run workflow → `production`.
5. Smoke test: `/api/config` should report `{"demo":false}`, and a spin from a
   test email should produce a scannable ticket.

> Keep `SPINWIN_SIGNING_KEY` stable. Tickets issued under a different key fail
> verification at the venue.

## The free fallback: GitHub Pages

`.github/workflows/pages.yml` publishes `server/frontend/` to GitHub Pages on
every push to `main` that touches the frontend. There's no backend there, so
`/api/config` 404s and the frontend switches itself into demo mode — the same
code path as `SPINWIN_DEMO=1`, no separate build.

This costs nothing, ever, and is independent of Fly. One-time setup: repo
**Settings → Pages → Source → GitHub Actions**. The site lands at
`https://<user>.github.io/spinwin/`.

The scanner (`scan.html`) reports itself unavailable there — it verifies
signatures against the server's public key and has nothing to check without a
backend. The admin dashboard is excluded from the Pages build entirely.

## Forcing a mode while testing

`?demo=1` forces demo mode, `?demo=0` forces the live flow, on any host. Useful
for checking the real path against a demo-configured server:

```
https://spinwin.fly.dev/?demo=0
```
