# Tasks — 073 consistent roster rows

Gated by `cargo fmt --all -- --check`, `clippy -D warnings`, `cargo test --workspace`, P11. Presentation only
(CSS + 3 templates); branch `feature/073-consistent-roster-rows`.

- [x] **T1 — CSS.** Unify `.unit` to the four-column grid `thumb · info · price · action`; add `.unit__price`
  (cost + time, right-aligned); drop the per-roster grid overrides + dead `.forge`/`.unit__role` CSS; scope
  the Smithy pips/effect into the info column; fix the mobile reflow. (AC1/AC2)
- [x] **T2 — Academy.** Move cost + time into a `.unit__price` column; action keeps Researched/Research/gate.
- [x] **T3 — Training.** Move cost out of the info column into `.unit__price`; action keeps count + Max +
  Train + batch-total / gate.
- [x] **T4 — Smithy.** Restructure to the shared shape: identity column holds name + role + forge level + pips
  + effect; cost + forge time go to `.unit__price`; action keeps Forge / "at the anvil" + countdown / gate.
- [x] **T5 — Tests.** Assert `unit__price` + `unit__cost` on the academy, training, and smithy pages (the
  shared price column), keeping the existing behaviour assertions. (AC1/AC2)
- [ ] **T6 — Gate + reviewer + PR.** fmt/clippy/`cargo test --workspace` green; Playwright (the three rosters
  align; cost in the same column); `eperica-reviewer` → APPROVE; PR opened.
