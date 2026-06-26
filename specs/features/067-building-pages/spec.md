# Feature 067 — the remaining building pages

## Why

066 established the building-page pattern (hero band + resource ribbon + main panel + aside) on the Smithy.
This slice rolls it out to the other in-game building pages so the whole in-village experience is consistent:
**Academy** (research), **Barracks / Stable / Workshop** (training, one shared template), **Rally Point** (send
troops), and **Marketplace** (trade). It also **DRYs the resource ribbon** into a shared partial + helper so
every page (and the Smithy) renders it from one source.

Presentation only — **no domain/sim change** (P3). Reuses the 063 unit thumbnails + 065 per-tribe building
backgrounds and the 066 chrome. Every action stays server-authoritative (P4); the existing forms, their
client-side previews (batch totals, army power/ETA, shipment estimate) and countdowns are preserved exactly.

## Acceptance criteria

- **AC1 — Shared resource ribbon.** A `ResourceRibbon` (amounts + rates + caps from the live economy) rendered
  by one shared partial appears on every building page, including the Smithy (refactored to use it). Numbers
  match the village page.
- **AC2 — Academy.** Hero band (academy art + crest + "Academy" + ← Village), the ribbon, and a **research
  roster**: each unit a row with its portrait, role, a compact stat line (att / def / speed / carry / upkeep),
  and either a **Researched** badge, the research cost + time + a **Research** action, or the gate reason; the
  research in progress shown in an aside with a countdown.
- **AC3 — Training (Barracks / Stable / Workshop).** Hero band (the building's art + crest + its name), the
  ribbon, and a **training roster**: each unit a row with its portrait, stats, per-unit cost + time, and a
  **count input + Train** with the live batch-total preferred; the running batch shown in an aside with a
  countdown.
- **AC4 — Rally Point.** Hero band (rally art + crest) + the ribbon around the existing **send-troops** form
  (order / target / scout / catapult / per-unit counts) with its live army power + travel-time preview intact.
- **AC5 — Marketplace.** Hero band (market art + crest) + the ribbon around the existing **send-resources**
  form with the merchant pool readout and its live merchants-needed + round-trip preview intact.
- **AC6 — Behaviour preserved.** Every POST, the gating (P4), the "build X first" notices, empty states, and
  all the existing client-side previews/countdowns work exactly as before — a reskin, not a rule change.

## Roles (see specs/roles.md)

- **Player** — owns these pages; the only role affected.
- **Visitor / Moderator / Admin** — unaffected (login-gated, P4).

## Constitution

- **P3** — pure domain untouched; web templates/handlers/CSS only.
- **P4** — all gates stay server-authoritative; the pages only display state.
- **P11** — the economy is already loaded by `village_view_data` (066); no new queries.

## Out of scope

- The **village page** (fortress plan) and the **Wonder** page — separate slices.
