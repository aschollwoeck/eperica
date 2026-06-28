# Feature 106 — pre-select the Rally Point order from the map link

## Why

The map inspector's "Send troops" / "Send settlers" links pre-filled the target tile but always left the Rally
Point on its default order (Raid). Clicking "Send settlers" should land on **Settle**; "Send troops" should
land on the order that fits the tile.

## Acceptance criteria

- **AC1 — Mode carried + pre-selected.** The map link carries `?mode=…`; the Rally Point pre-selects that order
  (`raid`/`attack`/`reinforce`/`scout`/`settle`). An absent/unknown mode falls back to `raid`. A `settle` mode
  is honoured only when the Settle order is available (otherwise the default shows).
- **AC2 — Contextual mode per tile.** Empty valley → `settle`; your own village → `reinforce`; another's
  village → `raid`; an oasis you hold → `reinforce`, a wild/other oasis → `attack`.

## Out of scope
- The order's server-side validation (007/013 — unchanged).
