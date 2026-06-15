# Feature 033 — Map info (distance + send shortcut)

**Status:** Reviewed
**Depends on:** 006 (the world map), 007/012 (movement; oases already link to the Rally Point)
**Roadmap:** app-layer UX — the last of the **UX information** pass (031/032): make the map actionable at a glance.

## Goal

Two small map enrichments so a player can judge and act from the map directly:

- **AC1 — Distance from home.** Each tile's hover label shows its toroidal distance (rounded, in fields)
  from the player's home village (capital, else first village). Own tile (distance 0) is omitted.
- **AC2 — Send shortcut.** Another player's village links to the Rally Point pre-filled with that tile
  (`/village/rally?x&y`) — the same affordance oases already have. Your own villages are not targetable.

## Notes

- Read-only presentation; distance uses the existing `WorldMap::distance` (toroidal). No rule change (P3/P4).
- The send link mirrors the existing oasis link; the Rally Point's own server-side validation is unchanged.

## Out of scope

- Per-target travel-time on the map (it depends on the army chosen — shown on the Rally Point, 031).
- Visual theming / imagery (the later pass).
