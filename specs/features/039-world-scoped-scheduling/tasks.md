# Feature 039 — World-scoped due processing — Tasks

**Plan:** ./plan.md · **Spec:** ./spec.md

Each task a commit; gates (`fmt` / `clippy -D warnings` / `test` + P11) pass before advancing. Additive,
behaviour-preserving — the existing suite (esp. the web scheduler flows) is the regression oracle. No
pure-domain task.

## Infrastructure

- [x] **T1 — World-scope the requeue_orphaned_* (6).** builds / unit orders / training / starvation
  (`village_id`); movements / trades (`home_village`) filter to `self.world_id`. (AC2)
- [x] **T2 — World-scope the due-claims (11).** build / unit / training / starvation (`village_id`); the six
  `troop_movements` kinds + trade (`home_village`) filter to `self.world_id`; bind `$N`. (AC1)

## Acceptance

- [x] **T3 — Cross-world isolation DB test.** `due_claims_are_world_scoped`: a due build in world B is
  neither claimed nor requeued by a repo scoped to world A; world A still claims/requeues its own. (AC3)
- [x] **T4 — Regression.** Full workspace suite passes **unchanged** — the web flows
  (build/train/combat/trade/move/scout/settle) exercise every claim end-to-end; the 023
  throughput/determinism guards hold. (AC4). Spec/plan/tasks + roadmap/ADR cross-refs.
