# Feature 016 — Ranking, leaderboards & statistics — Technical Plan

**Status:** Reviewed
**Spec:** ./spec.md

Ranking is the **competitive scoreboard**: public, filterable leaderboards (players by population; top
attackers / defenders / raiders; alliance aggregates) and per-player / per-alliance **statistics pages**,
plus the combat **amendment** that makes the defense metric faithful — **every defending player (owner +
each reinforcer) gets their own battle report**, and a battle's **defense points are split among
defenders by contributed defensive value** (GDD §9.6/§11.2). Population is **purely derived on read**
(P1). Attack/defense **points** and **loot** are **battle facts persisted at resolution** — exactly as
`loot_*` already is — so leaderboards are an indexed `SUM … ORDER BY … LIMIT` (P11) and the persisted
battles remain the single source of truth (P5). **No combat *math* changes** — only reporting + three
persisted scalars and a per-defender contribution row.

## Constitution check

- **P1 (event-driven / lazy):** no ranking tick, no recurring due-event, no denormalised rank/cache
  table. Population is computed on read via `domain::population` over current build rows. The per-defender
  reports + points are produced **inside the existing 009 battle due-event** (no new schedule). Time
  windows are `resolved_at ≥ now − W` filters at read time.
- **P2 / P6 (reproducible):** the per-defender contribution and the per-battle point yields are written
  **once**, in the **same crash-safe transaction** as the existing battle report (`apply_battle` is
  already keyed by `movement_id` for exactly-once). Points are a deterministic pure function of the
  battle's losses + balance (largest-remainder split has no randomness), so the same history yields the
  same standings.
- **P3 (pure domain):** the new rules are pure and unit-tested without I/O — `quadrant(Coordinate)`, the
  per-unit `point_value` lookup, `battle_points(losses, rules)`, and `apportion(total, weights)` (the
  largest-remainder defense split). The combat resolver only **gathers** per-defender forces/losses
  (already computed) and tags each with its defensive value from the existing `add_defense`.
- **P4 (server authority):** leaderboards/stat-pages are read-only server derivations; the client posts
  only scope+window+page (validated, bounded). The combat amendment is a System outcome at resolution. A
  reinforcer may read **only their own** contribution row — never the owner's full battle or others'
  troops; the read query is shaped to enforce this (no client-supplied report id trusted).
- **P5 (DB is truth):** points/loot are **facts about a battle** (like losses), not a separate ranking
  store; the leaderboard is a query over those facts + villages + memberships. Nothing caches an
  authoritative rank.
- **P7 (configurable):** per-unit **point values**, the **window lengths**, the **page size**, and (the
  quadrant rule is pure code) live in balance/world config — no hardcoded numbers. World speed does not
  scale points (they value troops, not time).
- **P11 (performance):** leaderboards are `SUM(points|loot|pop) … ORDER BY … LIMIT pageSize` over indexed
  columns (`battle_reports(attacker_player, occurred_at)`, `battle_defenders(player_id, occurred_at)`),
  not jsonb valuation per request. Population sums villages per player (indexed by owner). All reads are
  bounded by `leaderboardPageSize`.

> **Refinement of the spec's wording (flagged):** spec AC11/Decisions say points are "derived on read".
> For P11 on a populated world, valuing every battle's jsonb losses per request is too slow, so this plan
> **persists the point yield per battle/contribution at resolution** (a battle *fact*, identical in
> spirit to the already-persisted `loot_*`), and leaderboards SUM those facts. Population remains purely
> derived. This is also *more* faithful (Travian awards points at battle time) and resolves the
> value-tuning ambiguity (a later balance change does not rewrite history). **Spec decision/AC11 to be
> reworded to "points are the battle's persisted yield, summed on read"** before code (behavior-change
> rule). Called out for approval at the plan gate.

## Domain (`domain`, pure)

- `world.rs` — `enum Quadrant { Ne, Nw, Sw, Se }` + `pub fn quadrant(c: Coordinate) -> Quadrant` (sign
  rule from the spec: `x ≥ 0, y ≥ 0` → NE; `x < 0, y ≥ 0` → NW; `x < 0, y < 0` → SW; else SE — total,
  pure, P6). Unit-tested incl. the axis/origin ties.
- `ranking.rs` (new) — the pure point rules:
  - `RankingRules { point_value: HashMap<UnitId, i64>, windows: Vec<Duration>, page_size: usize }`
    (`Duration`/secs; `all-time` represented as the absence of a lower bound).
  - `fn battle_value(losses: &UnitCounts, rules: &RankingRules) -> i64` — `Σ count × point_value(unit)`.
    Attack points of a battle = `battle_value(defender_losses)`; the defense total =
    `battle_value(attacker_losses)`.
  - `fn apportion(total: i64, weights: &[i64]) -> Vec<i64>` — deterministic **largest-remainder**
    apportionment so the integer shares **sum to `total`** exactly (weights = each defender's defensive
    value; all-zero weights → even split / first-gets-remainder, documented). The crux defense-share
    rule, unit-tested with the spec's 3:1→75/25 example and rounding edge cases.
- `economy.rs` — no change; `population(fields, buildings, rules)` already exists and is reused as-is.
- (Defensive value per defender is the existing `add_defense` output (infantry+cavalry power) computed in
  `application::combat`; the domain just consumes the resulting weights in `apportion`.)

## Balance (`specs/balance/` + `infrastructure::balance`)

- `units.toml` — add `point_value` to each unit entry (faithful default ≈ its `crop_upkeep`, i.e. the
  troop's population value, GDD §11.2 — but an explicit, independently tunable field, P7). `UnitRule`
  (domain) gains `pub point_value: i64`; the unit DTO + `parse_unit_rules` gain the field (fail-fast if
  missing). A `point_value(unit) -> i64` lookup feeds `RankingRules.point_value`.
- `ranking.toml` (new) — `windows_days = [7, 30]` (plus implicit all-time) and `leaderboard_page_size =
  100`. Loaded fail-fast into `RankingRules` by `ranking_rules()` (mirroring `loyalty_rules()` /
  `alliance_rules()`), combined with the per-unit point values.

## Persistence (`infrastructure` + migration `0026_ranking.sql`)

- **Per-battle point yields (facts, like loot).** Add to `battle_reports`:
  `attack_points bigint not null default 0` (value of `defender_losses`, credited to `attacker_player`).
  Written at resolution. (Defense points are per-defender, below — not a single battle scalar, since they
  split.) Index `battle_reports(attacker_player, occurred_at desc)` already exists (reused for top
  attackers + raiders); add `battle_reports(occurred_at)` if window-only scans need it.
- **`battle_defenders` (new) — the per-defender contribution + its report.** One row per defending player
  in a battle (the owner's garrison **and** each reinforcing group):
  ```
  battle_defenders(
    id              uuid pk,
    battle_id       uuid not null references battle_reports(id) on delete cascade,
    player_id       uuid not null references users(id),
    village_id      uuid not null references villages(id) on delete cascade, -- the defender's home village
    is_owner        boolean not null,         -- true for the target's garrison owner
    forces          jsonb not null,           -- this defender's contributed troops
    losses          jsonb not null,           -- this defender's losses
    defense_value   bigint not null,          -- contributed defensive value (apportion weight)
    defense_points  bigint not null,          -- this defender's share of the battle's defense total
    occurred_at     timestamptz not null      -- copied from the battle for windowed SUMs without a join
  )
  ```
  Indexes: `(player_id, occurred_at desc)` (reinforcer inbox + top-defenders SUM), `(battle_id)` (render
  a battle's defenders). The owner is always a row (`is_owner = true`), so defense points are uniform
  across owner & reinforcers (no special-casing the owner downstream).
- **Write path:** `apply_battle` (repo.rs:2651) already receives `apply.reinforcement_losses` and inserts
  the `battle_reports` row (2700). Extend the same transaction to (a) set `attack_points`, (b) insert the
  `battle_defenders` rows from a new `apply.defender_contributions: Vec<DefenderContribution>` field.
  Still exactly-once (keyed by `movement_id`). Oasis battles (repo.rs:3023) are PvE (no player defender) —
  they insert **no** `battle_defenders` rows and `attack_points = 0` (wild animals carry no point value;
  Open question if we later value them).
- **Leaderboard queries (new repo reads):**
  - *Population:* `SELECT owner, SUM(pop)` where village population is computed — but population is a pure
    function of field/building levels not stored as a scalar. Plan: compute per-village population in the
    read by loading each player's villages' levels and applying `population()` (bounded by page size after
    an initial cheap ordering), **or** persist a maintained `villages.population` integer updated on every
    build apply (003) — see Notes (decision: derive on read first; optimise to a stored column only if P11
    needs it). Quadrant filter applies `quadrant(capital.coord)`.
  - *Top attackers:* `SELECT attacker_player, SUM(attack_points) … WHERE occurred_at ≥ $win [AND quadrant]
    GROUP BY … ORDER BY SUM DESC LIMIT $n`.
  - *Top defenders:* same over `battle_defenders(player_id, defense_points)`.
  - *Top raiders:* `SUM(loot_wood+loot_clay+loot_iron+loot_crop)` over `battle_reports` as attacker.
  - *Alliances:* join the player aggregates to `alliance_members` and `SUM` per `alliance_id`.
  - Quadrant scoping joins the relevant village/capital coordinate and filters by `quadrant()` (computed
    in Rust over a bounded candidate set, or via a SQL expression on `x`/`y` sign — decided in code).

## Application (`application`)

- **Combat amendment (`combat.rs` `resolve_one`):** build `defender_contributions: Vec<DefenderContribution>`
  — one for the owner (garrison forces/losses, `is_owner=true`) and one per reinforcement group (its
  forces from `reinforcements`, its losses from the existing `reinforcement_losses`, owner resolved from
  the group's home village — already available in the conquest path via `village_by_id`, or carried on the
  reinforcement row to avoid a lookup, P11). Each contribution's `defense_value` is the group's
  `add_defense` result (compute per-group into a fresh accumulator — the resolver already calls
  `add_defense` per group for `totals`). Compute `attack_points = battle_value(defender_losses_total)` and
  split the defense total with `apportion(battle_value(attacker_losses), weights)` across the
  contributions. Thread these through `BattleApply` (new fields `attack_points`,
  `defender_contributions`); **the battle outcome, casualties, loot, returns, conquest are untouched.**
- **`ports.rs`:** `DefenderContribution { player, village, is_owner, forces, losses, defense_value,
  defense_points }`; extend `BattleApply` with `attack_points` + `defender_contributions`; extend
  `NewBattleReport` mapping only if needed. Add a `RankingRepository` port: `population_board(scope,
  page)`, `attack_board/defense_board/raider_board(window, scope, page)`, `alliance_boards(window, scope,
  page)`, `player_stats(player)`, `alliance_stats(alliance)`, and `defender_reports_for(player, page)` (the
  reinforcer inbox). All return bounded, ranked DTOs.
- **`ranking.rs` (new use-cases):** thin read orchestration — validate the requested scope/window against
  `RankingRules` (reject unknown windows / oversized pages, P4/P7), call the repo, assemble the view. No
  writes. Quadrant/window are derived, never trusted as free-form from the client.

## Interface (`web`)

- **Leaderboard page(s)** (new handler + Askama templates, ui-style-guide): a category tab set (Population
  / Attackers / Defenders / Raiders / Alliances), a **quadrant** selector (All / NE / NW / SE / SW) and a
  **window** selector (All-time / 7d / 30d — from config), a ranked, paged table. **Public** (Visitor
  reaches it; no auth gate).
- **Statistics page** (player + alliance): population (with public per-village breakdown), attack/defense
  points, loot total, and battle history. Public; renders only public metrics (no troop counts /
  resources). Links from the map/profile tag (reuses 015's tag surfacing).
- **Reinforcer reports in the reports inbox:** the existing battle-report inbox gains the player's
  `battle_defenders` rows (battles where they reinforced) — each shows **their** forces/losses + the
  outcome, never the owner's full view (P4/§7.3). Server filters by `player_id`.
- All handlers are read-only GET; scope/window/page parsed + bounded server-side.

## Test strategy

| AC | Test |
|----|------|
| AC1 | domain/infra: a player's total population = Σ village `population()`; a conquered village (014) counts for the new owner on the next read. |
| AC2 | infra (DB): population board orders desc with the (pop, id) tie-break and the page bound; visible without auth (web). |
| AC3 | infra (DB): a battle defended by owner + 2 reinforcers writes **3** `battle_defenders` rows (forces/losses per player) in one tx; re-running the due-event (crash-resume) does not duplicate them; survivors still return (existing 007 tests stay green). |
| AC4 | domain: `battle_value` (valued sum) + `apportion` (3:1→75/25, sum-preserving, rounding edges); infra: a battle's `attack_points` = valued defender losses; the defenders' `defense_points` sum to the valued attacker losses. |
| AC5 | infra (DB): top-attackers / top-defenders over a window rank by summed points desc; a player with no battles in the window is omitted; all-time includes all. |
| AC6 | infra (DB): top-raiders ranks by summed loot as attacker over the window. |
| AC7 | domain: `quadrant()` truth table incl. axis/origin; infra: a quadrant-scoped board includes only that quadrant's players (by capital) / villages. |
| AC8 | infra (DB): alliance boards sum current members' population/points; a memberless player contributes to none; a disbanded alliance is absent. |
| AC9 | web/infra: a player's stats page shows pop/points/loot/history and **never** troop counts or resources. |
| AC10 | web/infra: an alliance stats page shows aggregates + roster contributions within 015 visibility rules. |
| AC11 | infra: recomputing a board over the same data+window+scope is identical; points persisted once at resolution; client cannot exceed the page bound or inject scope. |
| AC12 | infra (DB): a reinforcer reads their own `battle_defenders` report for a battle at a village they don't own, and gets **only** their row (not the owner's aggregate); web integration over a populated fixture stays within the latency budget. |

## Notes / open risks

- **Spec wording refinement (points persisted, not on-read).** The single deliberate divergence from the
  approved spec — surfaced above. If the reviewer/user prefers strictly on-read points, the fallback is a
  bounded valuation in SQL/Rust, but P11 on a large world argues for the persisted-fact design (consistent
  with `loot_*`). **Resolve at the plan gate; update spec AC11/Decisions before T-code.**
- **Population on the board: derive vs. store.** Deriving per-village population on read is clean (no new
  write path) but a full-world population *ranking* must consider every player. Plan: derive first with a
  bounded query shape; if P11 demands it, add a `villages.population` integer maintained on the 003 build
  apply (and settle/conquest), which makes the board a pure `SUM`+`ORDER`+`LIMIT`. Flagged as the main
  perf risk; pick during T-implementation against a populated fixture.
- **Owner is a `battle_defenders` row too.** Treating the owner as just another defender (with
  `is_owner=true`) keeps defense-point math uniform and the reinforcer/owner inbox queries identical. The
  legacy aggregate `battle_reports` row stays for the attacker/owner full-battle view and existing tests.
- **Oasis / PvE battles.** No player defender → no `battle_defenders` rows, `attack_points = 0` (wild
  animals carry no point value). If we later want PvE attack points, give animals a `point_value` (Open
  question in spec is adjacent). Keep the insert path tolerant of zero defenders.
- **Defensive-value weight basis.** `apportion` weights are each defender's `add_defense` value (the spec
  open question's proposed basis). Because the resolver applies a uniform `defender_loss_frac`, weighting
  by contributed defence (not by losses) is the faithful "who held the line" split — verify against 009's
  power model in T-domain.
- **Phasing (T1…T8), each green:** T1 — pure `quadrant` + `ranking.rs` (`battle_value`, `apportion`) +
  `units.toml` `point_value` + `ranking.toml`, test-first (P3). T2 — migration `0026` (`attack_points`,
  `battle_defenders`) + repo write in `apply_battle`. T3 — combat `resolve_one` builds
  `defender_contributions` + points through `BattleApply` (AC3/AC4; existing combat tests stay green).
  T4 — `RankingRepository` reads: population + attacker/defender/raider boards + quadrant/window (AC1–AC7).
  T5 — alliance boards + player/alliance stats + reinforcer inbox (AC8–AC10, AC12). T6 — web pages
  (leaderboards, stats, inbox surfacing). T7 — docs (architecture note + manual). T8 — reviewer.
