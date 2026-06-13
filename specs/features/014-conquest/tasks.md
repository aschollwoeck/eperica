# Feature 014 — Conquest — Tasks

**Plan:** ./plan.md · **Spec:** ./spec.md

Ordered for dependency and testability (pure domain first). Each task is a commit; gates
(`fmt`/`clippy -D warnings`/`test` + P11) pass before advancing. The conquest step rides the 009
battle apply — most of the slice is the **loyalty model** and the **ownership transfer**, not new combat
math.

## Domain & balance

- [ ] **T1 — Loyalty model + regen.** `loyalty.rs`: `LoyaltyRules` + pure `regenerate_loyalty`
  (accrue-to-100, clamped `[0,100]`, speed-scaled). `conquest.toml` + `loyalty_rules()` loader
  (fail-fast). Migration `00NN` adds `villages.loyalty smallint NOT NULL DEFAULT 100` +
  `loyalty_updated_at timestamptz NOT NULL DEFAULT now()`; repo fills the **regenerated** loyalty onto
  the `Village` read and exposes `set_loyalty(village, value, at)`. Domain + DB tests: loyalty reads back,
  regenerates toward 100, and clamps (**AC1**, **AC9**).

- [x] **T2 — Administrators (the conqueror id-set).** Identify administrators by a **balance
  `administrator_ids`** list (Senator/Chief/Chieftain) on `LoyaltyRules` + `conquest.toml` — not a
  `UnitSpec` flag (that would churn all 27 `UnitSpec` constructors for one bool, and administrators are
  already named per tribe). `LoyaltyRules::is_administrator` + `administrator_count`. Administrators
  **already** fight (Expansion falls through `attack_power`/`add_defense`); confirmed by a domain test —
  **no combat-math change** (the 013 settler-defence intent is left as a 013 carry-over, out of 014
  scope). Tests: administrators are identified/counted; each id is a real Expansion combatant trained in
  a Residence; an Expansion administrator contributes attack + defence (**AC2**).

## Domain — the conquest decision

- [x] **T3 — Loyalty drop + conquest outcome (pure).** `administrator_drop(surviving_admins, seed,
  rules)` (Σ seeded per-admin draws in `[drop_min, drop_max]`, the 009 luck RNG; `0` with no survivor);
  `conquest_outcome(loyalty_now, drop, is_capital, attacker_has_slot) -> { new_loyalty, transferred }`
  (capital drops nothing/never transfers; `new = max(0, loyalty − drop)`; `transferred = new==0 &&
  slot`). Unit tests: the drop sums and zeroes on a loss; the capital exception; the slot gate; transfer
  at zero (**AC3**, **AC4**, **AC5**, **AC6**).

## Persistence — the transfer

- [x] **T4 — Ownership transfer (one tx).** `apply_battle` gains the **conquest branch**, guarded on the
  defender still owning the village: re-point `owner_id`; set `loyalty = post_conquest`, `is_capital =
  false`; empty the garrison; **return** third-party reinforcements (007); **cancel** the old owner's
  pending build/unit/training orders + outgoing movements for the village; **re-anchor both** players'
  `player_culture` (013) at `battle_at`. A loyalty-only outcome writes `set_loyalty` instead. Enumerate
  every `village_id`-keyed dependency (garrison, reinforcements, queues, movements, oases, culture) and
  handle each. DB tests: a conquest transfers + keeps fields/buildings/resources, empties garrison,
  returns reinforcements, re-anchors cultures, cancels orders, tile unchanged, `is_capital=false`,
  exactly-once + guarded; losing the last village leaves a coherent account (**AC7**, **AC8**, **AC12**).

## Application — the resolution step

- [x] **T5 — Conquest step on the 009 resolver.** `process_due_combat`/`resolve_one`: after the main
  battle + catapults, if the attack carries administrators and the **attacker won**, read the defender
  village loyalty + `is_capital`, count **surviving administrators**, compute the seeded drop + the
  attacker's free-slot check (013), produce a `conquest_outcome`, and thread it into `BattleApply`
  (`loyalty_before/after`, `conquered`, `new_owner`) + the report. A capital / no-administrator battle
  skips the step (zero overhead). Fake/DB tests: a won admin-attack drops loyalty; conquest at zero;
  capital safe; no-slot ⇒ loyalty-only; the report carries the change (**AC4**, **AC5**, **AC6**,
  **AC10**).

## Interface — web

- [ ] **T6 — Loyalty read + web.** Each owned village's **loyalty** on the village page (013 area); the
  **battle report** shows loyalty **before → after** + **"Village captured"** when transferred; a
  just-conquered village appears in the conqueror's **switcher** and drops from the loser's. Integration
  tests: send administrators → the report shows the loyalty change + capture → the village is the
  conqueror's; a capital cannot be taken (**AC10**, **AC11**).

## Scheduler & acceptance

- [ ] **T7 — Technical/end-user docs.** No new scheduler tick (conquest rides the 009 combat tick; the
  existing movement requeue covers the attack). rustdoc; `docs/architecture/00NN-conquest.md`;
  `docs/manual/` conquest guide; `CLAUDE.md` active slice → 014.

- [ ] **T8 — Review & accept.** Full gates + P11; `eperica-reviewer` on the slice diff; fix until
  **APPROVE**; PR.

## Done when

Per the [Definition of Done](../../implementation-workflow.md#definition-of-done-checklist--applies-to-every-slice):
**AC1–AC12** pass with tests, all gates green, both docs written, reviewer **APPROVE**, PR merged,
`spec.md`/`plan.md` **Verified**, roadmap updated.
