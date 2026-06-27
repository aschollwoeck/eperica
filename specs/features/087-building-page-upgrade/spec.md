# Feature 087 — building pages own the upgrade; the village plan is a pure overview

## Why

On the village page, the only working upgrade control was an inline **inspector** (clicking a plot/field
filled a build form). The dedicated building pages (Smithy/Academy/…) instead showed a "Raise the X →" button
that was just a **link back to the village** — it never advanced the level (the bug the player hit).

This makes the village plan a pure **overview**: clicking a building or field is a link to that thing's page,
and the working **upgrade panel** lives on the page. Every building gets a page (a generic page for the ~11
without a dedicated one) and every resource field gets one too, so the inspector is removed entirely.

Presentation/handler wiring only — no domain rule change (P3); upgrades still go through the existing
`order_build` use-case and re-validate server-side (P4).

## Acceptance criteria

- **AC1 — Plan is links.** On the village plan, each building plot links to its page (its functional page when
  built, else the generic `/building/{kind}`) and each field links to `/field/{slot}`. The inline inspector is
  gone.
- **AC2 — Working upgrade panel.** Each building/field page shows a panel with the current level, the next
  level's effect + cost, the explicit gate (072) when blocked, an under-construction countdown when building,
  and a **Build/Upgrade** button that posts to `…/build` and actually advances the level. The dedicated
  functional pages' "Raise the X →" links are replaced by this panel.
- **AC3 — Generic detail page.** Buildings without a functional page (Warehouse, Granary, Main Building, …) and
  all resource fields render a generic detail page (hero + description + ribbon + the upgrade panel).
- **AC4 — Return to the page.** Ordering an upgrade returns to the page it was ordered from (a server-validated
  `back` leaf; an unsafe value is rejected — no open redirect, P4).

## Reuse

- `build_row` / `building_effect` / `field_effect` extracted from the village handler's `make_row` closure so
  the plan and every page produce the identical row. `order_build` (013) unchanged.

## Out of scope

- Any change to a building's function, costs, or the build queue/lane rules.
