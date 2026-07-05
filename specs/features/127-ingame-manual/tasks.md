# Tasks — 127 the in-game manual

**Status:** Verified (reviewer APPROVE, 2026-07-05). Gates per task: fmt, clippy -D warnings, cargo test --workspace.

- [x] **T1 — Manual infrastructure.** pulldown-cmark; manual.rs registry (six sections,
  compile-time embeds, link rewriting, callouts, anchors, escaping); /manual + /manual/{slug}
  public routes; manual layout (sidebar, breadcrumbs, prev/next) + CSS; footer/register/nav
  links. Unit + AC1/AC2/AC5 integration tests. (AC1, AC2, AC5, AC7)
- [x] **T2 — Generated reference.** /manual/reference/{units,buildings,mechanics} native
  templates fed from WorldRules; world-aware resolution (session world preset + speed, classic
  fallback) + banner; speed-adjusted durations only. AC3 equality tests vs loaded TOMLs; AC4
  world-switch test with a speed-preset world. (AC3, AC4)
- [x] **T3 — Prose rework wave 1.** Getting started + Economy + Military sections rewritten per
  the voice rules; big tables → reference links; README regenerated (six sections). (AC6)
- [x] **T4 — Prose rework wave 2.** Expansion + Society + Reference-section prose (artifacts,
  wonder) rewritten; cross-link pass over the whole corpus; corpus render test green. (AC6)
- [x] **T5 — Review & accept.** Gates green; reviewer APPROVE (incl. fact spot-checks);
  statuses flipped; merged when Verified.

## Done when

All ACs pass with tests, gates green, reviewer APPROVE, merged once Verified.
