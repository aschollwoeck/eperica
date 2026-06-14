# Feature 018 — Quests & onboarding

**Status:** Verified
**Depends on:** 005 (training — the "train troops" quest; the early core loop quests bootstrap), 003 (construction — the "upgrade a field / build the warehouse" quests), 009/011 (combat — the "send a raid" quest), 013 (settling — the capital that receives quest rewards; culture for CP rewards), 002 (resources/population — the population quest + resource rewards), 001 (accounts/world), 017 (the reward/credit + lazy-evaluation patterns this reuses)
**Roadmap:** M6 · slice 018 · GDD §12.1 — the **new-player bootstrap**: a **stage-gated quest chain** walks a fresh player through the core loop (upgrade a field → build the warehouse → train troops → send a raid → …), granting **rewards** (resources, culture, sometimes a few troops) on completion to accelerate the slow early game. Quests are **server-evaluated** from persisted state, completed **once**, and **taper off** — the chain is finite, not a perpetual task list.

## Goal

A new player isn't dropped into an empty village with no direction. A **guided quest chain** walks them
through the core loop one step at a time — *upgrade a resource field, build the warehouse, train your
first troops, send a raid* — and each completed quest grants a **reward** (resources, culture points, or
a small troop count) to soften the slow early game. The chain is **stage-gated** (completing one unlocks
the next), **server-evaluated** (the server detects when a quest's condition is met from persisted state,
P4), completed **exactly once** with its reward, and **resumable/reproducible** (progress is persisted,
P2). After the onboarding chain there are **no more quests** — ongoing play is driven by the game
systems, not a perpetual list (the chain **tapers off**, GDD §12.1).

## Concepts

- **The quest chain (stage-gated).** An **ordered** catalogue of quests (balance data, P7). A player has a
  **current** quest — the first one they haven't completed. A quest can be completed only when it is the
  **current** stage **and** its condition is met; completing it advances the current stage to the next.
  You cannot complete quest *N+1* before *N* (the chain is strict). The catalogue is **finite**; once the
  last quest is completed there is no current quest (tapering).

- **Quest conditions (server-evaluated from persisted state).** Each quest's condition is a **predicate
  over the player's persisted state** — the same authoritative facts the rest of the sim already stores —
  so it is reproducible and resumable (P2/P6). The seed conditions cover the core loop:
  - **Upgrade a resource field** to a target level (max field level across the player's villages).
  - **Build a center building** to a level (e.g. the **Warehouse** ≥ 1).
  - **Train troops** (the player's garrison holds any units).
  - **Send a raid** (the player has launched any attack/raid).
  - **Reach a population** threshold.
  "Detecting the completing event" (GDD §12.1) is realized as evaluating these predicates server-side at
  the natural moment (the player's authenticated view), which is equivalent and naturally resumable.

- **Quest rewards.** On completion a quest grants a one-time **reward** — any of: **resources** (credited
  to the player's **capital**, capped by its stores), **culture points**, and a **small troop count** (a
  unit added to the capital's garrison). The reward is applied **exactly once**, in the same transaction
  as the completion (P2/P4), tied to the once-only completion record.

- **Progress (persisted, resumable).** A player's **completed quests** are persisted. Evaluation is
  idempotent — re-running completes nothing already done and re-applies no reward — and **resumable**: a
  player who already did the actions (e.g. returning after the scheduler was down) completes the
  satisfied prefix of the chain in order on their next evaluation.

## User stories

- As a **new player**, I want a **guided chain of tasks** that teaches the core loop, so I'm not lost in
  an empty village.
- As a **new player**, I want **rewards** for completing quests, so the slow early game is less punishing.
- As a **player**, I want quests to **stop** once I've learned the game, so I'm not nagged forever.
- As an **administrator**, I want the **quest chain and reward values** to be **config**, so onboarding
  can be tuned without code.

## Acceptance criteria

> Quests are **server-authoritative** (P4) and **reproducible** from persisted state (P2/P6): the client
> cannot self-complete or self-reward. Completion + reward are **exactly once** (a persisted completion
> record). The chain, conditions, and rewards are **config** (P7).

- **AC1 — The chain is stage-gated.** Quests form an ordered chain. A quest is completable only when it is
  the player's **current** stage (every earlier quest already completed) **and** its condition is met.
  Completing the current quest advances the current stage to the next. Quest *N+1* can never complete
  before *N*.

- **AC2 — Conditions are evaluated server-side from persisted state.** A quest completes when its
  predicate over the player's persisted state holds (max field level, a building's level, garrison has
  troops, an attack/raid was launched, population). Evaluation is server-side (P4) and reproducible
  (P2/P6); the same persisted state yields the same completions.

- **AC3 — Completion is once-only and persisted.** A completed quest is recorded once per (player,
  quest); re-evaluation never completes it again. Progress survives restarts (resumable).

- **AC4 — Rewards applied exactly once, in the completion transaction.** Completing a quest applies its
  reward — resources to the capital (capped at storage), culture points, and/or a troop count to the
  capital's garrison — **in the same transaction** as the completion record, so it is applied **exactly
  once** (never on re-evaluation).

- **AC5 — The seed quests complete at their action.** Given the seed chain, each quest completes the first
  time its action is done and **not before**: upgrading a field to the target level, building the
  warehouse, training any troops, launching a raid, and reaching the population threshold — in chain
  order.

- **AC6 — Resumable prefix completion.** A player whose persisted state already satisfies several
  consecutive quests (e.g. evaluated late) completes that **prefix in order** in one evaluation, each
  with its reward, stopping at the first unmet quest (the stage gate).

- **AC7 — Tapering.** The chain is finite. Once the last quest is completed, the player has **no current
  quest** and no further quests are offered (no perpetual task list).

- **AC8 — Quests interface.** A player sees their **current quest** (its description + reward) and their
  **completed** quests; when the chain is done, a "complete" state. Quests are the player's **own** (an
  authenticated view); there is no public quest page.

- **AC9 — Authority, determinism & config (P2/P4/P6/P7).** Every completion + reward is produced
  server-side, exactly once, from persisted state; recomputing over the same state yields the same
  completions. The quest chain, conditions, and reward values are **config**.

## Roles & permissions

Per [roles.md](../../roles.md). Quests are a **player's own** onboarding state, granted by the **System**;
no public surface.

| Role | Permitted | Denied (server-enforced) |
|------|-----------|--------------------------|
| **Visitor** | N/A (considered) — quests require an account; a visitor has no quest state. | View/complete quests (redirected to login). |
| **Player** | View **their own** current + completed quests; **earn** completions + rewards by doing the core-loop actions. | **Self-completing** a quest or granting its reward; completing out of chain order; seeing or affecting another player's quests. |
| **Moderator** | N/A (considered) — no quest moderation surface in this slice. | — |
| **Administrator** | Configure (balance, P7) the **quest chain** (order, conditions, reward values). | Setting quest state per-request from the client. |
| **System** | *(system-initiated)* Detect a quest's condition from persisted state and **complete** it + apply its reward, **exactly once**, in chain order (P2/P4). | — |

## Out of scope

- **Quest presentation polish** — icons, a guided pointer/overlay, celebratory UI, and quest
  *notifications* are **app-layer** ([social-and-meta-features.md](../../social-and-meta-features.md));
  018 delivers the chain + completions + rewards + a functional quest view.
- **Daily/repeatable quests, quest lines beyond onboarding, achievements** — 018 is the **finite
  onboarding chain** only (achievements are 017; ongoing goals are the game systems themselves).
- **New game mechanics** — 018 only **reads** persisted state to detect completion and **grants** rewards
  via existing credit paths (resources/culture/garrison); it changes no economy/combat/build rule.
- **Beginner's protection & inactivity** (GDD §12.2–12.3) — slice **019**.

## Decisions

- **Reuse the 017 evaluator/reward shape.** Quests mirror achievements: a **pure catalogue** (predicates
  over a player-state value) + an **idempotent evaluator** + a **once-only completion record** with the
  reward applied in the grant transaction. The **difference** is **stage-gating** (the chain is ordered;
  only the current stage is completable) and a **troop** reward component (a unit added to the capital's
  garrison via the existing `village_units` upsert).
- **Conditions are state-predicates, evaluated lazily on the authenticated view (P1/P4).** Rather than
  threading detection into every processor, the player's quests are evaluated when they load their
  authenticated village page — server-authoritative, idempotent, resumable, and covering every condition
  without touching the hot resolution paths. (Same realization as 017's achievement hook.)
- **Stage gate is a pure chain walk.** Given the ordered catalogue, the completed set, and the player's
  state, the evaluator completes the **consecutive prefix** of not-yet-completed quests whose conditions
  are met, **stopping at the first unmet** — so order is enforced and a resumable prefix completes in one
  pass.
- **New persistence.** `player_quests (player_id, quest_id, completed_at, PRIMARY KEY (player_id,
  quest_id))` — the PK is the exactly-once guard. The **current** quest is derived (first catalogue entry
  not in the player's completed set). No per-quest "in progress" row.
- **Balance (P7).** `quests.toml` — the ordered chain: each entry's id, a condition (kind + parameter),
  and an optional reward (resources / culture / troops). Loaded fail-fast like `achievements.toml`.
- **"Send a raid" is detected from `battle_reports` as attacker** (a launched attack/raid that resolved),
  the persisted, reproducible fact; field/building levels from `village_fields`/`village_buildings`;
  troops from the garrison; population from `domain::population`.

## Open questions

- **Troop-reward unit.** Which unit does a troop reward grant — a fixed unit id in the quest's balance, or
  the player's tribe tier-1? **Proposed:** a **fixed unit id in balance** per quest (simple, explicit);
  the seed chain uses the relevant tribe-agnostic early unit or omits troop rewards where a fixed id
  doesn't fit. *(If a tribe-relative reward is wanted, revisit.)*
- **"Send a raid" before resolution.** Should the raid quest complete when the raid is **launched** or
  when it **resolves**? **Proposed:** on **resolve** (a `battle_reports` row as attacker) — the persisted,
  reproducible signal; an in-flight raid completes the quest a moment later when it lands. (Onboarding
  raids are short.)
- **Reward when the capital is missing.** A brand-new player's first village may not be flagged capital.
  **Proposed:** credit the **capital, else the oldest village** (same rule as 017's resource reward), so
  rewards always land.
- **Re-deriving "current" after a catalogue change.** If the chain is extended mid-world, existing players
  resume at the first not-completed quest (which may be a newly-added one). **Proposed:** **yes** — the
  current stage is always derived from the completed set against the current catalogue; new quests append
  to the chain for everyone.
