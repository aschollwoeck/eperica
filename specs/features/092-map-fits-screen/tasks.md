# Tasks — 092 map fits screen

CSS only; branch `feature/092-map-fits-screen`. Gated by fmt/clippy/`cargo test --workspace`.

- [x] **T1**: `.mgrid` gets `--mt: clamp(22px, calc((100svh - 400px)/13), 56px)`; base `.mtile` caps at
  `max-width: var(--mt)` and still flex-shrinks (mobile).
- [x] **T2**: desktop media (`min-width: 981px`) — fixed-size tiles (`flex: 0 0 var(--mt)`) + `.mgrid`
  `width: fit-content; margin-inline: auto` so the panel hugs the grid.
- [x] **T3 — Verify**: live — fits 768px (28px tiles) and scales to 50px @1050; no overflow/scroll on mobile
  (23px tiles); map tests pass.
- [ ] **T4 — Gate + reviewer + PR.**
