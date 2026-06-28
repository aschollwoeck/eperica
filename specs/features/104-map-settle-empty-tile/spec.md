# Feature 104 — send settlers to an empty valley from the map

## Why

Clicking an empty tile on the map showed no action button — so you couldn't send anything there, including
**settlers** to found a new village. Empty valleys are the settle targets (013), so the inspector should let
you send settlers to one.

## Acceptance criteria

- **AC1 — Empty valley is actionable.** Clicking an empty valley tile shows a **"Send settlers →"** button
  linking to the Rally Point pre-filled with the tile (where a Settle order sends settlers). Its label reads
  "free valley".
- **AC2 — Contextual label.** A village/oasis tile keeps **"Send troops →"**; only an empty valley reads
  "Send settlers →" (the cell's `settle` flag drives the label). Works server-rendered + over the drag-stream.

## Out of scope
- The settling flow itself (013 — CP/expansion-slot gates apply at the Rally Point as before).
