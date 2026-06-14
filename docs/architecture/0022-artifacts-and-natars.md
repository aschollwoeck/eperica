# Artifacts & Natar villages — the end-game's first phase

**Status:** Current
**Date:** 2026-06-14 · **Slice:** 020

## Context
The end-game (GDD §11.3) opens with **artifacts**: at a configured date they are released into **Natar**
(NPC) villages, captured by force, and grant powerful bonuses to whoever holds them. This slice delivers
that phase; the Wonder of the World + victory is slice 021.

## Design
- **Natar villages reuse the combat engine.** A Natar village is an ordinary `villages` row owned by a
  **synthetic NPC user** (`users.is_npc`) and flagged `villages.is_natar`. So attacking one is a normal
  battle — no parallel attack/resolve path. The only new combat step is an **artifact transfer**. NPC/Natar
  rows are excluded from leaderboards, stat pages, and the abandonment sweep by `is_npc = false` filters
  (mirroring the 019 `abandoned_at` filter). A Natar village is **unconquerable** (treated like a capital
  in `conquest_outcome`).
- **Pure artifact model (`domain/artifact.rs`, P3).** `ArtifactKind` (8: Speed, Storage, Sustenance,
  Trainer, Architect, Eyes, Confuser, Fool) × `ArtifactScope` (Small/Large/Unique). Effects are aggregated
  from holdings into `ArtifactEffects` (per-hook multiplicative factors); `aggregate_effects` applies
  **small** to the holding village and **large/unique** account-wide, stacking multiplicatively. **Fool**
  resolves to a concrete kind **deterministically from its id** (`fool_resolved`, P6 — never random at
  read). `can_capture`/`required_treasury_level` gate capture.
- **Release is a one-time, state-driven due check (P1).** The world carries an `artifact_release_at`
  (config offset, GDD §13.2). `process_due_artifact_release` (a scheduler tick) calls the idempotent
  `release_artifacts`: at/after the date, with nothing released yet, it ensures the Natar NPC owner, places
  one Natar village per artifact on the **reserved Natar tiles in seeded ring order** (P6) with a seeded
  garrison + a developed Main Building (so attacking it is a real battle, not a morale-crushed strike on an
  empty village), and inserts each artifact held by its Natar village. Guarded by existing-artifacts, so it
  runs at most once.
- **Capture is defeat-and-claim, Treasury-gated (chosen over conquest).** A **won** attack from a village
  with a qualifying **Treasury** (new `BuildingKind`, level ≥ the artifact's scope requirement, and the
  village holds none yet) transfers the target's artifact to the attacking village — from a Natar vault
  (AC4) or a beaten player holder (AC5, theft). `resolve_attack_one` decides the `ArtifactCapture`;
  `apply_battle` performs the `UPDATE artifacts SET holder_village` **in the battle transaction** (P2/P4),
  exactly once. The Natar village is **not** owned.
- **Effects fold into the reads (the oasis-bonus pattern, AC6).** No stored mutation; each effect injects a
  factor at one existing hook, scoped via `village_effects`/`account_effects`: **Speed** → travel time
  (`order_attack`/`order_reinforcement`), **Storage** → capacity (`load_economy`, re-capping accrued
  amounts), **Sustenance** → garrison upkeep (`load_economy`), **Trainer** → training time (`order_train`),
  **Architect** → catapult/siege durability (`resolve_attack_one`), **Eyes**/**Confuser** → scouting
  power/defence (`resolve_attack_one`). Losing an artifact removes its effect on the next read (P1).

## Persistence (migrations 0031/0032)
- `worlds.artifact_release_at timestamptz` (the schedule).
- `users.is_npc`, `villages.is_natar` (flag the synthetic Natar account/villages).
- `artifacts (id, world_id, kind, scope, magnitude, holder_village → villages ON DELETE SET NULL,
  origin_x/y, released_at)` — one row per released artifact; `holder_village` is the current holder (a
  Natar village at release, a player's village once captured; `SET NULL` ⇒ drops if the holder is deleted).

## Balance (P7)
- `artifacts.toml` — the released set (id, kind, scope, magnitude), the per-scope Treasury level
  requirements, and the Natar garrison spec; `construction.toml`/`economy.toml` carry the Treasury.

## Consequences
- The whole attack/garrison/report machinery is reused for Natar villages — the slice's complexity is
  concentrated in release, capture, and the read-time effect aggregation, not a new combat path.
- Artifact effects apply to the live read and the relevant action paths (movement/training/combat/
  scouting/economy reads); some internal recomputes (e.g. operation-time resource checks) use base values —
  the displayed/settled economy reflects Storage/Sustenance.
- Capture rides the existing won-attack transaction, so it is exactly-once and crash-safe like the rest of
  combat resolution.
