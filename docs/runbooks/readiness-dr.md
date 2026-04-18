# Readiness tables — DR runbook

Who this is for: an operator recovering mg-custody from a database
outage, a bad migration, a corrupted volume, or a region failover.

The readiness pipeline has two DB tables: `readiness_transitions`
(migration 036) and `readiness_silences` (migrations 037 + 038).
This document tells you exactly how much each one matters for
recovery and when you can defer it.

## TL;DR

* `readiness_transitions` — **soft-priority**. It's history-only;
  losing it degrades postmortem tooling but does not affect live
  state. Restore opportunistically.
* `readiness_silences` — **medium-priority**. Losing active
  silences means the refresh task will start paging oncall for
  known-maintenance features. Restore before turning the service
  back on, or manually re-create critical silences in the first
  few minutes.
* Live state (the in-memory `ReadinessHandle`) is reconstructed
  from scratch at boot by the probe task. **No DB is needed** to
  get the service back to serving or to get correct 503s on broken
  features — the first refresh tick populates everything.

## What each table holds

### readiness_transitions

Append-only log of every alertable state change:

```sql
id          bigserial primary key
feature     text        -- e.g. "scanner.ethereum"
kind        text        -- "degradation" | "misconfiguration" | "recovery"
reason      text null   -- probe error, missing config keys, or null for recovery
occurred_at timestamptz -- when the refresh task saw the transition
```

Written by the periodic refresh task (once per ~60s, or zero rows
if nothing changed). Read by the dashboard's "Recent incidents"
timeline, the uptime endpoint, and the per-feature detail page.

**Retention**: the service prunes rows older than 90 days hourly
(see `crates/server/src/main.rs`). Backup cadence should match —
anything older than 90 days is already gone on the live side.

### readiness_silences

Operator-created alert-muting windows (planned maintenance).

```sql
id           bigserial primary key
feature      text        -- exact feature or wildcard pattern
starts_at    timestamptz -- window opens
expires_at   timestamptz -- window closes
reason       text        -- free-form, written to audit trail
created_by   text        -- user_id
created_at   timestamptz
```

Rows never hard-delete — they just expire. The refresh task reads
every 60s; `list_active_silences()` filters `starts_at <= now <
expires_at`.

## Recovery scenarios

### Scenario 1: service restart, DB intact

Nothing to do. The refresh task rebuilds live state on boot; the
tables serve only history + silence lookups which Postgres will
happily expose as soon as the pool reconnects.

**Check after restart**:
- `GET /v1/health/features` — should populate within ~2s.
- `GET /v1/health/features/silences` — pre-existing silences
  should be listed.
- Startup log line `FEATURE READINESS` should appear once.

### Scenario 2: DB down but service running

The refresh task will:
- keep probing vendors correctly (no DB dependency in `build()`),
- fail to insert `readiness_transitions` rows (logged as warnings),
- treat `list_active_silences()` failures as empty (no silences
  applied — **alerts may fire for maintenance that was supposed
  to be muted**).

**Action items while DB is down**:
1. Notify oncall — expect alerts for any features you planned
   maintenance on.
2. If maintenance windows are critical, temporarily point
   `CUSTODY_READINESS__ALERT_WEBHOOK` to a dev-null receiver so
   the pager doesn't ring. Revert when DB is back.
3. Do NOT restart the service to "fix" — in-memory live state is
   already correct; the DB hiccup only affects history writes.

### Scenario 3: Fresh DB, no backup

Run migrations 001..038 in order (the standard startup path does
this). Both readiness tables come up empty.

**Behaviour once empty**:
- History dashboard shows "No transitions recorded yet".
- Uptime endpoints report 100% for every feature (no events
  means nothing went down in-window; technically correct but
  misleading for the first 24h).
- No silences are active. Any pre-planned maintenance needs to be
  re-created by hand (`POST /v1/health/features/silences`).

**Acceptance**: this is fine. The DR posture for custody is that
live state is authoritative; history is reconstructable by
watching new events.

### Scenario 4: restored from N-hour-old backup

After restoring both tables:

1. `transitions_in_window` will report old windows verbatim;
   recent events between the backup cutoff and now will be
   missing — creates a false "quiet period" on uptime
   calculations. Warn the team that 24h uptime % is temporarily
   pessimistic (missing recoveries look like the feature never
   came back).
2. `readiness_silences` is the critical one:
   - If any restored silence has `expires_at` in the past, it's
     harmless — `list_active_silences` filters it.
   - If any restored silence has `starts_at` in the future, it
     will take effect on schedule.
   - Restored-but-ended-early silences (the operator `DELETE`d
     them) will **re-activate** if their window is still live.
     This is a subtle gotcha — check the audit log for
     `ReadinessSilenceEnded` entries after the backup cutoff
     and re-run those DELETEs.

## Backup recommendations

### `readiness_transitions`

* Daily `pg_dump --table=readiness_transitions` is overkill; weekly
  is plenty given the 90-day retention cap.
* If you already back up the whole DB nightly, this table is
  included and you're done.
* For size estimates: budget ~200 rows/day on a healthy deployment,
  ~10k/day during a flapping outage. Row size ~200 bytes. Worst
  case: ~180 MB over 90 days.

### `readiness_silences`

* Back up **whenever a silence is created or ended**, not on a
  schedule — silences are short-lived (≤7 days per row) and a
  daily backup could easily miss the entire lifespan of a tight
  maintenance window.
* Cheapest sufficient approach: ship the row to an audit-log sink
  at the same time as the `ReadinessSilence*` audit_log entry.
  Recovery then replays audit events.

## Manual restore procedure

If you have a `pg_dump` of the tables:

```bash
# Restore transitions (safe anytime).
psql -d custody -f readiness_transitions.sql

# Restore silences. Run this DURING a service-off window if possible,
# so the refresh task doesn't see an inconsistent state.
psql -d custody -f readiness_silences.sql

# Verify.
psql -d custody -c "SELECT COUNT(*), MAX(occurred_at) FROM readiness_transitions;"
psql -d custody -c "SELECT feature, starts_at, expires_at FROM readiness_silences WHERE expires_at > now();"

# If the service is running, nudge the refresh task so the new
# silences take effect immediately rather than waiting up to 60s.
curl -X POST -H "Authorization: Bearer $ADMIN_TOKEN" \
     https://custody.internal/v1/health/features/refresh
```

## Known non-issues

* **"My recoveries disappeared after restore from stale backup."**
  Expected. The uptime endpoint treats any unclosed degradation
  as still-ongoing until recovery lands. Real recoveries that
  happened post-backup are missing; the pessimism corrects
  itself as new transitions accumulate.
* **"Audit log shows silence creation but silence table is empty."**
  You restored the audit log table from a different backup than
  the silences table. Mismatched restores leave audit without
  the referenced row, which is harmless: the audit row itself
  is the source of truth for "who did what", the silence table
  is just the current live state.
* **"Retention pruned a row that the dashboard was displaying."**
  The dashboard polls every 15s by default. A 90-day-old row being
  pruned mid-view will show up on the next reload as "not there
  anymore". This is not a bug — 90-day retention is intentional.
