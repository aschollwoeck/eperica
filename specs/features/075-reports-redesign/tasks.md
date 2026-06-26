# Tasks — 075 the battle/scout reports redesign

Gated by `cargo fmt --all -- --check`, `clippy -D warnings`, `cargo test --workspace`, P11. Presentation only;
branch `feature/075-reports-redesign`.

- [x] **T1 — CSS.** Add the reusable `.phead` content-page header + `.page` container; the reports list cards
  (`.replist`/`.repcard`); the battle report summary/callouts/combatant panels (`.repsum`/`.repnote`/
  `.repsides`/`.cas`). (AC1–AC4)
- [x] **T2 — reports.html.** Header + clickable report cards (kept the empty state + relative-time JS). (AC2)
- [x] **T3 — report.html.** Header + summary + loot/razed/loyalty/captured callouts + the attacker/defender
  panels with sent/lost rows. (AC3)
- [x] **T4 — scout_report.html.** Header + the intel card (resources / defences), preserving the
  detected-target + mission-lost states. (AC4)
- [x] **T5 — Tests.** Assert the new chrome (`phead`/`repcard`/`repsides`) on the list + battle + scout
  reports, keeping the existing content assertions (headlines, outcomes, units, Luck/Morale, Loot, captured,
  Resources). (AC1–AC5)
- [ ] **T6 — Gate + reviewer + PR.** fmt/clippy/`cargo test --workspace` green; Playwright (list chrome);
  `eperica-reviewer` → APPROVE; PR opened.
