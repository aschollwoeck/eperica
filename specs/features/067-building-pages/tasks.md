# Tasks — 067 the remaining building pages

Gated by `cargo fmt --all -- --check`, `clippy -D warnings`, `cargo test --workspace`, P11. Presentation only;
branch `feature/067-building-pages`.

- [x] **T1 — Shared ribbon.** `ResourceRibbon` struct + `resource_ribbon(&Economy)` helper +
  `templates/_ribbon.html` partial (4 gauges + fill JS). Refactor the Smithy to use it. (AC1)
- [x] **T2 — Roster CSS.** Add the research/train roster variants to `base.css` (`.roster--research` /
  `.roster--train` column layouts, `.unit__info`/`.unit__stats`, a count input + Train action, a Researched
  badge). Reuse the 066 `.unit`/`.unit--ready`/`.bld-*` chrome. (AC2/AC3)
- [x] **T3 — Academy.** `AcademyTemplate` (ribbon + `village_label`) + `AcademyRow.portrait`; handler sets
  them. Rewrite `academy.html`: hero + ribbon + research roster + aside. (AC2)
- [x] **T4 — Training.** `TroopsTemplate` (ribbon + `village_label` + `building_slug` for the art + level);
  handler sets them. Rewrite `troops.html`: hero + ribbon + training roster (count + batch JS) + aside. (AC3)
- [x] **T5 — Rally.** `RallyTemplate` (ribbon + `village_label`); handler sets them. Rewrite `rally.html`:
  hero + ribbon + the existing send form (markup + preview JS preserved). (AC4)
- [x] **T6 — Market.** `MarketTemplate` (ribbon + `village_label`); handler sets them. Rewrite `market.html`:
  hero + ribbon + the existing send form (markup + preview JS preserved). (AC5)
- [x] **T7 — Tests.** Extend the academy/training/rally/market integration assertions for the new chrome
  (hero, ribbon, roster/form), keeping the existing behaviour assertions. (AC1–AC6)
- [x] **T8 — Gate + reviewer + PR.** fmt/clippy/`cargo test --workspace` green; Playwright sweep of all four
  pages; `eperica-reviewer` → APPROVE; PR opened.
