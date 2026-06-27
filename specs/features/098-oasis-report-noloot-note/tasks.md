# Tasks — 098 oasis report no-loot note

Branch `feature/098-oasis-report-noloot-note`.

- [x] **T1**: `ReportTemplate.oasis_note: Option<String>`; the report handler sets it for an attacker-viewed
  won `OasisAttack`; report.html renders it as a repnote.
- [x] **T2 — Verify**: live (a won oasis report shows the note); integration — the oasis attack/occupy flow
  asserts the report contains the note. fmt/clippy/tests green.
- [ ] **T3 — Reviewer + PR.**
