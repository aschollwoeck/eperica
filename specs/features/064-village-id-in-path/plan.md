# Plan — 064 village id in the path

## Approach

Mirror the 056 world-in-path mechanics one level deeper. The `GameContext` extractor already reads `{world}`
arity-agnostically (`RawPathParams`), so adding a `{village}` segment to a route does **not** disturb it — the
handler just gains its own `Path<(String, String)>` (world, village). No extractor change needed.

### A. Helpers (`handlers.rs`)
- `village_seg(VillageId) -> String` = `Uuid::from_u128(id.0).to_string()` — the hyphenated id every
  village-coupled template carries (replaces the decimal `village.id.0.to_string()`).
- Repoint `selected_village` to parse a **hyphenated UUID** segment (`Uuid::parse_str → as_u128 → VillageId`)
  instead of decimal; absent/bad ⇒ `None` (capital default, unchanged P4 semantics).
- Replace `redirect_with_village`/`redirect_to_village` with `village_path(world, village, leaf)` →
  `/w/{world}/village/{village}{leaf}` (leaf `""` = overview, `"/academy"`, `"/barracks"`, …) and a
  `redirect_to_village_leaf(world, village, leaf)` returning the 302. Callers pass the path village they already
  hold.

### B. Routes (`lib.rs`, `world_router`)
Replace the flat village block with:
```
.route("/village", get(handlers::village_index))                 // → 302 to capital's path (AC4)
.route("/village/{village}", get(handlers::village))
.route("/village/{village}/build", post(handlers::build_submit))
.route("/village/{village}/academy", get(handlers::academy))
.route("/village/{village}/academy/research", post(handlers::research_submit))
.route("/village/{village}/smithy", get(handlers::smithy))
.route("/village/{village}/smithy/upgrade", post(handlers::smithy_upgrade_submit))
.route("/village/{village}/barracks", get(handlers::troops_barracks))
.route("/village/{village}/stable",   get(handlers::troops_stable))
.route("/village/{village}/workshop", get(handlers::troops_workshop))
.route("/village/{village}/train", post(handlers::train_submit))
.route("/village/{village}/rally", get(handlers::rally))
.route("/village/{village}/rally/send", post(handlers::rally_send))
.route("/village/{village}/rally/return", post(handlers::rally_return))
.route("/village/{village}/oasis/recall", post(handlers::oasis_recall))
.route("/village/{village}/market", get(handlers::market))
.route("/village/{village}/market/send", post(handlers::market_send))
```
The three training pages are **static** routes (not a `{building}` capture — that would conflict with the
static `academy`/`smithy`/… siblings in axum 0.8). Each is a thin wrapper calling a shared
`troops(ctx, village, BuildingKind)`.

### C. Handlers (`handlers.rs`)
Every village-coupled handler gains `Path((_world, village)): Path<(String, String)>` and reads the village
from it (`selected_village(Some(&village))`) instead of `Query<VillageQuery>` / `form.village`:
- GET: `village`, `academy`, `smithy`, `rally`, `market` (rally/market keep their `Query` for `x`/`y`).
- POST: `build_submit`, `research_submit`, `smithy_upgrade_submit`, `train_submit`, `rally_send`,
  `rally_return`, `oasis_recall`, `market_send` — drop the `village` field from `BuildForm`/`UnitForm`/
  `TrainForm`/`RallyReturnForm`/`OasisRecallForm`/market `SendForm` and the `rally_send` HashMap read; the PRG
  redirect targets `village_path(world, &village, leaf)`.
- New `village_index(ctx)` → load capital/first village, 302 to `village_path(world, &village_seg(id), "")`.
- New `troops_barracks/stable/workshop` wrappers; `troops` loses its slug `Path` and takes
  `building: BuildingKind`.
- `VillageQuery` is removed where it only carried `village`; `MapQuery` keeps `x`/`y` but loses `village`.

### D. Templates (the 7 village-coupled)
- `village_id` field now holds the **UUID** (`village_seg`). Switch row ids (`VillageSwitchRow.id`) too.
- Rewrite every `…?village={{ village_id }}` link to `/w/{{ world }}/village/{{ village_id }}/<leaf>`; the
  overview/switcher to `/w/{{ world }}/village/{{ v.id }}`.
- Form `action`s become `/w/{{ world }}/village/{{ village_id }}/<leaf>`; **delete the hidden
  `<input name="village">`** in each (build/research/train/rally/market/oasis forms).
- The village page's troop-building links: build `/w/{{ world }}/village/{{ village_id }}/{{ link.1 }}`
  where `link.1` is now the bare leaf (`barracks`/`stable`/`workshop`).

### E. Tests (`integration.rs`)
Migrate the ~53 references: GET `/village` → `/village/{uuid}` (derive the uuid from the seeded village),
training pages `…/village/troops/barracks` → `…/village/{uuid}/barracks`, POST bodies drop `village=…` (now in
the path). Add assertions: `/w/{world}/village` (no id) 302s to the capital path; a bad `{village}` falls back
to the capital; no response body contains `?village=`.

## Tasks (ordered; each gated: `fmt`/`clippy -D warnings`/`cargo test --workspace`/P11)
1. **Helpers** — `village_seg`, UUID-parsing `selected_village`, `village_path` + redirect helper.
2. **Routes + `village_index` + troop wrappers** — restructure `world_router`; new entry/redirect handler.
3. **GET handlers** — `village`/`academy`/`smithy`/`rally`/`market`/`troops` read village from the path.
4. **POST handlers + forms** — path village; drop `village` form fields; PRG → `village_path`.
5. **Templates** — UUID `village_id`; path links + form actions; drop hidden inputs.
6. **Tests** — migrate refs; add canonical-entry / fallback / no-`?village=` assertions.
7. **Gate + reviewer + PR.**

## Verification
- `cargo test --workspace` green (esp. the migrated integration tests + the new routing assertions).
- **Playwright:** log in → nav "Village" → URL is `/w/{world}/village/{uuid}`; open Barracks → URL is
  `/w/{world}/village/{uuid}/barracks` (200); train a unit → PRG back to that path; switch villages → uuid in
  the path changes; `/w/{world}/village` redirects to the capital; no `?village=` in any address.

## Key risks
- **axum arity** — every village route now has 2 path params (world, village); handler `Path` tuples must
  match or it fails at **runtime**. Audit each. The three troop pages are static (no `{building}` capture) to
  avoid a route overlap panic.
- **Form vs path** — POST handlers must read the path village, not the (now-removed) form field; a stale
  `form.village` read would not compile (field removed) — good, compile-time caught.
- **Test breadth** — every `?village=` and `/village/troops/` in the suite must move; a missed one 404s.
