# Feature 105 — "Send troops" to your own village from the map

## Why

Clicking your **own** village on the map showed no "Send troops" button — the rally shortcut was set only for
*other* players' villages ("you can't target your own"). But sending troops to your own village is valid:
**reinforce** it, or move troops between your villages. The button should appear for own villages too.

## Acceptance criteria

- **AC1 — Own village is a send target.** Clicking a tile holding the viewer's own village shows "Send troops →"
  (the Rally Point pre-filled with the tile, where Reinforce sends the defenders). Other villages and the
  merchant/settle behaviours are unchanged.

## Out of scope
- The Rally Point order validation (013/007 — unchanged; reinforce/attack/raid still server-validated).
