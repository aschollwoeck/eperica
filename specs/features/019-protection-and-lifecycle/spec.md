# Feature 019 — Protection & lifecycle

**Status:** Reviewed
**Depends on:** 009/011 (combat — the attack the protection gates), 002 (population — the early-end threshold), 001 (accounts/world — `users`, registration, the world clock), 006/015 (map valleys + villages — freed on abandonment), 016/017 (the population read + the state-driven recurring due-event pattern this reuses)
**Roadmap:** M6 · slice 019 · GDD §12.2–12.3 — a **live, fair, self-renewing world**: **beginner's protection** shields new players from attack for a (speed-scaled) window so they aren't spawn-camped, and an **inactivity lifecycle** turns long-abandoned accounts into farmable, then map-reclaimable, valleys.

## Goal

Two mechanics keep the world fair at the start and fresh over time:

- **Beginner's protection.** A freshly-registered player is **immune to attack** for a protection window
  (duration **scaled by world speed**, P7), ending early once they grow past a **population threshold** or
  the moment they **launch their own attack**. This prevents spawn-camping and gives newcomers a foothold.
- **Inactivity & abandonment.** Accounts that stop playing become **farmable** (flagged inactive, greyed on
  the map) and, after a long absence, are **abandoned**: their villages are **removed from the map** —
  freeing the valleys for resettlement — and the account is retired. This keeps the map live and
  reclaimable instead of clogged with dead villages.

Both are **server-authoritative** (P4) and **reproducible** from persisted state (P2/P6); the client never
decides who is protected, inactive, or abandoned.

## Concepts

- **Beginner's protection (an attack-immunity window).** At registration a player is granted protection
  until `protected_until = now + scaled(beginner_protection_secs)` (P7 — faster servers protect for less
  wall-clock). A player is **protected** while `now < protected_until`. Protection **ends early** (sets
  `protected_until = now`) when either: the player's **population reaches the threshold** (they're
  established), or the player **launches any attack/raid** (you can't shelter behind protection while
  attacking — the faithful rule). A protected player **cannot be attacked**; the attack is rejected
  server-side.

- **Activity signal.** Each authenticated player has a **`last_activity`** timestamp, refreshed when they
  act (authenticated view), throttled so it is a cheap conditional write, not write-per-request. It is the
  single source for the inactivity lifecycle.

- **Inactivity (stage 1 — farmable, derived).** A player is **inactive** iff
  `now − last_activity > scaled(inactive_after_secs)` and they are not yet abandoned. This is a **derived**
  read-time fact (no stored flag, no per-entity tick, P1): inactive villages are shown **greyed** on the
  map so active players can identify farms. (Inactive players are attackable like any non-protected
  player; inactivity makes them *discoverable*, it does not change combat rules.)

- **Abandonment (stage 2 — map reclaimed).** After a longer absence
  (`now − last_activity > scaled(abandon_after_secs)`), a **periodic sweep** retires the account: its
  **villages are deleted** (the valleys become free for resettlement) and the account is marked
  **abandoned**. Abandonment is a **soft-delete** of the account: the `users` row is **kept** so historical
  battle reports referencing the player stay referentially intact and auditable (P6), but the account
  **cannot log in** and is **excluded from leaderboards**. ("Deleted, villages return to the map" — GDD
  §12.3 — realized soundly: the map is renewed; history is preserved.)

- **The sweep (state-driven recurring due-event, P1).** Like the 017 weekly settlement, the sweep is
  **derived from a watermark, not a `scheduled_events` row**: the latest swept period is
  `MAX(inactivity_sweeps.period)`, and the scheduler tick settles any complete-but-unswept period. Each
  sweep is **atomic and idempotent** (the watermark + the deletions commit together; a re-sweep of the
  same period is a no-op). The deletion **cutoff is anchored to the period boundary**
  (`period_start(P+1) − scaled(abandon_after_secs)`), so a given period always abandons the same set from
  the same persisted activity data (P2/P6).

## User stories

- As a **new player**, I want to be **safe from attack** while I find my feet, so I'm not farmed into
  oblivion on day one.
- As a **new player**, I want my protection to **end when I'm established or when I go on the offensive**,
  so protection is a foothold, not a shield to abuse.
- As an **active player**, I want **abandoned accounts to become farmable and eventually free up the map**,
  so the world stays alive and there's room to expand.
- As an **administrator**, I want the **protection window, threshold, and inactivity timings** to be
  **config** (P7), so I can tune fairness per world.

## Acceptance criteria

> Protection, inactivity, and abandonment are **server-authoritative** (P4) and **reproducible** from
> persisted state (P2/P6). All durations/thresholds are **config** and the time-based ones **scale by world
> speed** (P7).

- **AC1 — New players are protected at spawn.** Registration sets `protected_until = now +
  scaled(beginner_protection_secs)`. While `now < protected_until` the player is **protected**.

- **AC2 — A protected player cannot be attacked.** An attack/raid whose target village is owned by a
  protected player is **rejected** server-side (a distinct error); no movement is created. (Scouting and
  other non-combat actions are unaffected — protection is **attack** immunity.)

- **AC3 — Protection ends early on offence.** The moment a player **launches** any attack/raid, their own
  protection ends (`protected_until ← now`); they are thereafter attackable. (Launching is what ends it —
  the faithful rule.)

- **AC4 — Protection ends early past the threshold.** When a protected player's **population reaches
  `protection_population_threshold`**, their protection ends (`protected_until ← now`), evaluated
  server-side from persisted state (lazily, on their authenticated view). After it ends it does not
  re-arm.

- **AC5 — Activity is tracked.** An authenticated player's `last_activity` is refreshed when they act
  (throttled to a cheap conditional write). Registration seeds `last_activity = now`.

- **AC6 — Inactive villages are flagged farmable.** A player with `now − last_activity >
  scaled(inactive_after_secs)` (and not abandoned) is **inactive**; their villages are shown **greyed /
  flagged** on the map. This is **derived on read** (no stored flag, no per-entity tick). Inactive players
  remain attackable under the normal combat rules.

- **AC7 — Abandonment frees the map, exactly once.** The periodic sweep, for each complete-but-unswept
  period, **deletes the villages** of every account past the abandon threshold (cutoff anchored to the
  period boundary) and marks the account **abandoned**, **in one transaction** with the period watermark.
  The freed valley tiles become **available for resettlement**. Re-running the sweep for a settled period
  is a **no-op** (idempotent); an already-abandoned account is never swept again.

- **AC8 — Abandoned accounts are retired soundly.** An abandoned account **cannot log in** and is
  **excluded from leaderboards/stat pages**, but its `users` row is **retained** so historical battle
  reports referencing it remain valid and auditable (P6).

- **AC9 — Interface.** A protected player sees their **protection status** (that they're protected and when
  it ends) on their village view; the map **distinguishes inactive (farmable) villages**. No client action
  can grant/extend protection or set inactivity/abandonment (P4).

- **AC10 — Authority, determinism & config (P2/P4/P6/P7).** Every protection grant/expiry,
  inactivity classification, and abandonment is produced server-side from persisted state; recomputing over
  the same state + period yields the same result. The protection window + threshold and the
  inactive/abandon timings + sweep cadence are **config**; time-based durations scale by world speed.

## Roles & permissions

Per [roles.md](../../roles.md). Protection and the lifecycle are **system-administered** account state; no
player can set them.

| Role | Permitted | Denied (server-enforced) |
|------|-----------|--------------------------|
| **Visitor** | N/A (considered) — no account, no protection/activity state. | Any of the below. |
| **Player** | See **their own** protection status; see **inactive (farmable)** villages on the map; **end their own** protection by growing past the threshold or by attacking. | Attacking a **protected** player; granting/extending **their own or others'** protection; setting inactivity/abandonment; logging into an **abandoned** account. |
| **Moderator** | N/A (considered) — no lifecycle moderation surface in this slice. | — |
| **Administrator** | Configure (balance, P7) the protection window + threshold and the inactive/abandon timings + sweep cadence. | Setting per-account protection/abandonment from the client per-request. |
| **System** | *(system-initiated)* Grant protection at spawn; end it on offence/threshold; classify inactivity (derived); run the **abandonment sweep** (atomic, idempotent, period-anchored). | — |

## Out of scope

- **Account sitting, vacation mode, anti-multi-account enforcement** — acknowledged but deferred (GDD §12.4,
  P10); the account model just leaves room for them.
- **Gradual building/troop decay of inactive villages.** 019 uses the **two-stage** model (inactive ⇒
  farmable as-is; abandoned ⇒ villages removed). Inactive villages are **not** progressively degraded —
  they are a frozen, farmable snapshot until abandonment. (Decided.)
- **Hard-deleting the `users` row.** Retained for referential/audit integrity (P6); abandonment removes the
  *villages* and retires the *account*.
- **Re-activation / un-abandonment flows, abandonment notifications/emails** — app-layer
  ([social-and-meta-features.md](../../social-and-meta-features.md)).
- **New combat/economy mechanics** — 019 only **gates** an attack on the target's protection and **removes**
  villages on abandonment via existing paths; it changes no battle/economy rule.

## Decisions

- **Reuse the 017 recurring-settlement pattern for the sweep.** Watermark-derived latest period
  (`MAX(inactivity_sweeps.period)`), scheduler tick settles complete-but-unswept periods, atomic +
  idempotent, period-anchored cutoff for reproducibility — the medal settlement is the explicit template
  for periodic work.
- **Stage 1 (inactive) is derived, not stored (P1).** Computed from `last_activity` on read — for map
  greying and any farmable indicator — so there is no per-entity tick and no flag to keep in sync.
- **Stage 2 (abandonment) is a soft-delete of the account + hard-delete of its villages, preserving battle
  history.** Villages gone ⇒ valleys freed (the map renews); `users` row kept; **battle reports + defender
  contributions survive** (their village references become `ON DELETE SET NULL`, with fallback coordinates
  on the report) so a still-active opponent keeps its report and ranking points (P6) — abandoning one
  account never rewrites another's standings. The account is flagged `abandoned_at`, which blocks login and
  excludes it from boards/stat pages via a **read-time filter** (not by destroying rows).
- **Protection scales by speed; thresholds/cadence are config (P7).** `protected_until` uses
  `scaled(beginner_protection_secs)`; the population threshold ends protection lazily on the authenticated
  view (same hook style as 017/018). Attacking ends protection in the attack use-case itself.
- **Protection check is a single timestamp compare on the hot path (P11).** `order_attack` compares the
  target owner's `protected_until` to `now`; the "end early" conditions are handled by writes at their
  moments (attack launch; threshold crossing on view), keeping attack validation O(1).
- **Balance (P7).** `lifecycle.toml` — `beginner_protection_secs`, `protection_population_threshold`,
  `inactive_after_secs`, `abandon_after_secs`, `sweep_interval_secs`. Loaded fail-fast like the other rules.
- **New persistence.** `users` gains `protected_until timestamptz NULL`, `last_activity timestamptz NOT
  NULL DEFAULT now()`, `abandoned_at timestamptz NULL`. New `inactivity_sweeps (world_id, period, swept_at,
  abandoned_count, PRIMARY KEY (world_id, period))` is the sweep watermark. Migration `0030` makes the
  `battle_reports`/`battle_defenders` village references `ON DELETE SET NULL` and adds `attacker_x/y`
  fallback coordinates so reports outlive a deleted village.

## Open questions

- **Activity throttle granularity.** How stale before `last_activity` is rewritten on a view? **Proposed:**
  a small config-free interval (e.g. rewrite only if older than a few minutes) so it's a cheap conditional
  `UPDATE … WHERE last_activity < now() − interval`; the exact interval is an implementation constant, not
  game balance.
- **Abandonment vs alliance membership.** A swept account may be an alliance member/founder. **Proposed:**
  the sweep first **detaches** the account from its alliance (leave; founder transfer/disband per the 015
  rules) within the same transaction, then removes villages + flags abandoned — so no dangling membership
  and no FK failure.
- **Protection threshold metric.** Population vs ranking points for the early end? **Proposed:**
  **population** (already computed on the view, simplest faithful signal); revisit if points are wanted.
