# Feature 017 — Medals & achievements

**Status:** Verified
**Depends on:** 016 (ranking — the leaderboards/stats the weekly medals are awarded from, and the population/points facts), 009/011 (combat + loot — defensive wins and raider stats), 012 (oases — the "occupy an oasis" milestone), 013 (settling — village count + the capital that receives resource rewards; culture points for CP rewards), 004 (research — the "research every unit" milestone), 001 (the due-event scheduler the weekly settlement rides; accounts/world)
**Roadmap:** M6 · slice 017 · GDD §11.2 — the **prestige layer**: a **weekly medal settlement** (the first **recurring** scheduled due-event) awards permanent **medals** to the top performers in each leaderboard category, and **achievements** (one-time milestone badges, with optional rewards) are granted server-side as players cross thresholds. This slice also **closes the 016 deferral**: the weekly **population snapshot** it introduces powers the **top-climbers leaderboard** and the **population-over-time** stat-page chart.

## Goal

The competition gains **lasting prestige**. At a fixed **weekly boundary** the server runs a **settlement**
(a recurring due-event, P1) that takes a **population snapshot** of every player and awards **medals** to
the top performers of the week in each category — **top attacker, defender, raider, climber, and top
alliances** — as **permanent account decorations**. Independently, **achievements** are one-time
**milestone badges** the server grants the instant a player crosses a defined threshold (found a 2nd
village, win N defensive battles, occupy a first oasis, reach population N, research every unit of your
tribe), each optionally carrying a small **reward** (resources or culture points). Both are
**game-rule grants** — server-authoritative (P4), idempotent, and reproducible from persisted state
(P2/P6). Because the snapshot now exists, this slice also delivers the **016-deferred** top-climbers
leaderboard and the population-over-time chart.

## Concepts

- **The weekly settlement (the first recurring due-event, P1).** A new scheduled event fires at each
  **weekly boundary** (period length is **config**, P7, anchored at the world's creation). When it runs
  it, in **one transaction**: (a) writes a **population snapshot** for every player; (b) **awards medals**
  for the just-ended period; and (c) **re-schedules itself** for the next boundary. The period is
  identified by an integer index from the world start, so the settlement is **idempotent** — reprocessing
  the same period (crash-resume) awards nothing twice and writes one snapshot set.

- **Population snapshot.** A per-player record of total population (016's `domain::population` summed over
  villages) **at a boundary**, tagged with the period. Snapshots are the **history** that current-value
  population (016) lacked: they power the **climber** metric (population gained between two boundaries)
  and the **population-over-time** chart.

- **Medals (permanent account decorations).** For each category the settlement ranks the period's
  performers and awards the **top `medalsPerCategory`** (config) a medal recording the **category**,
  **rank**, and **period**. Categories:
  - **Attacker / Defender / Raider** — most attack points / defense points / resources looted **within the
    period** (from 016's persisted battle facts).
  - **Climber** — most **population gained** over the period (this snapshot − the previous one). The
    **first** settlement only establishes the baseline snapshot — no climber medal that period.
  - **Alliances** — top alliances by aggregate member population and by aggregate attack/defense points
    over the period; an alliance medal is awarded to the **alliance** (shown on its page).
  Ties use a deterministic, documented rule (value desc, then ascending id). Medals are **permanent** —
  once awarded they are never revoked or recomputed; they accumulate on the account.

- **Achievements (one-time milestone badges).** A **catalogue** (balance data, P7) of milestones, each a
  **predicate** over a player's persisted progress plus an optional **reward**. The server **grants** an
  achievement the first time its predicate holds — **idempotently** (a player holds each achievement at
  most once) and **from persisted state** so the same history yields the same badges (P2/P6). The seed
  catalogue covers the GDD examples: **2nd village**, **N defensive wins**, **first oasis**, **population
  N**, **research every unit of your tribe**.

- **Achievement rewards.** An achievement may carry a one-time reward — **resources** (credited to the
  player's **capital**, capped by its stores) or **culture points** — applied **exactly once**, in the
  same transaction as the grant.

- **Top-climbers leaderboard & population history (the 016 deferral).** With snapshots in place, a
  **top-climbers** leaderboard (population gained over the latest period) joins the 016 board set
  (quadrant-filterable, bounded), and the player **statistics page** gains a **population-over-time**
  series from the snapshots.

## User stories

- As a **player**, I want to **earn medals** for topping a weekly category, so my best weeks are
  permanently recognized.
- As a **player**, I want **achievements** as I hit milestones (and small rewards), so progress is
  rewarded and visible.
- As **anyone**, I want to see a player's **medals and achievements** on their profile, and the
  **top-climbers** board, so prestige and momentum are public.
- As an **alliance**, we want **alliance medals** for topping the aggregate boards, so group success is
  recognized.
- As an **administrator**, I want the **period, categories, medal counts, tie-breaks, and the achievement
  catalogue** to be **config**, so I can tune the world without code.

## Acceptance criteria

> All grants are **server-authoritative** (P4) and **reproducible** from persisted state (P2/P6); the
> client cannot self-award. The settlement is **idempotent** per period (P1/P2). Periods, categories,
> medal counts, tie-breaks, and the achievement catalogue + rewards are **config** (P7).

- **AC1 — The settlement is a recurring due-event (P1).** A scheduled event fires at each weekly boundary
  (period length config in **real time**, anchored at world creation — **not** speed-scaled, faithful to
  Travian; see Decisions); when it runs it **re-schedules itself** for the next boundary. There is exactly
  one pending settlement at a time. No entity is ticked — it is a single due-event per period.

- **AC2 — Population snapshot per period (P2).** Running the settlement for period `P` writes **one**
  population snapshot per player (their total population at the boundary), tagged `P`. Re-running `P`
  (crash-resume) does not duplicate snapshots. Snapshots are reproducible from persisted build state.

- **AC3 — Medals awarded to the top performers per category (P4/P7).** For period `P` the settlement
  awards, in each category (attacker, defender, raider, climber, alliances), the **top
  `medalsPerCategory`** performers a medal recording category + rank + period, ranked by the period's
  stat with the deterministic tie-break. A category with no qualifying performers awards none. Awards are
  computed server-side from persisted facts only.

- **AC4 — Climber from the snapshot delta.** The **climber** ranking for period `P` is each player's
  `population(snapshot P) − population(snapshot P−1)`, descending. The **first** settlement (no `P−1`)
  establishes the baseline and awards **no** climber medal. Players present only at `P` (new since `P−1`)
  rank by their growth from zero (their first snapshot) — *(see Open questions)*.

- **AC5 — Medals are permanent, persisted decorations (P2).** An awarded medal persists on the account
  forever; it is never revoked or recomputed by a later settlement. A player accumulates medals across
  periods; an alliance medal is recorded against the **alliance**.

- **AC6 — Settlement is idempotent (P1/P2).** Processing period `P` twice (a crash between award and
  marking the event done) yields the **same** medals and snapshots — no double award — enforced by a
  per-period uniqueness on (period, category, rank) and (period, player snapshot).

- **AC7 — Achievement catalogue with predicates + rewards (P7).** The achievement catalogue is balance
  data: each entry has an **id**, a **milestone predicate** over a player's progress (village count,
  defensive wins, oases held, population, units researched of their tribe), and an **optional reward**
  (resources or culture points). The catalogue is loaded fail-fast.

- **AC8 — Achievements are granted once, server-side, idempotently (P4/P2/P6).** When a player's persisted
  progress first satisfies an achievement's predicate, the server **grants** it — recorded once per
  (player, achievement) so it cannot be granted twice — computed from persisted state so the same history
  yields the same achievements. A player cannot grant themselves an achievement.

- **AC9 — Achievement reward applied exactly once.** If an achievement carries a reward, granting it
  **also** applies the reward in the **same transaction**: resources are credited to the player's
  **capital** (capped by its warehouse/granary), or culture points are added. The reward never applies
  twice (tied to the one-time grant).

- **AC10 — The seed achievements work.** Given the seed catalogue: founding a **2nd village** (013),
  reaching **N defensive wins** (009 — battles where the player defended and the attacker lost),
  **occupying a first oasis** (012), reaching **population N** (016), and **researching every unit of the
  player's tribe** (004) each grant their badge the first time the milestone is crossed, and not before.

- **AC11 — Top-climbers leaderboard & population history (closes the 016 deferral).** A **top-climbers**
  board ranks players by population gained over the **latest** settled period (from snapshots),
  quadrant-filterable and bounded by the page size (P11). The player **statistics page** shows a
  **population-over-time** series from that player's snapshots. Both are **public** and derived from
  persisted snapshots.

- **AC12 — Public display, private state hidden (P4/§7.3).** Medals, achievements, the climbers board, and
  the population history are **public** (Visitor-visible) on the relevant pages; they expose no private
  state (current troop counts, stored resources) beyond what 016 already shows.

- **AC13 — Authority, determinism & config (P2/P4/P6/P7).** Every medal/achievement/snapshot is produced
  server-side, exactly once, from persisted state; recomputing over the same history yields the same
  result. Period length, category set, `medalsPerCategory`, tie-break, and the achievement catalogue +
  rewards are config — no hardcoded values.

## Roles & permissions

Per [roles.md](../../roles.md). Medals/achievements are **System-granted** prestige; players earn but
never self-grant; the displays are **public**.

| Role | Permitted | Denied (server-enforced) |
|------|-----------|--------------------------|
| **Visitor** | View **public** medals, achievements, the **top-climbers** board, and population-over-time on profile/stat/leaderboard pages. | Earning/granting anything; reading private state through these views. |
| **Player** | **Earn** medals (by topping a weekly category) and **achievements** (by crossing milestones) and receive their **rewards**; view all of the above. | **Self-granting** a medal/achievement or reward; influencing the settlement; granting to others. |
| **Moderator** | N/A (considered) — revoking medals for fair-play is a later moderation slice (022). | — |
| **Administrator** | Configure (balance/world, P7) the **period length**, **category set**, **`medalsPerCategory`**, **tie-break**, and the **achievement catalogue + rewards**; superset of read access. | Setting these per-request from the client. |
| **System** | *(system-initiated)* At each weekly boundary, **snapshot** population, **award** the period's medals, and **re-schedule** the next settlement — **idempotently** per period (P1/P2/P4). **Grant** achievements + apply rewards the first time a player's persisted progress crosses a milestone, exactly once. | — |

## Out of scope

- **Profile/medal visual design** — the *presentation* of medals/achievements (icons, profile layout,
  showcases) is **app-layer** ([social-and-meta-features.md](../../social-and-meta-features.md)); 017
  delivers the **grants + data + functional public views**, not the cosmetic profile.
- **Notifications** of a new medal/achievement (in-app/email) — app-layer; deferred.
- **Moderation** (revoking a medal for cheating) — moderation slice (022).
- **Hero / quests** — quests are slice **018**; the hero is out of the faithful baseline here.
- **Retroactive achievement backfill semantics beyond "from persisted state"** — achievements are
  granted going forward as milestones are crossed; whether a freshly-deployed catalogue grants
  already-eligible players immediately or on their next relevant action is an Open question.
- **New combat/economy math** — 017 only **reads** stats and **grants** decorations + small rewards via
  existing credit paths; it changes no battle/economy formula.

## Decisions

- **The settlement is a self-rescheduling due-event keyed by period index (P1/P2).** Period
  `P = floor((now − world_start) / period)`; the processor handles `P`, then schedules the next event at
  `world_start + (P+1) × period`. All of period `P`'s effects (snapshot + medals) are written in one
  transaction with per-period uniqueness, so a crash-resume re-run is a no-op. This introduces the first
  **recurring** event; the pattern (process-then-reschedule) is the template for future periodic work.
- **Period is real-time, not speed-scaled (decided).** `period` is config seconds (faithful default 7
  days) and is applied in **real time** regardless of world speed — faithful to Travian, where weekly
  medals settle on the wall clock even on fast servers. This is a **deliberate exception** to the
  engine's speed-scaling: P7 is satisfied (the period is config, no hardcoded value), and the exception is
  documented here so it isn't mistaken for a bug. (World speed still scales everything the medals are
  *awarded from* — battles, building — so faster worlds simply pack more activity into each real week.)
- **Medals are persisted facts, never recomputed.** A `medals` row (period, category, rank, subject) is
  written once at settlement and read thereafter — like 016's points. Subject is a **player** or an
  **alliance**. Permanent (no revoke).
- **Period-bounded stat reads.** Conflict medals rank over `[boundary(P−1), boundary(P))` — the
  settlement reads each board with both a lower and upper time bound (extending 016's windowed read with
  an upper bound) so the award is reproducible for that exact period.
- **Achievements: a pure catalogue + an idempotent evaluator.** The catalogue is pure domain data
  (predicates over a `PlayerProgress` value: `village_count`, `defensive_wins`, `oases_held`,
  `population`, `units_researched` / `tribe_unit_count`); the application gathers `PlayerProgress` from
  persisted state and `evaluate_achievements(player)` grants any newly-satisfied, not-yet-held
  achievements (DB unique `(player, achievement)` ⇒ exactly once) + applies rewards in one transaction.
  It is invoked at the natural **hook points** (after a battle resolves, a village is founded, an oasis is
  occupied, a research completes) and is cheap + idempotent, so over-invocation is harmless.
- **New persistence.** `population_snapshots (world, player, period, population, taken_at)` (unique
  `(world, player, period)`); `medals (id, period, category, rank, subject_kind, subject_id, awarded_at)`
  (unique `(period, category, rank)`); `player_achievements (player, achievement_id, granted_at)` (PK
  `(player, achievement_id)`). The `EventKind` enum gains the settlement variant (+ its string mapping).
- **Balance (P7).** `medals.toml` (period, `medals_per_category`, the active categories, tie-break) and
  `achievements.toml` (the catalogue: id, predicate parameters, optional reward), loaded fail-fast like
  `ranking.toml`.

## Open questions

- **Climber baseline for new players.** A player whose first snapshot is at period `P` has no `P−1`
  baseline. **Proposed:** their period-`P` climber value is their full population at `P` (growth from
  zero) — simple and rewards fast starters; alternative is to exclude them until they have two snapshots.
- **Catalogue change after launch.** If the achievement catalogue gains an entry mid-world, do already-
  eligible players get it immediately or on their next relevant action? **Proposed:** on their **next
  relevant action** (the evaluator runs at hook points) — avoids a world-wide backfill sweep; a player
  viewing their profile can also trigger a lazy evaluation.
- **Alliance medal subject vs members.** Award the alliance medal to the **alliance** (shown on its page)
  only, or also decorate each member? **Proposed:** the **alliance** only (members share its glory via
  the alliance page) — bounds the data and matches "top alliances".
- **`medalsPerCategory` default.** How many medals per category per week (1 / 3 / 10)? **Proposed:** **3**
  (gold/silver/bronze feel) — config, easily tuned.
