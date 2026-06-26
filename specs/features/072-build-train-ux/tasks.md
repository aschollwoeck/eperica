# Tasks — 072 build/train UX: explicit gate reason + train-to-max

Gated by `cargo fmt --all -- --check`, `clippy -D warnings`, `cargo test --workspace`, P11. Presentation only;
branch `feature/072-build-train-ux`.

- [x] **T1 — Gate reason.** `BuildRow` gains `gate: String`; `make_row` derives it (busy lane outranks the
  per-resource shortfall); the field-cap override clears it. (AC1)
- [x] **T2 — Village inspector.** `village.html` building + field plots carry `data-gate`; the inspector JS
  shows it (max → "Max level reached", else the explicit reason) instead of the generic hint. (AC1)
- [x] **T3 — Train max.** `troops.html` adds a **Max** button per training row + JS that computes the largest
  affordable count from the live ribbon values and refreshes the batch-total. (AC2)
- [x] **T4 — Tests.** A new `village_plan_names_the_build_gate` (0 resources → `data-gate="Need …"`, old
  generic hint gone); `training_flow_and_garrison` asserts the Max button. (AC1/AC2)
- [ ] **T5 — Gate + reviewer + PR.** fmt/clippy/`cargo test --workspace` green; Playwright (gated plot names
  the shortfall; Max sets the affordable count + batch-total); `eperica-reviewer` → APPROVE; PR opened.
