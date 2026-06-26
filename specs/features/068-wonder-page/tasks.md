# Tasks — 068 the Wonder page

Gated by `cargo fmt --all -- --check`, `clippy -D warnings`, `cargo test --workspace`, P11. Presentation only;
branch `feature/068-wonder-page`. No handler/struct change.

- [x] **T1 — CSS.** Add the Wonder progress-leaderboard + victory-banner components to `base.css`
  (`.wonder-board`/`.wonder-row` with the level bar + leader/done states, `.wonder-victory`, a narrow body).
  (AC2/AC3)
- [x] **T2 — Template.** Rewrite `wonder.html`: hero band (monument crest + title + win-condition/winner
  note) + the victory banner + the progress leaderboard; keep the empty state and the `{level} / {max}` +
  "The round is over" text the tests rely on. (AC1–AC4)
- [x] **T3 — Tests.** Extend the wonder integration assertions for the new chrome (hero, the board) while
  keeping the existing progress + winner-banner assertions. (AC1–AC4)
- [x] **T4 — Gate + reviewer + PR.** fmt/clippy/`cargo test --workspace` green; Playwright check (race +
  won states); `eperica-reviewer` → APPROVE; PR opened.
