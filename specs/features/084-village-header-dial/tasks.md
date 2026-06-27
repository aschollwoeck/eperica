# Tasks — 084 village header dial

Gated by fmt/clippy/`cargo test --workspace`/P11. CSS + one template class; branch `feature/084-village-header-dial`.

- [x] **T1 — CSS + class.** A `.vcmd--head` modifier (`flex-wrap: nowrap` + id `flex: 1`) scoped to the village header; `align-items: flex-end`→`flex-start` on the shared `.vcmd`. The map keeps the default wrap.
  `.vcmd__dials { flex: none }` so the dial pins top-right in the name row.
- [x] **T2 — Verify.** Live measurement (desktop: dial flush top-right, in the name row, shorter header;
  mobile 390px: no overflow, dial on the right).
- [ ] **T3 — Gate + reviewer + PR.** fmt/clippy/`cargo test --workspace` green; reviewer APPROVE; PR opened.
