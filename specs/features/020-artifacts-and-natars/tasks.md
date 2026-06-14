# Feature 020 — Artifacts & Natar villages — Tasks

**Plan:** ./plan.md · **Spec:** ./spec.md

Ordered for dependency and testability (**pure domain first**). Each task is a commit; gates
(`fmt` / `clippy -D warnings` / `test` + P11) pass before advancing. Natar villages reuse the combat
engine as **NPC-owned `villages`**; the only new combat step is an **artifact transfer** on a won attack.

## Domain & balance

- [x] **T1 — Artifact model + effects + Treasury (`domain/artifact.rs`, `building.rs`; P3/P7).**
  `ArtifactKind` (8), `ArtifactScope` (small/large/unique), `ArtifactDef`, `ArtifactEffects` (per-hook
  multipliers); `aggregate_effects` (by scope, stacking), `fool_resolved` (seeded determinism),
  `required_treasury_level`, `can_capture`. `BuildingKind::Treasury` + its population/construction balance.
  `artifacts.toml` (released set + magnitudes + treasury-level reqs + Natar garrison) and
  `artifact_catalogue()` loader. **Unit tests:** aggregation by scope + stacking; Fool determinism; capture
  truth table; required level; identity with no holdings; catalogue loads (AC3, AC6, AC7).

## World schedule

- [x] **T2 — Release-date schedule (`worlds` + `World` + config).** Migration: `worlds.artifact_release_at`.
  `World` carries it; `ensure_world` persists it from config (an offset/date, env-tunable). **DB test:**
  `ensure_world` round-trips the release date (AC1, AC7).

## Persistence & release

- [x] **T3 — Natar materialization + artifact persistence (`infrastructure`).** Migrations: `users.is_npc`,
  `villages.is_natar`, `artifacts` table. `ArtifactRepository`: `release_artifacts` (idempotent one-shot —
  ensure the NPC user, place N Natar villages on reserved Natar tiles in seeded ring order, seed garrisons,
  insert artifacts held by their Natar village), `artifact_at_village`, `held_by_player`,
  effect-input reads. `process_due_artifact_release` use-case. **DB tests:** nothing before the date;
  release materializes the set once (Natar villages + `is_natar` + NPC owner + seeded garrison + artifacts);
  re-run no-op (AC1, AC2, AC7).

## Capture

- [x] **T4 — Capture & theft in the battle path (`application` + `infrastructure`).** `resolve_attack_one`
  sets an `ArtifactCapture` on `BattleApply` when the attacker won, the target holds an artifact, and the
  attacker's home Treasury qualifies (one per village); `apply_battle` transfers it in the battle tx.
  Conquest guard: Natar villages never transfer ownership. **DB tests:** winning attack from a Treasury vs
  a Natar artifact ⇒ moved; vs a player holder ⇒ stolen; no/low Treasury or already-holding ⇒ no transfer;
  Natar village not conquerable (AC3, AC4, AC5).

## Effects

- [x] **T5 — Effect wiring across the read hooks.** Fold `ArtifactEffects` (village small + account
  large/unique) into: troop **Speed** (travel time), **Storage** (capacity), **Sustenance** (upkeep),
  **Trainer** (training time), **Architect** (siege durability), **Eyes**/**Confuser** (scouting). **Tests:**
  domain aggregation (T1) + representative infra/app: a held Speed artifact shortens travel and a Storage
  artifact raises capacity; losing it reverts; scope (small vs account) respected (AC6).

## Interface

- [x] **T6 — Web: release tick, map, holdings panel, exclusions.** Scheduler `process_due_artifact_release`
  tick; `AppState`/main wire catalogue + release date. Map renders Natar villages; village view shows held
  artifacts. Boards/stat pages + the abandonment sweep exclude `is_npc`/`is_natar`. **Integration tests:**
  the holdings panel shows a captured artifact; Natar villages on the map; boards exclude Natar (AC2, AC8).

## Docs & acceptance

- [x] **T7 — Technical/end-user docs + review.** rustdoc on new public items;
  `docs/architecture/0022-artifacts-and-natars.md`; `docs/manual/` artifacts guide; `CLAUDE.md` active
  slice → 020. Full gates + P11; `eperica-reviewer` on the slice diff; fix until **APPROVE**; PR.

## Done when

Per the [Definition of Done](../../implementation-workflow.md#definition-of-done-checklist--applies-to-every-slice):
**AC1–AC8** pass with tests, all gates green, both docs written, reviewer **APPROVE**, PR merged,
`spec.md` / `plan.md` **Verified**, roadmap updated (020 ✅).
