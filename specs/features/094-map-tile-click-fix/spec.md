# Feature 094 — fix: clicking a map tile does nothing (regression from 093)

## Why

093 added `setPointerCapture` so a drag keeps working when the pointer leaves the map viewport. But pointer
capture redirects the native `click` event off the tile (to the capturing viewport), so the per-tile `click`
listener never fired — clicking a tile no longer opened the inspector.

## Fix

Select the tile on **pointer-up when it wasn't a drag** (a tap), hit-testing with `document.elementFromPoint`
after releasing capture — instead of relying on the native click. The per-tile `click` listeners and the
capture-phase click-suppressor are removed (no longer needed).

## Acceptance criteria

- **AC1 — Tap selects.** Clicking/tapping a tile fills the 091 inspector aside (coord chip + label +
  Send/Centre) and highlights the tile.
- **AC2 — Drag still pans, doesn't select.** A press-drag pans the map and does not change the selection.
- **AC3 — No regression.** Buffered re-centre fetch, the "Go" form, mobile touch-drag, and the no-JS grid are
  unchanged; console is clean.

## Out of scope
- Any change to the tile data or the JSON endpoint (093).
