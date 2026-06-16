# Feature 043 — Request world-context — Tasks

**Plan:** ./plan.md · **Spec:** ./spec.md

Serial; each task a commit; gates (`fmt` / `clippy -D warnings` / `test` + P11) pass. Behaviour-preserving —
the existing suite is the regression oracle. No pure-domain task.

## Registry

- [x] **T1 — Registry world-context.** `WorldRegistry` caches `WorldMeta {seed, radius, speed}` per running
  world (in `start_world`) + the loaded `MapRules`; `context_for(world_id) -> Option<(PgAccountRepository,
  Arc<WorldMap>, GameSpeed, u32)>`. (AC1)

## Web — extractor + cookie

- [x] **T2 — `world` cookie + `GameContext` extractor.** Encrypted `world` cookie + helpers; the extractor
  resolves selected world (default home) → effective account → `player_in_world` (home fallback if not
  joined / world not running); yields `{accounts, map, player, world_id, speed, radius}`. (AC2)

## Web — proof migration

- [x] **T3 — Village handler on `GameContext`.** `/village` uses `GameContext` (rules/hubs still from
  `State<AppState>`). Existing village tests pass (home parity). (AC3/AC5)

## Acceptance

- [x] **T4 — Multi-world view + regression.** Integration: a player joined to a 2nd world + the `world`
  cookie set → `/village` renders that world's village (AC4). Full suite green (home behaviour preserved).
  Spec/plan/tasks. (AC4/AC5)
