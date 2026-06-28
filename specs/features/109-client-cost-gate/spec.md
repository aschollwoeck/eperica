# Feature 109 — client re-enables a cost-gated button as resources tick up

## Why

When a build/upgrade button is disabled because resources are short, it stays disabled until the page is
reloaded — even though the live resource ribbon (070) is visibly ticking up toward the cost. Re-enable such a
button on the client, in step with the ribbon tick, so the player can act the moment they can afford it. A
rough client-only estimate; the server still validates the order on submit (P4). Generic + reusable for any
resource-gated button (buildings, fields, …).

## Acceptance criteria

- **AC1 — Cost on the button.** A button disabled *only* because resources are short (not maxed, not a busy
  queue lane) carries its cost as `data-cost-wood/clay/iron/crop`; the "Need … more" note is flagged
  `data-cost-note`. (`BuildRow.cost_gated`.)
- **AC2 — Client re-enable.** The resource ribbon's existing tick (the live counter) re-enables any
  `button[disabled][data-cost-wood]` once the live amounts cover its cost, and hides its shortfall note —
  minimal JS, no new timer (it rides the 1 s ribbon tick).
- **AC3 — Applies to research & unit upgrades too.** The Academy (research) and Smithy (unit upgrade) render a
  *disabled* cost-bearing button (not a gate span) when the only obstacle is resources, so they self-enable
  the same way. Buttons disabled for a non-resource reason (requirements unmet, queue busy, max) stay a gate
  span with no `data-cost-*`.

## Out of scope
- Server-side validation (unchanged — re-checked on submit, P4). Troop *training* (Barracks/Stable/Workshop)
  is count-based — its button isn't resource-disabled at render (the per-unit cost drives the "Max" helper),
  so it's out of scope.
