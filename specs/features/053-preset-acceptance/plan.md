# Plan — 053 preset acceptance + ADR finalization

## Tasks

- **T1 — Acceptance test.** `classic_and_speed_worlds_are_served_divergent_rules` in
  `crates/web/tests/integration.rs`: build a `WorldRegistry` over the test pool, register a classic home
  world (1×) + a speed world (1×), and assert `context_for` serves each its own bundle — speed has shorter
  beginner protection and 2× unit map speed; classic matches the shipped balance; home stays classic (AC1/AC2).
- **T2 — Docs.** ADR 0035 Status → Accepted; CLAUDE.md + roadmap note the per-world configuration program
  (047–053) complete (AC3). Full gate + reviewer.

## Gates

`fmt --check`, `clippy -D warnings`, `cargo test --workspace`, P11. Docs-only edits in T2.
