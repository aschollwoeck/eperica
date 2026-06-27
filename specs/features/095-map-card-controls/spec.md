# Feature 095 — map controls into the tile card + recentre-on-home

## Why

The map's "x / y / Go" jump form sat in the command header, and there was no quick way to return to the
village after panning. Move the jump form into the tile card (making it the map's control panel) and add a
"Recentre on home" control.

## Acceptance criteria

- **AC1 — Controls in the card.** The x/y/Go jump form lives in the map's right-aside card (header now has just
  the title + quick-links); the card also holds the clicked-tile inspector below a divider.
- **AC2 — Recentre on home.** A "Recentre on home (x | y)" control returns the map to the player's home
  (capital, else first village). The jump inputs track the current centre as the map moves.
- **AC3 — Smooth + no-JS.** With JS, the jump form, the home control, and "Centre here" pan the map smoothly
  (no reload, via the 093 fetch); without JS they are a GET form / links to `/map?x&y` (server recenter).

## Out of scope
- The drag/stream behaviour (093) and tile data (unchanged).
