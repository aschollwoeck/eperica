# Tasks — 069 the village page (fortress plan)

Gated by `cargo fmt --all -- --check`, `clippy -D warnings`, `cargo test --workspace`, P11. Presentation only;
branch `feature/069-village-page`.

- [x] **T1 — Data.** `VillageTemplate` → `ribbon: ResourceRibbon` (drop the 10 flat fields) + `village_label`;
  `BuildRow` gains `res` (field resource slug, "" for buildings) + `page` (building leaf for the Enter link,
  "" if none). The village handler sets them (the ribbon from the economy it already loads). (AC1/AC2/AC4/AC5)
- [x] **T2 — Plan + chrome CSS.** Add to `base.css`: the command header (`.vlg-cmd`, dials, chips,
  quick-links), the fortress plan (`.vplan` rampart/towers/gate, `.vplot` plots **positioned by
  `.vplot--<kind>`**, ready/build/empty states, level badge), the fields grid (`.vfields`/`.vfield`), the
  inspector (`.vinspect`), and the rail (reuse `.bld-card`; feed rows). Responsive: the plan reflows to a grid
  on mobile (positions via `--x`/`--y` only applied on desktop). (AC1/AC3/AC4/AC5/AC6)
- [x] **T3 — Template.** Rewrite `village.html`: command header (+ alerts/switcher), ribbon, the fortress plan
  + inspector, the fields grid, and the war-room rail; keep every form/POST, the wonder-build, oasis recall,
  reinforcement send-back, and the countdown JS. (AC1–AC7)
- [x] **T4 — Inspector JS.** A click handler that fills the inspector from the selected plot's `data-*`
  (name, level→next, effect, cost, the build form's hidden table/slot/kind, the Enter link, gated state). Plus
  the countdown JS. (AC3/AC4/AC5)
- [x] **T5 — Tests.** Extend the village integration assertions for the new structure (command header, ribbon,
  the plan with building plots, the fields grid, the inspector, the rail) while keeping the existing behaviour
  assertions (build POST, garrison, oasis, switcher, protection). (AC1–AC7)
- [x] **T6 — Gate + reviewer + PR.** fmt/clippy/`cargo test --workspace` green; Playwright sweep (desktop +
  mobile; click-to-inspect; build action); `eperica-reviewer` → APPROVE; PR opened.
