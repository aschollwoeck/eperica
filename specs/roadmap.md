# Eperica — Build-Order Roadmap

**Status:** Draft v1
**Governed by:** [constitution.md](./constitution.md) · **Designs:** [game-design.md](./game-design.md),
[social-and-meta-features.md](./social-and-meta-features.md)

This is the **dependency-ordered path** of vertical slices from nothing to a launch-complete world.
Because the end-game is in launch scope (GDD §11.4), the road runs all the way to a *winnable* world.

## How to read this

- Each row is a **vertical slice** → becomes a `features/NNN-slug/` folder (spec → plan → tasks).
- Slices are numbered in **build order**; "Depends on" shows the hard prerequisites.
- A slice is **not** "build all of system X" — it is the smallest end-to-end increment that adds
  observable behavior while respecting the constitution (esp. P1 lazy time, P3 pure domain).
- **Balance data** (numbers) is produced alongside the slice that first needs it, not upfront.
- This is a **living document**: slices may split or merge as we spec them. Numbers are stable handles;
  don't renumber casually.

> **Stack note:** the implementation stack (web framework, DB engine, etc.) is deliberately still
> open. It gets decided in the **plan** for slice **001** — the first slice that writes code — not
> before. Everything up to that point is design.

---

## Milestone overview

| Milestone | Theme | Slices | Marker |
|-----------|-------|--------|--------|
| **M1** | Foundation & core economy | 001–003 | ▶ *First playable: a real-time economy* |
| **M2** | Military foundation | 004–005 | |
| **M3** | World & movement | 006–008 | |
| **M4** | Conflict | 009–012 | ▶ *First real PvP* |
| **M5** | Expansion & multi-village | 013–014 | |
| **M6** | Social & competition | 015–019 | |
| **M7** | End-game | 020–021 | ▶ *Launch-complete: a winnable world* |
| **M8** | Launch hardening | 022–023 | |

App-layer social/meta features (messaging, notifications, map UI, profiles) interleave from M4 onward
(see note after M8) rather than occupying a single block.

---

## M1 — Foundation & core economy
*Goal: a logged-in player owns a village whose resources accrue in real time and can be spent on
timed construction. Proves the entire architecture end-to-end on the smallest possible surface.*

| #   | Slice | Depends on | Delivers / proves | GDD |
|-----|-------|-----------|-------------------|-----|
| 001 ✅ | **Foundation & skeleton** — solution + layers (Domain/Application/Infrastructure/Web), DB, EF, auth (register/login), a world with **configurable speed** (P7), the **due-event scheduler** skeleton (P1), test harness/CI. A player can register, log in, and see one starting village. | — | The architecture itself: layering (P3), stateless web + DB-as-truth (P5), speed config (P7), the event engine (P1). **Stack is chosen here.** | §3, §13 |
| 002 ✅ | **Resource production (lazy accrual)** — 18 resource fields, production rate from field levels × speed, **compute-on-read** (`amount + rate × elapsed`), warehouse/granary caps + overflow, base crop upkeep. | 001 | P1 continuous-time model; reproducible state (P2). | §2, §3.1 |
| 003 ✅ | **Construction & build queue** — upgrade fields, build/upgrade center buildings, costs + prerequisites, **build time as due-events**, Main-Building speed-up, demolition. | 002 | P1 discrete events end-to-end; the first real queue. | §4 |

▶ **First playable** after 003: a single-player economy that runs while you're away.

---

## M2 — Military foundation
*Goal: turn resources into troops, with the crop-upkeep constraint biting.*

| #   | Slice | Depends on | Delivers / proves | GDD |
|-----|-------|-----------|-------------------|-----|
| 004 ✅ | **Tribes & units** — tribe choice at registration; per-tribe unit definitions; Academy **research**; Smithy **upgrades**. | 003 | Tribe identity (§5) and the unit attribute model (§6.2). | §5, §6 |
| 005 ✅ | **Training & upkeep** — Barracks/Stable/Workshop training queues (due-events), troops join garrison, **crop upkeep + starvation** when net crop ≤ 0. | 004 | Army economy and the natural army cap (§2.2). | §6.4, §2.2 |

---

## M3 — World & movement
*Goal: a shared map exists and troops/merchants can travel across it (no combat yet).*

| #   | Slice | Depends on | Delivers / proves | GDD |
|-----|-------|-----------|-------------------|-----|
| 006 ✅ | **World map generation** — toroidal tile grid (config radius), per-tile field distributions, oasis + Natar tiles, seeded generation; distance function. | 001 | The map (§7); seeded reproducibility (P6). |
| 007 | **Troop movement & travel** — Rally Point; send/return **non-combat** movements (reinforcement); travel time = distance ÷ (slowest-unit speed × speed); arrival as due-event. | 005, 006 | The movement engine (§8) before combat rides on it. | §8 |
| 008 | **Marketplace & trade** — merchants carry resources between villages/players; tribe merchant capacity; trade movements + returns. | 007 | Resource logistics (§2.4); reuses the movement engine. | §2.4, §4.2 |

---

## M4 — Conflict
*Goal: the PvP core — attack, defend, scout, loot, and take oases.*

| #   | Slice | Depends on | Delivers / proves | GDD |
|-----|-------|-----------|-------------------|-----|
| 009 | **Combat resolution** — attack/raid modes, infantry/cavalry attack vs. matching defense, power-law losses, **morale + seeded luck**, wall + rams, resolution order, **battle reports**. | 007 | The heart of the game; server authority (P4) + seeded determinism (P6). | §9 |
| 010 | **Scouting** — espionage movements + separate scout combat + intel reports. | 009 | Information warfare (§6.1, §9.4). | §6.1, §9 |
| 011 | **Siege & loot** — catapults damaging targeted buildings; carry-capacity loot; Cranny protection. | 009 | Destruction + plunder loop. | §9.3–9.4, §8.4 |
| 012 | **Oases — clear & occupy** — defeat wild animals, the **Outpost** building, occupy/hold/lose oases for production bonus. | 009, 006 | The PvE/contest layer (§7.4) without a hero. | §7.4, §4.4 |

▶ **First real PvP** after this milestone.

---

## M5 — Expansion & multi-village
*Goal: grow beyond one village — by settling and by conquest.*

| #   | Slice | Depends on | Delivers / proves | GDD |
|-----|-------|-----------|-------------------|-----|
| 013 | **Settling & culture points** — CP production, expansion slots, Residence/Palace, settlers found new villages on free tiles; the **capital** (Palace, uncapped fields). | 007, 005 | Multi-village economy + capital rule (§3.3–3.4, §11.1). | §3.3, §3.4, §11.1 |
| 014 | **Conquest** — administrators reduce **loyalty**; ownership transfer at zero; capital is unconquerable. | 013, 009 | The aggressive expansion path (§6.1). | §6.1, §3.4 |

---

## M6 — Social & competition
*Goal: the layers that make it a competitive MMO rather than parallel solitaire.*

| #   | Slice | Depends on | Delivers / proves | GDD |
|-----|-------|-----------|-------------------|-----|
| 015 | **Alliances & diplomacy** — Embassy, membership + roles/rights, diplomacy stances (confederation/war/neutral). | 014 | The group layer (§10). | §10 |
| 016 | **Ranking, leaderboards & statistics** — population metric, attack/defense points, leaderboard categories, stat pages. | 009, 014 | The competitive scoreboard (§11.2). | §11.2 |
| 017 | **Medals & achievements** — weekly medal settlement (scheduled due-event); milestone achievement grants. | 016 | Prestige/competition; both as server-side grants (P1/P4/P6). | §11.2 |
| 018 | **Quests & onboarding** — stage-gated quest chain with rewards, server-evaluated, tapering. | 005 | New-player bootstrap (§12.1). | §12.1 |
| 019 | **Protection & lifecycle** — beginner's protection, inactivity decay/abandonment back to the map. | 009, 016 | A live, fair, self-renewing world (§12.2–12.3). | §12.2, §12.3 |

---

## M7 — End-game (in launch scope, GDD §11.4)
*Goal: a world that can actually be **won** the faithful way.*

| #   | Slice | Depends on | Delivers / proves | GDD |
|-----|-------|-----------|-------------------|-----|
| 020 | **Artifacts & Natar villages** — Natar NPC villages, artifact release schedule, capturing artifacts for bonuses. | 014, 015 | The end-game escalation (§11.3 step 1). | §11.3, §7.1 |
| 021 | **Wonder of the World & victory** — WW building plans, Wonder construction to level 100, **alliance victory**, round lifecycle (start → arc → end → archive). | 020 | The win condition and seasonal shape (§11.3, §13.3). | §11.3, §13.3 |

▶ **Launch-complete** after 021: a world with a beginning, an arc, and a winner.

---

## M8 — Launch hardening

| #   | Slice | Depends on | Delivers / proves | GDD |
|-----|-------|-----------|-------------------|-----|
| 022 | **Fair play & anti-cheat tooling** — rate limiting, multi-account/bot detection signals, reporting + review/sanction tools. | 016 | The enforcement surface for §12.5. | §12.5 |
| 023 | **Performance & scale pass** — load-test toward "thousands," query/index tuning, scheduler throughput, horizontal-scale validation (P5). | 021 | Validates the scale ambition before opening a real server. | constitution |

---

## App-layer social/meta features (interleaved)

These come from [social-and-meta-features.md](./social-and-meta-features.md) and are **pulled in when
a slice needs them**, not deferred to one block:

- **Reports inbox** — needed as soon as combat exists (with 009).
- **Notifications / incoming-attack alerts** — alongside 009.
- **Messaging** — alongside or just after alliances (015).
- **Map UI** — alongside the world/movement work (006–007), maturing through M4.
- **Profile pages / leaderboard UI** — alongside ranking (016–017).
- **Admin/moderation UI** — alongside 022.

Each is its own feature spec; this list just records *when* in the order they naturally land.

---

## Scope honesty

This is a large road — 23 core slices plus interleaved UI — because **"full end-game before launch"**
(GDD §11.4) makes world #1 a complete game, not an MVP. The early **First playable** (after 003) and
**First real PvP** (after M4) markers exist so there are motivating, demonstrable milestones long
before launch. We build strictly in dependency order; we don't start a slice until its prerequisites
are done and verified.
