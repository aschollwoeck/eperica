# Feature 054 — inactivity & abandonment measured in real time

**Status:** Verified
**Amends:** 019 (protection & lifecycle). **Touches:** P7 (configurable speed) — refines *which* durations
scale.

**Note:** Decouple the **inactivity lifecycle** (greying/farmable → abandonment) from the world-speed
multiplier: both stages are now measured in **real wall-clock time**, because both gate on a **real person's**
login recency (`last_activity`), not on in-game time. Beginner's protection is unchanged — it remains
speed-scaled, an in-game grace period that faithfully compresses on fast servers (Travian-faithful).

## Problem

The inactivity → abandonment lifecycle (019) scales its thresholds by world speed (`scaled_time_secs`). On a
fast server this is wrong: at `WORLD_SPEED=1000`, the 30-day abandon window collapses to ~43 real minutes and
the 7-day inactivity (farmable) window to ~10 real minutes. A real human who checks a 1000× server once a day
is greyed as a farm within minutes and has their account abandoned (login-blocked, villages freed) within the
hour. The thresholds compare a real-world `last_activity` timestamp against a *game-time-compressed* window —
mixing two clocks. Beginner's protection, by contrast, correctly scales (it is an in-game mechanic).

## Goal

- **AC1 — Inactivity is real-time.** `is_inactive` compares real elapsed wall-clock time
  (`now − last_activity`) against `inactive_after_secs` **directly**, with no speed scaling. World speed does
  not change when a player greys.
- **AC2 — Abandonment is real-time.** `abandon_cutoff` anchors to the sweep-period boundary minus
  `abandon_after_secs` **directly** (no speed scaling); the sweep cadence (`sweep_interval_secs`) was already
  real-time. World speed does not change when an account is abandoned.
- **AC3 — Protection unchanged.** `protection_expiry` still scales `beginner_protection_secs` by speed (019).
- **AC4 — Plumbing.** `is_inactive`/`abandon_cutoff`/`process_due_lifecycle` drop their now-unused `speed`
  parameter; callers (map greying, the scheduler sweep) updated. Balance docs (`lifecycle.toml`, both presets)
  re-label the inactivity/abandon `_secs` as **real-time wall-clock**, not "base, speed-scaled".

## Design

- **`crates/domain/src/lifecycle.rs`** — `is_inactive(last, now, inactive_after_secs)` and
  `abandon_cutoff(period, world_start, sweep_interval_secs, abandon_after_secs)` drop `speed` and use the
  seconds directly (`* 1000` for ms). `protection_expiry` keeps `scaled_time_secs`. Module/field docs updated.
- **`crates/application/src/lifecycle.rs`** — `process_due_lifecycle` drops its `speed` param.
- **Callers** — `crates/web/src/handlers.rs` (map greying), `crates/infrastructure/src/event_store.rs` (the
  scheduler), and the repo tests drop the `speed` argument.
- **Balance** — `specs/balance/presets/{classic,speed}/lifecycle.toml` comments re-label the inactivity/abandon
  windows as real-time. Values unchanged (classic 7d/30d; the `speed` preset keeps its shorter 3d/10d — now a
  genuinely shorter *real-time* window for the shorter round, a deliberate per-preset choice).

## Out of scope

- Re-instating already-abandoned dev/test accounts (a data concern, handled separately). Beginner-protection
  scaling (unchanged). Changing the `_secs` values themselves (only the clock they are measured against).
