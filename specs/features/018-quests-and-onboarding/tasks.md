# Feature 018 — Quests & onboarding — Tasks

**Plan:** ./plan.md · **Spec:** ./spec.md

Ordered for dependency and testability (**pure domain first**). Each task is a commit; gates
(`fmt` / `clippy -D warnings` / `test` + P11) pass before advancing. Reuses 017's evaluator/reward
shape; **no new game mechanics** (reads persisted state, grants via existing credit paths).

## Domain & balance

- [x] **T1 — Pure quest rules + balance (`domain/quest.rs`, P3/P7).** `QuestId`, `QuestCondition`,
  `QuestReward`, `QuestDef`, `QuestProgress`, `quest_met`, `current_quest`, `newly_completed` (the gated
  consecutive-met prefix). `quests.toml` (ordered seed chain) + fail-fast `quest_chain()` loader. **Unit
  tests:** `quest_met` truth table; stage-gating (no out-of-order); resumable prefix; tapering (AC1, AC2,
  AC6, AC7); catalogue loads (AC9).

## Persistence

- [x] **T2 — Migration + `QuestRepository` (`infrastructure`).** Migration `0028_quests.sql`
  (`player_quests` PK `(player, quest)`). `QuestRepository`: `completed_quests`, `quest_progress`
  (field/building/garrison/raid/population), `complete_quest` (insert + reward — resources capped to
  capital / culture / troops to capital garrison — in one tx, idempotent). **DB tests:** completion
  once-per-(player,quest); reward applied exactly once (resources/culture/troops); `quest_progress`
  reflects state (AC3, AC4).

## Application

- [x] **T3 — `evaluate_quests` use-case + hook.** Gather completed + progress, complete the gated met
  prefix (idempotent), with an all-completed short-circuit; evaluate on the authenticated village view.
  **DB tests:** each seed quest completes at its action and not before; a resumable prefix completes in
  order in one pass (AC5, AC6); existing village-view behavior unaffected.

## Interface

- [x] **T4 — `/quests` web page.** Authenticated page: current quest (description + reward), completed
  list, all-done state; nav link; `AppState`/main/spawn wire the chain. **Integration test:** the page
  shows the logged-in player's current + completed quests; requires auth (AC8).

## Docs & acceptance

- [x] **T5 — Technical/end-user docs.** rustdoc on new public items; `docs/architecture/0020-quests.md`;
  `docs/manual/` quests & onboarding guide; `CLAUDE.md` active slice → 018.

- [x] **T6 — Review & accept.** Full gates + P11; `eperica-reviewer` on the slice diff; fix until
  **APPROVE**; PR.

## Done when

Per the [Definition of Done](../../implementation-workflow.md#definition-of-done-checklist--applies-to-every-slice):
**AC1–AC9** pass with tests, all gates green, both docs written, reviewer **APPROVE**, PR merged,
`spec.md` / `plan.md` **Verified**, roadmap updated (018 ✅).
