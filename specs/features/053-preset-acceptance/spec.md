# Feature 053 — per-world preset acceptance + ADR finalization

**Status:** Verified
**Depends on:** 047–050, 052 (the per-world end-game schedule + rule-preset machinery + the `speed` preset +
admin selection; 050 absorbed the planned 051 handler sweep).
**Roadmap:** Per-world configuration program — see [ADR 0035](../../../docs/architecture/0035-per-world-configuration.md) §B. The capstone.

**Program note:** Prove the per-world rule machinery end-to-end through the live serving path — the registry —
and flip [ADR 0035](../../../docs/architecture/0035-per-world-configuration.md) to **Accepted**. No new
behaviour: this slice is acceptance + docs.

## Problem

048–052 built and unit/edge-tested each layer (the bundle, the column, the loader, the registry cache, the
`speed` preset, the admin selector). What is not yet asserted in one place is the **whole chain**: that two
running worlds on different presets are served **divergent** rules by the registry — the property the program
exists to deliver. And the ADR still reads "Accepted (in progress)".

## Goal

- **AC1 — Divergent rules served per world (acceptance).** A test builds the live registry, registers a
  `classic` world and a `speed` world **at the same `GameSpeed`** (so the difference is the *preset*, not the
  speed multiplier), and asserts the registry serves each its own bundle: the `speed` world's served rules
  have a **shorter beginner protection** (the ADR example) and **2× unit map speed**, while the `classic`
  world matches the shipped balance. This exercises the full resolve path (`context_for` → `rules_for` →
  `load_world_rules` → preset directory), not just the loader.
- **AC2 — Home parity.** The home world (always `classic`) is served the classic bundle — a `speed` world
  existing alongside does not change it.
- **AC3 — ADR Accepted.** [ADR 0035](../../../docs/architecture/0035-per-world-configuration.md) flips to
  **Accepted**; the roadmap/CLAUDE status notes the per-world configuration program (047–053) complete.

## Design

- **`crates/web/tests/integration.rs`** — one acceptance test
  (`classic_and_speed_worlds_are_served_divergent_rules`): build a `WorldRegistry` over a `#[sqlx::test]`
  pool (as the 050 test does), `ensure_world` (home, classic, 1×), insert a second `speed` world at 1×, and
  assert via `context_for` that the two served bundles diverge (protection, unit speed) while home stays
  classic.
- **Docs** — ADR 0035 Status → Accepted; CLAUDE.md + roadmap note the program complete.

## Out of scope

- A full played HTTP scenario (training/sending troops to time arrivals) — the per-field divergence at the
  serving boundary is the faithful, deterministic acceptance; movement maths is already covered by the domain
  tests. Account-level beginner protection is **not** a per-world signal (it is set once at registration on
  the `users` row), so it is asserted on the *served bundle*, not a joined second-world player.
