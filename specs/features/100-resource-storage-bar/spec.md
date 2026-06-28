# Feature 100 — resource cards: a real storage progress bar

## Why

Each resource card (the 067 ribbon) has a thin line below the numbers that never changed — it read as a static
underline, not a fill. It was meant to be a storage progress bar (`amount / capacity`), but the fill (`<i>`)
was an inline element, and inline elements ignore `width`, so the JS- and never-set width had no effect.

## Acceptance criteria

- **AC1 — The bar fills by amount.** Each gauge's `.gauge__fill` width = `amount / capacity` (clamped 0–100%):
  empty resource ⇒ empty bar, full ⇒ full bar, partial ⇒ proportional.
- **AC2 — Server-rendered + live.** The width is rendered server-side (`ResourceRibbon::*_pct()`) so it's
  correct without/before JS; the 070 live counter keeps it moving (and `display:block` makes the width apply).

## Out of scope
- The numbers/rates (unchanged); any storage-cap mechanic.
