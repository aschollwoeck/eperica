# Plan — 054 real-time inactivity & abandonment

## Tasks

- **T1 — Domain.** `lifecycle.rs`: drop `speed` from `is_inactive` + `abandon_cutoff`; use the `_secs`
  directly (× 1000 → ms). `protection_expiry` unchanged. Update module + field doc comments
  ("speed-scaled" → "real-time" for inactivity/abandon). Update the two affected unit tests + add one
  asserting world speed does **not** change inactivity/abandon. Drop the now-unused `scaled_time_secs` import
  if no longer used (it still is — by `protection_expiry`).
- **T2 — Application + callers.** `application/lifecycle.rs`: `process_due_lifecycle` drops `speed`; its
  `abandon_cutoff` call drops `speed`. Update callers: `event_store.rs` scheduler, `handlers.rs` map greying,
  the repo tests. Re-label `lifecycle.toml` comments (both presets) as real-time.
- **T3 — Gate + reviewer.** `fmt`/`clippy -D warnings`/`test --workspace`/P11; reviewer → APPROVE.

## Gates

`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
`cargo test --workspace`, P11.
