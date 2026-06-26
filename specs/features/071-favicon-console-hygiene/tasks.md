# Tasks — 071 UI fixes: console hygiene + always-show research cost

Gated by `cargo fmt --all -- --check`, `clippy -D warnings`, `cargo test --workspace`, P11. Presentation only;
branch `feature/071-favicon-console-hygiene`.

- [x] **T1 — Favicon.** Add `static/favicon.svg` (brand shield) + a `<link rel="icon">` in `base.html` and the
  standalone `styleguide.html`. (AC1)
- [x] **T2 — Autocomplete.** Add `autocomplete` to the login (`username`/`current-password`) and register
  (`username`/`email`/`new-password`) inputs. (AC2)
- [x] **T3 — Academy cost.** Restructure `academy.html` so the cost + time show for every non-researched unit,
  with the Research action vs. gate reason nested below. (AC3)
- [x] **T4 — Art fallback.** An `art_blank_on_missing` middleware turns a 404 under `/static/buildings/` or
  `/static/units/` into a transparent 200 SVG; non-art 404s pass through. (AC4)
- [x] **T5 — Tests.** Assert the favicon link + login autocomplete; a missing art path → 200, a missing
  non-art path → 404; a gated Academy unit still shows its cost. (AC1–AC4)
- [ ] **T6 — Gate + reviewer + PR.** fmt/clippy/`cargo test --workspace` green; Playwright (clean console on
  login + academy; Academy gated unit shows cost); `eperica-reviewer` → APPROVE; PR opened.
