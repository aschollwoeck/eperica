# Feature 115 — tasks

Styles-only pass + one read-only tribe endpoint. Gates each task: `cargo fmt`, `clippy -D warnings`,
`cargo test --workspace`. CSS is served from disk; template/Rust changes need a rebuild.

- [x] **T1 — War-banner primary button.** `.btn--primary` restyle + `static/brush.svg` turbulence filter;
  hover wipe + ember sparks; per-tribe palette via `[data-tribe]`; disabled state. (`base.css`) (AC1/AC3)
- [x] **T2 — Tribe theming source.** `GET /me` carries the account tribe; add `GET /w/{world}/me`
  (`world_me`) for the per-world tribe; `base.html` sets `data-tribe` (account → world override in a world).
  (`handlers.rs`, `lib.rs`, `base.html`) (AC2)
- [x] **T3 — Training-row layout fix.** Stop the shared `.unit` grid collapsing its info column on long unit
  names (min-width floor + capped action column). (`base.css`)
- [x] **T4 — Mobile pass.** Hamburger topbar (`base.html` + `base.css`); slimmer buttons; batch-total nbsp
  (`troops.html`); village-plan grid fix; rail/aside stack above plan/units; building-hero alignment;
  world-selection `.table--cards` reflow (`worlds.html` + `base.css`). (AC4/AC5)
- [x] **T5 — Verify.** Render tests pass; `fmt`/`clippy` clean; live-checked on desktop + phone widths (no
  overflow; hamburger; green/blue buttons per tribe).
