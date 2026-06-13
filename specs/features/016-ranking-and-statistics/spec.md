# Feature 016 — Ranking, leaderboards & statistics

**Status:** Verified
**Depends on:** 009 (combat — `battle_reports` are the source of attack/defense kills; **016 amends 009** to emit a report per defending player), 007 (troop movement — the reinforcement groups whose per-owner forces/losses become per-defender reports), 011 (siege & loot — the per-battle `loot_*` that powers top-raiders), 014 (conquest — ownership transfer; rankings follow current ownership), 015 (alliances — `alliance_members`, the grouping the alliance boards aggregate over), 013 (settling — a player's population is summed across **all** their villages; the capital pins their quadrant), 006 (world map — the `Coordinate` whose sign gives the quadrant filter), 002/003 (resource fields + buildings — the built levels population derives from)
**Roadmap:** M6 · slice 016 · GDD §11.2, §9.6 — the **competitive scoreboard**: a public, filterable view of who is winning, on what axis, where, and over what window. Population is the headline metric; battle points and loot are the conflict metrics; alliances aggregate their members. The ranking views are **derived on read** (P1/P2) — no ranking tick. To make **defense points faithful** (GDD §11.2: "shared among all defenders present, including reinforcements"), 016 also **amends combat (009)** so that **every defending player** — the village owner *and* each reinforcer — receives their **own battle report**, and defense points are split by each defender's contribution.

## Goal

The world stops being parallel solitaire and becomes a **competition with a visible scoreboard**. A
player (or anyone) can see **public leaderboards** — players by **population**, top **attackers**, top
**defenders**, top **raiders**, and the same for **alliances** — filterable by **map quadrant** and by
**time window**, and can open a **statistics page** for any player or alliance showing population,
attack/defense points, loot totals, and battle history. Every ranking number is **derived from
persisted game state on read** (P2), so the same history always yields the same standings (P2/P6).

To make the **defense** metric faithful (GDD §11.2), this slice also closes a combat gap: when a battle
is defended by a village's garrison **plus reinforcements from other players**, **each defending player
now receives their own battle report** (the reinforcer sees what happened to *their* troops, faithful
Travian §9.6), and the battle's **defense points are shared among the defenders in proportion to the
defensive value each contributed**. This slice therefore delivers the read-side ranking layer **plus**
the per-defender combat-report change it rests on. It does **not** introduce medals/weekly awards or
achievements (017), nor any population time-series (top-climbers / population-over-time charts), which
need snapshots and land with 017.

## Concepts

- **Population (the headline metric).** A village's population is a pure function of its built **resource
  field** and **center building** levels (the existing `domain::population(fields, buildings, rules)`,
  GDD §11.2). A **player's total population** is the sum across **all** their villages (013); it is the
  primary ranking axis and the public measure of a player's size. Population is a **current** value
  derived on read — 016 ranks the *current* standing, not its change over time (deltas need snapshots →
  017).

- **Per-defender battle reports (the combat amendment, GDD §9.6).** A defence is the target's **garrison**
  (the owner's troops) plus zero or more **reinforcement groups**, each owned by a (possibly different)
  player (007/009). The combat resolver **already computes each group's own losses** (`reinforcement_losses`,
  used to send survivors home) — today they are merged into one aggregate report attributed to the
  village owner. 016 changes this so that **each distinct defending player** receives a battle report:
  - the **village owner**'s report shows the **whole** battle (all defending forces/losses, as today —
    unchanged for the owner);
  - **each reinforcing player** additionally receives a report for that battle showing **their own**
    contributed forces, **their own** losses, and the outcome;
  - each defending player's **contribution** (forces, losses, and defensive value) is **persisted** (P2)
    so the reinforcer's report is reproducible and defense points are derivable.
  The attacker side is unchanged: an attack/raid has a **single** attacker (reinforcements are
  defence-only), so attack points need no split.

- **Battle points (the conflict metrics).** Earned by **destroying enemy military value**, valued by a
  per-unit **point value** (balance data, P7 — the numbers live in balance, not this spec):
  - **Attack points** — credited to the **single attacker**, equal to the summed point value of **all
    defender units killed** in that battle (the battle's total `defender_losses`).
  - **Defense points** — for a battle, the total is the summed point value of the **attacker units
    killed** (`attacker_losses`); this total is **split among the defending players in proportion to the
    defensive value each contributed** to the battle (the per-defender contribution above). The shares
    sum to the battle total — no points lost or double-counted.
  Both are **derived on read** by aggregating persisted battle data over the selected window — there is
  **no** running points counter.

- **Loot (the raider metric).** The **resources looted** in a raid/attack are already persisted per
  battle (`battle_reports.loot_*`, 011). A player's **loot total** over a window is the sum of those
  amounts across the battles they won as attacker. Powers the **top-raiders** board.

- **Leaderboards (public, filterable).** Ranked lists, each over a **scope** (whole world or one **map
  quadrant**) and, for the conflict boards, a **time window**:
  - **Players by population** (current standing) — *quadrant-filterable; window N/A (current value).*
  - **Top attackers** (attack points) · **Top defenders** (defense points, post-sharing) · **Top
    raiders** (loot) — *quadrant- and window-filterable.*
  - **Alliances** — by **aggregate member population**, and by **aggregate attack / defense points**.
  - *(Top-climbers — population gained over a period — is GDD §11.2 but **deferred to 017**: it needs
    population snapshots that don't exist yet. See Out of scope.)*

- **Map quadrant (the region filter).** A faithful Travian-style region: the world's four quadrants
  about the origin (NE / NW / SE / SW), derived **purely** from a `Coordinate`'s sign (P6, no storage). A
  **village**'s quadrant is its coordinate's quadrant; a **player**'s quadrant (population board) is the
  quadrant of their **capital** (013). The boundary rule (axis/origin) is fixed and documented (see
  Decisions).

- **Time window.** Conflict boards rank over a window — **all-time** plus rolling windows (e.g. 7-day,
  30-day) whose lengths are **config** (P7) — filtering battles by resolution time. The population board
  has **no** window (population is a current value; its history is 017).

- **Statistics page (per player / per alliance).** A public page deriving, from persisted state (P2):
  total population (with a public per-village breakdown), attack/defense points, loot total, and **battle
  history** (the player's resolved battles — including the per-defender reports they now receive). For an
  **alliance**: aggregate population and points plus the member roster contribution. **Private state is
  never exposed** — current troop counts and stored resources stay hidden (P4/§7.3); only the **public
  ranking metrics** and **already-visible** battle reports appear.

## User stories

- As **anyone (incl. a Visitor)**, I want to see **public leaderboards** of the top players and
  alliances, so I can read the state of the competition.
- As a **player**, I want to **filter** a leaderboard by **my quadrant** and by a **time window**, so I
  can see who matters near me and who's active *now*, not just all-time.
- As a **player who reinforced an ally**, I want **my own battle report** when that village is attacked,
  so I see what happened to my troops — and I want the **defense points** I earned, so defending allies
  is rewarded.
- As a **player**, I want a **statistics page** for any player or alliance (population, points, loot,
  battle history), so I can scout rivals and judge allies.
- As a **member of an alliance**, I want **alliance leaderboards** (aggregate population and points), so
  our group's collective standing is measurable.

## Acceptance criteria

> Rankings are **public** (GDD §11.2). **Population** is **derived on read** from current build state
> (P1). **Attack/defense points and loot** are a **battle's persisted yield** — computed **once at
> resolution** and stored as facts (exactly like `loot_*`), so leaderboards **sum persisted facts**
> rather than re-valuing history; this is also faithful (points are awarded when the battle happens) and
> survives later balance tuning (P2/P6). There is **no** ranking tick, **no** separate rank/aggregate
> cache. The combat amendment (AC3) is a **server-authoritative** (P4), exactly-once outcome of battle
> resolution (P1 due-event). Balance numbers (per-unit point value, window lengths, page size, quadrant
> rule) are **config** (P7). Read paths respect the latency budget (P11).

- **AC1 — Population is the player metric (derived).** Given a player owning villages whose built
  field/building levels yield village populations `p1…pn` (via `domain::population`), then the player's
  **total population** is `Σ pᵢ`, computed on read from current build state. A village transferred by
  conquest/settling (014/013) counts for its **current** owner on the next read.

- **AC2 — Players-by-population leaderboard.** When the **population** board is requested, players are
  returned **ranked by total population descending**, with a deterministic tie-break (population, then
  ascending player id) and a bounded page size (config, P7). The board is **public** (a Visitor sees it).

- **AC3 — Per-defender battle reports (combat amendment, GDD §9.6).** Given an attack/raid on a village
  defended by the **owner's garrison** plus reinforcement groups owned by players `P1…Pk`, when the
  battle resolves (009 due-event), then:
  - the **owner** receives a report describing the **whole** battle (all defending forces and losses, as
    before);
  - **each** reinforcing player `Pi` receives a report for the battle recording **`Pi`'s contributed
    forces**, **`Pi`'s losses**, and the outcome;
  - each defending player's **contribution** (forces, losses, defensive value) is **persisted**, computed
    **exactly once** (crash-safe like the existing resolution, P1/P2), and surviving troops still return
    home per 007 (**unchanged**). The attacker's report is **unchanged** (single attacker).

- **AC4 — Attack & defense points from valued kills, defense shared (P7).** For a resolved battle with
  persisted attacker/defender losses and a per-unit **point value** `value(unit)` (balance, P7):
  - the **attacker** is credited **attack points = Σ value(defender unit killed)** (the whole battle);
  - the battle's **defense-point total = Σ value(attacker unit killed)** is **split among the defending
    players in proportion to each player's contributed defensive value**, so the shares **sum exactly** to
    the total (deterministic rounding, see Decisions). *Example:* if defenders A and B contributed defence
    in a 3:1 ratio and the attacker lost troops worth 100 points, A earns 75 and B earns 25.
  Points are **aggregated on read** from the persisted per-defender contributions and attacker rows; no
  battle is double-counted; a battle with zero enemy losses yields zero points.

- **AC5 — Top-attackers & top-defenders leaderboards (windowed).** When the **attackers** (resp.
  **defenders**) board is requested for a **time window** `W`, players are ranked by their summed attack
  (resp. defense) points over battles **resolved within `W`**, descending, with the AC2 tie-break and
  page bound. **All-time** is the window over all history. A player with no qualifying battles in `W` is
  **omitted** (see Open questions).

- **AC6 — Top-raiders leaderboard (windowed).** When the **raiders** board is requested for window `W`,
  players are ranked by **total resources looted** (`Σ loot_*` as attacker over battles in `W`),
  descending. The ranking key is the **sum of the four resources** (one scalar); the per-resource split
  may appear on the stat page (AC9).

- **AC7 — Quadrant filter (pure, reproducible).** Every board accepts a **scope**: whole **world** or one
  **quadrant** (NE/NW/SE/SW). A **village**'s quadrant is the sign of its `Coordinate` about the origin; a
  **player**'s quadrant (population board) is their **capital**'s quadrant (013). The mapping is a **pure
  function** (P3/P6) with a fixed boundary rule (Decisions) — no stored region column. A quadrant-scoped
  board ranks only the players/villages in that quadrant.

- **AC8 — Alliance leaderboards (aggregate over members).** Given alliances with members (015), when an
  **alliance** board is requested, alliances are ranked by **aggregate member population**, and
  (separately) by **aggregate member attack / defense points** over the window, descending with a
  deterministic tie-break. A player's contribution counts for their **current** alliance; a player in no
  alliance contributes to no alliance row; disbanded alliances (015) do not appear.

- **AC9 — Player statistics page (public metrics only).** Given any player, their **stats page** shows:
  **total population** (with a public per-village breakdown — name/coordinate/population), **attack
  points**, **defense points**, **loot total**, and a **battle history** (their resolved battles,
  including the per-defender reports from AC3). It must **not** expose current **troop counts** or
  **stored resources** (P4/§7.3). The page is **public**.

- **AC10 — Alliance statistics page.** Given any alliance (015), its **stats page** shows **aggregate
  population**, **aggregate attack/defense points**, **loot total**, and the **member roster** with each
  member's public contribution. It exposes no member's private state beyond existing 015 shared-visibility
  rules and the public ranking metrics.

- **AC11 — Authority, persistence & determinism (P2/P4/P5/P6).** A battle's **point yield** — its attack
  points and each defender's defense points — and the per-defender contribution (AC3) are computed and
  **persisted once at resolution** as facts about that battle (alongside the existing `loot_*`),
  server-authoritatively (P4) and exactly-once. **Leaderboards and stat pages sum these persisted facts**
  (and derive population on read); the client cannot influence ranking, page beyond the bound, or read
  hidden state. There is **no** separate authoritative rank/aggregate cache — the persisted
  battles/contributions/villages/memberships are the single source of truth (P5). Recomputing a board
  over the same persisted data + same window/scope yields the **same** ordering (P2/P6); a later balance
  change does not rewrite already-awarded points.

- **AC12 — Reinforcer report visibility & read performance (P11/§7.3).** A reinforcing player may read
  **their own** report for a battle at a village they do **not** own (the troops were theirs); they do
  **not** thereby gain the owner's full-battle view or any other private state. Leaderboard and stat-page
  reads meet the latency budget on a populated world (aggregations indexed; page size bounded, P7).

## Roles & permissions

Per [roles.md](../../roles.md). Rankings are **public** (GDD §11.2); the constraint is that public views
never leak private game state (troops, resources — P4/§7.3), and no role may manipulate a ranking
(rankings are pure derivations). The combat amendment (AC3) is a **System**-performed outcome.

| Role | Permitted | Denied (server-enforced) |
|------|-----------|--------------------------|
| **Visitor** | View **all** public leaderboards and **player/alliance statistics pages** (population, attack/defense points, loot totals, battle history), filtered by quadrant and window. | Reading any **private** state through these views (current troop counts, stored resources); nothing here needs an account. |
| **Player** | Everything a Visitor can; receive and read **their own** battle reports — including reports for battles where **they reinforced** another player's village (AC3/AC12); see their own stats. | Influencing ranking order/points; paging past the bound; reading another player's hidden troops/resources; reading the **owner's full-battle** report for a village they only reinforced (they see only **their own** contribution). |
| **Moderator** | N/A (considered) — 016 adds no moderation surface; fair-play review of suspicious stats is the later moderation slice (022). | — |
| **Administrator** | Configures (world/balance, P7) the **per-unit point values**, **window lengths**, **page size**, and the **quadrant rule**; superset of read access. | Setting these per-request from the client (server config, not request input). |
| **System** | *(system-initiated)* At battle resolution (P1 due-event), **compute each defending player's contribution and emit their report** (owner = whole battle; each reinforcer = their own troops), **exactly once** (crash-safe), server-authoritatively (P4). Derive every board and stat page on read. | — |

## Out of scope

- **Top-climbers leaderboard & population-over-time charts** (GDD §11.2) — need **population snapshots**
  over time, which do not exist; deferred to **017** (which introduces the recurring scheduled
  settlement, the natural home for snapshots). 016 ranks **current** population only.
- **Medals / weekly awards** (GDD §11.2) — the weekly scheduled settlement granting permanent account
  medals → **017**. 016 only *exposes* the stats those awards will consume.
- **Achievements / milestone badges** (GDD §11.2) → **017**.
- **Profile-page presentation & charts** — the *visual* profile/leaderboard UI polish and social
  surfacing are **app-layer** ([social-and-meta-features.md](../../social-and-meta-features.md)); 016
  delivers the **data + functional read views**.
- **New combat *math*.** AC3 changes only **reporting** (who gets which report) and adds the persisted
  per-defender contribution; it does **not** change casualties, loss fractions, loot, wall, morale, luck,
  or who survives — the battle outcome is byte-for-byte the existing 009/011 resolution. Attack-side
  multi-party (multiple attackers in one battle) is not a thing in faithful Travian and stays out.
- **Per-resource raider ranking** — top-raiders ranks on the **summed** loot scalar; per-resource boards
  are not in scope (the split shows only on the stat page).

## Decisions

- **Points are a persisted battle fact; rankings sum them (P2/P5/P11).** A battle's attack points and each
  defender's defense points are computed at resolution and **persisted** (like `loot_*`). Leaderboards are
  **queries that `SUM` those facts** (and derive **population** on read) over a window/scope, bounded and
  indexed — **no** separate `ranking`/rank-cache table, **no** ranking tick. Deterministically
  reproducible from the persisted facts (P2/P6); points are *not* recomputed against later balance.
- **Per-defender contribution is a new persisted fact (AC3).** The resolver already computes each
  reinforcement group's forces and losses; 016 **persists** them as a per-defender contribution
  alongside the battle (e.g. a `battle_defenders` row per defending player: battle, player, defending
  village, forces, losses, defensive value), written **exactly once** in the same crash-safe transaction
  as the existing report (P1/P2/P4). The owner's aggregate `battle_reports` row is **kept as-is** (so
  existing attacker/owner report behaviour and tests are undisturbed); reinforcer reports and defense
  points are derived from the new per-defender rows.
- **Defense points split by contributed defensive value (faithful, GDD §11.2).** The battle's defense
  total = `Σ value(attacker killed)`; each defender's share = `total × (their defensive value / Σ
  defensive value)`, using the **same per-group defensive value the resolver already computes** for the
  battle math. **Rounding:** shares are computed with a deterministic **largest-remainder** apportionment
  so integer shares **sum exactly** to the total (no points created or lost).
- **Per-unit point value is balance (P7).** A scalar `value(unit)` ("military value destroyed", GDD
  §11.2) lives in balance (a `ranking.toml` or a `point_value` on unit balance — decided in plan), loaded
  fail-fast by `infrastructure::balance`. A pure `battle_points(...)` in the domain makes the rule
  testable with exact numbers (P3).
- **Attack points to the single attacker; no split.** Reinforcements are defence-only, so a battle has
  one attacker; attack points = value of the **whole** `defender_losses`, credited to that attacker.
- **Quadrant = sign of the coordinate about the origin (pure, P6).** `x ≥ 0, y ≥ 0` → NE; `x < 0, y ≥ 0`
  → NW; `x < 0, y < 0` → SW; `x ≥ 0, y < 0` → SE (origin/positive axes resolve to NE by the `≥` rule — a
  fixed, total, reproducible tie). A pure `quadrant(Coordinate)` in the domain (P3); a player's quadrant =
  their **capital**'s (013).
- **Time windows & page size are config (P7/P11).** Conflict boards expose **all-time** plus rolling
  windows (default 7-day + 30-day) applied as `resolved_at ≥ now − W`. Every board returns at most
  `leaderboardPageSize` rows; the stat page's battle history is paged — no unbounded query.
- **Public, but private state stays hidden (P4/§7.3).** Stat pages surface only public ranking metrics
  and already-visible reports; a reinforcer's report shows **only their own** contribution, never the
  owner's full battle or others' troops. Enforced in the read-query shape, not by client trust.

## Open questions

- **Zero-activity rows on windowed boards.** Omit a player with **no** qualifying battles in window `W`,
  or show them at zero? **Proposed:** **omit** — the board lists *performers*; zero rows add noise and
  unbounded length. (Players-by-population always lists everyone with population > 0.)
- **Defensive-value basis for the split.** Defense points split by each defender's **contributed
  defensive value** (their share of total defence power) — as opposed to raw troop count or troops lost.
  **Proposed:** **defensive value** (faithful; reuses the resolver's existing per-group defence
  computation). Flagged so the precise basis is reviewed against the 009 math.
- **Alliance points window basis.** Aggregate alliance points over `W` = sum of **current** members'
  battle points in `W` (even battles fought before they joined)? **Proposed:** **yes** — attribute by
  **current** membership at read time (matches AC1/AC8's "current ownership" philosophy); revisit if
  gameable.
- **Conquered/relocated villages and historical battles.** A battle's points stay credited to the
  **player** recorded on the battle/contribution at battle time — a later conquest (014) does **not**
  retroactively move past points. **Proposed:** keep it — points record *who did the killing*; ownership
  transfer changes only **population** going forward.
- **Reinforcer report retention on troop return.** A reinforcer's report persists independently of their
  troops returning home (007). **Proposed:** **yes** — the report is a historical fact (P2), unaffected by
  later movement; it remains in the reinforcer's battle history.
