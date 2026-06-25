# Tasks — 066 building-page redesign (Smithy)

Gated by `cargo fmt --all -- --check`, `clippy -D warnings`, `cargo test --workspace`, P11. Presentation only;
branch `feature/066-building-page-redesign`.

- [x] **T1 — Economy through `village_view_data`.** Return `(Village, Economy)` (not just amounts) so building
  pages get rates + capacities; update the 6 callers (academy uses `economy.amounts`; the rest take `_`). (AC2)
- [x] **T2 — Smithy data.** Extend `SmithyTemplate` (resource ribbon: amounts/rates/caps; `village_label`)
  and `SmithyRow` (`portrait` `<tribe>_<id>`, `role`, `forging` flag, `target` level); the smithy handler sets
  them from the economy + the active order. (AC1–AC4)
- [x] **T3 — Building-page CSS.** Add the shared building-page components to `base.css`: `.bld-hero` (art band
  + scrim + crest + title), `.res-ribbon`/`.gauge`, `.roster`/`.unit`/`.forge`/`.pips` (+ ready/forging
  states), `.bld-aside`/`.bld-card`. Reuse the theme tokens; responsive (no animation to guard). (AC1/AC3)
- [x] **T4 — Smithy template.** Rewrite `smithy.html`: hero band, resource ribbon, armoury roster, aside;
  keep the upgrade form/POST, the no-smithy notice, and the countdown JS. (AC1–AC5)
- [x] **T5 — Tests.** Extend the smithy integration test: hero/level present, the ribbon renders, a roster row
  shows portrait + pip track + cost + Upgrade, an affordable row is marked, gated rows show the reason, the
  active upgrade shows in the aside with a countdown. (AC1–AC5)
- [x] **T6 — Gate + reviewer + PR.** fmt/clippy/`cargo test --workspace` green; Playwright check on the live
  page; `eperica-reviewer` → APPROVE; PR opened.
