# Feature 049 — `worlds.rule_preset` + name-aware rule loader

**Status:** Verified
**Depends on:** 048 (`WorldRules` bundle + `load_world_rules`).
**Roadmap:** Per-world configuration program — see [ADR 0035](../../../docs/architecture/0035-per-world-configuration.md) §B.
**Program note:** Give each world a **named rule preset** it plays under, and make the bundle loader load by
name. Behaviour-preserving — only the `classic` preset exists (= today's balance), the column defaults to it,
and nothing yet reads the column at runtime (050 wires the registry to it). No domain change (P3).

## Problem

A world has no rule identity: every world implicitly uses the one global `classic` bundle. Before the
registry can serve a per-world bundle (050) or an admin can pick one (052), a world must **carry** a preset
name, and the loader must take that name.

## Goal

- **AC1 — `worlds.rule_preset` column.** A migration adds `rule_preset text NOT NULL DEFAULT 'classic'`;
  every existing/boot/admin world is `classic` (the default). The `World` row gains `rule_preset`.
- **AC2 — Name-aware loader.** `load_world_rules(preset: &str) -> Result<WorldRules, BalanceError>` loads the
  named preset (only `classic` is known today; an unknown name is a clear error). A `known_preset(name)`
  helper + a `KNOWN_PRESETS` list expose the valid set (for 052's form + validation). Existing callers pass
  `"classic"`.
- **AC3 — Behaviour preserved.** `classic` is exactly today's balance, so every world behaves identically;
  the full suite passes. The registry/handlers still use the single global `classic` bundle (050 changes
  that).

## Design

- **Migration `0046_world_rule_preset.sql`** — `ALTER TABLE worlds ADD COLUMN rule_preset text NOT NULL
  DEFAULT 'classic';`. (Touch `db.rs` so `sqlx::migrate!` re-embeds.)
- **`world.rs`** — `World.rule_preset: String`; the `SELECT` columns include it; `create_world`/
  `ensure_world*` leave the column to its default (no new param — 052 sets it from the form).
- **`world_rules.rs`** — `load_world_rules(preset)` matches the name (`"classic"` → assemble today's bundle;
  else `BalanceError`); `pub const KNOWN_PRESETS: &[&str] = &["classic"]` + `known_preset(name)`. The no-arg
  convenience is dropped in favour of the explicit name.
- **Callers** (`main.rs`, perf, the test harness) pass `"classic"`.

## Out of scope

- The **registry serving the world's preset** + the request context carrying the bundle + the scheduler
  using the world's bundle (050). The **admin preset selector** + a real 2nd preset and its balance overlay
  (052) — including the overlay-vs-full-directory mechanism decision, made when the first non-`classic`
  preset is authored.
