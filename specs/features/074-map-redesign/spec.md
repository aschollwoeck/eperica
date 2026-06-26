# Feature 074 — the world-map redesign

## Why

The `/map` page — the grid players navigate constantly — is still the original `<table>` on a plain panel,
visually out of step with the redesigned village/building pages. This slice brings it onto the design system:
the shared command-header chrome, a styled **tile grid**, and a **click-to-inspect** affordance that drives the
send-troops / recenter actions (mirroring the village fortress plan).

Presentation only — **no sim/visibility change** (P3/P4): the same tiles, the same fog (troop counts/resources
stay hidden until scouted), the same per-tile action links; only the layout and a client-side inspector change.

## Acceptance criteria

- **AC1 — Command header.** A stamped header (reusing the `.vcmd` chrome) with the centre coordinate chip and
  world radius, the **recenter controls** (N/S/E/W + the x/y "Go" jump), and a ← Village return.
- **AC2 — Tile grid.** The viewport renders as a styled grid of tiles (not a `<table>`): valley / oasis /
  Natar terrain colours, ★ for villages, the player's own village + capital highlighted, occupied oases and
  inactive (farmable) villages marked — preserving every existing `map-grid__cell--*` state.
- **AC3 — Tile inspector.** Clicking a tile shows its full label (coordinate, owner/alliance, presence,
  distance) and the actions it affords: **Send troops** (the Rally-Point link the tile already carries, for an
  enemy village / oasis) and **Center here** (recenter the map on that tile). Plain/own tiles just show the
  label.
- **AC4 — Behaviour preserved.** The recenter links + Go form, the per-tile rally links, the fog-of-war
  (nothing new revealed), and the legend all work exactly as before — a reskin, not a rule change.

## Roles (see specs/roles.md)

- **Player** — navigates the map. No authority/visibility change (P4).

## Constitution

- **P3** — pure domain untouched; one handler tweak (expose each cell's x/y for the inspector links), a
  template rewrite, CSS. **P4** — fog-of-war unchanged; the inspector only shows what the cell already carried.
  **P11** — no new query.

## Out of scope

- A pannable/zoomable canvas map (the viewport-recenter model is kept).
