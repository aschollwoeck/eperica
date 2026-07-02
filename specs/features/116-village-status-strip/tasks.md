# Feature 116 — tasks

Presentation-only slice (no domain rules, no writes). Gates each task: `cargo fmt`, `clippy -D warnings`,
`cargo test --workspace`.

- [x] **T1 — Template model.** `IncomingRow { arrive_ms }` (arrival only — no coordinate/troops, P4/§7.3),
  `VillageTrainingRow { label, remaining, complete_ms }`, and `VillageTemplate.{cp_pct, incoming, training}`;
  `TroopsTemplate.active_portrait`. (`crates/web/src/templates.rs`)
- [x] **T2 — Village handler.** Fetch `incoming_against(&[village.id])` and `active_training(village.id)`,
  build the rows (arrival-only for attacks; `saturating_sub` for remaining), and compute `cp_pct` (0–100,
  clamped) from the already-loaded 013 culture. Best-effort: failures degrade to empty, never a 500 (AC5).
  (`crates/web/src/handlers.rs`, village handler)
- [x] **T3 — Strip markup + styles.** The three-card `.vtop` strip above the plan in `village.html`; the
  Attacks card flags alert when non-empty; Culture bar uses `cp_pct`. CSS in `base.css`. (AC1–AC4)
- [x] **T4 — Drill-yard portrait.** Compute `active_portrait` (`<tribe>_<unit>`) in the troops handler and
  render it on the "On the drill yard" card, mirroring the Smithy. (`handlers.rs`, `troops.html`) (AC6)
- [x] **T5 — Verify.** Village render tests pass with empty strip data; live-checked with an active training
  batch (Training card + drill-yard portrait) and a level-full culture bar. `fmt`/`clippy` clean.
