# Tasks — 101 settlers without research

Branch `feature/101-settlers-no-research`.

- [x] **T1 (domain)**: `UnitRules::new` counts research-free **combat** units only (Expansion excluded);
  docs updated; test `allows_research_free_expansion_units`.
- [x] **T2 (balance)**: remove the settler `research` block in classic + speed presets (3 tribes each).
- [x] **T3 (test)**: `balance.rs` roster test excludes Expansion from the tier-1 count; the 099 settler
  integration test drops the research seed (settler trains with none).
- [x] **T4 — Verify**: live — the settler shows + trains at the Residence/Palace with no research; Academy
  shows it ✓ available; administrators still require research. fmt/clippy/workspace green.
- [ ] **T5 — Reviewer + PR.**
