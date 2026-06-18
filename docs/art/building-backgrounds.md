# Building background art — AI image prompts

One image per building, shown as a **darkened background** behind that building's page, in the landing
page's "Grim Forged Steel" look (cold gunmetal night, torch/ember warmth, tarnished brass, heavy vignette).

## How to use

1. For each building below, the full prompt is **`<subject>` + the [Style block](#style-block)**. Use the
   [Negative prompt](#negative-prompt) on tools that take one (Stable Diffusion, etc.).
2. Save each result as `crates/web/static/buildings/<slug>.webp` — the `<slug>` shown matches the game's
   building slug, so the page can resolve `/static/buildings/<slug>.webp` automatically.
3. These sit **under page text**, so keep them dark and low-key. The page also lays a black 55–70 % scrim +
   vignette over them in CSS, so a little extra darkness in the art is good, not bad.

## Technical specs

- **16:9**, ≥ 1600×900 (1920×1080 ideal). Export **WebP**, aim for **< ~250 KB** each (page-weight budget, P11).
- Very dark / low-key, desaturated. The recognisable structure should read as a moody silhouette/establishing
  shot with large shadow areas and empty sky for the UI to overlay.
- Midjourney params: `--ar 16:9 --style raw --stylize 250`

## Style block

> *— grim dark-fantasy medieval concept art, cinematic night establishing shot. Forged-steel palette:
> near-black gunmetal and cold iron, deep blue-grey shadow, lit only by flickering torchlight and
> forge-embers (warm amber-orange) with a faint cold moonlight rim; tarnished brass accents. Heavy
> chiaroscuro, volumetric haze and drifting smoke, soot, weathered stone, iron and aged timber. Muted,
> desaturated, low overall brightness, strong vignette, deep atmospheric depth. Framed as a darkened
> background plate — the structure set back with large areas of shadow and empty sky for text overlay,
> wide 16:9. Painterly, highly detailed, oppressive, war-torn, no people in the foreground.*

## Negative prompt

> *text, words, letters, logo, watermark, UI, frame, border, signature, people in foreground, faces,
> bright daylight, blue sky, vivid saturated colours, cheerful, cartoon, anime, cute, modern objects,
> cars, low detail, blurry, washed out, overexposed.*

---

## Resource fields

- **Woodcutter** (`woodcutter`) — A lumber camp at the edge of a black pine forest: felled trunks and
  stacked timber, a sawpit and axes buried in stumps, a crude log-frame lodge, woodsmoke and cold mist
  between the trees, a single guttering torch.
- **Clay pit** (`clay_pit`) — A terraced clay pit gouged into a hillside: wet grey-brown clay, wooden
  scaffolds and ladders, mud-caked buckets and a hand-winch, standing pools reflecting torchlight, cold fog
  clinging to the excavation.
- **Iron mine** (`iron_mine`) — The timber-braced mouth of an iron mine cut into a dark crag: ore carts on
  wooden rails, a brazier glowing at the entrance, rusted picks and chains, ember-light bleeding from the
  tunnel, slag heaps and cold smoke.
- **Cropland** (`cropland`) — Dark farmland under a cold moon: rows of grain and bound sheaves, a leaning
  scarecrow, a timber granary silhouetted on the horizon, low ground-mist, a lone torch on a fence post,
  circling crows.

## Buildings

- **Main Building** (`main_building`) — The village's main hall and works yard: a stout stone-and-timber
  great hall with a watch-tower and a banner, torches flanking heavy iron-bound doors, timber scaffolding
  and stacked stone of ongoing construction — the beating heart of a war-village at night.
- **Rally Point** (`rally_point`) — A muster yard and war camp: a raised wooden command post hung with a
  tattered war-banner, racked spears and shields, churned mud tracked by marching boots, campfires and
  torches, ranks of soldiers as shadows in the haze.
- **Warehouse** (`warehouse`) — A fortified storehouse: a long iron-banded timber building with massive
  barred doors, stacked crates, barrels and sacks under a low torch-lit interior glow, a hooded guard,
  frost on the stone.
- **Granary** (`granary`) — A tall timber grain store raised on stone stilts: sacks of grain and bound
  straw, a loading hoist, scampering rats, a cold draught and lantern light, the silo dark against a
  bruised night sky.
- **Marketplace** (`marketplace`) — A night market square: canvas-covered stalls, merchant scales and
  crates of goods, a laden cart and tethered ox, swinging lanterns and braziers, coins and cold fog, the
  cobbles wet and gleaming.
- **Embassy** (`embassy`) — An austere diplomatic hall, grander than its neighbours: alliance banners and
  heraldic shields above a sealed studded door, braziers either side, a standard planted in the courtyard,
  cold mist and long shadows.
- **Wall** (`wall`) — The village's defensive wall and gatehouse: a massive crenellated stone rampart with
  a barred portcullis and flanking towers, torches burning along the battlements, arrow-slits and iron,
  sentries silhouetted, smoke drifting. (Echo the fortress + torches of the title screen.)
- **Barracks** (`barracks`) — A soldiers' barracks and drill yard: racks of swords, axes and shields, straw
  practice dummies hacked to splinters, armour on stands, a muddy training ground, torches and a forge-glow
  from a side door.
- **Academy** (`academy`) — A war academy and map-room: a strategist's table strewn with charts and weapon
  schematics, shelves of scrolls, a quill and dividers, austere stone walls, candle and lamplight pooling
  in the dark.
- **Smithy** (`smithy`) — A blacksmith's forge roaring in the dark: a great stone hearth blasting amber
  light, an anvil with glowing iron and flying sparks, racked hammers, tongs and half-forged blades, smoke
  and soot — the warmest, most fire-lit of all (the literal heart of "forged steel").
- **Stable** (`stable`) — A war-horse stable: rows of heavy timber stalls, armoured destriers shifting in
  shadow, hay, tack and horse-barding on hooks, breath steaming in the cold, a single swaying lantern.
- **Workshop** (`workshop`) — A siege workshop: half-built catapults, ballistae and a battering ram in
  timber frames, coils of rope, sawn beams and iron fittings, a hand-crane and sawdust, torch-lit, menace
  in the gloom.
- **Residence** (`residence`) — A lord's fortified residence wing: a stone manor with a warm hearth glimpsed
  through a narrow window, banners, a planning table and maps for new settlements, retainers in shadow,
  braziers at the steps.
- **Cranny** (`cranny`) — A hidden cache below the village: a concealed cellar reached by a trapdoor, sacks
  and small chests of resources tucked in the dark, a single guttering candle, cobwebs and damp stone,
  secrecy and shadow.
- **Outpost** (`outpost`) — A frontier outpost on a captured oasis: a wooden watchtower and palisade beside
  dark reed-fringed water in the wilds, a signal-beacon brazier, a lone sentry, mist over the pool, the
  wilderness pressing close.
- **Town Hall** (`town_hall`) — A great civic hall where the village celebrates its triumphs: a large stone
  hall with a raised dais and long banners, braziers and the embers of festivities, trophies of war on the
  walls, grander and prouder, smoke and torchlight.
- **Palace** (`palace`) — The capital's palace, the grandest and most imposing structure: a towering royal
  keep and throne-hall with tarnished brass and gold accents, sweeping banners, armoured guards at the
  gate, commanding the village from on high — cold and magnificent in the night.
- **Treasury** (`treasury`) — A guarded vault holding a captured artifact: a heavy stone treasury, an
  ancient relic on a pedestal radiating an eerie glow (cold blue or ember), iron-bound chests and chains,
  runes and dread, deep shadow and a single shaft of light.
- **Wonder** (`wonder`) — The Wonder of the World under construction: a colossal cathedral-scale monument
  rising amid immense timber scaffolding and cranes, hundreds of tiny torch-lights of labourers, an
  awe-and-dread silhouette against the night sky at a conquered Natar site — the endgame of the war.

---

## Fully assembled example (Smithy)

> A blacksmith's forge roaring in the dark: a great stone hearth blasting amber light, an anvil with glowing
> iron and flying sparks, racked hammers, tongs and half-forged blades, smoke and soot — the warmest, most
> fire-lit of all — grim dark-fantasy medieval concept art, cinematic night establishing shot. Forged-steel
> palette: near-black gunmetal and cold iron, deep blue-grey shadow, lit only by flickering torchlight and
> forge-embers (warm amber-orange) with a faint cold moonlight rim; tarnished brass accents. Heavy
> chiaroscuro, volumetric haze and drifting smoke, soot, weathered stone, iron and aged timber. Muted,
> desaturated, low overall brightness, strong vignette, deep atmospheric depth. Framed as a darkened
> background plate — the structure set back with large areas of shadow and empty sky for text overlay, wide
> 16:9. Painterly, highly detailed, oppressive, war-torn, no people in the foreground. `--ar 16:9 --style raw --stylize 250`
