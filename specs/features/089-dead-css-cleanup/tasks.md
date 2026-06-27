# Tasks — 089 dead CSS cleanup

CSS only; branch `feature/089-dead-css-cleanup`. Gated by fmt/clippy/`cargo test --workspace`.

- [x] **T1 — Audit.** Confirm 0 references (templates + handlers + JS) for each candidate; rule out the
  dynamic `.vplot--{kind}`/`.vfield--{res}` false positives.
- [x] **T2 — Remove** `.vinspect*`, `.vplot--sel`, `.feed__ico--atk`; fix the stale plot comment.
- [x] **T3 — Verify** build/clippy/tests green; base.css 780→767.
- [ ] **T4 — Reviewer + PR.**
