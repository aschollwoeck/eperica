# Feature 019 — Protection & lifecycle — Technical Plan

**Status:** Verified
**Spec:** ./spec.md

Two server-authoritative account-lifecycle mechanics: **beginner's protection** (a speed-scaled attack
immunity window that ends on offence or at a population threshold) and the **inactivity lifecycle**
(derived "inactive ⇒ farmable" greying, then a periodic **abandonment sweep** that removes villages and
retires the account). The sweep **reuses 017's recurring-settlement shape** (watermark-derived period,
atomic + idempotent, period-anchored cutoff). Stage 1 is **derived on read** (no tick); stage 2 is a
**soft-delete** of the account + hard-delete of its villages, so the map renews while battle-report
history stays referentially intact. **No new combat/economy mechanics** — an attack is *gated* on the
target's protection, and villages are removed via the existing delete path.

## Constitution check

- **P1 (event-driven / lazy):** protection is a stored timestamp compared on read; "inactive" is **derived**
  from `last_activity` with no stored flag and no per-entity tick; the abandonment sweep is a **state-driven
  recurring due-event** (watermark `MAX(inactivity_sweeps.period)`), not a per-account scheduled row.
- **P2 / P6 (reproducible):** protection grant/expiry and the sweep are functions of persisted state +
  the period; the deletion **cutoff is anchored to the period boundary** so a period always abandons the
  same set. Soft-delete retains `users` rows so historical reports stay auditable.
- **P3 (pure domain):** `domain/lifecycle.rs` holds the pure predicates (`is_protected`, `is_inactive`,
  `protection_expiry`, `abandon_cutoff`) reusing the medal `period_index`/`period_start`. Unit-tested, no I/O.
- **P4 (server authority):** protection is granted at spawn and ended by the server (on attack / threshold);
  inactivity + abandonment are System-only. No client path sets any of it. The attack gate is server-side.
- **P7 (configurable, speed-scaled):** `lifecycle.toml` carries the window/threshold/timings/cadence; the
  time-based durations use `scaled_time_secs` against world speed.
- **P11 (performance):** the attack hot-path adds **one PK-indexed `protection_of` lookup** on the
  target owner plus a timestamp compare; the activity write is a **throttled conditional UPDATE**; the
  sweep is one bounded batch per period.

## Domain (`domain/lifecycle.rs`, pure)

- `struct LifecycleRules { beginner_protection_secs: i64, protection_population_threshold: i64,
  inactive_after_secs: i64, abandon_after_secs: i64, sweep_interval_secs: i64 }`.
- `fn is_protected(protected_until: Option<Timestamp>, now: Timestamp) -> bool` — `Some(t) && now < t`.
- `fn protection_expiry(now, base_secs, speed) -> Timestamp` — `now + scaled_time_secs(base_secs, speed)`.
- `fn is_inactive(last_activity, now, after_secs, speed) -> bool` —
  `now − last_activity > scaled_time_secs(after_secs, speed)`.
- `fn abandon_cutoff(period, world_start, interval_secs, abandon_after_secs, speed) -> Timestamp` —
  `period_start(period+1, world_start, interval_secs) − scaled_time_secs(abandon_after_secs, speed)`
  (reuses `medals::period_start`). The sweep deletes accounts with `last_activity < cutoff`.
- `fn protection_ended_by_population(pop, threshold) -> bool` — `pop >= threshold`.
- Unit tests: protected/expired/never-protected; speed scaling halves at 2×; inactivity threshold edge;
  cutoff anchored to the period boundary; population-threshold end.

## Balance (`specs/balance/lifecycle.toml` + `infrastructure::balance`)

- `lifecycle.toml`:
  ```toml
  [protection]
  beginner_protection_secs = 259200          # 3 days (×speed-scaled down)
  population_threshold      = 200             # protection ends once established
  [inactivity]
  inactive_after_secs = 604800               # 7 days idle ⇒ farmable (greyed)
  abandon_after_secs  = 2592000              # 30 days idle ⇒ swept (villages freed)
  sweep_interval_secs = 86400                # the sweep cadence (period length)
  ```
- `lifecycle_rules() -> Result<LifecycleRules, BalanceError>` loader (mirrors `medal_rules()`), fail-fast.

## Persistence (`infrastructure` + migration `0029_lifecycle.sql`)

- `ALTER TABLE users ADD COLUMN protected_until timestamptz, ADD COLUMN last_activity timestamptz NOT NULL
  DEFAULT now(), ADD COLUMN abandoned_at timestamptz;` + index on `last_activity` (sweep scan).
- `CREATE TABLE inactivity_sweeps (world_id uuid NOT NULL, period bigint NOT NULL, swept_at timestamptz NOT
  NULL DEFAULT now(), abandoned_count int NOT NULL, PRIMARY KEY (world_id, period));` — the watermark.
- `create_account` sets `protected_until = now + scaled(beginner_protection_secs)` and `last_activity =
  now` (needs speed + rules; pass the expiry in, computed by the caller / via rules held by the repo).
- `AccountRepository` additions:
  - `end_protection(player, now)` — `UPDATE users SET protected_until = $now WHERE id = $p AND
    protected_until > $now` (idempotent; only ends an active window).
  - `touch_activity(player, now)` — throttled `UPDATE … SET last_activity = $now WHERE id = $p AND
    last_activity < $now − <interval>` (cheap; no write if fresh).
  - `protection_of(player) -> Option<Timestamp>` and surface the **target owner's** `protected_until` in the
    attack-target lookup (so `order_attack` gates without an extra round-trip).
  - `authenticate` path: reject login when `abandoned_at IS NOT NULL`.
- `LifecycleRepository` (new port + impl):
  - `latest_swept_period() -> Option<i64>` — `MAX(period) FROM inactivity_sweeps WHERE world_id = $w`.
  - `sweep_abandoned(period, cutoff) -> usize` — **one transaction**: insert the watermark row (`ON CONFLICT
    DO NOTHING`), select non-abandoned users with `last_activity < cutoff` (`FOR UPDATE`), `DELETE FROM
    villages WHERE owner_id = ANY(...)` (cascades resources/buildings/fields/units/movements — but **not**
    battle reports, which are `SET NULL`; see Notes — freeing the valleys), `UPDATE users SET abandoned_at
    = now()`; record the count; commit together. Idempotent (already-abandoned excluded; re-settle of a
    period is a no-op).
- The `villages_at` / map-marker read joins the owner's `last_activity` + `abandoned_at` so the viewport can
  mark **inactive** villages (derived via `is_inactive`).

## Application

- `combat::order_attack` — after target resolution, **reject if the target owner is protected**
  (`is_protected`) with a new `CombatError::TargetProtected`; on a valid launch, **end the attacker's own
  protection** (`end_protection`) in/with the order. (Scouting/trade unaffected.)
- `lifecycle.rs`:
  - `process_due_lifecycle(repo, world_start, now, rules, speed) -> Result<Vec<(i64, usize)>, RepoError>`
    (where `R: LifecycleRepository`): mirror `process_due_medal_settlement` — settle each
    complete-but-unswept period (`period_index` vs `latest_swept_period`), `cutoff = abandon_cutoff(P, …)`,
    `sweep_abandoned(P, cutoff)`; return `(period, count)` per swept period.
  - `end_protection_if_established(repo, econ, player) -> Result<bool, RepoError>` — the lazy threshold end:
    if the player is protected and `population >= threshold`, `end_protection`. Called on the village view.
- `auth::authenticate` returns an `Abandoned` rejection for abandoned accounts; `touch_activity` invoked on
  the authenticated view.

## Interface (`web`)

- Scheduler: add `lifecycle_rules: Arc<LifecycleRules>` to `Scheduler` + a `process_due_lifecycle` tick
  (mirrors the medal tick); `main`/`AppState` wire `lifecycle_rules` and pass world speed/`world_start`.
- Village view hook: `touch_activity` (throttled) + `end_protection_if_established`; the template shows
  **protection status** (protected + when it ends) when active.
- Map view: the viewport marks **inactive** villages (greyed) using the derived `is_inactive`.
- Login: an abandoned account is rejected with a clear message.

## Test strategy

| AC | Test |
|----|------|
| AC1 | infra (DB): `create_account` sets `protected_until = now + scaled(window)`; domain: `protection_expiry` scaling. |
| AC2 | app/infra (DB): `order_attack` on a protected target is rejected (`TargetProtected`); no movement row. |
| AC3 | infra (DB): launching an attack sets the attacker's `protected_until = now` (now attackable). |
| AC4 | app/infra (DB): a protected player crossing the population threshold has protection ended on view; does not re-arm. |
| AC5 | infra (DB): `touch_activity` updates when stale, no-ops when fresh; registration seeds `last_activity`. |
| AC6 | domain: `is_inactive` threshold/scaling; web: the map greys an inactive player's village. |
| AC7 | infra (DB): the sweep abandons accounts past the cutoff in one tx, deletes their villages (valley freed → resettlable), idempotent re-run; period-anchored cutoff. |
| AC8 | infra (DB): an abandoned account cannot log in; excluded from leaderboards; `users` row retained (a battle report referencing it still reads). |
| AC9 | web: protected player's village view shows protection status; map distinguishes inactive villages; no client mutator. |
| AC10 | domain/infra: determinism (same state + period → same sweep); config drives all timings; speed scaling. |

## Notes / open risks

- **Village deletion preserves battle history (AC8/P6).** Deleting an abandoned player's villages cascades
  most **village-scoped** child rows (resources/buildings/fields/units, in-flight movements, scout intel),
  but **battle reports survive**: migration `0030` makes `battle_reports.attacker_village`/`defender_village`
  and `battle_defenders.village_id` **`ON DELETE SET NULL`**, and the report carries fallback coordinates
  (`attacker_x/y`, plus the existing `defender_x/y`) so it stays readable with a deleted village. So a
  still-active opponent keeps its report **and its ranking points** when the other party is abandoned —
  abandoning one account never rewrites another's history. The **`users` row is kept** too (FKs like
  `alliances.founder_id` never dangle). Boards/stat pages then exclude abandoned accounts by a **read-time
  `abandoned_at IS NULL` filter**, not by destroying rows.
- **Alliance membership of an abandoned account** — left attached in this slice (safe: the user row is
  retained, so no FK breaks; a villageless retired member is cosmetic). Founder-transfer/auto-leave on
  abandonment is **deferred** (noted in spec open questions).
- **Protection gate placement.** Gate in `order_attack` (the single attack entry) via one PK-indexed
  `protection_of(dest.owner)` lookup + the pure `is_protected` compare — a single extra cheap query, not
  an N+1 (P11). Re-checking at resolution is unnecessary (protection only *shrinks*; a launch already
  validated stays valid).
- **`create_account` needs the protection window.** The repo computes `protected_until` from the lifecycle
  rules + world speed at spawn; thread the precomputed expiry (or the rules+speed) into the create path.
- **Phasing (T1–T6):** T1 pure domain (`lifecycle.rs`) + `lifecycle.toml` + loader; T2 migration 0029 +
  `users` columns + `AccountRepository` activity/protection methods + `create_account` grant, with DB
  tests; T3 beginner's protection in `order_attack` (gate + end-on-offence) + the lazy threshold end, with
  tests; T4 `LifecycleRepository` + `process_due_lifecycle` sweep + abandoned-login block, with DB tests;
  T5 web (scheduler tick + AppState/main wiring + village protection status + map greying + integration
  tests); T6 docs (`0021-protection-and-lifecycle.md`, manual) + reviewer + finalize.
