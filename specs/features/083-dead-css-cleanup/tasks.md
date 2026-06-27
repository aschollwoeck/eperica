# Tasks — 083 dead CSS cleanup

Gated by fmt/clippy/`cargo test --workspace`/P11. Pure cleanup; branch `feature/083-dead-css-cleanup`.

- [x] **T1 — Audit.** Confirm 0 references (templates + handlers + JS) for each candidate class.
- [x] **T2 — Remove** the verified-dead rules from `base.css` + delete the orphaned `theme-ash.css`.
- [x] **T3 — Verify.** Build/clippy/full suite green; live computed-style smoke test of the kept components.
- [ ] **T4 — Reviewer + PR.** reviewer APPROVE; PR opened.
