# Tasks — 088 fields ring the village

Presentation only; branch `feature/088-fields-around-village`. Gated by fmt/clippy/`cargo test --workspace`.

- [x] **T1 — village.html**: wrap the building plots in `.vcenter`; move the 18 fields into a `.vfield-ring`
  inside `.vcanvas` as icon tiles; drop the separate `.vfields` grid section.
- [x] **T2 — base.css**: taller `.vcanvas`; `.vcenter` (offset parent for plots); rampart/towers/gate inside it;
  `.vfield` icon-tile + 18 `:nth-child` ring positions; `.lg-res` legend; mobile stacked-grid reflow; remove
  the old `.vfields`/`.vfield__*` card styles.
- [x] **T3 — Verify**: live (fields surround the centre on all 4 sides, 18 tiles, no overflow; mobile stacked);
  migrate the 3 tests asserting `vfields`/"Resource fields"/"Village plan" to the new markup.
- [ ] **T4 — Gate + reviewer + PR.**
