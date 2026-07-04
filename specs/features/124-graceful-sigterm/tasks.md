# Tasks — 124 graceful SIGTERM

**Status:** Draft. Gates per task: fmt, clippy -D warnings, cargo test --workspace.

- [ ] **T1 — Handle SIGTERM.** select over SIGINT+SIGTERM (unix; cfg-gated), log the signal,
  same drain. Manual kill -TERM smoke recorded. (AC1/AC2)
- [ ] **T2 — Review & accept.** Reviewer APPROVE; statuses flipped; merged when Verified.
