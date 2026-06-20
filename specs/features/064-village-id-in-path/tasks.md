# Tasks — 064 village id in the path

Ordered; each gated by `cargo fmt --all -- --check`, `clippy -D warnings`, `cargo test --workspace`, and the
P11 budget. Routing/presentation only; branch `feature/064-village-id-in-path`.

- [x] **T1 — Helpers.** `village_seg(VillageId) -> String` (hyphenated UUID); repoint `selected_village` to
  parse a UUID segment; add `village_path(world, village, leaf)` + `redirect_to_village_leaf`; drop the old
  `redirect_with_village`/`redirect_to_village`. (AC1/AC4)
- [x] **T2 — Routes + entry + troop wrappers.** Restructure `world_router`'s village block to
  `/village/{village}/…`; static `barracks`/`stable`/`workshop` routes; `village_index` (no-id → 302 capital);
  `troops_barracks/stable/workshop` wrappers calling a shared `troops(ctx, village, BuildingKind)`. (AC1/AC4)
- [x] **T3 — GET handlers.** `village`/`academy`/`smithy`/`rally`/`market`/`troops` read the village from
  `Path<(String, String)>` (rally/market keep `Query` for `x`/`y`, minus `village`). (AC1)
- [x] **T4 — POST handlers + forms.** `build`/`research`/`smithy_upgrade`/`train`/`rally_send`/`rally_return`/
  `oasis_recall`/`market_send` take the path village; drop the `village` field from their form structs/reads;
  PRG → `village_path`. (AC2/AC3)
- [x] **T5 — Templates.** UUID `village_id` (+ switch-row ids); rewrite links + form actions to
  `/w/{world}/village/{village}/<leaf>`; delete hidden `village` inputs. (AC3/AC5)
- [x] **T6 — Tests.** Migrate the ~53 refs (GET paths, training pages, POST bodies); add canonical-entry
  (no-id → capital 302), bad-id fallback, and no-`?village=` assertions. (AC1–AC4)
- [x] **T7 — Gate + reviewer + PR.** Full workspace tests green; Playwright sweep (nav → village → barracks →
  train → switch → no-id redirect); `eperica-reviewer` → APPROVE; PR opened.
