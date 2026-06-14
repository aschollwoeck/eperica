# Feature 018 — Quests & onboarding — Technical Plan

**Status:** Reviewed
**Spec:** ./spec.md

The new-player bootstrap: a **stage-gated quest chain** evaluated server-side from persisted state, with
once-only completions + rewards (resources/culture/troops). It **reuses 017's shape** — a pure catalogue
of predicates, an idempotent evaluator at the authenticated-view hook, and a reward applied in the
completion transaction — adding **chain gating** (only the current stage is completable) and a **troop**
reward (a unit added to the capital garrison via the existing `village_units` upsert). **No new game
mechanics** — reads persisted state, grants via existing credit paths.

## Constitution check

- **P1 (event-driven / lazy):** no quest tick. Quests are evaluated lazily when the player loads their
  authenticated village page — a few indexed reads for one player. No per-entity work.
- **P2 / P6 (reproducible):** completions are unique per `(player, quest)`; conditions are predicates over
  persisted state (field/building levels, garrison, battle rows, population) with no randomness, so the
  same state yields the same completions. The reward is applied in the completion transaction, so it is
  exactly-once.
- **P3 (pure domain):** `domain/quest.rs` — `QuestCondition`, `QuestReward`, `QuestDef`, `QuestProgress`,
  `quest_met`, `current_quest`, and the chain walk `newly_completed` (the consecutive met prefix from the
  current stage). All unit-tested without I/O.
- **P4 (server authority):** completions/rewards are produced only by the System at the hook; no client
  path self-completes. The chain gate is enforced in the pure walk + the persisted completed set.
- **P7 (configurable):** `quests.toml` is the ordered chain (id, condition, reward, description); loaded
  fail-fast.

## Domain (`domain/quest.rs`, pure)

- `struct QuestId(String)`.
- `enum QuestCondition { FieldLevel(u8), BuildingLevel(BuildingKind, u8), TrainTroops, SendRaid,
  Population(i64) }`.
- `struct QuestReward { resources: ResourceAmounts, culture: i64, troops: Option<(UnitId, u32)> }`.
- `struct QuestDef { id: QuestId, description: String, condition: QuestCondition, reward: QuestReward }`.
- `struct QuestProgress { max_field_level: u8, building_levels: HashMap<BuildingKind, u8>, has_troops:
  bool, has_raided: bool, population: i64 }` — the player's persisted facts.
- `fn quest_met(c: &QuestCondition, p: &QuestProgress) -> bool`.
- `fn current_quest<'a>(chain: &'a [QuestDef], completed: &HashSet<QuestId>) -> Option<&'a QuestDef>` —
  first not-completed.
- `fn newly_completed<'a>(chain, completed, progress) -> Vec<&'a QuestDef>` — walk the chain in order:
  skip completed, push consecutive met quests, **stop at the first unmet** (the gate).
  Unit-tested: gating (can't skip), resumable prefix, tapering (all completed → none / no current).

## Balance (`specs/balance/quests.toml` + `infrastructure::balance`)

- `quests.toml` — an ordered `[[quests]]` array: `id`, `description`, a condition (`kind` +
  parameters: `level`/`building`/`population`), and an optional `reward` (`wood/clay/iron/crop`,
  `culture`, `troop_unit` + `troop_count`). Seed chain: upgrade a field → build warehouse → train troops
  → send a raid → reach population, with modest resource/culture/troop rewards.
- `quest_chain() -> Vec<QuestDef>` loader (mirrors `achievement_catalogue()`), fail-fast on unknown
  condition kind / building.

## Persistence (`infrastructure` + migration `0028_quests.sql`)

- `player_quests (player_id uuid, quest_id text, completed_at timestamptz default now(), PRIMARY KEY
  (player_id, quest_id))` — the PK is the exactly-once guard. The **current** quest is derived (no
  in-progress row).
- `QuestRepository` (port + `PgAccountRepository` impl): `completed_quests(player) -> HashSet<QuestId>`;
  `quest_progress(econ, player) -> QuestProgress` (max field level; per-building max level map; garrison
  non-empty; any `battle_reports` as attacker; population via the 016 population SQL); `complete_quest(
  econ, player, def) -> bool` (insert the completion `ON CONFLICT DO NOTHING`; if newly inserted, apply
  the reward — resources to the capital capped via `deposit_capped`, culture `+=`, troops upserted into
  the capital's `village_units` — all in one transaction; return whether newly completed). Capital =
  `ORDER BY is_capital DESC, created_at ASC LIMIT 1` (the 017 rule).

## Application (`application/quest.rs`)

- `evaluate_quests(repo, econ, chain, player) -> Result<Vec<QuestId>, RepoError>` (where `R:
  QuestRepository + AccountRepository`): load `completed_quests`; short-circuit if all chain ids are
  completed (cheap steady state, P11); else gather `quest_progress` and, for each `def` in
  `newly_completed(chain, completed, progress)`, `complete_quest` (idempotent) — returning the ids
  completed this call. The chain walk guarantees order + gating.

## Interface (`web`)

- The authenticated **village** handler evaluates the player's quests on load (the hook), best-effort.
- A **`/quests`** page (authenticated): the **current** quest (description + reward), the **completed**
  list, and a "all done" state when the chain is finished. A nav link from the village page.
- `AppState` gains `quest_chain: Arc<Vec<QuestDef>>`; `main` + the integration `spawn` wire it.

## Test strategy

| AC | Test |
|----|------|
| AC1 | domain: `newly_completed` enforces order — a later quest's met condition does not complete it while an earlier is unmet. |
| AC2 | domain: `quest_met` truth table per condition; infra: `quest_progress` reflects field/building/garrison/raid/population state. |
| AC3 | infra (DB): a completion is once-per-(player,quest); re-eval completes nothing. |
| AC4 | infra (DB): completing a quest credits the capital resources (capped) / culture / troops exactly once. |
| AC5 | infra/app (DB): each seed quest completes at its action (upgrade field, build warehouse, train, raid, population) and not before. |
| AC6 | infra/app (DB): a player whose state satisfies the first N quests completes the prefix in order in one `evaluate_quests`. |
| AC7 | domain: `current_quest` is `None` when all completed; `newly_completed` returns none. |
| AC8 | web: the `/quests` page shows the current quest + completed for the logged-in player; requires auth. |
| AC9 | infra: determinism (same state → same completions); config drives the chain. |

## Notes / open risks

- **Stage-gate correctness is the crux** — the chain walk must stop at the first unmet quest, never
  complete out of order. Covered by a focused domain test (gating + resumable prefix).
- **Troop reward** is the one new credit path — a `village_units` upsert on the capital (the pattern
  already used by training completion). Verify the reward applies in the completion transaction (AC4).
- **Hook reuse.** The village-view evaluation mirrors 017's achievement hook; keep both cheap (the
  all-completed short-circuit). Offline players complete quests on next login (acceptable; onboarding is
  player-driven).
- **Phasing (T1–T6):** T1 pure domain (`quest.rs`) + `quests.toml` + loader; T2 migration 0028 +
  `QuestRepository` (progress/completed/complete+reward) with DB tests; T3 `evaluate_quests` use-case +
  the village-view hook; T4 the `/quests` web page + nav + integration test; T5 docs; T6 reviewer.
