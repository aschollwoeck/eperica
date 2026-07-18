# Plan — 129 tribe themes

**Status:** Draft (spec approved)

## Constitution check

Cosmetic only: no domain/application change; the one server-side touch is `/me` exposing the
selected world's tribe (data the player already sees everywhere). P4 unaffected (theming is
client cosmetics); P11 unaffected (CSS + one cached probe already made for the nav).

## Design system (the design-lead pass — implementation executes this, not its own taste)

Discipline: **one bold signature per theme (the primary-action button); everything else is quiet
token work.** All motion `prefers-reduced-motion`-gated; only transform/opacity/filter animate.

### Romans — "Aged Triumph" (the painted ground — the operator supplied a reference painting, static/roman_back_style.png, which IS the page background; every prior round was an approximation of it)

| Token | Value | Note |
|---|---|---|
| ground | `static/roman_back_style.png` (avg `#c4a77f`) | the painting, cover/fixed — warm sandy haze with light sweeping through dust; a soft vignette + whisper of grain knit the UI to it |
| panel | translucent sand `rgba(230,212,172,.78)` + backdrop blur (opaque fallback `#d6c095`) | the painting glows through every surface |
| border | `#8a7148` | worn bronze |
| accent | `#4f3d16` | dark bronze — at this depth gold no longer reads as text; ornament stays golden in spirit |
| secondary | `#9a4a32` | terracotta-brick — the cape |
| text | `#2b2112` / muted `#453723` | near-black sepia ink |
| radius | 2px | crisp imperial edges |

The one LIGHT theme of the three (Teutons/Gauls stay dark) — components that hardcode the dark
chrome (`.gauge`, `.vquick a`, `.vchip`, `.vplan`/`.vcanvas`) carry explicit parchment overrides.
Signature button: **cape swing** — terracotta drape, unchanged mechanics. Header ornament: laurel
sprig in deep gold. Extra touches per the checklist, re-based on the light ground.

### Teutons — "Iron & Ember"

| Token | Value | Note |
|---|---|---|
| ground / panel | `#191b1e` / `#22262a` | cold steel |
| panel highlight | `#2b3036` | worn metal sheen |
| border | `#3d444c`, 2px | heavier, blunt |
| accent | `#d9622b` | ember |
| glow | `#ff7a33` | hover-only, via shadow/filter |
| text | `#cfd6dd` | cold light |
| radius | 0–2px | blunt, forged |

Signature button: **forge ember** — a smoldering inner edge (inset shadow in ember) that
breathes on hover (two-step opacity/filter pulse, like coals catching draft). Header ornament:
twin rune strokes (SVG data-URI). Extra touches: table-row hover in ember at 7% alpha; nav
active = thick blunt ember rule; countdowns ember; status-strip borders steel with ember left
edge.

### Gauls — "Moss & Bronze"

| Token | Value | Note |
|---|---|---|
| ground / panel | `#16201a` / `#1e2b22` | deep forest |
| panel highlight | `#27392c` | moss light |
| border | `#3a4f3f` | mossy |
| accent | `#b08d3f` | bronze |
| secondary | `#6fae6a` | leaf green, sparingly |
| text | `#dce8dc` | misty light |
| radius | 8–10px | rounded, organic |

Signature button: **leaf unfurl** — a bronze→green gradient sweep unfurls across the button
(translateX overlay with a soft curved leading edge) on hover. Header ornament: a knot loop
(SVG data-URI). Extra touches: table-row hover in leaf green at 7% alpha; nav active = soft
rounded bronze rule; countdowns bronze; status-strip cards fully rounded with moss borders.

## Mechanism

- **Server:** `/me` (the web JSON probe, not the agent API) gains the selected world's `tribe`
  (slug) when a world is selected. One field.
- **Client (base.html):** the existing `/me` probe also sets
  `document.documentElement.dataset.theme = tribe` **iff** `location.pathname` starts with `/w/`
  (world-scoped pages only), else removes it; caches the last value in `localStorage`
  (`eperica-theme`) and applies it via an inline pre-paint snippet on world paths to avoid a
  flash. Logout/world-switch correctness comes free from the probe running per page.
- **CSS (base.css):** three `[data-theme="…"]` blocks overriding the existing custom properties
  + the bounded component rules above. Neutral (no attribute) untouched. Selector discipline:
  every themed rule is `[data-theme="x"] .component` — same specificity plane, no cancellations.

## Test strategy

- Integration: `/me` carries `tribe` with a world selected (and not without); base.html contains
  the theme script with the `/w/` gate; a world page render includes the pre-paint hook.
- Static CSS pins: three `[data-theme]` blocks exist, each defines the ground+accent tokens and
  a `prefers-reduced-motion` guard; the signature-button rules exist per tribe.
- Full suite green (AC5); visual acceptance: operator screenshots per tribe (Botland bots cover
  all three tribes — log in as each via dev password or screenshot bot villages via spectator).

## Risks

- **Taste** — mitigated by this pinned token system + operator screenshot review before the
  formal review.
- **Contrast regressions** — each text/ground pair above clears WCAG AA on paper; reviewer
  spot-checks computed pairs.
- **Flash of neutral theme** — the localStorage pre-paint snippet; acceptable residual: first
  visit ever on a world page.
