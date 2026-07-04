# Plan — 124 graceful SIGTERM

**Status:** Draft (spec approved)

`shutdown_signal` (crates/web/src/main.rs) becomes: on unix, `tokio::select!` over
`signal::ctrl_c()` and `signal::unix::signal(SignalKind::terminate())`; else ctrl_c only.
Log which signal fired. Signal-handler behaviour is not unit-testable in-process portably;
verification = build + clippy + a manual `kill -TERM` smoke against the dev server (recorded
in the PR), plus the reviewer's read. Constitution: no P-impact (infrastructure glue).
