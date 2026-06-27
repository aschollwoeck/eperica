# Tasks — 091 map two-column

Branch `feature/091-map-two-column`. Gated by fmt/clippy/`cargo test --workspace`.

- [x] **T1 — map.html**: `.vcols` (map grid + legend left, `.vrail` aside with the `.bld-card` tile inspector);
  JS fills the coord chip + label + actions.
- [x] **T2 — base.css**: smaller `.mtile` (flex cap ~64px, centred rows); restyle `.minspect__label/__act` for
  the vertical card; drop the old `.minspect` bar + `.mwrap` rules.
- [x] **T3 — handlers.rs**: `MAP_HALF` 4→6 (13×13 viewport) so smaller tiles fill the column.
- [x] **T4 — Verify**: live (map left + inspector aside, tiles ~60px, click fills the card, no overflow;
  mobile stacks + tiles shrink); map tests pass; added a `vcols`/`vrail`/`minspect` assertion.
- [ ] **T5 — Gate + reviewer + PR.**
