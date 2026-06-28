# Tasks — 107 map scoped to selected village

Branch `feature/107-map-selected-village`.

- [x] **T1**: routes — `/village/{village}/map` + `/village/{village}/map/tiles` (village-scoped); bare `/map`
  → `map_default` (capital default). `map`/`map_default` share `map_for(ctx, path_village, q)`.
- [x] **T2**: `map_for` + `map_tiles` resolve the acting village from the path (validated; fallback
  capital/first); acting_vid + origin/home + centre default use it. `MapTemplate.village` threads it.
- [x] **T3**: map.html — recentre/jump/tiles URLs use `/village/{{ village }}/map…`; label "Recentre on this
  village". village.html — Map link carries `village_id`.
- [x] **T4 — Verify**: live multi-village (Leberkas5, 3 villages) — village 2's map recentres on (-3|-2) and
  sends from village 2, not the capital (-2|-2). Tests updated to the village-scoped URLs; full web suite green.
- [ ] **T5 — Reviewer + PR.**
