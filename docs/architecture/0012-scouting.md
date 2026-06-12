# Scouting — seedless espionage and intel reports on the combat rails

**Status:** Current
**Date:** 2026-06-12 · **Slice:** 010

## Context
Scouting is information warfare (GDD §6.1, §9.4 step 1): reveal a village's hidden resources or
defenses (GDD §7.3). Slice 010 adds two ways to spy — a **standalone scout mission** (scouts only, no
battle) and **scouts riding an attack/raid** (the village is scouted *in addition* to being fought).
Both resolve **at arrival**, run the espionage step **first and separately** from the main battle, and
emit **intel reports**. It rides the 007 movement engine and the 009 combat resolver/report rails.

## Design
- **Espionage is pure domain and seedless.** `scouting.rs::resolve_scouting(attackerPower,
  defenderPower, rules) -> {attacker_loss_frac, detected}` — `scouting_power` sums each side's
  Scout-role `scouting` strength (not Smithy-scaled). With no counter power the scout is **clean**
  (zero loss, undetected); counter ≥ attacker wipes the attacking scouts; otherwise a **power-law**
  `(def/atk)^k` fraction. **Only the attacking scouts can die — defending scouts never do.** Unlike the
  main battle (009) there is **no luck, no morale, no Wall bonus**, so the outcome is fully determined
  by the persisted counts — reproducible (P2/P6) with **no seed at all**. The loss exponent is balance
  data (`combat.toml [scouting]`, P7); the per-unit `scouting` strength lives in `units.toml`.
- **Detection is stealthy.** The defender learns scouting happened **iff ≥ 1** attacking scout died to
  counter-espionage. A fully clean scout is invisible (GDD §7.3).
- **Intel is a snapshot at arrival, read in the application layer.** `gather_intel` (shared by both
  paths) computes **Resources** by accruing the target's stored resources to the arrival instant (002
  compute-on-read, P1) or **Defenses** as the merged garrison + reinforcements troops plus the Wall
  level. Reading persisted state is I/O, so this is application, not domain (P3).
- **Standalone missions are a new `scout` movement kind.** They share `troop_movements` (widened CHECK)
  with a nullable `scout_target` column, are claimed by a dedicated `claim_due_scouts`, and resolve via
  `process_due_scouts` → `apply_scout` (one tx: insert the intel report, schedule the survivor return,
  mark the movement `done`). Survivors return via the existing 007 `return` kind. No defender troops
  change at resolution — only the attacker's scouts can be lost (they were already debited at send) —
  so the resolve write-set is the scouter's side alone.
- **Scouts riding an attack extend 009's `process_due_combat`.** When an attack/raid carries scouts
  (its `scout_target` is set; `order_attack` defaults it to **Defenses**), `resolve_one` runs the
  **espionage step first** — scouts add no combat power (009 already excludes them), so it never alters
  the battle. The espionage-surviving scouts then take the **attacker main-battle loss fraction** (a
  lost normal attack wipes them with the army); intel is delivered **iff ≥ 1 scout is among the
  returning survivors**, so an annihilated attack brings nothing home. The scouter's intel rides as a
  `scout_report`; the defender's 009 battle report gains a `scouted` flag (set only when detected),
  written in the **same transaction** as the battle.
- **Reports redact at the source.** `scout_reports` is visible in full to the scouter; a **detected,
  standalone** mission also surfaces a redacted **notification** to the target (scouts destroyed only —
  no intel, no scouts-sent), enforced in `scout_report`/`scout_reports_for` (P4), not the template.
  Combined attacks notify the defender via the battle report's `scouted` flag instead, so there is no
  duplicate notification and intel rendering stays one path.

## Consequences
- Siege & loot (011) can read `Defenses` intel toward catapult targeting; conquest (014) and ranking
  (016) read the same report rails. Counter-espionage is **scouts-only** in 010 — Wall/Palace detection
  bonuses are a deferred refinement.
- Espionage being seedless is a *stronger* determinism guarantee than the main battle: there is no RNG
  to persist, and re-resolution after a crash is trivially identical (P2/P6).
- A combined attack always emits a scouter-only `scout_report` plus the 009 battle report; the defender
  sees the battle and, only when detected, the scouted flag.

## Links
specs/constitution.md (P1, P2, P3, P4, P6, P7); specs/features/010-scouting/; specs/balance/combat.toml
([scouting]), specs/balance/units.toml (scouting); crates/domain/src/scouting.rs (resolve_scouting,
scouting_power, ScoutTarget); crates/application/src/scouting.rs (order_scout, process_due_scouts,
gather_intel), crates/application/src/combat.rs (espionage sub-step in resolve_one),
crates/application/src/ports.rs (ScoutRepository, ScoutIntel); crates/infrastructure/src/repo.rs
(start_scout, claim_due_scouts, apply_scout, scout reports + redaction),
crates/infrastructure/src/event_store.rs (scout tick); crates/web/src/handlers.rs (rally scout mode,
merged inbox, scout detail); migrations/0013_scouting.sql.
