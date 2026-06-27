# Tasks — 090 building backdrop

CSS only; branch `feature/090-building-backdrop`. Gated by fmt/clippy/`cargo test --workspace`.

- [x] **T1 — Backdrop**: `.bld-page main::before` fixed full-page building-art layer (reads --building-img*),
  fading to --c-bg; inert over --c-bg where no image var is set.
- [x] **T2 — Hero**: drop its own image background (the backdrop provides it); min-height 320→190 (mobile
  270→150) + a bottom scrim so the title reads.
- [x] **T3 — Verify**: live (smithy hero 320→190, body top 432→302, doc 1219→1089; image behind cards + to the
  footer; no overflow; village backdrop inert; mobile ok); building_bg lib tests still pass.
- [ ] **T4 — Gate + reviewer + PR.**
