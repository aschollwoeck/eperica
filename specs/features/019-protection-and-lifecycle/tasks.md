# Feature 019 — Protection & lifecycle — Tasks

**Plan:** ./plan.md · **Spec:** ./spec.md

Ordered for dependency and testability (**pure domain first**). Each task is a commit; gates
(`fmt` / `clippy -D warnings` / `test` + P11) pass before advancing. **No new combat/economy mechanics** —
an attack is gated on the target's protection; abandonment removes villages via the existing delete path.

## Domain & balance

- [ ] **T1 — Pure protection/lifecycle rules + balance (`domain/lifecycle.rs`, P3/P7).** `LifecycleRules`;
  `is_protected`, `protection_expiry` (speed-scaled), `is_inactive` (speed-scaled), `abandon_cutoff`
  (period-anchored, reuses `medals::period_start`), `protection_ended_by_population`. `lifecycle.toml`
  (window/threshold/inactive/abandon/sweep) + fail-fast `lifecycle_rules()` loader. **Unit tests:**
  protected/expired/never; scaling halves at 2×; inactivity threshold edge; cutoff anchored to the period
  boundary; population-threshold end; rules load (AC1, AC4, AC6, AC10).

## Persistence — accounts

- [ ] **T2 — Migration + account protection/activity (`infrastructure`).** Migration `0029_lifecycle.sql`
  (`users` += `protected_until`, `last_activity NOT NULL DEFAULT now()`, `abandoned_at`; `last_activity`
  index; `inactivity_sweeps` watermark table). `create_account` grants `protected_until = now +
  scaled(window)` + seeds `last_activity`. `AccountRepository`: `end_protection`, `touch_activity`
  (throttled conditional), surface the target owner's `protected_until` in the attack-target lookup;
  `authenticate` rejects `abandoned_at IS NOT NULL`. **DB tests:** spawn sets a scaled `protected_until`;
  `touch_activity` writes when stale / no-ops when fresh; abandoned login rejected (AC1, AC5, AC8).

## Application — protection

- [ ] **T3 — Beginner's protection in combat + threshold end.** `CombatError::TargetProtected`;
  `order_attack` rejects an attack on a protected target's village (no movement created) and **ends the
  attacker's own protection** on a valid launch; `end_protection_if_established` (lazy population-threshold
  end). **DB tests:** protected target ⇒ rejected, no movement; attacking ends own protection (then
  attackable); expired protection allows attack; crossing the threshold ends protection and does not
  re-arm (AC2, AC3, AC4).

## Application — lifecycle sweep

- [ ] **T4 — `LifecycleRepository` + abandonment sweep.** `latest_swept_period`; `sweep_abandoned(period,
  cutoff)` (one tx: watermark insert + select past-cutoff non-abandoned users + delete their villages
  (valleys freed) + flag `abandoned_at`; count). `process_due_lifecycle` use-case (settle each
  complete-but-unswept period; period-anchored cutoff), mirroring `process_due_medal_settlement`. **DB
  tests:** the sweep abandons past-cutoff accounts, deletes their villages so the valley is resettlable,
  retains the `users` row (a referencing battle report still reads), is idempotent on re-run, and respects
  the period-anchored cutoff; leaderboards exclude abandoned (AC7, AC8, AC10).

## Interface

- [ ] **T5 — Web: scheduler tick + protection status + map greying.** Scheduler gains `lifecycle_rules` + a
  `process_due_lifecycle` tick; `AppState`/main/spawn wire the rules + world speed/start. Village view:
  `touch_activity` (throttled) + `end_protection_if_established`; template shows **protection status** (and
  when it ends). Map viewport marks **inactive** villages (greyed, derived). **Integration tests:** a fresh
  player's village shows protection status; the map greys an inactive player's village; an abandoned login
  is rejected (AC6, AC9).

## Docs & acceptance

- [ ] **T6 — Technical/end-user docs + review.** rustdoc on new public items;
  `docs/architecture/0021-protection-and-lifecycle.md`; `docs/manual/` protection & lifecycle guide;
  `CLAUDE.md` active slice → 019. Full gates + P11; `eperica-reviewer` on the slice diff; fix until
  **APPROVE**; PR.

## Done when

Per the [Definition of Done](../../implementation-workflow.md#definition-of-done-checklist--applies-to-every-slice):
**AC1–AC10** pass with tests, all gates green, both docs written, reviewer **APPROVE**, PR merged,
`spec.md` / `plan.md` **Verified**, roadmap updated (019 ✅).
