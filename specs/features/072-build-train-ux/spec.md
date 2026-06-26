# Feature 072 — build/train UX: explicit gate reason + train-to-max

## Why

Two small clarity improvements to the build/train flow:

1. **Vague build gate.** On the village plan (069), a building/field that can't be raised shows a generic
   inspector hint — *"Can't build yet — short on resources or the queue is busy."* It doesn't say which. The
   only reasons `can_order` is false (and the slot isn't at max) are a **busy queue lane** or **insufficient
   resources**, both of which the server already knows — so the message can name the exact reason (and, when
   short, exactly how much of each resource is missing).
2. **No "train max".** The training pages (Barracks/Stable/Workshop) take a count, but a player who wants "as
   many as I can afford" must compute it by hand. A **Max** button should fill in the largest count the current
   resources allow.

Presentation only — **no domain/sim change** (P3). Gating/affordability stay server-authoritative (P4): the
gate string only *reports* the server-known reason, and the Max button only *prefills* a count the server
re-validates (`order_train` rejects an unaffordable batch, so the prefill must be the true affordable max).

## Acceptance criteria

- **AC1 — Explicit gate.** When a village plot can't be built (not at max, `can_order` false), the inspector
  states the actual reason: if the queue lane is busy, that a construction is already underway; otherwise the
  resource shortfall, naming each short resource and the amount needed (e.g. "Need 540 wood, 120 iron"). At max
  it still reads "Max level reached".
- **AC2 — Train max.** Each training row has a **Max** button that sets the count to the largest number
  affordable from the current (live-displayed) resources for that unit, then refreshes the batch-total preview.
  If none is affordable it sets 0. The Train action and its server-side affordability check are unchanged.
- **AC3 — Behaviour preserved.** The build/train POSTs, gating, the existing batch-total preview, and the live
  resource counter all keep working — the gate text and Max button are display/input aids only.

## Roles (see specs/roles.md)

- **Player** — benefits. No authority change; gating stays server-side.

## Constitution

- **P3** — no domain change (the gate reason is derived in the web handler from data already loaded; the Max
  button is client-side). **P4** — server re-validates every build/train. **P11** — no new query.

## Out of scope

- Reflecting building **prerequisites** in the plan's buildable state (a separate gap — `can_order` already
  only covers lane + affordability).
