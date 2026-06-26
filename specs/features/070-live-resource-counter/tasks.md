# Tasks — 070 live-ticking resource counters

Gated by `cargo fmt --all -- --check`, `clippy -D warnings`, `cargo test --workspace`, P11. Presentation only
(one shared partial); branch `feature/070-live-resource-counter`.

- [x] **T1 — Ribbon partial.** In `_ribbon.html`: add `data-rate` to each gauge, wrap the amount number in a
  `.gauge__now` span, and replace the static fill script with a 1 Hz tick that estimates `amount + rate ×
  hours-since-load`, clamps to `[0, cap]`, counts crop down on a negative rate, and updates both the number and
  the fill width. Plain integers (no separators) so the value matches the server snapshot format. (AC1–AC4)
- [x] **T2 — Test.** Assert the ribbon now carries `data-rate` + a `.gauge__now` element (alongside the
  existing `res-ribbon`/`data-amt`), on a page that includes it. (AC1)
- [x] **T3 — Gate + reviewer + PR.** fmt/clippy/`cargo test --workspace` green; Playwright check (numbers tick
  up over a few seconds, crop ticks down when starving, fill grows, reload re-syncs); `eperica-reviewer` →
  APPROVE; PR opened.
