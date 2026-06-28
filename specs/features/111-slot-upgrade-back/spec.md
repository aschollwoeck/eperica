# Feature 111 — upgrading a multi-instance building stays on its slot

## Why

After 110, a village can hold multiple Warehouses/Granaries/Crannies, each on its own slot. But the upgrade
panel's "return to" link (`_upgrade.html`'s `back`) was still **kind-keyed** (`/building/{kind}`), which
resolves to the **first** instance of that kind. So upgrading Warehouse #2 redirected the player back to
Warehouse #1's page. The redirect must be **slot-keyed**.

## Acceptance criteria
- **AC1** — Ordering an upgrade from a (non-functional) building's slot page returns to **that slot's** page
  (`/slot/{slot}`), so upgrading the 2nd Warehouse stays on the 2nd Warehouse. Functional buildings
  (Barracks, Smithy, …) still return to their functional page (they're one-per-village). Fields unchanged.

## Out of scope
- The build flow itself (110, unchanged) — only the post-order redirect target.
