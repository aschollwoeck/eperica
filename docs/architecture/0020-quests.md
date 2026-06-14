# Quests & onboarding — the new-player bootstrap

**Status:** Current
**Date:** 2026-06-14 · **Slice:** 018

## Context
A new player should not be dropped into an empty village with no direction (GDD §12.1). A
**stage-gated quest chain** walks a fresh player through the core loop one step at a time — *upgrade a
field → build the warehouse → train troops → send a raid → grow* — and each completed quest grants a
**reward** (resources, culture, sometimes a few troops) to soften the slow early game. The chain is
**finite**: after onboarding there are no more quests (it **tapers off**), and ongoing play is driven
by the game systems, not a perpetual task list.

## Design
This slice deliberately **reuses the 017 shape** — a pure catalogue of predicates, an idempotent
evaluator, a once-only completion record, and the reward applied in the completion transaction — and
adds two things: **stage-gating** (the chain is ordered; only the current stage is completable) and a
**troop** reward component.

- **The stage gate is a pure chain walk (`domain/quest.rs`, P3).** Given the ordered catalogue, the
  player's completed set, and their persisted progress, `newly_completed` walks the chain in order,
  skips already-completed quests, and collects the **consecutive prefix** of not-yet-completed quests
  whose conditions are met — **stopping at the first unmet quest**. So order is strictly enforced
  (quest *N+1* can never complete before *N*), and a player whose state already satisfies several
  consecutive quests (evaluated late) completes that prefix **in one pass** (AC1, AC6). The **current**
  quest is derived (`current_quest`): the first catalogue entry not in the completed set — there is no
  per-quest "in progress" row.
- **Conditions are predicates over persisted state (P2/P4/P6).** Each `QuestCondition` reads the same
  authoritative facts the rest of the sim already stores: max field level
  (`village_fields`), a center building's level (`village_buildings`), garrison-has-troops
  (`village_units`), a launched raid (a `battle_reports` row **as attacker** — the persisted,
  reproducible signal; an in-flight raid completes the quest a moment later when it lands), and total
  population (`domain::population`). The same state always yields the same completions (AC2, AC9).
- **Completion + reward are exactly-once, in one transaction (P2/P4).** `QuestRepository::complete_quest`
  inserts the `player_quests` row (PK `(player, quest)` = the exactly-once guard) **and** applies the
  reward in the **same transaction**: culture points added, resources credited to the player's capital
  capped at its stores (`deposit_capped`), and a troop count upserted into the capital's garrison. A
  re-evaluation completes nothing already done and re-applies no reward (AC3, AC4).
- **Shared reward credit.** The capital-credit logic (resources capped / culture / troops to garrison,
  crediting the **capital else the oldest village** so rewards always land) is factored into a single
  `credit_reward` helper in the repository, **reused by both** 017's `grant_achievement` and 018's
  `complete_quest`.
- **Hook: lazy on the authenticated village view (P1).** Like 017, the evaluator is not threaded
  through every processor; `evaluate_quests` runs when the logged-in player loads their village (and
  the `/quests` page) — server-authoritative, idempotent, resumable, covering every condition within
  one view of crossing it, and leaving the hot combat/settle paths untouched. A cheap early-out skips
  the progress gather once the (finite) chain is fully completed.
- **Interface.** The authenticated `/quests` page shows the player's **current** quest (description +
  reward), their **completed** quests in chain order, and the **all-done** state once the chain tapers
  off. Quests are the player's **own** — there is no public quest page (AC7, AC8).

## Persistence (migration 0028)
- `player_quests (player_id, quest_id, completed_at)` — PK `(player_id, quest_id)`, the exactly-once
  guard. The **current** quest is derived; there is no in-progress row.

## Balance (P7)
- `quests.toml` — the **ordered** chain: each entry's id, a description, a condition (`kind` +
  parameter), and an optional reward (any of resources / culture / a troop unit + count). Loaded
  fail-fast like `achievements.toml`. The current stage is always re-derived from the completed set
  against the current catalogue, so a chain **extended mid-world** simply appends new quests for
  everyone.

## Consequences
- A player who did the actions while offline (or while the scheduler was down) completes the satisfied
  prefix of the chain — with rewards — the next time they load their village; still
  server-authoritative and exactly-once, just next-view rather than instant.
- Evaluation is O(chain length) per view with a constant set of small reads, short-circuited to nothing
  once the chain is done — negligible.
- 018 adds **no new game mechanics**: it only reads persisted state to detect completion and grants
  rewards via the existing credit paths (resources/culture/garrison).
