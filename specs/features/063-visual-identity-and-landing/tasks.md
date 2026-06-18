# Tasks — 063 visual identity, landing & per-building art

Ordered, each gated by `cargo fmt --all -- --check`, `clippy -D warnings`, `cargo test --workspace`, and the
P11 budget. Presentation-layer; commit per task group on `feature/063-visual-identity-and-landing`.

- [x] **T1 — Grim Forged Steel theme + atmosphere.** `static/theme-steel.css` (gunmetal/blued-steel/
  tarnished-brass/ember palette); switch `base.html` to it (keep `theme-ash.css`); global vignette + grain
  overlays in `base.css`. (AC1)
- [x] **T2 — Header restyle.** `base.css` topbar → sticky flex bar (crest, themed search, muted nav,
  ember "Create account" CTA); add `base.html` `head`/`body_class`/`body_style` hooks + the crest SVG + the
  CTA + an SVG bell; fix the leaking empty `.badge[hidden]`. (AC4)
- [x] **T3 — Footer + legal pages.** Site footer in `base.html` (+ `base.css`); `impressum/privacy/terms`
  templates, structs, handlers, and public routes. (AC4)
- [x] **T4 — Landing hero.** `landing.css` + `index.html`: full-bleed editorial hero, the "Raid inbound"
  war-table (forged-plate + corner brackets + coordinate-map) with the vanilla countdown clock, pillars,
  closing CTA; responsive + reduced-motion. (AC2)
- [x] **T5 — Fortress skyline.** Inline-SVG ramparts + gatehouse, flickering torches, rising embers, cold
  rim-light + moon, warm uplight; **anchored to the centred content column** (no wide-width text overlap);
  glow `overflow: visible`; hidden on mobile. (AC2)
- [x] **T6 — Emotional copy + dispatches.** Battle-cry eyebrow/headline/sub/note, "A fair fight" pillar,
  "Raid inbound" label; the "Dispatches from the front" war-log strip. (AC2)
- [x] **T7 — Worlds on the landing + register-into-world.** `IndexTemplate.worlds` + `LandingWorldRow`;
  `index` lists open worlds; "Choose your battlefield" cards → `/register?world=`; `RegisterTemplate` +
  `RegisterQuery` + hidden field/note; `resolve_world_choice` + `route_after_register`; integration test
  (chosen world → `/w/<id>/village`; none → `/worlds`). (AC3)
- [x] **T8 — Per-building backgrounds.** `.has-building-bg` (image-under-scrim, graceful fallback, panel
  translucency) in `base.css`; `body_class`/`body_style` slug blocks on the 7 building templates;
  `static/buildings/` (README) + `docs/art/building-backgrounds.md` prompt sheet. (AC5)
- [x] **T9 — Cache freshness.** `static_cache_control` middleware → `Cache-Control: no-cache` on all
  responses. (AC6)
- [ ] **T10 — Gate + reviewer + PR.** Full workspace tests + Playwright sweep (widths + register + building
  pages + legal); `eperica-reviewer` → APPROVE; squash-merge.

> Implementation landed iteratively (live Playwright review); T1–T9 are complete and verified, T10 (reviewer
> gate + PR) is the remaining step. Tasks are recorded here to keep the spec→plan→tasks trail intact.
