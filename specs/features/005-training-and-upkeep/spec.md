# Feature 005 — Training & upkeep

**Status:** Reviewed
**Depends on:** 004 (tribes, unit definitions, research, Smithy; due-event order machinery)
**Roadmap:** M2 · slice 005 · GDD §6.4, §2.2 — the army economy and the natural army cap.

## Goal

Players **train researched units** in batches at their troop buildings; finished units **join the
village garrison one by one** as due-events (P1). Every garrisoned unit **eats crop**, and when the
crop store runs dry the army **starves down to a sustainable size** — the natural cap on army size
(GDD §2.2). This turns the 004 unit definitions into a real army economy.

## Concepts

- **Training batch:** an order at a troop building — **Barracks** (infantry), **Stable** (cavalry +
  mounted scouts), **Workshop** (siege) — for `count` units of one researched type. The full cost
  (`count × unitCost`) is debited up front; units complete **sequentially**, one every
  `perUnitTime`, and each joins the garrison **at its own due time** (GDD §6.4).
- **Per-unit time:** `trainTime ÷ (worldSpeed × buildingFactor(level))` — a higher training
  building trains faster (balance table, like the Main Building factor; P7).
- **Garrison:** the village's standing troops, per unit type. (Movement is slice 007; troops stay
  home in this slice.)
- **Upkeep:** each garrisoned unit consumes `cropUpkeep` crop/hour; net crop = field output −
  population − **troop upkeep** (extends the 002 model).
- **Starvation:** while net crop < 0 the store drains (002); when it reaches **zero**, troops die —
  deterministically, highest-upkeep first — until net crop ≥ 0 again. Modeled as a
  **due-timestamped depletion check** per village (P1), never a world tick.

## User stories

- As a **player**, I want to train batches of my researched units, so that I build an army over time.
- As a **player**, I want to see my garrison and what is training, so that I can plan.
- As a **player**, I want my net crop to reflect my army, so that I can feed it — or shrink it.

## Acceptance criteria

> All actions are server-authoritative (P4): costs, times, counts, completions, and starvation are
> computed and enforced server-side; the client only issues commands.

- **AC1 — Stable & Workshop constructable.** Through the 003/004 build catalog with prerequisites
  (balance): Stable ← Academy ≥ 5 and Smithy ≥ 1; Workshop ← Main Building ≥ 5 and Academy ≥ 10.
  Unmet prerequisites are rejected (003 AC4 applies).

- **AC2 — Start a training batch.** Given the unit's training building is present, the unit is
  researched (tier-1 counts), `1 ≤ count ≤ 9999`, **no batch is active at that building**, and
  resources cover `count × cost`, when the player orders training, then resources are settled and
  the full cost debited (optimistic-snapshot settle, as 004), and a batch is created whose `i`-th
  unit completes at `now + i × perUnitTime`.

- **AC3 — Batch rejected.** Rejected with nothing debited when any of: the unit is unresearched;
  the unit's training building is absent (or the unit trains in a building from a later slice);
  another batch is active at that building; `count` is out of range; resources are insufficient;
  the unit is not of the owner's tribe.

- **AC4 — Building level & speed scale training (P7).** A higher training-building level yields a
  strictly shorter per-unit time; a higher world speed shortens it proportionally.

- **AC5 — Units join the garrison one at a time (P1/P2).** As each unit's due time passes it is
  added to the garrison **exactly once** (a batch interrupted by a restart resumes and completes;
  no unit is lost or duplicated). The batch finishes when all `count` units have joined.

- **AC6 — Garrison upkeep drains crop.** Net crop production subtracts the garrison's total
  `cropUpkeep`; the village view shows the reduced (possibly negative) net rate. With a negative
  net, the crop store drains and floors at 0 (002 behavior preserved).

- **AC7 — Starvation (the army cap).** When the crop store **reaches 0 while net crop < 0**, units
  starve at that moment (system-initiated, due-timestamped): repeatedly remove **one unit of the
  garrisoned type with the highest `cropUpkeep`** (ties: balance roster order) until net crop ≥ 0
  or the garrison is empty. Applied **exactly once** per depletion; survives restarts. A village
  whose situation improves before depletion (e.g. an upgrade completes, troops starve elsewhere —
  later: trade) does **not** starve: the check re-validates at fire time and reschedules or no-ops.

- **AC8 — No troops ⇒ 002 behavior.** A village with negative net crop and **no garrison** keeps
  its 002 behavior: the store sits at 0, buildings are unaffected (only troops starve).

- **AC9 — Interface.** The village page shows the **garrison** (unit names + counts) and links to
  built troop buildings. Each troop building page lists the researched units it trains with cost,
  per-unit time, and a count form; while a batch runs it shows what is training, how many remain,
  and a live countdown to the **next** completion. Unavailable actions are not offered (and are
  rejected server-side regardless).

## Roles & permissions

Per [roles.md](../../roles.md).

| Role | Permitted | Denied (server-enforced) |
|------|-----------|--------------------------|
| **Visitor** | N/A (considered). | Train/view any village (redirected to login). |
| **Player** | Train researched units of **their own tribe** in **their own** villages; see their garrison and queues. | Train in another player's village; forge cost/time/count; exceed the one-batch-per-building rule; train unresearched or other-tribe units. |
| **Moderator** | N/A (considered). | — |
| **Administrator** | World speed scales training times (AC4). | — (superset). |
| **System** | *(system-initiated)* Add completed units to the garrison at their due times (AC5); starve units at crop depletion (AC7). | — |

## Out of scope

- **Troop movement / reinforcement / combat** → 007/009. Troops stay in their home garrison.
- **Multiple queued batches per building** (Travian queue depth) → later; one active batch per
  building here (matching the one-order pattern of 003/004).
- **Residence/Palace training** (settlers, administrators) → 013/014; those units remain
  trainable-nowhere for now.
- **Great Barracks / Great Stable** → end-game scope.
- **Gradual Travian-style starvation pacing** — see Decisions; the sustainable end state is the
  same.
- **Trade as a starvation remedy** → 008 (the re-validation hook in AC7 already accommodates it).

## Decisions

- **One active batch per training building** (Barracks, Stable, Workshop are independent queues),
  enforced race-proof by storage like 003/004 queues.
- **Starvation is a single deterministic cull at depletion time** (kill highest-upkeep-first until
  net ≥ 0), not Travian's gradual one-by-one pacing: the end state (a sustainable army) is
  identical, the model stays lazy (one due event per starving village, P1), and the rule is exactly
  testable. Pacing can be revisited in a later slice without schema changes.
- **Depletion checks are (re)scheduled at every point net crop can worsen or the store is settled**
  (orders, completions, training start, troop completion); at most one pending check per village.
  The check re-validates from live state at fire time, so stale checks are harmless (no-op or
  reschedule).
- **Troops in training do not eat**; upkeep starts when a unit joins the garrison (faithful).
- **Training is allowed even when net crop is or would become negative** — starvation, not a
  pre-check, is the cap (faithful).
- **Batch size cap 9999** (server-side sanity bound; resources are the real constraint).
