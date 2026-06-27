# Tasks — 086 village card density

Gated by fmt/clippy/`cargo test --workspace`/P11. Presentation; branch `feature/086-village-card-density`.

- [x] **T1 — Links beside name.** village.html: wrap `.vcmd__name` + `.vquick` in `.vcmd__namerow`; CSS
  `.vcmd__namerow` flex + `.vcmd__namerow .vquick { display: contents }` (village-scoped).
- [x] **T2 — Tighter cards.** Reset heading margins (`.bld-cols__head h2`, `.bld-card__h h3`, `.vplan__head h2`
  → `margin: 0`); trim `.bld-card__h/__b`, `.bld-cols__head`, `.psec`, `.statline`, `.vrail` gap/`.feed__row`,
  `.vinspect`, `.vplan__head`, `.vfields` paddings/margins.
- [x] **T3 — Verify.** Live: links right of the name + on its row; card header 66→34px; no overflow desktop/
  mobile; map vquick stays flex; smithy renders.
- [ ] **T4 — Gate + reviewer + PR.** fmt/clippy/`cargo test --workspace` green; reviewer APPROVE; PR opened.
