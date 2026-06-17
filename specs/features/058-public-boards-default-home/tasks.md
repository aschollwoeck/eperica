# Tasks — 058 public boards default to home

- [x] **T1** — `redirect_home_leaderboard`/`redirect_home_wonder` handlers (→ `/w/{home}/…`); router points
  bare `/leaderboard` `/wonder` at them (game `/village` `/map` keep `redirect_to_lobby`); base.html public
  nav links use `data-wl-public` (no-world → bare leaf → home). Test
  `bare_public_boards_default_to_home_world` + update the 056 `bare_routes_…` test. Full gate + reviewer.

Gates: `fmt --check`, `clippy -D warnings`, `cargo test --workspace`, P11.
