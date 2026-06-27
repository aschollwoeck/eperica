# Feature 088 — split buildings & fields: the resource fields ring the village

## Why

The village plan showed the buildings in a walled centre and the resource fields as a separate card-grid
below. Faithful to Travian, the buildings belong to the village centre and the resource fields belong to the
land **around** it. This unifies them into one map: the walled village in the middle, the 18 resource fields
ringing it as icon tiles.

Presentation only — village template + CSS; no domain/handler change (P3). Each field tile still links to its
`/field/{slot}` page (087) where the upgrade lives.

## Acceptance criteria

- **AC1 — Fields ring the village.** The 18 resource fields render as resource-icon tiles positioned around
  the perimeter of the canvas (top/right/bottom/left), surrounding the walled village centre on all four
  sides. Each tile shows its resource icon + level, a build/ready state, and an on-tile countdown when under
  construction; it links to `/field/{slot}`.
- **AC2 — Buildings in the centre.** The building plots stay in the walled village centre (a `.vcenter` box
  with the rampart/towers/gate), each linking to its page (087) — unchanged behaviour.
- **AC3 — One view.** The separate "Resource fields" grid section is removed; the map is a single `.vcanvas`
  ("Village & fields"). No horizontal overflow; on a phone the map reflows to stacked grids (fields, then
  buildings).

## Out of scope

- Any change to field/building costs, levels, or the upgrade flow (087).
