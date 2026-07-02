# Feature 116 — village status strip & drill-yard portrait

**Status:** In progress
**Depends on:** 005 (training), 007 (movement), 013 (culture/expansion), 015 (alliance incoming-defence data),
069 (village fortress plan)
**Roadmap:** app-layer UX pass (surfaces existing state; no new sim rules).

## Goal

Surface a village's most time-critical status **at a glance, above the fold**, and give the troop-training
"drill yard" card a face. This is a **read-only presentation** slice: it aggregates already-persisted state
on read (P1) and adds **no game rules and no writes**.

Two surfaces:
1. A **status strip** at the top of the village page — three cards: **Incoming attacks**, **Training**,
   **Culture** — rendered above the fortress plan.
2. The **drill-yard card** on a troop building (Barracks/Stable/Workshop/Residence) shows the
   **currently-training unit's portrait**, matching the Smithy's forging card.

## Concepts

- **Incoming attacks** reuse the 015 incoming-defence data (`incoming_against`): hostile movements landing
  on the village, **arrival time only**. Faithful Travian (§7.3, P4): the attacker's **origin and troop
  composition are never revealed** here — only that an attack is inbound and when it lands.
- **Training** reuses `active_training`: the batches currently in progress across the village's troop
  buildings (unit + remaining count + next-completion time).
- **Culture** reuses the 013 expansion data already on the village page (pooled CP, CP/h, villages
  used/allowed, next-village threshold) shown compactly with a progress bar.
- **Drill-yard portrait** is the training unit's `<tribe>_<unit>.webp`, mirroring the Smithy's forging card.

## User stories

- As a **player**, I want incoming attacks, active training, and culture progress visible without scrolling,
  so I can react to threats and plan without hunting through the page.
- As a **player**, I want the "on the drill yard" card to show what's being trained, so it reads at a glance.

## Acceptance criteria

- **AC1 — Strip placement.** The village page renders a status strip of three cards — **Incoming attacks**,
  **Training**, **Culture** — above the fortress plan (069). It is read-only.

- **AC2 — Incoming attacks (P4/§7.3).** The Incoming-attacks card lists each hostile movement landing on the
  village with a **live countdown to arrival**, and shows **no attacker coordinate and no troop count** — the
  attacker's origin/composition are withheld. The card is visually **flagged (alert)** while any attack is
  inbound; it reads "None — all quiet." when there are none. Count is shown.

- **AC3 — Training.** The Training card lists each active training batch — unit name + remaining count — with
  a **live countdown** to the next completion; it reads "Not training." when idle. Count is shown.

- **AC4 — Culture.** The Culture card shows pooled **CP** (+ CP/h), **villages used/allowed**, the
  next-village **threshold** (or "max"), and a **progress bar** toward it (0–100%, clamped).

- **AC5 — Compute-on-read & resilient (P1/P11).** All three cards are derived from persisted state on read;
  no scheduler/tick and **no writes**. A lookup error for any source degrades to its empty/"none" state and
  **never** produces a 500 for the page.

- **AC6 — Drill-yard portrait.** On a troop building's page, when a batch is training, the "On the drill yard"
  card shows the training unit's tribe-specific portrait (`/static/units/<tribe>_<unit>.webp`); idle shows no
  portrait. A missing image degrades to the empty plate (no broken layout).

- **AC7 — Ownership (P4).** Every source is scoped to the acting player's own, already-authorized village
  (`incoming_against(&[village.id])`, `active_training(village.id)`, the player's culture); no cross-owner
  data is exposed.

## Roles & permissions

Per [roles.md](../../roles.md).

| Role | Permitted | Denied (server-enforced) |
|------|-----------|--------------------------|
| **Visitor** | N/A — cannot reach a village. | The village page (redirected to login). |
| **Player** | See the strip + drill-yard portrait for **their own** village (AC1–AC7). | Any other player's village/status. |
| **Moderator** | N/A (considered) — no moderation surface here. | — |
| **Administrator** | N/A (considered) — superset; no admin-only behaviour. | — |
| **System** | *(none)* — no background job; all compute-on-read (P1). | — |

## Out of scope

- **Acting on** an incoming attack (dodging, reinforcing) — the Rally Point / Reports already own that.
- Revealing attacker origin/troops (arrives with **scouting**; withheld by design, P4/§7.3).
- The **mobile responsiveness** pass and the **primary-button** redesign (slice 115).
- Historical/aggregated stats (rankings own that).

## Decisions

- **Arrival-only incoming attacks.** Faithful to Travian: the overview warns *that* an attack is inbound and
  *when* it lands; the attacker's origin and troop counts stay hidden until a scout report or the battle.
- **Reuse, don't recompute.** Culture reuses the 013 numbers already loaded for the page; training/attacks
  reuse the existing repository reads — this slice only presents them.
