# Eperica — Game Design Document (GDD)

**Status:** Draft complete (v1) — mechanics defined; numeric balance tables deferred to the balance dataset
**Governed by:** [constitution.md](./constitution.md) — esp. P9 (faithful first), P7 (configurable speed).

This document describes *what Eperica is as a game* — its mechanics and rules. It deliberately
avoids implementation. Per **P9**, the baseline is a faithful clone of **Travian: Legends (T4)** with
the explicit exclusions below; all other deviations are called out where they occur. Exact numeric
balance tables (production per level, build costs/times, unit stats) are *data* and will live in a
dedicated balance dataset referenced by the feature specs — this document defines the **models and
formulas**, not the full number tables.

### Baseline scope — deliberate exclusions

We follow the T4 baseline **except**:

- **No hero system.** Removed entirely. This also removes everything hero-dependent: **adventures**,
  the **auction house**, **hero items**, and the **hero resource bonus**.
- **No NPC merchant.** No premium/instant resource conversion at a fixed ratio.
- **No premium ("Gold/Plus") pay features** in the baseline. *(May be revisited far later; out of
  scope for the faithful clone.)*

We **keep**: the **Marketplace** (player-to-player trade and resource transfer between your own
villages), quests/tasks, culture points, alliances & diplomacy, farm lists, and the Wonder-of-the-
World end-game with the Natars.

**Knock-on (resolved):** in T4 **oases** are occupied via the Hero's Mansion. With no hero, occupation
is handled by the **Outpost** building instead — see §7.4.

## Section roadmap

| #  | Section                         | Status   |
|----|---------------------------------|----------|
| 1  | Overview & core loop            | Drafted  |
| 2  | Resources                       | Drafted  |
| 3  | The village                     | Drafted  |
| 4  | Buildings                       | Drafted  |
| 5  | Tribes                          | Drafted  |
| 6  | Units & troops                  | Drafted  |
| 7  | The world map                   | Drafted  |
| 8  | Troop movements & travel        | Drafted  |
| 9  | Combat resolution               | Drafted  |
| 10 | Alliances & diplomacy           | Drafted  |
| 11 | Progression, ranking & win cond.| Drafted  |
| 12 | Player lifecycle & protection   | Drafted  |
| 13 | Game speed & server config      | Drafted  |

---

## 1. Overview & core loop

Eperica is a persistent, real-time strategy MMO. Each player governs one or more **villages** on a
shared **world map**, develops an economy, raises an army, and competes with and against thousands of
other players until the server reaches its **end-game victory condition**.

The core loop, repeated at every scale from a single field to a server-wide alliance war:

1. **Produce** — resource fields generate wood, clay, iron, and crop continuously over real time.
2. **Build** — spend resources to upgrade fields and construct/upgrade buildings, unlocking new
   capabilities. Construction takes real time and is queued.
3. **Expand** — found or conquer new villages to multiply production and reach.
4. **Fight** — train troops; raid for loot, attack to destroy, defend to survive.
5. **Cooperate** — form alliances, coordinate defense, share intelligence and resources.
6. **Win** — contribute to the server-end victory race.

The genre's signature tension runs through all of it: **grow internally vs. act externally**, and
**the world moves while you are offline** — you log in to consequences.

---

## 2. Resources

Four resources, identical to Travian:

| Resource | Role |
|----------|------|
| **Wood**  | Construction (general). |
| **Clay**  | Construction (general). |
| **Iron**  | Construction, and weighted toward military. |
| **Crop**  | Construction, and **ongoing upkeep** for population (buildings) and troops. |

### 2.1 Production
Each resource is produced by dedicated **resource fields** (§3.1). Production is a continuous rate
(units per hour) determined by the sum of that resource's fields at their current levels, multiplied
by the server **speed** (P7) and any active bonuses (buildings, oases, hero).

Per **P1**, a village stores each resource as `amount + lastUpdated + ratePerHour`; the current
amount is computed on read as `min(capacity, amount + ratePerHour × elapsed)`. The world is never
ticked to accrue resources.

### 2.2 Crop upkeep (the constraint that shapes everything)
Crop is special: every building level and every trained troop consumes crop per hour.
**Net crop production = crop fields output − total upkeep.**

- If net crop is **positive**, crop accumulates like the others.
- If net crop is **negative**, the crop store drains. When it hits **zero, troops starve** — units
  die off until upkeep is affordable again. This is the natural cap on army size and a core
  strategic pressure.

### 2.3 Storage capacity
- **Warehouse** caps wood, clay, and iron (shared cap per resource).
- **Granary** caps crop.
- Production beyond capacity is **lost** (overflow), creating pressure to spend or expand storage.

### 2.4 Trading
The **Marketplace** (§4) enables moving resources between your own villages and trading with other
players at freely negotiated ratios. Resources travel physically over the map and take time to
arrive (merchant capacity and speed are tribe-dependent — see §5). There is **no NPC merchant** and
no instant conversion (see baseline exclusions).

---

## 3. The village

A village occupies one tile on the world map and has two zones: the **resource fields** (outer ring)
and the **village center** (building slots).

### 3.1 Resource fields
- **18 resource-field tiles** per village, each dedicated to one resource (a woodcutter, clay pit,
  iron mine, or cropland).
- The *distribution* of those 18 fields is fixed by the map tile the village sits on
  (e.g. 4·wood / 4·clay / 4·iron / 6·crop is the balanced default; "cropper" tiles like 3·3·3·9 or
  3·3·3·15 trade flexibility for huge crop output).
- Each field is upgraded independently up to a maximum level (baseline cap **10**, raisable by
  special buildings later). Higher level → higher hourly output, per the balance table.

### 3.2 Village center
- A set of **building slots** (baseline: 20 general slots plus fixed special positions for
  the **Rally Point** and **Wall**; the **Main Building** is present from founding). Each general slot
  holds at most one building; the player chooses what to build on each free slot (slice 110).
- **Most buildings are one per village.** A few may be built **multiple times**, each on its own slot,
  with **stacking** effects: **Warehouse** and **Granary** (total capacity is the sum of each instance's
  capacity at its level) and the **Cranny** (total hidden amount is the sum). A built slot may be
  **demolished** (Main-Building-gated) to free it (slice 110).
- Buildings are constructed and upgraded here (§4). Most have prerequisites (other buildings at a
  given level) and a main-building-dependent build time.

### 3.3 Multiple villages
- A player starts with **one** village and can gain more by **settling** (with settlers + a free map
  tile + a Residence/Palace) or **conquering** (with chiefs/senators, reducing a target's loyalty).
- Each village runs its own independent economy, queues, and defense. There is no global resource
  pool — resources are **per village** and must be physically moved to be shared.

### 3.4 The capital (decided)
One village per player is the **capital**, designated by building a **Palace** (§4.2). The capital:
- may raise its **resource fields beyond the normal level cap** (unlocking the highest production), and
- **cannot be conquered** (its loyalty cannot be reduced to transfer ownership; it can still be
  attacked, looted, and have buildings damaged).

The capital can be **relocated** by building a Palace elsewhere, but only one capital exists at a time.
This makes "where to plant the unconquerable powerbase" a core strategic decision.

---

## 4. Buildings

Buildings occupy the village center slots (§3.2). Each has a level (typically 1–20), level-dependent
cost (wood/clay/iron/crop), a **crop upkeep** per level, a build **time**, and **prerequisites**
(other buildings at a minimum level). All costs/times/upkeep are balance data; this section defines
the set and their roles.

### 4.1 Build mechanics
- **Build queue:** a village builds **one thing at a time** by default. *(Tribe exception: Romans may
  build one resource field **and** one center building simultaneously — §5.)*
- **Build time** scales down with **Main Building** level — a higher Main Building speeds all
  construction in that village.
- **Prerequisites** gate access (e.g. an Academy requires Main Building + Barracks at given levels).
- **Demolition** of buildings is possible once the Main Building reaches its max level.
- A completed build is a **discrete due-event** (P1): enqueue → event fires at `now + buildTime/speed`
  → level applied. The world is never polled to check progress.

### 4.2 Infrastructure buildings
- **Main Building** — speeds construction; enables demolition at max level.
- **Warehouse** — caps wood/clay/iron storage (§2.3).
- **Granary** — caps crop storage.
- **Marketplace** — trade and resource transfer (§2.4); more merchants at higher levels.
- **Embassy** — required to join/found an alliance; level gates alliance membership size.
- **Cranny** — hides a quantity of resources from raiders (un-lootable up to its capacity).
- **Town Hall** — produces **culture points** (gate expansion) and holds celebrations.
- **Residence / Palace** — enable founding/holding villages; train expansion units (settlers,
  administrators); provide loyalty defense. A **Palace** designates the **capital**; only one Palace.
- **Trade Office** — increases merchant carry capacity.
- **Treasury** — required to hold an artifact / Wonder mechanics in the end-game (§11).

### 4.3 Resource-boost buildings
Multiplicative bonuses on top of field output: **Sawmill** (wood), **Brickyard** (clay),
**Iron Foundry** (iron), **Grain Mill** + **Bakery** (crop).

### 4.4 Military buildings
- **Rally Point** — fixed slot; required to send/return troops; governs movement and (later) farm
  lists. Always present from village founding.
- **Barracks** — trains infantry. **Stable** — trains cavalry. **Workshop** — builds siege
  (rams, catapults).
- **Academy** — researches new unit types (unlock before training).
- **Smithy** — upgrades unit attack/defense levels.
- **Wall** — tribe-specific (§5); boosts defense of the garrison.
- **Trapper** — *(Gaul only)* traps a number of attacking units.
- **Outpost** — occupies cleared oases (§7.4); level determines how many oases the village may hold.
  Replaces T4's Hero's Mansion in this role.
- **Great Barracks / Great Stable** — higher-throughput training (end-game scope).

> **Removed vs T4:** **Hero's Mansion** (no hero system). Its oasis-occupation role is taken over by
> the **Outpost** above.

---

## 5. Tribes

Three classic tribes, chosen once at registration. Tribes share the economy but differ in troops,
their **Wall**, **merchant** profile, and one or two **traits**. (Faithful to T4's three core tribes;
later T4 tribes such as Egyptians/Huns are **not** in the baseline.)

### 5.1 Romans
- **Trait:** can build a **resource field and a center building simultaneously** (parallel queue).
- **Troops:** strong and versatile but **expensive** and slower to train; excellent all-round.
- **Wall:** **City Wall** — highest defensive bonus, but more fragile to rams.
- **Merchant capacity:** 500 (baseline value).
- **Feel:** balanced, forgiving economy; rewards investment.

### 5.2 Teutons
- **Trait:** aggressive early game; can **plunder resources from enemy Cranny** (others can't fully).
- **Troops:** **cheap** and fast to produce — the best early raiders; weaker per-unit quality.
- **Wall:** **Earth Wall** — most hit points (hard to destroy) but lowest defensive bonus.
- **Merchant capacity:** 1000 (carry most per merchant).
- **Special building:** **Brewery** — boosts attack for a celebration, at a crop cost.
- **Feel:** offensive, raiding-focused, low-cost aggression.

### 5.3 Gauls
- **Trait:** defensive specialists; **fastest** cavalry in the game.
- **Troops:** strong cheap **defense** (Phalanx) and very fast raiders/scouts; the recommended
  beginner tribe.
- **Wall:** **Palisade** — balanced bonus and durability.
- **Special building:** **Trapper** — traps attacking units (defensive deterrent).
- **Merchant capacity:** 750.
- **Feel:** safe, mobile, defense-oriented.

### 5.4 Balance data
- Per-tribe merchant **capacity** (above) and **speed** values are tuned in the balance dataset, not
  fixed here.

---

## 6. Units & troops

Each tribe has its own roster (~10 unit types) across these roles. All stats — attack, defense vs.
infantry, defense vs. cavalry, **speed** (map fields/hour), **carry capacity** (loot), **crop
upkeep**, training cost and time — are balance data per tribe.

### 6.1 Unit roles
- **Infantry** — trained in the Barracks; the backbone of both offense and defense.
- **Cavalry** — trained in the Stable; faster, higher carry capacity, costlier.
- **Scout** — reconnaissance; gathers intel on resources/troops/defenses and counters enemy scouts.
  Combat for scouts is resolved separately from the main battle.
- **Siege** — built in the Workshop: **Rams** damage the **Wall**; **Catapults** damage **buildings**
  (attacker may target specific buildings).
- **Expansion / administrative:**
  - **Settlers** — a group (3) founds a new village on a free tile.
  - **Administrators** (Senator / Chief / Chieftain, tribe-named) — lower a target village's
    **loyalty**; loyalty reaching zero **conquers** it.

### 6.2 Unit attributes (model)
Every unit type carries: `attack`, `defenseInfantry`, `defenseCavalry`, `speed`, `carryCapacity`,
`cropUpkeep`, plus `cost` and `trainTime`. Combat uses attack vs. the appropriate defense type
(§9). Upkeep feeds the crop-starvation constraint (§2.2).

### 6.3 Research & upgrades
- A unit type must be **researched** in the **Academy** before it can be trained.
- The **Smithy** upgrades a unit type's attack and defense in levels, per village.

### 6.4 Training
- Training a batch enqueues units that complete sequentially as **due-events** (P1); each finished
  unit joins the village garrison at its due time.

---

## 7. The world map

The world is a single shared grid of tiles addressed by integer coordinates `(x, y)`, centered on
`(0, 0)`. Map radius (and therefore size) is **server config** — a normal world spans roughly
`-200..+200` on each axis. The map is **toroidal**: it wraps at the edges, so the far east is
adjacent to the far west (configurable; faithful default = wrap).

### 7.1 Tile types
- **Valleys (occupiable tiles)** — where villages sit. Every valley has a fixed **field
  distribution** (§3.1), e.g. `4-4-4-6` balanced or cropper layouts like `3-3-3-9` / `3-3-3-15`. The
  distribution is a property of the tile, decided at world generation (P6 seeded).
- **Oases** — special tiles that grant a **resource production bonus** to the village that occupies
  them. They are guarded by **wild animals** (nature defenders) that must be defeated first, and can be
  raided for the animals' loot. Occupation is via the **Outpost** building, not a hero (§7.4).
- **Natar / special tiles** — reserved for the end-game (Wonder of the World, artifacts — §11).

### 7.2 Distance & adjacency
Distance between two tiles is the **Euclidean** distance on the (wrapped) grid. Distance drives
**travel time** (§8) and some building/troop range effects. Because the map wraps, distance always
uses the *shortest* path around the torus.

### 7.3 Map visibility
The map layout (tiles, who owns which village, alliance tags) is public. **Troop counts, resources,
and defenses are hidden** and revealed only by **scouting** (§6.1 / §9).

### 7.4 Oasis occupation — the Outpost (decided)
Replacing T4's hero-driven occupation: a village occupies oases via a dedicated **Outpost** building
(§4.4). The owner must first **clear the wild animals**, then can claim the oasis by holding it
through the Outpost; the Outpost's level determines **how many oases** that village may occupy
(faithful to the old Hero's Mansion thresholds, e.g. 1 / 2 / 3 oases at rising levels). An occupied
oasis applies its production bonus to that village and can be **lost** if attacked and the claim is
broken. Preserves the "clear then hold" contest without a hero.

---

## 8. Troop movements & travel

A **movement** is a body of troops (or merchants) leaving a village's **Rally Point** toward a target
tile, arriving after a computed travel time. Every movement is a **discrete due-event** (P1):
arrival is scheduled at a timestamp; nothing polls it in between; on firing, the server resolves the
movement (combat, delivery, or garrison) authoritatively (P4) and schedules any **return** trip.

### 8.1 Travel time
`travelTime = distance / (effectiveSpeed × serverSpeed)`, where **effectiveSpeed = the speed of the
slowest unit in the movement** (so siege and settlers slow an army). Modifiers (e.g. a Tournament
Square reducing time beyond a radius, end-game artifacts) adjust effective speed. All as data.

### 8.2 Movement types
- **Attack (normal)** — fight to destroy; resolves full combat (§9); siege may damage wall/buildings.
- **Raid (plunder)** — fight to loot; both sides hold back, survivors return with stolen resources.
- **Reinforcement** — troops travel to a friendly/own village and **stay** to defend.
- **Return** — surviving troops travel home, carrying any loot (capacity-limited).
- **Scout** — espionage movement, resolved separately from main combat (§9).
- **Settle** — settlers travel to a free valley to found a village.
- **Trade** — merchants carry resources to another village (own or other), then return.

> **Removed vs T4:** **Adventure** movements (hero-only).

### 8.3 Recall & cancellation
- **Outgoing attacks/raids cannot be recalled** once sent (faithful).
- **Reinforcements** stationed in another village can be **sent back** by their owner.
- A movement that has not yet departed (still queued) can be canceled.

### 8.4 Loot
Resources looted are bounded by the army's total **carry capacity**. The defender's **Cranny**
shields a quantity of resources from looting (§4.2; Teutons partially bypass it, §5.2).

---

## 9. Combat resolution

Combat resolves **at the instant of arrival** (the movement's due-event), entirely server-side (P4),
**deterministically** given the inputs and a persisted random seed (P6) — so any battle report can be
recomputed and explained (P2). This section defines the **model**; exact constants are balance data.

### 9.1 Inputs
- **Attacker:** the attacking units (with Smithy upgrade levels), split by class into an **infantry
  attack** sum and a **cavalry attack** sum. Siege (rams, catapults) handled separately (§9.4).
- **Defender:** all troops stationed in the target (home garrison + reinforcements), each
  contributing **defenseInfantry** against attacking infantry and **defenseCavalry** against
  attacking cavalry; plus the village's small **base defense** and the **Wall** bonus (§9.3).

### 9.2 Battle formula
Total attack power is compared to total defense power. Casualties follow Travian's
**non-linear (power-law) loss formula**: the side-to-side power ratio is raised to an exponent so
that a small power advantage yields a disproportionately favorable casualty ratio. Two settlement
modes:
- **Normal attack:** the **loser loses all** participating troops; the winner loses a fraction
  determined by the power ratio.
- **Raid:** **both sides take proportional losses**; survivors on both sides remain (attacker's
  survivors return with loot).

Modifiers applied to the comparison:
- **Morale** — dampens an attack by a much larger player against a much smaller one (population-ratio
  based), protecting newer/weaker players.
- **Luck** — a bounded random factor (e.g. ±25%) drawn from the **seeded RNG** (P6).

### 9.3 Wall, rams, and defense bonus
- The **Wall** adds a percentage **defense bonus** scaling with its level and tribe type (§5).
- **Rams** reduce the effective Wall level **before** the defense bonus is computed (heavier ram
  force → more wall destroyed).

### 9.4 Resolution order
For a single arriving attack the server resolves, in order:
1. **Scouting** (if a scout movement) — separate espionage combat; produces an intel report; no
   main battle.
2. **Rams vs. Wall** — reduce effective wall level.
3. **Main battle** — attack vs. defense with morale + luck → casualties on both sides.
4. **Catapults** (if attacker prevails) — damage targeted building(s).
5. **Loyalty** — any **administrators** reduce the target's loyalty; at zero the village is
   **conquered** (ownership transfers).
6. **Loot** — surviving attackers load resources up to carry capacity (minus Cranny protection) and a
   **return** movement is scheduled.

### 9.5 Battle report
Every resolution emits a **battle report** to both parties: forces, losses on each side, wall/building
damage, loot, and loyalty change. Reports are derived from persisted state + seed (P2/P6) and never
depend on who was online.

### 9.6 Report transparency (decided)
Battle reports **surface the luck and morale modifiers** that affected the outcome, alongside forces,
losses, and loot. This matches T4, aids transparency, and fits **P6** (outcomes are reproducible and
explainable).

---

## 10. Alliances & diplomacy

Alliances are the social/political layer that turns the server into a competition between *groups*.

### 10.1 Membership
- An alliance is founded and joined via the **Embassy** (§4.2); the Embassy level caps how many
  members a village can support, bounding alliance size (server-config max, faithful default ~60).
- **Roles & rights:** the founder grants leaders granular rights (invite/expel, diplomacy, manage
  contributions, post announcements). Ordinary members have none of these by default.

### 10.2 Diplomacy
Pairwise stances between alliances, set by those with diplomacy rights:
- **Confederation** — trusted allies: shared map visibility and reinforcement intent.
- **War** — formal hostility (enables war statistics / kill tracking).
- **Neutral** — the default absence of a declared stance.

### 10.3 Alliance bonuses *(optional, later slice)*
Contribution-funded alliance-wide bonuses (e.g. recruitment, culture, defense, trade) that members
pay into. Faithful to later T4 but **not required for the core loop** — modelled here, deferred in
build order.

### 10.4 Communication
Alliances communicate via in-game **messaging**, an **alliance forum**, and **report sharing**. These
are application-layer features, not simulation rules, so their detailed design lives in
[social-and-meta-features.md](./social-and-meta-features.md) rather than this GDD.

---

## 11. Progression & win condition

### 11.1 Expansion gating — culture points
Founding or conquering each additional village requires accumulated **culture points (CP)**, produced
over time by buildings (notably the **Town Hall**) and one-off **celebrations**. CP gates the *pace*
of expansion so growth is a strategic investment, not just a resource question. The Nth village
requires a rising CP threshold (balance data).

### 11.2 Ranking, statistics & competition
Ranking is a primary long-term motivator and the connective tissue of the competitive layer.

**Population.** Each village's population derives from its built levels (resource fields + center
buildings); a player's **total population** across all villages is the headline ranking metric and
underpins map standing and beginner-protection thresholds (§12.2).

**Battle points.** Offensive and defensive action earn points proportional to the enemy
troop/population value destroyed:
- **Attack points** — earned by killing enemy troops on the offensive.
- **Defense points** — earned by killing attacking troops while defending (shared among all
  defenders present, including reinforcements).

**Leaderboards.** Public, filterable rankings (by region/quadrant and by time window) across these
categories:
- **Players by population** (overall standing)
- **Top attackers** (attack points)
- **Top defenders** (defense points)
- **Top climbers** (population gained over the period — fastest growers)
- **Top raiders** (resources looted)
- **Alliances** — by aggregate population, and by aggregate attack/defense points

**Statistics.** Per-player and per-alliance stat pages: population over time, troop kills, battle
history, loot totals — all derived from persisted state (P2), never from in-memory tallies.

**Medals / weekly awards.** At a fixed weekly boundary the server awards **medals** to the top
performers in each leaderboard category (top attacker, defender, climber, raider; top alliances).
Medals are permanent account decorations and a key prestige driver. The weekly settlement is a
**scheduled due-event** (P1), computed server-side (P4) from the week's accumulated, persisted stats
so awards are reproducible (P2/P6). Weekly boundaries, categories, and tie-breaking rules are config.

**Achievements (milestone badges).** Distinct from competitive medals: one-time, **non-competitive**
badges granted when a player crosses a defined milestone (e.g. found your 2nd village, win 100
defensive battles, occupy your first oasis, reach population N, research every unit of your tribe).
These are **game-rule grants**, not cosmetics: the server detects the triggering event and awards the
badge server-side (P4), idempotently, from persisted state so the same history always yields the same
badges (P2/P6). Each achievement has a trigger condition and may carry a small one-off reward
(resources or culture points). The catalogue of achievements (conditions + rewards) is balance data;
their *display* on profiles is presentation (see social-and-meta-features.md).

### 11.3 End-game: Wonder of the World
A server is a **finite round** with a scripted end-game:
1. At a configured date, **artifacts** are released — held in **Natar** (NPC) villages, captured by
   attacking them. Artifacts grant powerful bonuses (troop speed, building durability, larger armies,
   etc.) to the holding village/account.
2. Later, **Wonder of the World building plans** appear. An alliance must hold a plan and build a
   **Wonder** up through levels toward **100**.
3. **The first alliance to complete a Wonder (level 100) wins the server.** The round then ends and a
   fresh world begins.

This gives the game a **seasonal shape** (P-aligned with "ship a real game": rounds have a beginning,
arc, and conclusion rather than infinite grind).

### 11.4 End-game scope (decided)
**The first Eperica world ships with the complete end-game** — artifacts, Natar villages, Wonder-of-
the-World construction, and the first-Wonder-to-level-100 victory. There is **no interim/simpler win
condition**; "launch" means a world that can be *won* the faithful way.

**Sequencing implication:** the end-game is therefore part of the launch scope, not a deferred
add-on. It is still built **last** in dependency order (it presupposes the economy, military, map,
movement, combat, and conquest systems), but world #1 is not considered shippable until it is in
place. Build-order planning must account for this larger launch surface.

---

## 12. Player lifecycle & protection

### 12.1 Onboarding & quests
- **Register → choose tribe (§5) → spawn** one starting village on a free valley (optionally by map
  quadrant). The new village begins with starter resources and beginner's protection (§12.2).
- **Quest system.** A guided chain of **tasks** bootstraps the new player by walking them through the
  core loop (upgrade a resource field, build the warehouse, train first troops, send a first
  raid, etc.). Quests:
  - are **stage-gated** — completing one unlocks the next, forming an onboarding chain;
  - grant **rewards** on completion (resources, sometimes a small troop count or culture points) to
    accelerate the slow early game;
  - are evaluated **server-side** by detecting the completing event (P4), and a player's quest
    progress is persisted so it is reproducible and resumable (P2).
- After the onboarding chain, quests **taper off**; ongoing play is driven by the game systems
  themselves, not a perpetual task list. (The exact quest chain, conditions, and reward values are
  balance data; the *presentation* of quests is in social-and-meta-features.md.)

### 12.2 Beginner's protection
- New players are **immune to attacks** for a protection window (duration scaled by server speed, and
  ending early past a population/points threshold). Prevents spawn-camping and gives a foothold.

### 12.3 Inactivity & abandonment
- Accounts that go inactive eventually **decay** (becoming farms) and, after a long period, are
  **deleted**; their villages return to the map as abandoned/grey valleys. Keeps the world live and
  the map reclaimable.

### 12.4 Out-of-scope (for now)
- **Account sitting** (authorized co-login), **vacation mode**, and anti-multi-account enforcement are
  acknowledged but deferred — flagged here so the account model leaves room for them later (P10).

### 12.5 Fair play & anti-cheat
Fairness is foundational for a competitive MMO and rests primarily on the constitution:
- **Server authority (P4).** The client never computes outcomes; it only displays state and posts
  intents. Every action is validated server-side against the rules, so a tampered client cannot gain
  an advantage — it can only send requests the server will reject.
- **Reproducibility (P6/P2).** Seeded randomness and fully-persisted state mean any disputed result
  (a battle, an award) can be recomputed and audited.

On top of that foundation, the **policy surface** (built progressively, P10):
- **Multi-accounting / pushing** — rules against one person running several accounts or funnelling
  resources to a main; detection via behavioural/association signals.
- **Bot / automation detection** — flagging inhuman action patterns.
- **Rate limiting & input validation** — server-side guards on action frequency and payloads.
- **Sitting limits** — bounding authorized co-login to prevent it becoming shared-account play.

Enforcement **tooling** (reporting, review queues, sanctions) is application-layer and lives in
social-and-meta-features.md; the **rules** above are game design and live here.

---

## 13. Game speed & server config

### 13.1 Speed multiplier (P7)
A server runs at a **speed multiplier** (e.g. 1x, 3x, 5x, 10x). Per **P7**, no wall-clock value is
hardcoded; the multiplier scales the time-dependent systems at runtime:
- resource **production rates**,
- **construction**, **training**, and **research** times,
- **troop movement** speed (and therefore travel time).

Costs and combat math are *not* speed-scaled (they are about quantities, not time).

### 13.2 World configuration
A world instance is parameterized by config, including:
- **map radius / size** and wrap behavior (§7),
- **speed multiplier** (and any separate troop-speed multiplier, faithful to T4 "speed×troop-speed"),
- **beginner-protection** duration and threshold,
- **end-game schedule**: artifact-release date, Wonder/Natar spawn date,
- storage/queue limits and other balance toggles.

### 13.3 World lifecycle
A world is **round-based**: it is created with a start date, runs through the artifact and Wonder
phases, ends when the win condition (§11.3) is met (or a hard end date), and is then archived to make
way for a new round. Multiple worlds (different speeds) may run concurrently as independent instances
— consistent with **P5** (state per world lives in the database; the app tier is stateless).

---

*GDD v1 draft complete. Numeric balance tables are intentionally deferred to the balance dataset
referenced by feature specs.*
