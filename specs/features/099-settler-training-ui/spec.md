# Feature 099 — train settlers (Residence/Palace expansion page)

## Why

Settlers (the expansion units that found new villages, 013) are `trained_in = residence` in the balance
data, and the domain lets a **Palace** stand in for a Residence — but the web app had **no page, route, or
link** to train them, and the DB rejected a settler training batch. A player with a Palace (or Residence)
literally could not train settlers, even though the village page told them to.

## Acceptance criteria

- **AC1 — Expansion training page.** `GET /village/{village}/residence` shows the researched expansion units
  (settlers/administrators) with a train form, like the Barracks/Stable/Workshop pages. A **Palace** serves
  the same page (it substitutes for a Residence, 013) — the page is labelled by whichever the village has.
- **AC2 — Linked from the village.** The village page lists the expansion building (Residence or Palace, when
  built) alongside the other training buildings.
- **AC3 — Training works.** Ordering a settler starts a batch and returns to the page. The batch is keyed to
  `residence`; the DB `training_orders` CHECK constraint is widened to allow it (migration 0049). A Palace
  village trains at the Palace's level (not level 0) — `training_building_level` (domain) treats the Palace as
  a Residence for the training-speed level.

## Out of scope
- Settler **research** (Academy, unchanged) and the settle flow itself (013, unchanged).
