# Tasks — 056 URL-based world routing

- [ ] **T1** — World `name` column (migration 0047 + backfill home); thread through World/create_world/port/
  repo/use-case/CreateWorldForm/admin.html + lobby display. Mirror 052. Independently shippable.
- [ ] **T2** — Extractors read world from path (`world_from_path` via `RawPathParams`); GameContext/WorldScope
  miss → `/worlds`; drop cookie reads; 2-field `Path` structs on the 7 two-capture handlers.
- [ ] **T3** — Router: `world_router()` + `.nest("/w/{world}",…)`; drop `/world/select`; bare→`/worlds` stubs.
- [ ] **T4** — `redirect_with_village`/`redirect_to_village` + `world_path` take the world; 13 callers +
  `join_world` → `/w/{uuid}/village`.
- [ ] **T5** — 33 direct redirects by category (world-coupled→world_path; account unchanged; auth→`/worlds`).
- [ ] **T6** — `world` field on the ~19 world-scoped template structs + link rewrite; account templates→`/worlds`.
- [ ] **T7** — `world` in `/me` JSON + base.html nav JS; last-visited cookie middleware on `world_router()`.
- [ ] **T8** — Test sweep + new routing tests; full gate + reviewer + PR.

Gates per task: `fmt --check`, `clippy -D warnings`, `cargo test --workspace`, P11.
