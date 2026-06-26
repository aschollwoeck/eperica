# Tasks — 079 the alliance overview redesign

Gated by fmt/clippy/`cargo test --workspace`/P11. Presentation only; branch `feature/079-alliance-page`.

- [x] **T1 — Template.** Rewrite `alliance.html`: `.page` + adaptive `.phead` + `.bld-cols__head` section
  heads over every block; keep all tables, inline forms, role/right gates, confirm dialogs, and the when-JS.
- [x] **T2 — Test.** Assert `phead`+`psec` on the alliance page, keeping the found/invite/role/diplomacy flow.
- [ ] **T3 — Gate + reviewer + PR.** fmt/clippy/`cargo test --workspace` green; Playwright; reviewer APPROVE; PR.
