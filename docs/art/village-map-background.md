# Village-page background art — AI image prompt

The redesigned village page (the "fortress plan" — see the mockup work) renders the buildings and resource
fields as interactive plots over a **painted top-down terrain plate**. This is the prompt for that ground
image. It plugs into the page via a `--plan-bg` CSS variable; the interface draws the rampart and the building
/ field plots **on top**, so the art is terrain only.

Same world as the rest of the art (`building-backgrounds.md`, slices 063 / 065): grim "Forged Steel" —
cold gunmetal night, torch/ember warmth, heavy vignette. Kept **dark and low-key** so the overlaid plaques and
text stay legible.

## How to use

1. Full prompt = **Subject** + **Style** below; add the **Negative prompt** on tools that take one.
2. Save the result as `crates/web/static/buildings/village-ground.webp` (or wire whatever path into the
   page's `--plan-bg`).
3. Keep the **centre and edges shadowed / uncluttered** — the keep, plots and inspector sit over them.

## Technical specs

- **16:9**, ≥ 1920×1080. Export **WebP**, aim **< ~400 KB** (page-weight budget, P11).
- Very dark, desaturated, low-key. Top-down **orthographic** (flat map view) — not isometric, not angled.
- Midjourney params: `--ar 16:9 --style raw --stylize 250`.

## Subject

> A top-down (bird's-eye, orthographic) plate of the ground of a grim early-medieval war-village, as a
> game-map background. The packed-earth inner compound of a walled hold: worn dirt paths radiating from a
> central muster yard, a stone well, churned mud, cart ruts, scattered timber, hay-bales and barrels, a few
> bare trees. Ringed by darker outlying land — tilled field furrows, a muddy track, the black edge of a pine
> forest, a thin stream catching cold light. **Empty of buildings** (they are placed on top by the interface)
> — only terrain, yard, paths and fields.

## Style block

> *— grim dark-fantasy medieval concept art, top-down orthographic map view, cinematic night. Forged-steel
> palette: near-black soot and cold earth, deep brown mud and dead-grass ochre, lit only by faint cold
> moonlight and the warm amber of distant torch-fire and embers; muted, desaturated, very dark and low-key.
> Weathered, war-torn, oppressive; volumetric ground-haze and drifting smoke. Painterly, highly detailed
> terrain, heavy vignette, large areas of shadow so overlaid UI stays legible. No buildings, no text, no
> people.*

## Negative prompt

> *text, words, UI, buildings, structures, people, faces, bright daylight, blue sky, vivid saturated colours,
> isometric or angled perspective, cartoon, anime, cute, low detail, blurry, washed out, overexposed.*

## Variant — full painted village (optional)

If you'd rather the art **include** the painted rampart and surrounding fields (a complete top-down village),
drop "Empty of buildings…" from the subject and add: *"a stone curtain wall with round corner towers and a
timber gatehouse enclosing the compound; tilled resource fields and a forest in the outer land."* The
interface would then **omit its own CSS rampart** and draw only the small interactive plot markers on top —
richer, but it ties the art to fixed plot positions. The terrain-only version above is the flexible default.
