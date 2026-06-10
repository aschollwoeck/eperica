# Feature 003 — Construction & build queue

**Status:** Verified
**Depends on:** 001 (village, fields, buildings), 002 (resources, spending base)
**Roadmap:** M1 · slice 003 · GDD §4 — reaches the **First playable** milestone.

## Goal

Players **spend resources** to **upgrade** their village over real time. An upgrade is **queued** and
**completes via the due-event scheduler** (P1) — the *first real game effect* driven through that
engine — after which the field/building gains a level (and produces more). This closes the core loop:
*produce → spend → build up → produce more.*

## Concepts

- **Build order:** raising one resource field or one center building by **one level**. It has a
  **cost** (wood/clay/iron/crop) and a **build time**, both from balance data (per building/field and
  level).
- **Spending:** ordering settles the village's resources to now (002), then debits the cost.
- **Build queue:** one active order per village (the free-tier Travian default). The order's
  completion is a persisted **due-event**; when it fires, the level is applied **exactly once**.
- **Build time** is `baseTime(target, level) ÷ (worldSpeed × mainBuildingFactor(mbLevel))` — a higher
  **Main Building** shortens construction (P7 + faithful).

## User stories

- As a **player**, I want to upgrade a resource field, so that it produces more over time.
- As a **player**, I want to see what's building and how long is left, so I can plan.

## Acceptance criteria

> All actions are server-authoritative (P4): cost, level, time, and completion are computed/enforced
> server-side; the client only issues the "upgrade slot X" command.

- **AC1 — Start an upgrade.** Given a target below max level, no active build order, and sufficient
  resources, when the player orders the upgrade, then resources are settled and the **cost is
  debited**, and a build order is created that **completes at** `now + buildTime` (buildTime per the
  formula above).

- **AC2 — Insufficient resources rejected.** Given the cost exceeds current resources, the order is
  rejected; **no** resources are debited and **no** order is created.

- **AC3 — One active order.** Given an active build order exists for the village, a second order is
  rejected (queue length 1 in this slice).

- **AC4 — Prerequisites.** An upgrade whose prerequisites (balance: required buildings/levels) are
  unmet is rejected. (Trivially satisfied for fields and the two starting buildings; enforced for any
  with prerequisites.)

- **AC5 — Completion applies the level once (P1/P2).** When the order's due time passes, the scheduler
  applies **+1 level** to the target **exactly once**; the new level is persisted and reflected in
  production/effects. A pending order **survives a restart** and still completes.

- **AC6 — Main Building speeds construction.** A higher Main Building level yields a strictly shorter
  build time for the same target/level.

- **AC7 — Speed scales construction (P7).** A higher world speed yields a proportionally shorter build
  time.

- **AC8 — Village view.** The village page shows, per upgradeable target, its level and the **upgrade
  cost**, an **order** action when affordable/idle, and — while an order is active — **what is building
  and a live countdown** to completion (client-side countdown from the server deadline; htmx for the
  order action).

## Roles & permissions

Per [roles.md](../../roles.md).

| Role | Permitted | Denied (server-enforced) |
|------|-----------|--------------------------|
| **Visitor** | N/A (considered). | Order builds / view a village (redirected to login). |
| **Player** | Order upgrades on **their own** village; see its queue. | Order on another player's village; forge cost/level/time; bypass the one-order rule. |
| **Moderator** | N/A (considered). | — |
| **Administrator** | World speed scales build time (AC7). | — (superset). |
| **System** | *(system-initiated)* Apply a build's level at its due time (AC5). | — |

## Out of scope

- **Parallel build queue** (Roman trait) → slice 004; one active order here.
- **Queue length > 1** (paid/extra slots) → later.
- **Demolition** → later.
- **Most building types** (Barracks, Academy, …) → military/other slices; see Decisions for which
  buildings are constructable in this slice.
- **Instant-finish / premium** → out.

## Decisions

- **Include Warehouse + Granary** as new building kinds in 003; warehouse/granary **capacity grows
  with their levels** (replacing the fixed base cap once built).
- **Support constructing new buildings in empty center slots** via a small **build catalog**
  (Warehouse, Granary) — in addition to upgrading existing fields/buildings.
- **Max level = 10** for this slice (balance), matching the production table.
- **Order action via plain form POST + redirect** (not htmx partial-swap, despite the plan's
  parenthetical). It works without JS; the live countdown is client JS reading the server deadline.
  htmx partial-swap is deferred. (P8.)
- **Building slots are fixed per kind** (Main Building 0, Rally Point 1, Warehouse 2, Granary 3) and
  derived **server-side** from the kind — the client never supplies a building slot (P4). Dynamic
  slots / parallel queues are later work.
