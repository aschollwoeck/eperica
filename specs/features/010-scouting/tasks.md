# Feature 010 — Scouting — Tasks

**Plan:** ./plan.md · **Spec:** ./spec.md

Ordered for dependency and testability (pure domain first). Each task is a commit; gates
(`fmt`/`clippy -D warnings`/`test` + P11) pass before advancing.

## Domain (pure, test-first)

- [x] **T1 — Scouting domain.** `MovementKind::Scout`; `UnitSpec.scouting: u32`; new `scouting.rs`:
  `ScoutTarget`, `ScoutRules`, `scouting_power` (Scout-role only, no Smithy scaling), `resolve_scouting`
  → `ScoutOutcome { attacker_loss_frac, detected }` (zero counter ⇒ no loss/undetected; ≥ attacker ⇒
  all lost/detected; else power-law; defenders never lost). Unit tests: the four cases + monotonicity +
  determinism without a seed (**AC4**, **AC5**). Backfill every `UnitSpec` literal with `scouting`.

## Balance & persistence

- [x] **T2 — Balance.** `units.toml`: `scouting = N` on the three Scout units (loader defaults 0
  elsewhere). `combat.toml`: `[scouting] loss_exponent` + `scout_rules()` loader.
- [x] **T3 — Migration + scout repository.** `0013_scouting.sql`: widen `troop_movements.kind` CHECK to
  add `scout` + `scout_target text NULL`; `battle_reports` gains `scouted bool` + `scout_target`; new
  `scout_reports` table + indexes. `ScoutRepository` (`start_scout`, `claim_due_scouts`, single-tx
  `apply_scout`, `scout_reports_for`/`scout_report` with scouter/target redaction); `DueAttack.scout_target`
  + `BattleApply` scout fields wired into `apply_battle`; narrow `claim_due_movements` to exclude `scout`.
  DB tests: standalone resolve writes the report + survivor return once; crash-resume; scouter reads
  intel, target reads only the notification (and nothing when undetected), third party nothing
  (**AC8**, **AC9**, **AC10**, **AC11**).

## Application

- [x] **T4 — Scout use-cases.** `ScoutError`; `order_scout` (validate own village → scouts-only →
  garrison → target → travel → debit/schedule → starvation re-sync); `process_due_scouts` (gather power
  → `resolve_scouting` → apply losses → gather intel if a scout survives → survivor return → report).
  Shared `gather_intel` helper (Resources accrues via 002; Defenses merges troops + Wall). Fake tests:
  send success + every rejection incl. `NotAllScouts`; resolution wires the domain outcome + intel to the
  repo apply (**AC1**, **AC3**, **AC6**).
- [x] **T5 — Combined scouting in `order_attack`/`process_due_combat`.** `order_attack` accepts
  `scout_target` (defaults to `Defenses` when scouts present); `resolve_one` runs the espionage sub-step
  **first** (split scouts/non-scouts, espionage losses, detection), then the unchanged 009 battle, then
  carries espionage-surviving scouts through the attacker loss fraction; intel delivered iff a scout
  returns; writes the scouter `scout_report` + the defender battle report's `scouted` flag — all in the
  existing one tx. Tests: espionage doesn't change the battle; wiped attacker ⇒ no intel; survivor ⇒
  intel; defender flagged (**AC2**, **AC6**, **AC7**, **AC8**).
- [x] **T6 — Scheduler.** Tick `process_due_scouts`; startup orphan requeue (shared). DB test via the
  processor (**AC10** restart path).

## Web

- [x] **T7 — Scout send + intel reports.** Rally Point **mode** gains `scout` (scout units + target
  type) → `order_scout`; attack/raid mode shows a scout-target dropdown when scouts are present (default
  Defenses) → `order_attack`. `GET /reports` merges battle + scout reports; `GET /reports/scout/{id}`
  detail (scouter intel vs target notification, redacted server-side); the 009 battle-report detail shows
  the `scouted` marker for the defender. Integration tests (**AC12**).

## Documentation & acceptance

- [x] **T8 — Technical docs.** rustdoc; `docs/architecture/00NN-scouting.md` (espionage sub-step,
  seedless determinism, intel snapshot, report redaction); `CLAUDE.md` active slice → 010.
- [x] **T9 — End-user docs.** `docs/manual/` scouting guide (standalone vs scouts-with-attack,
  Resources vs Defenses, stealth); link from index.
- [x] **T10 — Review & accept.** Full gates + P11; `eperica-reviewer` on the slice diff; fix until
  **APPROVE**; PR.

## Done when

Per the [Definition of Done](../../implementation-workflow.md#definition-of-done-checklist--applies-to-every-slice):
**AC1–AC12** pass with tests, all gates green, both docs written, reviewer **APPROVE**, PR merged,
`spec.md`/`plan.md` **Verified**, roadmap updated.
