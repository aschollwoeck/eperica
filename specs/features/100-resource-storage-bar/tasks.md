# Tasks — 100 resource storage bar

Branch `feature/100-resource-storage-bar`.

- [x] **T1**: `.gauge__fill { display: block; width: 0; … }` (an inline `<i>` ignored width). `transition`
  for the live counter.
- [x] **T2**: `ResourceRibbon::{wood,clay,iron,crop}_pct()` (clamped 0–100, divide-by-zero safe); `_ribbon.html`
  renders `style="width: {{ … }}%"` on each fill. JS already updates the width live (now it applies).
- [x] **T3 — Verify**: live — bars fill 26/51/76/99% for 3k/6k/9k/12k of 12k; unit test on the pct logic.
  fmt/clippy/test green.
- [ ] **T4 — Reviewer + PR.**
