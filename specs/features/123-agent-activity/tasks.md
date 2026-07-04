# Tasks — 123 agent activity

**Status:** Verified (in-loop acceptance, 2026-07-04). Gates per task: fmt, clippy -D warnings, cargo test --workspace.

- [x] **T1 — Touch + tests.** `bearer_account` touches `last_activity` (throttled port,
  log-only on failure); integration tests per AC1/AC2 (+ failed-auth negative). (AC1–AC4)
- [x] **T2 — Review & accept.** Reviewer APPROVE; statuses flipped; PR merged when Verified.
