# Tasks — 084 village header dial

Gated by fmt/clippy/`cargo test --workspace`/P11. CSS-only; branch `feature/084-village-header-dial`.

- [x] **T1 — CSS.** `.vcmd` → `flex-wrap: nowrap; align-items: flex-start`; `.vcmd__id { flex: 1 }` +
  `.vcmd__dials { flex: none }` so the dial pins top-right in the name row.
- [x] **T2 — Verify.** Live measurement (desktop: dial flush top-right, in the name row, shorter header;
  mobile 390px: no overflow, dial on the right).
- [ ] **T3 — Gate + reviewer + PR.** fmt/clippy/`cargo test --workspace` green; reviewer APPROVE; PR opened.
