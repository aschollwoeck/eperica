# Feature 007 — Troop movement & travel

**Status:** Reviewed
**Depends on:** 005 (garrison, units, upkeep), 006 (map, toroidal distance)
**Roadmap:** M3 · slice 007 · GDD §8 — the movement engine, before combat rides on it (009).

## Goal

Troops can **leave a village** and travel across the map to **reinforce** another village, arriving
after a computed travel time, then be **sent back** home — all as **due-events** (P1). This is the
non-combat movement engine: the rails the combat, trade, scouting, and settling slices will all run
on. No fighting yet.

## Concepts

- **Movement:** a body of troops leaving a source village toward a destination, arriving after
  `travelTime`. It is a **discrete due-event** (P1): arrival is scheduled at a timestamp; nothing
  polls it in between; on firing, the server resolves it authoritatively (P4) — for this slice,
  **station** (reinforce) or **rejoin garrison** (return).
- **Travel time:** `travelTime = distance ÷ (effectiveSpeed × worldSpeed)`, where `distance` is the
  toroidal map distance (006) and `effectiveSpeed` is the **slowest** unit's map speed in the
  movement (so a slow unit slows the whole army) — GDD §8.1. World speed scales it (P7).
- **Reinforcement:** troops sent to another village to defend it. They **leave the source garrison**
  and, on arrival, are **stationed** at the target — still owned by the sender (a later slice gives
  them defensive weight in combat). The sender can recall them.
- **Return:** stationed troops travel home and **rejoin the source garrison** on arrival.

## User stories

- As a **player**, I want to send some of my troops to defend another village, so that I can support
  an ally.
- As a **player**, I want to bring my troops back home when they are no longer needed.
- As a **player**, I want to see where my troops are and when movements arrive.

## Acceptance criteria

> All movement is server-authoritative (P4): the troops sent, the target, travel time, and arrival
> are computed and enforced server-side; the client only issues the command.

- **AC1 — Send a reinforcement.** Given the player's village with a garrison and a target village
  (another existing village on a different tile), when the player sends a chosen subset of their
  troops (each `1..=` the count in the garrison), then those troops **leave the source garrison**
  and a movement is created arriving at `now + travelTime` (per the formula above).

- **AC2 — Send rejected.** Rejected with **nothing removed** from the garrison when any of: a
  requested count exceeds the garrison; the whole composition is empty; the target tile holds no
  village; the target is the source village's own tile. (The client never supplies whose troops —
  only the owner's own garrison can be sent, P4.)

- **AC3 — Travel time scales with distance, the slowest unit, and world speed (P7).** A farther
  target yields a proportionally longer travel time; a slower world-speed is proportionally longer;
  adding a slower unit to the mix lengthens the whole movement (effectiveSpeed = the slowest unit's
  map speed). Never below 1 second.

- **AC4 — Arrival stations the troops (P1/P2).** When the movement's due time passes, the troops are
  **stationed at the target** village as the sender's reinforcement, **exactly once**; the state is
  persisted, survives a restart, and a pending movement still completes after one. The target's
  owner can see reinforcements stationed with them; the sender can see their troops abroad.

- **AC5 — Send reinforcements back.** The sender can recall troops they have stationed at a village;
  this **removes the stationed reinforcement** and creates a **return** movement back to the source
  village, arriving after the (recomputed) travel time. On arrival the troops **rejoin the source
  garrison exactly once** (P1/P2), and the source's crop upkeep rises again accordingly.

- **AC6 — Away troops don't eat at home.** Troops in transit or stationed abroad are no longer in
  the source garrison, so they **no longer consume the source village's crop** (005 upkeep reads the
  garrison) — the home's net crop rises while they are away and falls again when they return.

- **AC7 — Interface.** The **Rally Point** page lets the player pick a target coordinate and per-unit
  counts and send a reinforcement, showing the resulting travel time. The **village** page shows the
  garrison, **reinforcements stationed here** (with the owner's name), the player's **troops
  stationed abroad** (with a **Send back** action), and **movements in progress** (direction,
  composition, and a live arrival **countdown**). Unavailable actions are not offered (and are
  rejected server-side regardless).

## Roles & permissions

Per [roles.md](../../roles.md).

| Role | Permitted | Denied (server-enforced) |
|------|-----------|--------------------------|
| **Visitor** | N/A (considered). | Send/view movements (redirected to login). |
| **Player** | Send reinforcements from **their own** village; recall **their own** stationed troops; see their movements, reinforcements here, and troops abroad. | Move another player's troops; send troops the garrison does not hold; forge the target, composition, or travel time; recall troops they do not own. |
| **Moderator** | N/A (considered). | — |
| **Administrator** | World speed scales travel time (AC3). | — (superset). |
| **System** | *(system-initiated)* Deliver arrivals at their due time — station reinforcements / rejoin the garrison (AC4/AC5); never mid-flight. | — |

## Out of scope

- **Combat: attack & raid, the battle formula, loot, walls/rams** → slice 009. Movements here never
  fight; only own/friendly destinations are reinforced.
- **Scouting** (010), **settling new villages** (013), **trade/merchants** (008) — other movement
  types on this same engine.
- **In-transit recall/cancellation.** Once sent, a movement completes; recall happens **after
  arrival** via a return movement (faithful — outgoing movements are not recalled mid-flight; §8.3).
- **Upkeep of stationed reinforcements charged to the host village** (faithful Travian) → a later
  refinement; for now away troops simply stop eating at home (AC6) and stationed troops add no
  upkeep to the host.
- **Rally Point level effects** (farm lists, etc.) and **travel-time modifiers** (Tournament Square,
  artifacts) → later.
- **Reinforcing your own other villages** → needs multi-village (013); a player has one village now,
  so reinforcement targets are other players' villages.

## Decisions

- **A movement is one advancing due-event row** with a troop child table; the same pattern as the
  build/training/unit-order queues (claim → apply → done; `FOR UPDATE SKIP LOCKED`; orphan requeue).
- **Away troops leave `village_units`,** so the source's 005 upkeep/starvation automatically drops
  for them with no cross-village rework. Stationed reinforcements are tracked in their own table and
  do **not** yet feed the host's upkeep (deferred, see Out of scope).
- **`effectiveSpeed` is the slowest unit's `speed`** (balance, fields/hour); world speed multiplies;
  distance is the toroidal map distance. All as data (P7).
- **Reinforcement targets are resolved from a coordinate** the player enters; the server requires a
  village there, not the player's own tile. The destination village id is fixed into the movement at
  send (so a later change of that tile's owner does not redirect troops already in flight).
- **Recall returns the whole stationed group** from a chosen host village (subset return is a later
  refinement).
