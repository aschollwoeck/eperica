# Feature 009 — Combat resolution — Technical Plan

**Status:** Verified
**Spec:** ./spec.md

Battle resolution layered on the 007 movement engine: two new movement kinds (`attack`/`raid`), a
pure battle formula, a new **Wall** building, and persisted **battle reports**. The attacker's
survivors come home via the existing `return` kind. No new external dependencies (reports use the
existing `serde_json`/`jsonb`).

## Constitution check

- **P1 (event-driven):** a battle is the **apply of one due-timestamped movement**; nothing polls it.
  Survivors return as another due-event.
- **P2 (reproducible):** both sides' troops, the Wall, populations, and the seed fully determine the
  outcome; resolution + casualty application + report happen in **one transaction** (exactly-once,
  orphan-requeue safe). A report is re-derivable from persisted state.
- **P3 (pure domain):** `resolve_battle` and `apply_losses` are pure over numbers + injected
  `CombatRules`; luck is a pure seeded hash. No I/O in the math.
- **P4 (server authority):** the client sends only `(target, per-unit counts, mode)`; power, the
  target, casualties, luck, and the report are server-computed.
- **P6 (seeded determinism):** luck = `splitmix64(worldSeed, movementId)` mapped to `[1−L, 1+L]`;
  re-resolving is identical.
- **P7 (configurable):** loss exponent, luck range, morale exponent, base defense, per-tribe wall
  bonus/durability are balance data; world speed scales travel time.
- **P11 (performance):** resolving reads the target's garrison/reinforcements/buildings (indexed) and
  writes them back in one tx; the claim reuses the indexed `(status, arrive_at, id)` order.

## Domain (`domain`, pure)

- `MovementKind` gains `Attack`, `Raid` (exhaustive — drives mapping updates).
- `UnitSpec` gains `siege_kind: Option<SiegeKind>` (`Ram` | `Catapult`); non-siege ⇒ `None`.
- New `combat.rs`:
  - `CombatRules { loss_exponent, luck_range, morale_exponent, base_defense, walls: HashMap<Tribe,
    WallProfile> }`, `WallProfile { bonus_per_level: Vec<f64>, ram_durability: f64 }`.
  - `luck_factor(world_seed: u64, movement_id: u128, range: f64) -> f64` — SplitMix64 hash → `[1−L,1+L]`.
  - `BattleInput { inf_attack, cav_attack, def_inf_total, def_cav_total, ram_power, wall_tribe,
    wall_level, attacker_pop, defender_pop }` and `AttackMode { Attack, Raid }`.
  - `resolve_battle(mode, input, rules, luck) -> BattleOutcome { attacker_won, attacker_loss_frac,
    defender_loss_frac, wall_razed, effective_wall_level, morale }`:
    effective wall = `level − floor(ram_power/durability)`; defender power =
    `(infShare·defInf + cavShare·defCav + base)·(1 + bonus(tribe, effWall))`; attacker power =
    `(infAttack+cavAttack)·morale·luck` with `morale = min(1, (defPop/atkPop)^e)`; power-law losses —
    **attack**: loser 100 %, winner `(loser/winner)^k`; **raid**: each side `otherᵏ/(atkᵏ+defᵏ)`.
  - `apply_losses(counts: &UnitCounts, frac: f64) -> (survivors, losses)` — deterministic rounding.
  - `attack_pools(troops, roster, levels, smithy_rules)` / `defense_totals(...)` — split helpers
    (Smithy-scaled; **Scout** role excluded from the main battle; **Ram** force summed separately).
  - Tests: stronger attacker wipes the defender (attack) and bleeds little; raid bleeds both; the Wall
    raises defender power; rams cut the Wall; morale dampens a big attacker; same luck ⇒ same outcome.

## Balance (`specs/balance/` + `infrastructure::balance`)

- New `combat.toml` — scalars + `[walls.romans|teutons|gauls] bonus_per_level, ram_durability`;
  `combat_rules()` loader.
- `construction.toml` — `[buildings.wall]` (fixed slot, no prereq) + `economy.toml` wall population;
  `BuildingKind::Wall` arm in every mapping (balance/repo/web) + buildable list.
- `units.toml` — `siege = "ram" | "catapult"` on siege units; parsed to `siege_kind`.

## Persistence (`infrastructure` + migration `0012_combat.sql`)

- `ALTER TABLE troop_movements` — widen the `kind` CHECK to include `attack`,`raid`.
- `battle_reports(id uuid PK, occurred_at timestamptz, kind text, attacker_player/defender_player uuid,
  attacker_village/defender_village uuid, attacker_won bool, luck double, morale double, wall_before
  int, wall_after int, attacker_forces/attacker_losses/defender_forces/defender_losses jsonb)` +
  indexes on `(attacker_player, occurred_at)` and `(defender_player, occurred_at)`.
- Port `CombatRepository` (impl on `PgAccountRepository`):
  - `start_attack(home, deliver, owner, origin, dest, now, arrive, kind, troops)` — guarded garrison
    debit + insert the attack/raid movement (the 007 guarded-debit, generalised over kind).
  - `claim_due_attacks(now, limit) -> Vec<DueAttack>` (`kind IN ('attack','raid')`, troops loaded).
  - `apply_battle(BattleApply)` — **one tx**: set the defender garrison + each reinforcement group to
    their after-counts; insert the report; insert the survivor `return` movement (if any); mark the
    attack movement `done`. Exactly-once.
  - `reports_for(player, limit)` + `report(id, player)` (visible to attacker or defender only, P4).
  - `claim_due_movements` is narrowed to `kind IN ('reinforce','return')` so the two processors don't
    fight over rows; `requeue_orphaned_movements` already covers all kinds.

## Application (`application`)

- `CombatError` (Insufficient / EmptyComposition / NoTargetThere / SameTile / NotFound / Backend).
- `order_attack(accounts, combat, starvation, economy_rules, unit_rules, map, speed, now, owner,
  target, troops, mode)` — validate (own village, garrison, target ≠ self), travel via 007, debit +
  `start_attack`, re-sync home starvation (garrison shrank).
- `process_due_combat(accounts, combat, economy_rules, unit_rules, combat_rules, map, speed,
  world_seed, now, limit) -> Vec<VillageId>` — for each due attack/raid: load target village +
  garrison + reinforcements + both populations; compute attacker pools (Smithy levels) + defender
  totals + ram power; `luck_factor`; `resolve_battle`; `apply_losses` to each party; assemble the
  report + survivor return (travel home); `apply_battle`. Returns the home/target villages whose
  troops changed (starvation re-sync). Ticked by the scheduler; orphan requeue at startup (shared).

## Interface (`web`)

- **Rally Point** gains a **mode** (`reinforce` / `attack` / `raid`); `POST /village/rally/send`
  routes `attack`/`raid` to `order_attack`. (Reinforce stays 007.)
- **`GET /reports`** — the player's battle reports (newest first: opponent, kind, won/lost, when).
- **`GET /reports/{id}`** — detail: both forces + losses, wall razed, **luck & morale**, outcome
  (visible only to the two parties, P4). A **Reports** link in the village header.
- Auth via `AuthUser` (Visitor → login); everything re-validated server-side (P4).

## Test strategy

| AC | Test |
|----|------|
| AC1/AC2 | app (fakes): attack/raid debits + schedules; each reject leaves the garrison untouched. |
| AC3 | domain: same `(input, luck)` ⇒ identical outcome; luck within range for varied ids. |
| AC4 | domain: attacker≫defender ⇒ defender 100 % (attack), attacker small loss; raid ⇒ both partial; blended inf/cav split. |
| AC5 | domain: a Wall raises defender power (fewer defender / more attacker losses); rams lower the effective Wall to 0 at enough force. |
| AC6 | infra (DB): resolving reduces the defender garrison + reinforcements and the attacker troops once; a survivor `return` is scheduled; re-claim after a crash does not double-apply. |
| AC7 | infra/app (DB): a report is persisted and readable by both parties (and not by a third). |
| AC8 | web integration: launch a raid (PRG); a report appears in both inboxes with forces/losses/luck/morale; visitor → login. |

## Notes

- **Exactly-once**: casualty application + report + survivor return + the movement `done` flip share
  one transaction; a crash before commit is requeued (`processing → in_transit`) and re-resolved —
  deterministic, so the re-run yields the identical outcome (P2/P6).
- **Survivors return empty** (loot is 011); the return reuses the 007 `return` apply (garrison rejoin)
  + its starvation re-sync.
- Defender reinforcements take losses **pro-rata with the garrison** (same fraction), so helping
  troops share the defence's fate — faithful and simple.
- The movement id (a UUID) seeds luck via its low 64 bits through SplitMix64 — distinct per battle,
  independent of wall-clock (P6).
