# Plan — 063 visual identity, landing & per-building art

## Approach

A presentation-layer slice. The design system is already token-driven (`static/base.css` holds
theme-agnostic structure + components; a `static/theme-*.css` defines the `--c-*` colour tokens), so the new
look is a new **theme file** plus structural CSS, and the landing is a new `landing.css` + an Askama
template. Reusable hooks are added to `base.html` so any page can inject page CSS, a body class, and inline
body style. Only one path touches the backend: the landing worlds list + routing a registration into its
chosen world.

### Art direction — "Grim Forged Steel"
Cold gunmetal base (`#090C10`), iron panels, **blued-steel** accent for actions, **tarnished brass** for
hairlines/wordmark/numerals, **ember/torch** warmth as the only hot colour, dried-blood danger. A global
**vignette** + fine **grain** (fixed, pointer-transparent overlays). "Forge at night / cold iron in the
dark." `theme-ash.css` (the original warm theme) stays for A/B.

### Hero thesis (the signature)
The game's core idea — *sub-second timing is gameplay* — is shown, not told: a live **"Raid inbound"
war-table** with a ticking centisecond impact clock + a coordinate-map fragment, and a **fortress skyline**
(inline SVG) with flickering torches, rising embers and a cold moon. One bold element; everything else quiet.
Copy is emotional (battle-cry), with the technical promises demoted to fine print.

### Reusable base.html hooks
- `{% block head %}` — per-page stylesheet (`landing.css`).
- `{% block body_class %}` / `{% block body_style %}` — page-scoped class + inline CSS vars (the landing's
  `lp`, and the building pages' `has-building-bg` + `--building-img`).

### Worlds-on-landing → register-into-world
`index` lists open worlds (`list_worlds`, filter `won_ms.is_none()`); each links to `/register?world=<uuid>`.
`register_form` resolves the choice to (validated id, name) and preselects it; `register_submit` keeps it
through error re-renders and, on success, calls `route_after_register` — home world ⇒ straight to its
village (registration already made the player); another world ⇒ reuse the 045 join primitive
(`context_for` + `create_player_in_world` with the registered tribe) then redirect there; no/invalid ⇒ lobby.

### Per-building background
`has-building-bg` lays the building image **under** a scrim gradient + the theme base colour, with panels
slightly translucent. A missing `/static/buildings/<slug>.webp` simply paints nothing → scrim + theme show
through (graceful fallback). Slugs match the game's `BuildingKind` slugs; the troops page resolves
`{{ building|lower }}` (barracks/stable/workshop).

### Cache freshness
A small `static_cache_control` middleware sets `Cache-Control: no-cache` on every response (the app is
dynamic and `ServeDir` sent no caching header, so browsers heuristically cached stale CSS/HTML).

## Files

- **`static/theme-steel.css`** (new) — the Grim Forged Steel token set.
- **`static/base.css`** — header (sticky flex bar, crest, themed search, CTA, `.badge[hidden]` fix), footer,
  legal prose, vignette/grain overlays, `.has-building-bg` background + panel translucency.
- **`static/landing.css`** (new) — hero, war-table (forged-plate + brackets), skyline/torches/embers/moon
  (anchored to the content column; glow `overflow: visible`), pillars, dispatches feed, worlds cards, footer.
- **`templates/base.html`** — `head`/`body_class`/`body_style` blocks; crest + CTA + SVG bell; site footer.
- **`templates/index.html`** — hero + war-table + skyline SVG + countdown JS; worlds, dispatches, pillars,
  finale.
- **`templates/register.html`** — hidden `world` field + "Enlisting in …" note.
- **`templates/{impressum,privacy,terms}.html`** (new) — legal scaffolds.
- **`templates/{village,academy,smithy,rally,market,troops,wonder}.html`** — `has-building-bg` + slug.
- **`src/templates.rs`** — `IndexTemplate{worlds}`, `LandingWorldRow`, `RegisterTemplate{world,world_name}`,
  `Impressum/Privacy/Terms` templates.
- **`src/handlers.rs`** — `index` (worlds), `register_form`/`register_submit` (+ `RegisterQuery`,
  `resolve_world_choice`, `route_after_register`), `impressum`/`privacy`/`terms`.
- **`src/lib.rs`** — legal routes; `static_cache_control` middleware.
- **`docs/art/building-backgrounds.md`** (new) + **`static/buildings/README.md`** (new).

## Key decisions & risks

- **One slice, presentation-only.** Large but cohesive; reviewable in ordered commits (theme → header/footer
  → landing → copy/dispatches → worlds flow → building bg → cache).
- **Worlds anchored to the centred content column** (skyline + torches) so they never drift under text at
  wide widths — the fix for the >1350px regression.
- **Graceful image fallback by CSS layering** (image under scrim) — no server-side existence check on the
  hot path; the trade-off is a harmless console 404 until art lands.
- **Risk: page weight** from building art → WebP + size budget; **animation** → reduced-motion guard;
  **stale cache** → the `no-cache` middleware (the bug that masked earlier visual fixes).

## Verification

`cargo build`/`fmt`/`clippy`/`cargo test --workspace` green; an integration test for the new
register-into-world routing (chosen world → `/w/<id>/village`; no world → `/worlds`); Playwright sweep of the
landing across widths (1280–2560 + mobile), the register flow, a building page (image + fallback), and the
legal pages; `eperica-reviewer` → APPROVE; PR.
