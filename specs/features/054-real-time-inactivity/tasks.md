# Tasks — 054 real-time inactivity & abandonment

- [ ] **T1** — Domain `lifecycle.rs`: `is_inactive`/`abandon_cutoff` drop `speed`, use real-time; docs + tests
  updated; new test asserts speed-independence. `protection_expiry` unchanged.
- [ ] **T2** — `process_due_lifecycle` drops `speed`; callers (scheduler, map greying, repo tests) updated;
  `lifecycle.toml` comments (classic + speed) re-labelled real-time.
- [ ] **T3** — Full gate + reviewer → APPROVE.

Gates: `fmt --check`, `clippy -D warnings`, `cargo test --workspace`, P11.
