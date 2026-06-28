# Tasks — 114 fixed crop upkeep
- [x] **T1** — spec: this + supersede 002 AC4 / note 005. (no code)
- [x] **T2** — domain: `production_rates` crop_net scales only field output, subtracts unscaled
  population + upkeep; drop `net_crop_base`; update economy tests. (AC1)
- [x] **T3** — application: the starvation cull budget = `crop_net + upkeep` (scaled output − population),
  not the old unscaled base; tests. (AC2)
- [x] **T4** — integration + live: high-speed village is crop-positive; under-developed still starves. (AC3)
- [ ] **T5** — reviewer + PR + merge + restart.
