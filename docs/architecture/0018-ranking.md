# Ranking, leaderboards & statistics — the competitive scoreboard

**Status:** Current
**Date:** 2026-06-13 · **Slice:** 016

## Context
Ranking is the **competitive layer** (GDD §11.2): public, filterable leaderboards (players by
**population**; top **attackers** / **defenders** / **raiders**; alliance aggregates) plus per-player
and per-alliance **statistics pages**. To make the **defence** metric faithful (GDD §9.6/§11.2 — defence
points are "shared among all defenders present, including reinforcements"), this slice also **amends
combat (009)** so that every defending player — the garrison owner **and** each reinforcer — receives
their own battle report, and a battle's defence points are split among them by contributed defensive
value. Out of scope (→ 017): **top-climbers** and **population-over-time** charts (need population
snapshots that do not exist yet), **medals**/weekly awards, and **achievements**.

## Design
- **Population is derived on read (P1).** A village's population is the pure
  `domain::population(fields, buildings, rules)`; a player's total is the sum across their villages. No
  population is stored — the boards compute it from current build levels.
- **Points & loot are persisted battle facts (P2/P5), summed on read.** A battle's **attack points**
  (value of defender troops destroyed) live on `battle_reports.attack_points`; each defender's **defence
  points** live on a new `battle_defenders` row. They are computed **once at resolution** from balance
  (like the already-persisted `loot_*`), so leaderboards are an indexed `SUM … ORDER BY … LIMIT` (P11),
  the standings are reproducible (P2/P6), and a later balance change never rewrites awarded points. This
  is a deliberate refinement of the spec's "derived on read" wording — population stays derived; points
  are a persisted fact.
- **Pure ranking rules (`domain/ranking.rs`, P3).** `RankingRules` (per-unit `point_value`, the
  leaderboard windows, the page bound); `battle_value(losses)` = Σ count × point_value;
  `apportion(total, weights)` splits the defence total across defenders by contributed defensive value
  using **largest-remainder** apportionment so the integer shares sum exactly to the total (no points
  created or lost). `domain::quadrant(Coordinate)` is the total, pure sign rule for the region filter
  (NE/NW/SE/SW about the origin). All unit-tested without I/O.
- **The combat amendment (009).** `resolve_one` already computes each reinforcement group's losses (to
  send survivors home); 016 now also builds a per-defender **contribution** (the owner's garrison + each
  reinforcing player) tagged with the defensive value `add_defense` produced, computes `attack_points`,
  and `apportion`s the defence total — then threads them through `BattleApply`. `apply_battle` persists
  `attack_points` on the report and one `battle_defenders` row per contribution, in the **same
  exactly-once transaction** as the report (so crash-resume never duplicates). **No battle outcome
  changes** — only reporting plus the persisted facts. PvE/oasis battles have no player defenders, so
  they write no `battle_defenders` rows and `attack_points = 0`.
- **Persistence (migration 0026).** `battle_reports.attack_points bigint`; `battle_defenders`
  (battle → report cascade, player, village, `is_owner`, `forces`/`losses` jsonb, `defense_value`,
  `defense_points`, `occurred_at default now()`), indexed by `(player_id, occurred_at)` (reinforcer inbox
  + top-defenders) and `(battle_id)` (a battle's defenders). The owner is just another defender row
  (`is_owner = true`), so defence-point maths and the inbox query are uniform.
- **Read-side queries (`RankingRepository` on `PgAccountRepository`).** Population boards compute the
  per-village population in SQL from the balance tables passed as `unnest` array binds — one bounded,
  indexed query, no village data pulled into Rust (P11). Conflict boards `SUM` the persisted points/loot
  over a window (`occurred_at ≥ now − W`). Alliance boards aggregate over `alliance_members`. Quadrant
  scope filters by the player's / member's **capital** via a pure sign `CASE` in SQL. Every board is
  bounded by the configured page size; zero-activity players are omitted from conflict boards.
- **Use-cases (`application/ranking.rs`).** Thin read orchestration that validates the requested
  **window** against `RankingRules` (an arbitrary window is rejected — P4/P7) and bounds the page, then
  reads. No writes.
- **Web (public).** `/leaderboard` (category tabs + quadrant/window selectors), `/stats/player/{id}`,
  `/stats/alliance/{id}` — all reachable by a visitor (rankings are public, GDD §11.2) and exposing only
  public metrics, never troop counts or stored resources (P4/§7.3). A reinforcer's own contributions
  surface in the `/reports` inbox.

## Balance (P7)
- `units.toml` — an optional per-unit `point_value` (defaults to the unit's `crop_upkeep`, the faithful
  population value); `ranking.toml` — `windows_days = [7, 30]` and `leaderboard_page_size`.

## Consequences
- Battles now write `1 + (distinct reinforcing players)` defender rows; the legacy aggregate
  `battle_reports` row is kept for the attacker/owner full-battle view, so existing report behaviour is
  undisturbed.
- A reinforcer reads only **their own** contribution (defence points + their losses), never the owner's
  full battle (P4/§7.3 / AC12).
- Top-climbers and population history wait on 017's snapshot mechanism; their absence is the only GDD
  §11.2 board not delivered here.
