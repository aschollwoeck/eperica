# Tasks — 065 tribe-specific building backgrounds

Gated by `cargo fmt --all -- --check`, `clippy -D warnings`, `cargo test --workspace`, P11. Presentation only.

- [x] **T1 — CSS layer.** `.has-building-bg` gains a `var(--building-img-tribe, none)` plate **above** the
  neutral `var(--building-img, none)` (so a missing tribe plate 404s to transparent → neutral shows). (AC1/AC2)
- [x] **T2 — Tribe slug to the templates.** `tribe_slug: &'static str` on the six building templates
  (Village/Academy/Smithy/Troops/Rally/Market), set in each handler from `village.tribe.map_or("", |t|
  t.slug())`; each `body_style` conditionally emits `--building-img-tribe: …/<tribe>_<slug>.webp` when
  `tribe_slug` is non-empty. Wonder stays neutral. (AC1/AC3)
- [x] **T3 — Art prompts + drop-in docs.** Per-tribe architecture descriptors (Roman stone / Teuton timber /
  Gaul oak) + a tribe-specific assembled example in `docs/art/building-backgrounds.md`; `static/buildings/`
  README documents the `<tribe>_<slug>.webp` drop-in + fallback. (AC2)
- [x] **T4 — Gate + reviewer + PR.** fmt/clippy/`cargo test --workspace` green; live check (a Teuton village
  page emits both `--building-img` and `--building-img-tribe`); reviewer → APPROVE; PR opened.

> Art assets are generated separately from the prompt sheet; until a `<tribe>_<slug>.webp` is dropped in, every
> building shows the neutral 063 plate (AC2 fallback) — so this slice ships dark with no missing-image risk.
