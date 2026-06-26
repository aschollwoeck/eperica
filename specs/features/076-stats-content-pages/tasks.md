# Tasks — 076 stats & content pages redesign

Gated by `cargo fmt --all -- --check`, `clippy -D warnings`, `cargo test --workspace`, P11. Presentation only;
branch `feature/076-stats-content-pages`.

- [x] **T1 — CSS.** Add `.statgrid`/`.statcard`, the leaderboard `.tabs`/`.tab`/`.tab--active` + `.filters`,
  the `.questcard`/`.donelist`, and the `.bioform`/`.bio-read` — reusing `.phead`/`.page` (075),
  `.bld-cols__head` section heads, `.bld-card`, and `.table`.
- [x] **T2 — Leaderboard.** `.phead` + styled tabs/filters + the ranked `.table`.
- [x] **T3 — Player stats.** `.phead` (+ presence) + a `.statgrid` summary + section heads over the villages /
  medals / achievements / history tables + the report form in a card.
- [x] **T4 — Alliance stats.** `.phead` + `.statgrid` aggregate + members/medals section tables.
- [x] **T5 — Quests.** `.phead` + the current quest as a `.questcard` + the completed `.donelist`.
- [x] **T6 — Profile.** `.phead` + the bio form in a `.bld-card` (styled textarea).
- [x] **T7 — Tests.** Assert the new chrome (`phead`/`tabs`/`statgrid`/`donelist`) across the pages, keeping
  the existing content + auth-scope assertions.
- [ ] **T8 — Gate + reviewer + PR.** fmt/clippy/`cargo test --workspace` green; Playwright sweep;
  `eperica-reviewer` → APPROVE; PR opened.
