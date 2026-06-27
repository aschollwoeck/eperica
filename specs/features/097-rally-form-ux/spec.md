# Feature 097 — Rally Point form UX

## Why

The send form was static: no quick "max", "Reinforce" was the default order, and the "Spy on" / "Catapult
target" fields always showed even when irrelevant. Make the form lead with the unit selection and reveal the
order-specific fields contextually.

## Acceptance criteria

- **AC1 — Unit selection first.** The troop table is the first field, so the JS can read the army to decide
  which fields to show.
- **AC2 — Max button.** Each unit row has a "max" button that fills its count to the garrison size.
- **AC3 — Raid default.** The Order select defaults to "Raid".
- **AC4 — Contextual fields.** "Spy on" shows only when the army includes a scout (or the order is Scout);
  "Catapult target" shows only when the army includes a catapult. Both are hidden via JS; **without JS they
  render visible** (progressive enhancement) so the form still works. Driven by per-unit `data-scout` /
  `data-catapult` flags (`role == Scout`, `siege_kind == Catapult`) emitted server-side.

## Out of scope
- Server-side send validation (unchanged; server-authoritative).
