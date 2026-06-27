# Tasks — 095 map card controls

Branch `feature/095-map-card-controls`. Gated by fmt/clippy/`cargo test --workspace`.

- [x] **T1**: handler/template — `home_x`/`home_y` on `MapTemplate` (origin = capital/first village).
- [x] **T2**: map.html — move the jump form into the card; add the `#mhome` recentre control + a divider;
  remove the header `.map-nav`. JS: jump/home/"Centre here" → smooth `fetchRegion`; sync the inputs on render.
- [x] **T3**: base.css — `.mjump` form + `.minspect__rule`; drop the unused `.map-nav`.
- [x] **T4 — Verify**: live — controls in the card; Recentre-on-home jumps to the village; Go jumps; both
  smooth (no reload). Map tests pass + assert `mjump`/"Recentre on home".
- [ ] **T5 — Gate + reviewer + PR.**
