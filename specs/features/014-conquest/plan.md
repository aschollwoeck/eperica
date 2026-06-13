# Feature 014 — Conquest — Technical Plan

**Status:** Verified
**Spec:** ./spec.md

Conquest is the **aggressive** multi-village path: every village carries a **loyalty** that regenerates
over time; an **administrator** (Senator/Chief/Chieftain) that survives a **won** attack lowers it, and
at **zero** loyalty — with the attacker holding a **free 013 expansion slot** and the target **not a
capital** — **ownership transfers**. It reuses the **009 battle engine verbatim** (administrators are
ordinary combatants) and adds only **resolution step 5** (GDD §9.4) and the **transfer**, applied in the
**same transaction** as the existing battle apply. No new movement kind, no new scheduler tick, no new
combat math.

## Constitution check

- **P1 (event-driven):** loyalty is **computed on read** from `(value, updated_at)`, regenerating toward
  100 — never ticked. The conquest is the existing 009 **due-attack**; the loyalty step runs inside its
  resolution.
- **P2 (reproducible):** the loyalty value, the seeded drop, the capital exception, the slot gate, and
  the transfer derive from persisted rows + the world seed; the transfer (or the loyalty-only change)
  applies in **one transaction** with `apply_battle` (exactly-once, orphan-requeue safe).
- **P3 (pure domain):** loyalty regen, the per-administrator drop, and the conquest decision are pure
  over numbers + injected balance (`loyalty.rs`). Reading villages / transferring ownership is I/O.
- **P4 (server authority):** the client only sends the attack composition; loyalty, the drop, the
  capital/slot gates, and the transfer are server-computed at the resolution instant and **re-validated**
  (the defender must still own the village; the attacker must still have a free slot).
- **P6 (seeded determinism):** the per-administrator drop is drawn from the **battle id** seed (the 009
  luck discipline), so a replay yields the same loyalty change and the same transfer.
- **P7 (configurable):** starting/post-conquest loyalty, regen/hour, and the drop range are balance; the
  capital exception and the slot gate reuse 013 (no new balance there). World speed scales regen.
- **P11 (performance):** loyalty is one extra column filled on the existing village read; the conquest
  step adds a few guarded statements only to battles that **carry an administrator and win** — the
  common battle path is unchanged. No per-tick work.

## Domain (`domain`, pure)

- `loyalty.rs` (new) — `LoyaltyRules { starting_loyalty, post_conquest_loyalty, regen_per_hour,
  drop_min, drop_max }`. Pure fns:
  - `regenerate_loyalty(value, elapsed_secs, rules, speed) -> i64` — `min(100, value + regen·elapsed)`,
    clamped `[0, 100]` (the 002 accrue shape with a cap; speed-scaled, P7).
  - `administrator_drop(surviving_admins: u32, seed, rules) -> i64` — `Σ` of `surviving_admins` seeded
    draws in `[drop_min, drop_max]` from the battle-id RNG (P6); `0` when no administrator survives.
  - `conquest_outcome(loyalty_now, drop, is_capital, attacker_has_slot) -> ConquestOutcome` — the pure
    decision: a **capital** drops nothing and never transfers; otherwise `new = max(0, loyalty_now −
    drop)`; `transferred = new == 0 && attacker_has_slot`. Returns `{ new_loyalty, transferred }`.
- `loyalty.rs` — administrators are identified by a **balance `administrator_ids`** list on
  `LoyaltyRules` (not a `UnitSpec` flag, which would churn all 27 `UnitSpec` constructors for one bool;
  administrators are already tribe-named units): `LoyaltyRules::is_administrator(id)` +
  `administrator_count(troops, rules)`, so the resolver counts **surviving administrators** by id without
  role overloading (both administrators and settlers are `UnitRole::Expansion`). Administrators **already
  contribute** attack/defence (`attack_power`/`add_defense` only exclude `Scout`/`Wild`, so the
  `Expansion` administrator falls through to the combatant path — confirmed by a domain test, **no
  combat-math change**). The 013 "settlers provide no defence" intent is a 013 carry-over left out of 014
  scope (settlers never realistically garrison a contested village; attack 0 makes them inert on offence).

## Balance (`specs/balance/` + `infrastructure::balance`)

- `conquest.toml` (new) — `starting_loyalty = 100`, `post_conquest_loyalty`, `loyalty_regen_per_hour`,
  `loyalty_drop_min`, `loyalty_drop_max`. Loaded into `LoyaltyRules` (a `loyalty_rules()` loader,
  fail-fast, mirroring `culture_rules()`).
- `units.toml` — add `conquers = true` to `[[*.units]]` `senator` / `chief` / `chieftain`; the field
  defaults `false` for every other unit (serde default). No attribute changes (they already fight).

## Persistence (`infrastructure` + migration `00NN_loyalty.sql`)

- `villages.loyalty smallint NOT NULL DEFAULT 100` + `villages.loyalty_updated_at timestamptz NOT NULL
  DEFAULT now()` — the lazy per-village loyalty, **regenerated on read** and filled onto the `Village`
  read (like 013's `is_capital`/012's `oasis_bonus`), so the resolver + web see it with no extra query.
  Seeded at registration (002 starting village → 100) and at founding (013 `apply_settle` → 100) by the
  column default; reset to `post_conquest_loyalty` on a conquest.
- Repo (`PgAccountRepository`): fill `Village.loyalty` (regenerated) on the village read; `set_loyalty(
  village, value, at)` for the loyalty-only change (re-anchor). **`apply_battle` extends** (the 009 one-
  tx apply) with the **conquest branch**, guarded on the defender still owning the village:
  - re-point `villages.owner_id` to the attacker; set `loyalty = post_conquest`, `loyalty_updated_at =
    battle_at`; `is_capital = false`;
  - **empty** the village garrison; **return home** any third-party stationed reinforcements (insert 007
    `return` movements to their home villages);
  - **cancel** the old owner's pending `build_orders` / `unit_orders` / `training` / outgoing
    `troop_movements` for that village;
  - **re-anchor both** players' `player_culture` (013): settle each at `battle_at` at their old rate
    before the village count/rate moves between them.
  A **loyalty-only** result instead writes `set_loyalty` in the same tx. The capital exception + slot
  gate are decided in the application (the apply just executes the resolved outcome).

## Application (`application`)

- `combat.rs` — `process_due_combat`/`resolve_one` gains the **loyalty step** after the main battle +
  catapults: if the attack carries administrators and the **attacker won**, read the defender village's
  (regenerated) loyalty + `is_capital`, count **surviving administrators** (the `conquers` units left in
  the attacker's survivors), compute `administrator_drop` (seeded from the battle id), check the
  **attacker's free slot** (013 `allowed_villages` via the culture read), and produce a
  `conquest_outcome`. Thread the result into `BattleApply` (new fields: `loyalty_before`,
  `loyalty_after`, `conquered: bool`, `new_owner: Option<PlayerId>`); `apply_battle` performs the
  transfer or the loyalty write. The **report** (`NewBattleReport`) gains `loyalty_before/after` +
  `conquered`. A capital or a no-administrator battle skips the step entirely (zero overhead).
- Culture re-anchor reuses 013 `reanchor_culture`, called for **both** owners inside the transfer tx
  (via the repo, like the 013 founding) — losing a village lowers the old owner's rate, gaining one
  raises the new owner's.
- No new use-case entry point: conquest is issued as an ordinary `order_attack` (009) whose composition
  includes administrators (the web already sends arbitrary garrison units).

## Interface (`web`)

- **Loyalty on the village page** — each of the player's villages shows its **loyalty** (013 culture/
  switcher area), regenerated on read; a freshly conquered village shows `post_conquest_loyalty`.
- **The battle report** (009/011 report view) shows **loyalty before → after** and **"Village
  captured"** when ownership transferred; visible to both parties (the existing report rails).
- **Sending administrators** — the Rally Point attack form already sends a composition; administrators
  appear among the garrison units and ride along (a hint when the target is an enemy village).
- **The switcher** (013) lists a just-conquered village immediately; the old owner's switcher drops it.

## Test strategy

| AC | Test |
|----|------|
| AC1 | domain: `regenerate_loyalty` accrues toward 100 and clamps; infra: loyalty reads back + regenerates. |
| AC2 | balance/domain: administrators load with `conquers=true` and contribute to `attack_power`/`add_defense`. |
| AC3 | domain: `administrator_drop` sums seeded per-admin draws; zero with no survivor / on a loss. |
| AC4 | domain: `conquest_outcome` transfers at ≤0 with a slot; app: a won admin-attack drops loyalty. |
| AC5 | domain/infra: an admin strike on a **capital** drops nothing and never transfers. |
| AC6 | domain/app: no free slot ⇒ loyalty drops but no transfer (even at 0). |
| AC7 | infra (DB): a conquest transfers owner, keeps fields/buildings/resources, empties garrison, returns reinforcements, re-anchors both cultures, cancels old orders — one tx, guarded, once. |
| AC8 | infra (DB): the conquered village is the new owner's (switcher/economy), tile unchanged, `is_capital=false`. |
| AC9 | domain/infra: a reduced-but-not-taken loyalty regenerates toward 100 over time. |
| AC10 | infra/web: the battle report records loyalty before→after + captured, visible to both. |
| AC11 | web integration: send administrators, the report shows the loyalty change + capture, the village appears in the conqueror's switcher; capital is safe. |
| AC12 | infra (DB): the transfer/loyalty-change applies once, crash-resume safe; reproducible from seed. |

## Notes / open risks

- **`is_capital` on the read already exists (013).** Loyalty joins it as a second per-village field
  filled on the village read — one column pair, no extra query.
- **The transfer is the subtle, blast-radius part.** Re-pointing ownership must move/clear **every**
  per-village dependency keyed by `village_id` (garrison, reinforcements stationed there, build/unit/
  training queues, outgoing movements, occupied oases, culture contribution). Enumerate these against
  the 002–013 schema and handle each in the apply tx (or document why one is safe to leave). The 013
  founding tx is the template for the culture re-anchor; the 012 oasis-occupy race is the template for
  the ownership guard.
- **Settler defence intent (013 carry-over).** `add_defense` excludes only `Scout`; `Expansion` settlers
  (defence 80) currently *would* defend a garrison. 013's spec said settlers provide no defence — verify
  whether that is already handled (settlers may never realistically sit in a defended garrison) or
  whether 014 should exclude **non-conquering Expansion** from `add_defense` behind the new `conquers`
  flag. Decide in T2; do not silently change 009 behaviour without a test.
- **Losing your last village.** Allowed per the spec; ensure the transfer of a player's only (non-
  capital) village leaves a coherent village-less account (the web `/village` already handles "no
  village" → it must not 500). Add a guard/test.
- **Slot-race at conquest.** Two simultaneous admin-attacks could both pass the slot check; the
  **transfer** is guarded on the defender still owning the village, so the second resolves as a
  loyalty-only change (the village is already gone) — like the 012 capacity race.
- **Phasing** (T1–T8) lets each phase land green: loyalty model + regen (T1) and administrators + the
  `conquers` flag (T2) are standalone; the conquest decision (T3, pure) precedes the transfer
  persistence (T4) and the 009 resolution wiring (T5); web + report (T6); scheduler is unchanged, docs
  (T7); review (T8).
