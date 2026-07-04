# Tasks — 124 graceful SIGTERM

**Status:** Verified (in-loop acceptance, 2026-07-04). Gates per task: fmt, clippy -D warnings, cargo test --workspace.

- [x] **T1 — Handle SIGTERM.** select over SIGINT+SIGTERM (unix; cfg-gated), log the signal,
  same drain. Manual kill -TERM smoke recorded. (AC1/AC2)
- [x] **T2 — Review & accept.** Reviewer APPROVE; statuses flipped; merged when Verified.
