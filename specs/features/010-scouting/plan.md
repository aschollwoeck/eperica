# Feature 010 — Scouting — Technical Plan

**Status:** Verified
**Spec:** ./spec.md

Espionage layered on the 009 combat rails: a pure scout-loss formula (no luck/morale, no RNG), one new
movement kind (`scout`) for standalone missions, an **espionage sub-step folded into 009's
`process_due_combat`** for scouts riding an attack/raid, and persisted **intel reports** that reuse the
existing report inbox. Surviving scouts come home via the existing `return` kind. A new `scouting`
balance attribute drives both espionage and counter-espionage. No new external dependencies (intel uses
the existing `serde_json`/`jsonb`).

## Constitution check

- **P1 (event-driven):** a mission resolves as the **apply of one due-timestamped movement**; nothing
  polls it. Surviving scouts return as another due-event. Intel (resources) is **computed-on-read** at
  the arrival instant via the 002 accrual model — never a live feed.
- **P2 (reproducible):** the scout counts, the `scouting` balance, and the target's resources/troops/
  Wall at arrival fully determine losses, detection, and intel; loss application + report + survivor
  return + the movement `done` flip happen in **one transaction** (exactly-once, orphan-requeue safe).
  A report is re-derivable from persisted state.
- **P3 (pure domain):** `resolve_scouting` and `scouting_power` are pure over numbers + injected
  `ScoutRules`. Reading the target's state to build intel is **application** (it touches I/O), not domain.
- **P4 (server authority):** the client sends only `(target, scout counts, target type[, attack mode])`;
  power, losses, detection, the revealed intel, and the reports are server-computed. The defender never
  sees intel the attacker gathered (view-level redaction).
- **P6 (seeded determinism):** espionage has **no luck/morale**, so it needs **no seed** — it is fully
  determined by the persisted counts (a stronger guarantee than 009's seeded luck). The combined main
  battle keeps 009's `luck_factor(world_seed, movement_id)`, unchanged.
- **P7 (configurable):** the per-unit `scouting` strength and the scout **loss exponent** are balance
  data; world speed scales travel time (007).
- **P11 (performance):** standalone resolution reads only the target's garrison + reinforcements (the
  same indexed reads 009 already makes for a combined attack) and writes the scouter's report + one
  return in one tx; the claim reuses the indexed `(status, arrive_at, id)` order. A combined attack
  reuses the garrison/reinforcements/village 009 already loaded for the battle; the **only** extra read
  is one indexed `stored_resources` lookup, and only for a *Resources*-target attack whose scouts
  survive (Defenses intel needs no extra query).

## Domain (`domain`, pure)

- `MovementKind` gains `Scout` (exhaustive — drives mapping updates across balance/repo/web, like 009).
- `UnitSpec` gains `scouting: u32` (espionage **and** counter-espionage strength; `0` for non-scouts).
  Every `UnitSpec` literal (domain + application tests) gets the new field — mechanical, like 009's
  `siege_kind` addition.
- New `scouting.rs`:
  - `ScoutTarget { Resources, Defenses }` — what a mission spies on.
  - `ScoutRules { loss_exponent: f64 }` — balance (P7).
  - `scouting_power(troops: &UnitCounts, roster: &[UnitSpec]) -> f64` — `Σ count·scouting` over
    **Scout-role** units only (no Smithy scaling; scouting is not a Smithy stat). Non-scouts contribute 0.
  - `resolve_scouting(attacker_power: f64, defender_power: f64, rules: &ScoutRules) -> ScoutOutcome`
    where `ScoutOutcome { attacker_loss_frac: f64, detected: bool }`:
    - `defender_power <= 0` (or `attacker_power <= 0`) ⇒ `(0.0, false)` — no counter, clean scout.
    - `defender_power >= attacker_power` ⇒ `(1.0, true)` — all attacking scouts lost.
    - else ⇒ `((defender_power / attacker_power).powf(k), true)` — power-law partial loss.
    - `detected = attacker_loss_frac > 0` (≥ 1 scout dies to counter-espionage). **Defending scouts are
      never lost** — the resolver returns no defender fraction at all.
  - Tests: zero counter ⇒ no loss, undetected; equal/greater counter ⇒ all lost, detected; partial ⇒
    `0 < frac < 1`, detected; stronger attacker loses fewer (monotonic in the ratio); two identical
    calls are equal (deterministic, no seed).
- `combat.rs` is **unchanged** — scouts are already excluded from `attack_power`/`add_defense` (009);
  the comment "(010)" there now refers here.

## Balance (`specs/balance/` + `infrastructure::balance`)

- `units.toml` — add `scouting = N` to the three Scout-role units (Roman *Equites Legati*, Gaul
  *Pathfinder* higher; Teuton *Scout* lower — consistent with their speed/cost tiers); the loader
  defaults `scouting` to `0` for every other unit (and for forward-compat).
- `combat.toml` — add `[scouting] loss_exponent = k`; a `scout_rules()` loader returns `ScoutRules`.
  (Kept beside combat balance; espionage is a combat sibling.)

## Persistence (`infrastructure` + migration `0013_scouting.sql`)

- `ALTER TABLE troop_movements` — widen the `kind` CHECK to add `scout`; add `scout_target text NULL`
  (`'resources' | 'defenses'`) carried by **standalone scout** movements and by **attack/raid movements
  that include scouts** (NULL otherwise).
- `ALTER TABLE battle_reports` — add `scouted boolean NOT NULL DEFAULT false` and `scout_target text
  NULL`, so a **combined** attack's defender report can flag that scouting occurred (AC8) without a
  duplicate notification.
- New `scout_reports(id uuid PK, occurred_at timestamptz, scouter_player uuid, scouter_village uuid,
  target_player uuid, target_village uuid, target_coord_x/y int, target_type text, scouts_sent jsonb,
  scouts_lost jsonb, detected boolean, standalone boolean, intel jsonb NULL)` + indexes on
  `(scouter_player, occurred_at)` and `(target_player, occurred_at)`.
  - `intel` is `NULL` when no scout returned; otherwise a tagged payload — `{"type":"resources",
    "wood":…,"clay":…,"iron":…,"crop":…}` or `{"type":"defenses","troops":{unit→count},"wall":level}`.
- Port surface (impl on `PgAccountRepository`, beside `CombatRepository`):
  - **Standalone** — a `ScoutRepository`:
    - `start_scout(home, deliver, owner, origin, dest, now, arrive, troops, target)` — the 009 guarded
      garrison debit, generalised, inserting a `scout` movement with `scout_target`.
    - `claim_due_scouts(now, limit) -> Vec<DueScout>` (`kind = 'scout'`, troops + `scout_target` loaded).
    - `apply_scout(ScoutApply)` — **one tx**: insert the `scout_report`; insert the survivor `return`
      movement (if any); mark the scout movement `done`. (No defender troops change — scouts already left
      the attacker's garrison at send; the defender is untouched.) Exactly-once.
    - `scout_reports_for(player, limit)` + `scout_report(id, player)` — visible to the **scouter** (full
      intel) or, only when `detected AND standalone`, to the **target** (redacted: scouted + scouts
      destroyed, no intel). A third party gets `None`/nothing (P4).
  - **Combined** — extend the 009 surface:
    - `DueAttack` gains `scout_target: Option<ScoutTarget>` (loaded from the row).
    - `BattleApply` gains an optional `scout_report: Option<NewScoutReport>` and the `scouted`/
      `scout_target` flags; `apply_battle` writes them in its existing single transaction.
  - `claim_due_movements` already excludes combat kinds; it is narrowed to also exclude `scout` so the
    three processors don't fight over rows; `requeue_orphaned_movements` already covers all kinds.

## Application (`application`)

- `ScoutError` (Insufficient / EmptyComposition / **NotAllScouts** / NoTargetThere / SameTile / NotFound
  / Backend) — `NotAllScouts` is the standalone scouts-only rule (AC3).
- `order_scout(accounts, scout, starvation, economy_rules, unit_rules, map, speed, now, owner, target,
  troops, scout_target)` — validate own village, **every requested unit is Scout-role** (else
  `NotAllScouts`), garrison covers the counts, target is another village on a different tile; travel via
  the slowest scout (007, P7); debit + `start_scout`; re-sync home starvation (garrison shrank). Mirrors
  `order_attack`.
- `order_attack` **extended** with `scout_target: Option<ScoutTarget>` — when the composition contains
  scouts and `scout_target` is `None`, it defaults to `Defenses`; the value is persisted on the movement
  (so resolution is deterministic from state). No scouts ⇒ `scout_target` stays `None`.
- `process_due_scouts(accounts, movements, units, scout, economy_rules, unit_rules, scout_rules, map,
  speed, now, limit)` — claim due `scout` movements; for each: load the target village + garrison +
  reinforcements; `attacker_power = scouting_power(scouts)`, `defender_power = Σ scouting_power(target
  garrison + every reinforcement group)`; `resolve_scouting`; `apply_losses` to the movement scouts; if
  ≥ 1 survives, **gather intel** (Resources → accrue the target's stored resources to `now` via the 002
  model; Defenses → the target's troops [garrison + reinforcements, merged] + Wall level already in
  hand) and schedule the survivor `return`; assemble the `scout_report` (`standalone = true`); `apply_scout`.
- `process_due_combat` (009) **extended** — in `resolve_one`, when `attack.scout_target.is_some()` and
  the movement holds Scout-role units, run the **espionage sub-step first** (GDD §9.4 step 1), then the
  existing main battle:
  1. Split `attack.troops` into scouts / non-scouts.
  2. Espionage: `resolve_scouting(scouting_power(scouts), defender_counter, scout_rules)` →
     `scout_loss_frac`, `detected`. `scouts_after_espionage = apply_losses(scouts, scout_loss_frac).0`.
  3. Main battle: unchanged inputs (scouts already excluded from `attack_power`/`add_defense`) → the
     existing `outcome.attacker_loss_frac`.
  4. Survivors: non-scout survivors `= apply_losses(non_scouts, attacker_loss_frac).0`; scout survivors
     `= apply_losses(scouts_after_espionage, attacker_loss_frac).0`; the attacker's returning force is
     their union. `attacker_losses` (report) `= troops − survivors`.
  5. Intel: gathered **iff ≥ 1 scout is among the returning survivors** (AC7); the same Resources/
     Defenses gather as standalone. Written as a `scout_report` (`standalone = false`, scouter-only).
  6. The defender's `battle_report` gets `scouted = detected`, `scout_target = Some(target)`.
  The combined apply stays **one transaction** (the 009 `apply_battle`, now also writing the scout_report
  + flags).
- A small shared `gather_intel(target, target_type, garrison, reinforcements, economy_rules, now) ->
  Intel` helper builds the payload for both processors (Resources accrues; Defenses merges troops + reads
  Wall).

## Interface (`web`)

- **Rally Point** (`/village/rally`, `POST /village/rally/send`):
  - The **mode** selector gains `scout`. In `scout` mode the form offers **scout-role units only** and a
    **target type** (Resources | Defenses); `rally_send` routes to `order_scout`.
  - In `attack`/`raid` mode, a **scout target** dropdown appears when the composition includes scouts
    (default **Defenses**); its value is passed to `order_attack` as `Some(ScoutTarget)`.
  - Everything is re-validated server-side (P4); unavailable choices are also rejected server-side.
- **Reports inbox** (`GET /reports`) — merges 009 battle reports with **scout reports** (newest first):
  `reports_for(player)` ∪ `scout_reports_for(player)`, each mapped to a unified `ReportRow` with a kind
  tag and a link to the right detail route. A battle report whose `scouted` flag is set shows a
  "+ scouted" marker for the defender.
- **`GET /reports/scout/{id}`** — scout-report detail: the **scouter** sees scouts sent/lost and the
  revealed intel (resources, or troops + Wall) or "no intel"; the **target** (standalone detected) sees
  only "your village was scouted — N scouts destroyed". Redaction is enforced in `scout_report(id,
  player)` (P4), not the template.
- **`GET /reports/{id}`** (009) — the combined defender's battle report additionally renders "the enemy
  scouted your &lt;resources|defenses&gt;" when `scouted` is set.
- Auth via `AuthUser` (Visitor → login).

## Test strategy

| AC | Test |
|----|------|
| AC1 | app (fakes): a standalone scout debits scouts + schedules a `scout` movement at `now + travelTime` with the chosen `scout_target`. |
| AC2 | app (fakes): an attack/raid including scouts schedules with `scout_target` carried (defaulting to `Defenses` when omitted). |
| AC3 | app: each reject leaves the garrison untouched — over-garrison, empty, **non-scout in a standalone** (`NotAllScouts`), no target, own tile. |
| AC4 | domain: `resolve_scouting` gives identical results on repeat (no seed); no luck/morale terms exist. |
| AC5 | domain: defender 0 ⇒ `(0, undetected)`; defender ≥ attacker ⇒ `(1, detected)`; partial ⇒ `0<frac<1, detected`; stronger attacker loses monotonically less. |
| AC6 | app/infra: a standalone runs only espionage (no defender troop change, no main battle); a combined attack runs espionage **then** the 009 battle, and the espionage never alters the battle inputs/outcome. |
| AC7 | app: combined — espionage survivors then take the attacker loss fraction; a wiped attacker yields **no intel** though espionage gathered it; a survivor delivers intel. |
| AC8 | infra: standalone undetected (target has no scouts) ⇒ **no** target-visible report; detected ⇒ target sees a notification; combined detected ⇒ the defender's battle report has `scouted = true`. |
| AC9 | infra: Resources intel equals the target's resources accrued to arrival (P1); Defenses intel equals merged troops + Wall level. |
| AC10 | infra (DB): surviving scouts get a `return` movement (rejoin home); all-dead ⇒ none; a combined attack's surviving scouts return with the army; re-claim after a crash does not double-apply. |
| AC11 | infra (DB): a scout report is persisted; the scouter reads the intel, the target reads only the notification (and nothing when undetected), a third party reads nothing. |
| AC12 | web integration: launch a standalone scout and a combined attack (PRG); an intel report appears in the scouter's inbox; the defender sees the scouted flag; visitor → login. |

## Notes

- **Exactly-once**: scout-loss application is moot for the defender (untouched); for the scouter the
  report + survivor return + the movement `done` flip share one tx. A crash before commit is requeued
  (`processing → in_transit`) and re-resolved — deterministic, so the re-run is identical (P2).
- **Defenders are never harmed by scouting** — counter-espionage costs them nothing; only the attacker's
  scouts can die. This is the faithful asymmetry and keeps the resolve write-set to the scouter's side.
- **Intel is a snapshot**: resources via the 002 accrual to the arrival timestamp; troops/Wall as
  persisted at resolution. No building-level list (that is 011's catapult-targeting concern).
- **Combined intel always rides as its own `scout_report`** (scouter-only), so intel rendering is one
  path for both standalone and combined; the defender's awareness rides on the battle report's `scouted`
  flag — no duplicate defender notification.
- The movement id is **not** used for any randomness here (espionage is seedless); it remains the
  movement's identity and the combined main battle's luck seed (009).
