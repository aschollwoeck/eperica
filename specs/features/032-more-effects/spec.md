# Feature 032 — More building effects + resource bars

**Status:** Verified
**Depends on:** 031 (the informative-actions pass this extends), 009 (combat — Wall/Cranny), 008 (trade — merchants), 013 (culture — Town Hall/Residence)
**Roadmap:** app-layer UX — continuation of the **UX information** pass (031): cover the building effects whose rules live *outside* the economy, and add live resource bars.

## Goal

Finish surfacing the **effect** of every village upgrade — including buildings whose rules come from
combat / trade / culture / construction / training, not just the economy — and give the resource panel a
**fill bar + time-to-full / time-to-empty** so a player can see their economy at a glance. Read-only
presentation of existing domain rules; no game-rule change.

## Acceptance criteria

- **AC1 — Building effects beyond the economy.** The village build table shows the next-level effect for:
  Wall (defence %), Cranny (resources hidden), Marketplace (merchant count), Town Hall (culture/h),
  Residence & Palace (expansion slots), Main Building (build-speed factor), Barracks/Stable/Workshop
  (training-speed factor) — alongside the 031 economy effects (production / storage / oasis / population).
  Blank at max level.

- **AC2 — Wall is tribe-correct.** The Wall defence bonus uses the village's tribe's Wall profile (P7).

- **AC3 — Resource bars.** Each resource line shows a fill bar (amount / capacity) and a live ETA —
  "full in h:mm:ss" when filling, "empty in …" when the (crop) rate is negative, "full" at capacity —
  computed client-side from the displayed amount/cap/rate.

- **AC4 — No rule change (P3/P4).** All effects derive from existing pure rules; new domain code is a single
  read-only accessor (`CombatRules::wall_bonus`). Server-side validation unchanged.

- **AC5 — Reproducibility (P2/P7).** Effects are deterministic from the balance rules + world speed.

## Notes

- New pure accessor: `CombatRules::wall_bonus(tribe, level)`. The rest reuses existing public accessors
  (`cranny_capacity`, `merchants_total`, `culture_rate`, `expansion_slots`, `main_building_factor`,
  `TrainingRules::building_factor`).
- The resource-bar ETA is client-side (instant, no server round-trip); it reads the same amount/cap/rate the
  panel already shows.

## Out of scope

- Embassy / Academy / Smithy / Rally-Point per-level numeric effects (enablers, not scalar upgrades).
- Visual theming / imagery (the later pass).
