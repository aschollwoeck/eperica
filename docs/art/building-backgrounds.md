# Art prompts — building backgrounds & unit portraits

AI image prompts for the game's art, all in the landing page's "Grim Forged Steel" look (cold gunmetal night,
torch/ember warmth, tarnished brass, heavy vignette). Two families:

- **[Building backgrounds](#building-backgrounds--ai-image-prompts)** — one 16:9 image per building, shown as a
  **darkened background** behind that building's page. Each has an optional **[per-tribe architecture
  variant](#per-tribe-architecture)** (Roman stone / Teuton timber / Gaul oak).
- **[Troop & unit portraits](#troop--unit-art--ai-image-prompts)** — one figure portrait per unit, already
  written **per tribe** (+ wild animals), for the roster / training cards.

# Building backgrounds — AI image prompts

One image per building, shown as a **darkened background** behind that building's page.

## How to use

1. The full prompt is **`<subject>` + a [tribe-architecture descriptor](#per-tribe-architecture) + the
   [Style block](#style-block)**. The building **subject** (below) is the *what*; the tribe descriptor is the
   *how it's built* (Roman dressed stone vs Teuton rough timber vs Gaul carved oak); the style block is the
   shared grim-night *mood*. Use the [Negative prompt](#negative-prompt) on tools that take one.
2. Slugs:
   - **Tribe-neutral** (the default the page loads today): save as `crates/web/static/buildings/<slug>.webp`.
     Use a neutral, weathered medieval rendering — skip the tribe descriptor or blend all three.
   - **Tribe-specific** (optional, richer): save as `crates/web/static/buildings/<tribe>_<slug>.webp`
     (`romans_`/`teutons_`/`gauls_`), built with that tribe's architecture descriptor. **This is wired (065):**
     each building page already resolves its village's `<tribe>_<slug>.webp` and falls back to the neutral
     `<slug>.webp` (and then the plain theme) when the tribe plate is absent — drop one in and it appears for
     that tribe only, no code change.
3. These sit **under page text**, so keep them dark and low-key. The page also lays a black 55–70 % scrim +
   vignette over them in CSS, so a little extra darkness in the art is good, not bad. The tribe should read in
   the **architecture and silhouette**, never in bright colour — the palette stays the shared forged-steel one.
4. **Hero-band composition (important).** The redesign uses the image as a **full-bleed hero band across the
   top of the building page**, with the building's **title, crest and a one-line note overlaid on the
   lower-left** — not just a faint full-page wash. So compose for that overlay:
   - Keep the **lower-left third dark and quiet** (shadow, haze, empty ground) so white title text reads over
     it; push the lit subject (the forge/hearth, the stalls, the structure) toward the **centre-right and
     upper area**.
   - Leave **dark headroom along the top** (the sticky nav bar crosses it) and a soft fade-friendly bottom
     (the band blends down into the dark page).
   - It's a **wide band, not a full frame** — roughly the top ~340 px of a ~1180 px-wide column is what shows,
     so the interesting read should live in the **upper ~40 %** of the 16:9 image.

## Technical specs

- **16:9**, ≥ 1600×900 (1920×1080 ideal). Export **WebP**, aim for **< ~250 KB** each (page-weight budget, P11).
- Very dark / low-key, desaturated. The recognisable structure should read as a moody silhouette/establishing
  shot with large shadow areas and empty sky for the UI to overlay.
- **Composition for the hero band:** subject toward centre-right / upper, a **dark quiet lower-left** for the
  overlaid title + crest, dark headroom top, fade-friendly bottom (see *Hero-band composition* above).
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

## Per-tribe architecture

Each tribe builds in a distinct way. Drop **one** of these descriptors between the building `<subject>` and the
[Style block](#style-block) to make a tribe-specific variant (`<tribe>_<slug>.webp`). The descriptor governs
**materials, construction and silhouette only** — the colour palette and night mood always come from the shared
style block (a tribe is recognised by *how it's built*, not by bright colour). The three echo the unit
identities ([Romans](#romans--iron-discipline) / [Teutons](#teutons--brute-iron-and-fury) /
[Gauls](#gauls--lithe-and-warded)).

- **Romans** (`romans_`) — *imperial military engineering: precise dressed-ashlar stone and brick-faced
  concrete, round arches, barrel vaults and columns, low terracotta-tiled roofs, an ordered rectilinear plan
  with right-angles and symmetry; tarnished marble and bronze fittings, an eagle standard and a muted oxblood
  banner; disciplined, monumental, built to last — even war-worn it reads as engineered and exact.*
- **Teutons** (`teutons_`) — *brutal frontier construction: massive rough-hewn timber and undressed fieldstone,
  steep shingled or thatched gable roofs, log palisades and earth ramparts, wattle-and-daub between heavy
  beams; animal skulls, antlers and horns nailed up, bone and furs, crude black iron banding and runes carved
  into the wood; squat, heavy, fortress-like and unrefined — smoke-stained and defiant.*
- **Gauls** (`gauls_`) — *organic Celtic craft: conical thatched roundhouses and finely carpentered oak frames,
  woven-wattle walls and dry-stone footings, an earthwork hillfort with palisade; carved knotwork and spirals,
  standing stones and weather-greyed totems, mistletoe-and-oak and antler motifs, woad-tinted cloth muted to
  blue-grey; lithe, naturalistic and warded — graceful where the Teutons are brutal.*

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

# Troop & unit art — AI image prompts

One **character portrait** per unit, for the roster / training cards (Barracks, Stable, Workshop, Residence)
and reports. Same "Grim Forged Steel" world as the buildings, but **framed and lit differently**: a unit is a
*subject* you want to read clearly, not a structure receding into shadow. So these are single figures, lit from
a forge/torch key, on a dark out-of-focus backdrop.

## How to use (units)

1. The full prompt is **`<subject>` + the [Unit portrait style block](#unit-portrait-style-block)**. Reuse the
   same [Negative prompt](#negative-prompt) above (units *are* the foreground, so drop only "people in
   foreground / faces" from it if your tool lets you — a clear face is wanted here).
2. Save each as `crates/web/static/units/<slug>.webp`. **Slugs are tribe-prefixed** (`romans_legionnaire`,
   `teutons_ram`, `gauls_settler`) because unit ids collide across tribes (`ram`, `settler`, `chief`/`chieftain`).
3. Keep each tribe visually coherent: **Romans** — disciplined, segmented iron plate + a muted oxblood/crimson
   accent and crested helm; **Teutons** — brutal, furs and bone, raw hammered iron, unkempt; **Gauls** — lithe
   and woad-marked, leather and teal/blue-grey cloth, druidic. Always inside the cold gunmetal/ember palette —
   the accent is a *tint*, never a bright saturated colour.

## Technical specs (units)

- **Portrait 3:4** (or 1:1 for the roster grid), ≥ 1024 px on the short side. Export **WebP**, **< ~150 KB** each.
- Single figure, knees-up or bust, **centred**, slight low angle for menace. Subject sharply lit; background a
  dark, smoky, shallow-depth forge/war-camp suggestion — no competing detail.
- Midjourney params: `--ar 3:4 --style raw --stylize 250`

## Unit portrait style block

> *— grim dark-fantasy medieval character portrait, cinematic. A single battle-worn figure, centred, lit by
> warm flickering forge/torch light from one side with a faint cold moonlight rim on the other. Forged-steel
> world: near-black gunmetal and cold iron armour, weathered leather, soot and grime, tarnished brass buckles;
> dark smoky war-camp background at shallow depth of field. Heavy chiaroscuro, volumetric haze, muted and
> desaturated, oppressive and war-torn. Painterly, highly detailed, sharp focus on the figure. No text.*

## Romans — iron discipline

- **Legionnaire** (`romans_legionnaire`) — A Roman legionary in segmented iron lorica and a crested helm, a
  scarred rectangular scutum shield and short gladius, oxblood-red tunic muted to near-grey, jaw set, the
  backbone of the legion.
- **Praetorian** (`romans_praetorian`) — An elite Praetorian guardsman, heavier blackened plate and a tall
  tower shield braced for defence, spear couched, an unyielding bulwark, dust and torch-glow on the iron.
- **Imperian** (`romans_imperian`) — A hard-faced assault legionary, lighter agile armour, gladius drawn
  mid-stride and shield up, a brutal close-quarters attacker lit by raid-fire.
- **Equites Legati** (`romans_equites_legati`) — A light Roman scout-rider leaning low on a lean horse, hooded
  cloak, no heavy armour, a spyglass-sharp watchful look, mist and moonlight, built for speed and seeing.
- **Equites Imperatoris** (`romans_equites_imperatoris`) — A Roman cavalryman on an armoured horse, mailed and
  helmed with a long spear and oval shield, charging through smoke, disciplined and deadly.
- **Equites Caesaris** (`romans_equites_caesaris`) — The heaviest Roman knight, man and destrier in full
  blackened barding, lance levelled, a crimson-grey crest, an unstoppable shock charge in ember-light.
- **Battering Ram** (`romans_battering_ram`) — A great iron-headed battering ram slung in a timber A-frame on
  wheels, an iron ram's-head cap, crewed by shadows, scarred from gate-work, framed as a siege engine portrait.
- **Fire Catapult** (`romans_fire_catapult`) — A heavy Roman torsion catapult, arm cocked with a blazing pitch
  payload, ropes and iron fittings, sparks and smoke, menace in the dark war-camp.
- **Senator** (`romans_senator`) — A robed Roman senator-instigator, an austere toga over a mail shirt, a
  scroll or seal of conquest in hand, cold and calculating, braziers behind — a taker of villages, not a
  fighter.
- **Settler** (`romans_settler`) — A Roman pioneer with a pack, spade and surveyor's rod, a cloak against the
  cold, gazing at unclaimed land, a hardy founder of new villages by torchlight.

## Teutons — brute iron and fury

- **Clubswinger** (`teutons_clubswinger`) — A wild Teuton warrior swinging a crude iron-studded club, furs and
  bare scarred arms, matted hair, no real armour, snarling — cheap, savage, and many.
- **Spearman** (`teutons_spearman`) — A Teuton spearman in furs and a rough round shield, long boar-spear set
  low to gut cavalry, braced and grim in the mud and smoke.
- **Axeman** (`teutons_axeman`) — A hulking Teuton with a great two-handed war-axe, leather and iron scraps,
  bone charms, mid-roar, a brutal cleaver of shields.
- **Scout** (`teutons_scout`) — A Teuton outrider on foot in a fur hood, light and watchful, a hand-axe and a
  horn, slipping through cold fog at the treeline.
- **Paladin** (`teutons_paladin`) — A heavily armoured Teuton horseman, mailed and fur-cloaked on a shaggy
  warhorse, round shield up, a stout defensive rider in moonlit haze.
- **Teutonic Knight** (`teutons_teutonic_knight`) — The dread Teutonic Knight, blackened plate and a horned or
  winged helm on an armoured destrier, a heavy blade raised, the terror of the raid in ember-light.
- **Ram** (`teutons_ram`) — A rough-hewn Teuton battering ram, a whole tree trunk iron-shod and slung under a
  hide-covered timber frame on log wheels, brutal and crude, scarred from gates.
- **Catapult** (`teutons_catapult`) — A massive Teuton catapult of heavy raw timber, arm loaded with a boulder,
  rope and iron, smoke and sparks in a dark camp — siege-engine portrait.
- **Chief** (`teutons_chief`) — A Teuton chief in furs and battle-trophies, a torc and a great horn, arm raised
  to rally and to break a village's loyalty, firelit and commanding.
- **Settler** (`teutons_settler`) — A Teuton settler dragging an ox-cart of stakes and stores into the wilds,
  furs against the frost, an axe on the shoulder, founding a new village in the dark.

## Gauls — lithe and warded

- **Phalanx** (`gauls_phalanx`) — A Gaulish spearman in leather and a tall oval shield, woad blue-grey markings,
  a long spear set against cavalry, calm and defensive in the mist — the wall the Gauls are known for.
- **Swordsman** (`gauls_swordsman`) — A Gaulish swordsman with a long iron blade and a light shield, teal cloak,
  quick-footed and lean, a balanced attacker lit by torchfire.
- **Pathfinder** (`gauls_pathfinder`) — A Gaulish scout on a fast light horse, hooded, a bow or short spear,
  reading the land, the fastest eyes on the map, moonlit and silent.
- **Theutates Thunder** (`gauls_theutates_thunder`) — A Gaulish rider on a famously swift horse at full gallop,
  light armour and a couched lance, hair and cloak streaming, raw speed and raiding fury in the haze.
- **Druidrider** (`gauls_druidrider`) — A mystic Gaulish cavalryman, antler or hooded druid helm, runic charms
  and a warded shield on an armoured horse, a defensive rider wreathed in cold mist and faint eerie light.
- **Haeduan** (`gauls_haeduan`) — The elite Haeduan knight, fine scale armour with teal and tarnished-brass
  accents, a long lance and shield on a swift armoured horse, proud and lethal in ember-light.
- **Ram** (`gauls_ram`) — A Gaulish battering ram, an iron-capped trunk in a lighter, finely braced timber
  frame on wheels, leather-shrouded, crewed by shadows — siege-engine portrait.
- **Trebuchet** (`gauls_trebuchet`) — A tall Gaulish trebuchet, counterweight raised and sling loaded, elegant
  heavy timber and rope against the night sky, smoke and torch-glow — the longest reach of the siege lines.
- **Chieftain** (`gauls_chieftain`) — A Gaulish chieftain in a wolf-pelt and torc, woad-marked, a horn and a
  fine blade, arm raised to sway a village's loyalty, firelit and defiant.
- **Settler** (`gauls_settler`) — A Gaulish settler with a laden pony, surveyor's cord and stakes, a warded
  charm at the neck, scouting unclaimed wilderness to found a new village under the moon.

## Wild animals — oasis & nature defenders

These guard oases on the map; portrait them as **menacing beasts** in the same dark, smoky, low-key world
(reed-fringed water, cold mist, ember-eyes), single creature centred. Slug is the bare id (no tribe prefix).

- **Rat** (`rat`) — A swarm-leader dire rat, wet matted fur and bared yellow teeth, red ember-eyes in the dark,
  crawling over slick stone.
- **Spider** (`spider`) — A huge dark cave-spider on a torn web, glistening carapace and clustered eyes catching
  cold light, legs splayed, menace in the gloom.
- **Snake** (`snake`) — A great coiled serpent rearing to strike, scales gleaming dully, forked tongue, half in
  shadow over wet reeds.
- **Bat** (`bat`) — A monstrous bat mid-flight, leathery wings spread against a moonlit sky, fangs and
  ember-eyes, wisps of cold fog.
- **Wild Boar** (`boar`) — A massive scarred wild boar, tusks and bristled hide, breath steaming, lowered head
  charging through dark undergrowth.
- **Wolf** (`wolf`) — A gaunt dire wolf, hackles raised and fangs bared, pale eyes glowing in moonlight, mist
  curling at its paws.
- **Bear** (`bear`) — A towering battle-scarred bear reared on its hind legs, claws out, roaring, firelight
  catching its shaggy fur in the dark wilds.
- **Crocodile** (`crocodile`) — A huge ancient crocodile half-submerged in black oasis water, armoured scutes
  and jagged jaws agape, eyes above the waterline catching torchlight.
- **Tiger** (`tiger`) — A powerful tiger crouched to spring, striped coat muted to grey-amber in the gloom,
  eyes blazing, low through reeds and mist.
- **Elephant** (`elephant`) — A colossal war-tusked elephant, scarred and draped in tattered barding, trunk
  raised and trumpeting, dust and torch-glow around its bulk in the night.

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

## Fully assembled example — tribe-specific (Teuton Main Building → `teutons_main_building`)

The same subject + the **Teuton** architecture descriptor + the style block:

> The village's main hall and works yard: a stout great hall with a watch-tower and a banner, torches flanking
> heavy iron-bound doors, timber scaffolding and stacked stone of ongoing construction — the beating heart of a
> war-village at night — *brutal frontier construction: massive rough-hewn timber and undressed fieldstone, a
> steep shingled gable roof, log palisades and earth ramparts, wattle-and-daub between heavy beams; animal
> skulls, antlers and horns nailed up, bone and furs, crude black iron banding and runes carved into the wood;
> squat, heavy, fortress-like and unrefined* — grim dark-fantasy medieval concept art, cinematic night
> establishing shot. Forged-steel palette: near-black gunmetal and cold iron, deep blue-grey shadow, lit only by
> flickering torchlight and forge-embers (warm amber-orange) with a faint cold moonlight rim; tarnished brass
> accents. Heavy chiaroscuro, volumetric haze and drifting smoke, soot, weathered stone, iron and aged timber.
> Muted, desaturated, low overall brightness, strong vignette, deep atmospheric depth. Framed as a darkened
> background plate — the structure set back with large areas of shadow and empty sky for text overlay, wide
> 16:9. Painterly, highly detailed, oppressive, war-torn, no people in the foreground. `--ar 16:9 --style raw --stylize 250`

> Swap the italic descriptor for the **Roman** or **Gaul** one (and the slug prefix) to get the other two
> tribe variants of the same building.

---

# Roman set — "Warm Ruin" style (full, copy-paste prompts)

The actual shipped Roman art (`crates/web/static/units/romans_legionnaire.webp`) uses a warm dusty-sepia,
hazy-backlit look (the [shared style blocks](#repeatable-part-the-shared-style--params) below), **not** the
grim forged-steel night style at the top of this file. Below is the **complete Roman roster + every building**, each as a
**fully assembled, copy-paste-ready Midjourney prompt** (style and params already baked in — no need to append
anything). Slugs and save paths are as documented above.

> **Make the whole set match:** generate the legionnaire (or the main building) first, then add
> `--sref <that image's URL> --sw 75` to every other prompt — text alone won't reproduce the haze and brushwork
> across a set. Drag the image into Midjourney's prompt bar to get a URL (it can't read a local file path).

## Repeatable part (the shared style + params)

Every prompt below = **`<subject>` + one of these style blocks + its params**. If you write new subjects, just
paste the matching block on the end.

**Unit style block** (single-figure portraits):
```
Painterly digital concept art, moody and cinematic. Single figure centred, slight low angle. Strong hazy backlight, warm sepia and dusty cream tones with muted oxblood accents, the figure reading dark against a glowing pale sky; a thriving monumental ancient-Roman city — Colosseum, columns, arches — dissolving in dusty haze behind at shallow depth of field; soft atmospheric perspective, drifting dust, desaturated, highly detailed. --ar 1:1 --style raw --stylize 250 --no text, watermark, frame, bright colours, blue sky, modern objects, cartoon, anime, ruins, rubble, broken, derelict, decay, deformed hands, extra limbs
```

**Building style block** (exterior background plates):
```
Painterly digital concept art, moody and cinematic, monumental and orderly. Wide establishing shot, the structure reading dark against a glowing pale sky; strong hazy backlight, warm sepia and dusty cream tones with muted oxblood accents; a thriving ancient-Roman city solid in the dusty haze behind, soft atmospheric perspective, gentle drifting dust, desaturated, highly detailed, large empty hazy sky above for text overlay. --ar 16:9 --style raw --stylize 250 --no text, watermark, frame, people in foreground, faces, bright colours, blue sky, modern objects, cartoon, anime, ruins, rubble, broken, derelict, decay
```

**Interior style block** (for `cranny` and `treasury` — no open sky):
```
Painterly digital concept art, moody and cinematic. Dim interior in warm sepia and dusty cream tones with muted oxblood accents, lit by a single narrow shaft of dusty light against deep sepia shadow; soft atmospheric haze, fine drifting dust, desaturated, highly detailed, dark empty areas for text overlay. --ar 16:9 --style raw --stylize 250 --no text, watermark, frame, people in foreground, faces, bright colours, blue sky, modern objects, cartoon, anime, ruins, rubble, broken, derelict, decay
```

## Roman units → `crates/web/static/units/<slug>.webp`

### `romans_legionnaire`
```
A Roman legionary in segmented iron lorica and a crested helm hiding his eyes, a scarred rectangular scutum on one arm and a short gladius in the other, a tattered oxblood cloak, jaw set, standing on dusty ground, the backbone of the legion. Painterly digital concept art, moody and cinematic. Single figure centred, slight low angle. Strong hazy backlight, warm sepia and dusty cream tones with muted oxblood accents, the figure reading dark against a glowing pale sky; a thriving monumental ancient-Roman city — Colosseum, columns, arches — dissolving in dusty haze behind at shallow depth of field; soft atmospheric perspective, drifting dust, desaturated, highly detailed. --ar 1:1 --style raw --stylize 250 --no text, watermark, frame, bright colours, blue sky, modern objects, cartoon, anime, ruins, rubble, broken, derelict, decay, deformed hands, extra limbs
```

### `romans_praetorian`
```
An elite Roman Praetorian guardsman in heavier blackened iron plate, a tall tower shield braced before him and a spear couched, an oxblood crest and sash, an unyielding bulwark in the dust and haze. Painterly digital concept art, moody and cinematic. Single figure centred, slight low angle. Strong hazy backlight, warm sepia and dusty cream tones with muted oxblood accents, the figure reading dark against a glowing pale sky; a thriving monumental ancient-Roman city — Colosseum, columns, arches — dissolving in dusty haze behind at shallow depth of field; soft atmospheric perspective, drifting dust, desaturated, highly detailed. --ar 1:1 --style raw --stylize 250 --no text, watermark, frame, bright colours, blue sky, modern objects, cartoon, anime, ruins, rubble, broken, derelict, decay, deformed hands, extra limbs
```

### `romans_imperian`
```
A hard-faced Roman assault legionary in lighter agile armour, gladius drawn mid-stride and scutum raised, cloak streaming, a brutal close-quarters attacker caught moving through dusty light. Painterly digital concept art, moody and cinematic. Single figure centred, slight low angle. Strong hazy backlight, warm sepia and dusty cream tones with muted oxblood accents, the figure reading dark against a glowing pale sky; a thriving monumental ancient-Roman city — Colosseum, columns, arches — dissolving in dusty haze behind at shallow depth of field; soft atmospheric perspective, drifting dust, desaturated, highly detailed. --ar 1:1 --style raw --stylize 250 --no text, watermark, frame, bright colours, blue sky, modern objects, cartoon, anime, ruins, rubble, broken, derelict, decay, deformed hands, extra limbs
```

### `romans_equites_legati`
```
A light Roman scout-rider leaning low on a lean horse, a hooded cloak and no heavy armour, a sharp watchful look, dust and pale haze around horse and rider, built for speed and seeing. Painterly digital concept art, moody and cinematic. Single figure centred, slight low angle. Strong hazy backlight, warm sepia and dusty cream tones with muted oxblood accents, the figure reading dark against a glowing pale sky; a thriving monumental ancient-Roman city — Colosseum, columns, arches — dissolving in dusty haze behind at shallow depth of field; soft atmospheric perspective, drifting dust, desaturated, highly detailed. --ar 1:1 --style raw --stylize 250 --no text, watermark, frame, bright colours, blue sky, modern objects, cartoon, anime, ruins, rubble, broken, derelict, decay, deformed hands, extra limbs
```

### `romans_equites_imperatoris`
```
A Roman cavalryman on an armoured horse, mailed and helmed with a long spear and an oval shield, charging through drifting dust, disciplined and deadly. Painterly digital concept art, moody and cinematic. Single figure centred, slight low angle. Strong hazy backlight, warm sepia and dusty cream tones with muted oxblood accents, the figure reading dark against a glowing pale sky; a thriving monumental ancient-Roman city — Colosseum, columns, arches — dissolving in dusty haze behind at shallow depth of field; soft atmospheric perspective, drifting dust, desaturated, highly detailed. --ar 1:1 --style raw --stylize 250 --no text, watermark, frame, bright colours, blue sky, modern objects, cartoon, anime, ruins, rubble, broken, derelict, decay, deformed hands, extra limbs
```

### `romans_equites_caesaris`
```
The heaviest Roman knight, man and destrier in full blackened barding, lance levelled, a crimson-grey crest, an unstoppable shock charge kicking up dust in the pale backlight. Painterly digital concept art, moody and cinematic. Single figure centred, slight low angle. Strong hazy backlight, warm sepia and dusty cream tones with muted oxblood accents, the figure reading dark against a glowing pale sky; a thriving monumental ancient-Roman city — Colosseum, columns, arches — dissolving in dusty haze behind at shallow depth of field; soft atmospheric perspective, drifting dust, desaturated, highly detailed. --ar 1:1 --style raw --stylize 250 --no text, watermark, frame, bright colours, blue sky, modern objects, cartoon, anime, ruins, rubble, broken, derelict, decay, deformed hands, extra limbs
```

### `romans_battering_ram`
```
A great iron-headed Roman battering ram with a ram's-head cap, slung in a wheeled timber A-frame, crewed by shadowed soldiers, scarred from gate-work, a heavy siege-engine portrait in dusty haze. Painterly digital concept art, moody and cinematic. Subject centred, slight low angle. Strong hazy backlight, warm sepia and dusty cream tones with muted oxblood accents, the engine reading dark against a glowing pale sky; a thriving monumental ancient-Roman city — Colosseum, columns, arches — dissolving in dusty haze behind at shallow depth of field; soft atmospheric perspective, drifting dust, desaturated, highly detailed. --ar 1:1 --style raw --stylize 250 --no text, watermark, frame, bright colours, blue sky, modern objects, cartoon, anime, ruins, rubble, broken, derelict, decay, deformed hands, extra limbs
```

### `romans_fire_catapult`
```
A heavy Roman torsion catapult, arm cocked with a blazing pitch payload throwing warm firelight, ropes and iron fittings, smoke and sparks against the pale dusty backlight, a siege-engine portrait. Painterly digital concept art, moody and cinematic. Subject centred, slight low angle. Strong hazy backlight, warm sepia and dusty cream tones with muted oxblood accents, the engine reading dark against a glowing pale sky; a thriving monumental ancient-Roman city — Colosseum, columns, arches — dissolving in dusty haze behind at shallow depth of field; soft atmospheric perspective, drifting dust, desaturated, highly detailed. --ar 1:1 --style raw --stylize 250 --no text, watermark, frame, bright colours, blue sky, modern objects, cartoon, anime, ruins, rubble, broken, derelict, decay, deformed hands, extra limbs
```

### `romans_senator`
```
A robed Roman senator-instigator, an austere toga over a mail shirt, a scroll and wax seal of conquest in hand, cold and calculating, a taker of villages rather than a fighter, framed against hazy columns. Painterly digital concept art, moody and cinematic. Single figure centred, slight low angle. Strong hazy backlight, warm sepia and dusty cream tones with muted oxblood accents, the figure reading dark against a glowing pale sky; a thriving monumental ancient-Roman city — Colosseum, columns, arches — dissolving in dusty haze behind at shallow depth of field; soft atmospheric perspective, drifting dust, desaturated, highly detailed. --ar 1:1 --style raw --stylize 250 --no text, watermark, frame, bright colours, blue sky, modern objects, cartoon, anime, ruins, rubble, broken, derelict, decay, deformed hands, extra limbs
```

### `romans_settler`
```
A Roman pioneer with a heavy pack, a spade and a surveyor's rod, a travel cloak against the dust, gazing out at unclaimed land, a hardy founder of new villages in the pale haze. Painterly digital concept art, moody and cinematic. Single figure centred, slight low angle. Strong hazy backlight, warm sepia and dusty cream tones with muted oxblood accents, the figure reading dark against a glowing pale sky; a thriving monumental ancient-Roman city — Colosseum, columns, arches — dissolving in dusty haze behind at shallow depth of field; soft atmospheric perspective, drifting dust, desaturated, highly detailed. --ar 1:1 --style raw --stylize 250 --no text, watermark, frame, bright colours, blue sky, modern objects, cartoon, anime, ruins, rubble, broken, derelict, decay, deformed hands, extra limbs
```

## Roman resource fields → `crates/web/static/buildings/<slug>.webp`

### `woodcutter`
```
A Roman lumber yard at the edge of a sun-hazed forest: felled trunks and neatly stacked timber, a sawpit, ordered woodpiles, a tiled-roof lodge, drifting dust and woodsmoke, the city skyline faint beyond the trees. Painterly digital concept art, moody and cinematic, monumental and orderly. Wide establishing shot, the structure reading dark against a glowing pale sky; strong hazy backlight, warm sepia and dusty cream tones with muted oxblood accents; a thriving ancient-Roman city solid in the dusty haze behind, soft atmospheric perspective, gentle drifting dust, desaturated, highly detailed, large empty hazy sky above for text overlay. --ar 16:9 --style raw --stylize 250 --no text, watermark, frame, people in foreground, faces, bright colours, blue sky, modern objects, cartoon, anime, ruins, rubble, broken, derelict, decay
```

### `clay_pit`
```
A terraced Roman clay pit cut into a hillside: ordered timber scaffolds and ladders, buckets and a hand-winch, stepped excavation walls, standing pools catching the pale light, dust hanging in the air. Painterly digital concept art, moody and cinematic, monumental and orderly. Wide establishing shot, the structure reading dark against a glowing pale sky; strong hazy backlight, warm sepia and dusty cream tones with muted oxblood accents; a thriving ancient-Roman city solid in the dusty haze behind, soft atmospheric perspective, gentle drifting dust, desaturated, highly detailed, large empty hazy sky above for text overlay. --ar 16:9 --style raw --stylize 250 --no text, watermark, frame, people in foreground, faces, bright colours, blue sky, modern objects, cartoon, anime, ruins, rubble, broken, derelict, decay
```

### `iron_mine`
```
The timber-braced mouth of a Roman iron mine in a dusty crag: ore carts on wooden rails, ordered slag heaps, picks and chains, a faint warm glow from the tunnel, the city hazed in the distance. Painterly digital concept art, moody and cinematic, monumental and orderly. Wide establishing shot, the structure reading dark against a glowing pale sky; strong hazy backlight, warm sepia and dusty cream tones with muted oxblood accents; a thriving ancient-Roman city solid in the dusty haze behind, soft atmospheric perspective, gentle drifting dust, desaturated, highly detailed, large empty hazy sky above for text overlay. --ar 16:9 --style raw --stylize 250 --no text, watermark, frame, people in foreground, faces, bright colours, blue sky, modern objects, cartoon, anime, ruins, rubble, broken, derelict, decay
```

### `cropland`
```
Sun-hazed Roman farmland: ordered rows of grain and bound sheaves, a stone-and-timber granary on the horizon, low ground-haze and drifting dust, the monumental city skyline pale beyond the fields. Painterly digital concept art, moody and cinematic, monumental and orderly. Wide establishing shot, the structure reading dark against a glowing pale sky; strong hazy backlight, warm sepia and dusty cream tones with muted oxblood accents; a thriving ancient-Roman city solid in the dusty haze behind, soft atmospheric perspective, gentle drifting dust, desaturated, highly detailed, large empty hazy sky above for text overlay. --ar 16:9 --style raw --stylize 250 --no text, watermark, frame, people in foreground, faces, bright colours, blue sky, modern objects, cartoon, anime, ruins, rubble, broken, derelict, decay
```

## Roman buildings → `crates/web/static/buildings/<slug>.webp`

### `main_building`
```
A grand Roman great hall and works yard: a stout dressed-stone hall with round arches, a square watch-tower and a tiled roof, an oxblood banner hanging proud, torches at heavy iron-bound doors, fresh scaffolding and neatly stacked stone of active construction, the thriving heart of the village. Painterly digital concept art, moody and cinematic, monumental and orderly. Wide establishing shot, the structure reading dark against a glowing pale sky; strong hazy backlight, warm sepia and dusty cream tones with muted oxblood accents; a thriving ancient-Roman city solid in the dusty haze behind, soft atmospheric perspective, gentle drifting dust, desaturated, highly detailed, large empty hazy sky above for text overlay. --ar 16:9 --style raw --stylize 250 --no text, watermark, frame, people in foreground, faces, bright colours, blue sky, modern objects, cartoon, anime, ruins, rubble, broken, derelict, decay
```

### `rally_point`
```
A Roman muster yard and war camp: a raised stone command platform hung with an oxblood standard and an eagle, racked spears and scuta, ordered tent rows, marching tracks in the dust, ranks of soldiers as faint shapes in the haze. Painterly digital concept art, moody and cinematic, monumental and orderly. Wide establishing shot, the structure reading dark against a glowing pale sky; strong hazy backlight, warm sepia and dusty cream tones with muted oxblood accents; a thriving ancient-Roman city solid in the dusty haze behind, soft atmospheric perspective, gentle drifting dust, desaturated, highly detailed, large empty hazy sky above for text overlay. --ar 16:9 --style raw --stylize 250 --no text, watermark, frame, people in foreground, faces, bright colours, blue sky, modern objects, cartoon, anime, ruins, rubble, broken, derelict, decay
```

### `warehouse`
```
A fortified Roman storehouse: a long dressed-stone building with massive iron-banded doors, ordered stacks of crates, amphorae and sacks, a colonnaded loading porch, a hooded guard, dust and pale light. Painterly digital concept art, moody and cinematic, monumental and orderly. Wide establishing shot, the structure reading dark against a glowing pale sky; strong hazy backlight, warm sepia and dusty cream tones with muted oxblood accents; a thriving ancient-Roman city solid in the dusty haze behind, soft atmospheric perspective, gentle drifting dust, desaturated, highly detailed, large empty hazy sky above for text overlay. --ar 16:9 --style raw --stylize 250 --no text, watermark, frame, people in foreground, faces, bright colours, blue sky, modern objects, cartoon, anime, ruins, rubble, broken, derelict, decay
```

### `granary`
```
A tall Roman grain store raised on stone arches: sacks of grain and bound straw, a loading hoist, a colonnaded base, the silo standing solid against a pale hazy sky, drifting chaff and dust. Painterly digital concept art, moody and cinematic, monumental and orderly. Wide establishing shot, the structure reading dark against a glowing pale sky; strong hazy backlight, warm sepia and dusty cream tones with muted oxblood accents; a thriving ancient-Roman city solid in the dusty haze behind, soft atmospheric perspective, gentle drifting dust, desaturated, highly detailed, large empty hazy sky above for text overlay. --ar 16:9 --style raw --stylize 250 --no text, watermark, frame, people in foreground, faces, bright colours, blue sky, modern objects, cartoon, anime, ruins, rubble, broken, derelict, decay
```

### `marketplace`
```
A thriving Roman market square: canvas-awning stalls, merchant scales and crates of goods, a laden cart and tethered ox, columns and a fountain, faint townsfolk and drifting dust in the warm backlight. Painterly digital concept art, moody and cinematic, monumental and orderly. Wide establishing shot, the structure reading dark against a glowing pale sky; strong hazy backlight, warm sepia and dusty cream tones with muted oxblood accents; a thriving ancient-Roman city solid in the dusty haze behind, soft atmospheric perspective, gentle drifting dust, desaturated, highly detailed, large empty hazy sky above for text overlay. --ar 16:9 --style raw --stylize 250 --no text, watermark, frame, people in foreground, faces, bright colours, blue sky, modern objects, cartoon, anime, ruins, rubble, broken, derelict, decay
```

### `embassy`
```
An austere Roman diplomatic hall, grander than its neighbours: heraldic shields and alliance banners above a sealed studded bronze door, flanking columns, an eagle standard in the courtyard, long hazy shadows. Painterly digital concept art, moody and cinematic, monumental and orderly. Wide establishing shot, the structure reading dark against a glowing pale sky; strong hazy backlight, warm sepia and dusty cream tones with muted oxblood accents; a thriving ancient-Roman city solid in the dusty haze behind, soft atmospheric perspective, gentle drifting dust, desaturated, highly detailed, large empty hazy sky above for text overlay. --ar 16:9 --style raw --stylize 250 --no text, watermark, frame, people in foreground, faces, bright colours, blue sky, modern objects, cartoon, anime, ruins, rubble, broken, derelict, decay
```

### `wall`
```
A massive Roman defensive wall and gatehouse: a crenellated dressed-stone rampart with a barred iron portcullis and flanking towers, an eagle standard on the battlements, sentries silhouetted, dust drifting in the pale light. Painterly digital concept art, moody and cinematic, monumental and orderly. Wide establishing shot, the structure reading dark against a glowing pale sky; strong hazy backlight, warm sepia and dusty cream tones with muted oxblood accents; a thriving ancient-Roman city solid in the dusty haze behind, soft atmospheric perspective, gentle drifting dust, desaturated, highly detailed, large empty hazy sky above for text overlay. --ar 16:9 --style raw --stylize 250 --no text, watermark, frame, people in foreground, faces, bright colours, blue sky, modern objects, cartoon, anime, ruins, rubble, broken, derelict, decay
```

### `barracks`
```
A Roman soldiers' barracks and drill yard: racks of gladii, spears and scuta, straw practice posts, armour on stands, an ordered colonnaded yard, recruits as faint shapes drilling in the dusty haze. Painterly digital concept art, moody and cinematic, monumental and orderly. Wide establishing shot, the structure reading dark against a glowing pale sky; strong hazy backlight, warm sepia and dusty cream tones with muted oxblood accents; a thriving ancient-Roman city solid in the dusty haze behind, soft atmospheric perspective, gentle drifting dust, desaturated, highly detailed, large empty hazy sky above for text overlay. --ar 16:9 --style raw --stylize 250 --no text, watermark, frame, people in foreground, faces, bright colours, blue sky, modern objects, cartoon, anime, ruins, rubble, broken, derelict, decay
```

### `academy`
```
A Roman war academy and map-room: a strategist's table strewn with charts and weapon schematics, shelves of scrolls, a quill and dividers, austere columned stone, warm dusty light pooling through high windows. Painterly digital concept art, moody and cinematic, monumental and orderly. Wide establishing shot, the structure reading dark against a glowing pale sky; strong hazy backlight, warm sepia and dusty cream tones with muted oxblood accents; a thriving ancient-Roman city solid in the dusty haze behind, soft atmospheric perspective, gentle drifting dust, desaturated, highly detailed, large empty hazy sky above for text overlay. --ar 16:9 --style raw --stylize 250 --no text, watermark, frame, people in foreground, faces, bright colours, blue sky, modern objects, cartoon, anime, ruins, rubble, broken, derelict, decay
```

### `smithy`
```
A Roman blacksmith's forge: a great stone hearth blasting warm light, an anvil with glowing iron and flying sparks, racked hammers, tongs and half-forged blades, smoke and dust, the warmest most fire-lit plate, set in a colonnaded yard. Painterly digital concept art, moody and cinematic, monumental and orderly. Wide establishing shot, the structure reading dark against a glowing pale sky; strong hazy backlight, warm sepia and dusty cream tones with muted oxblood accents; a thriving ancient-Roman city solid in the dusty haze behind, soft atmospheric perspective, gentle drifting dust, desaturated, highly detailed, large empty hazy sky above for text overlay. --ar 16:9 --style raw --stylize 250 --no text, watermark, frame, people in foreground, faces, bright colours, blue sky, modern objects, cartoon, anime, ruins, rubble, broken, derelict, decay
```

### `stable`
```
A Roman war-horse stable: rows of dressed-stone stalls under a tiled roof, armoured destriers shifting in warm shadow, hay, tack and horse-barding on hooks, dust in the air, pale light through the arches. Painterly digital concept art, moody and cinematic, monumental and orderly. Wide establishing shot, the structure reading dark against a glowing pale sky; strong hazy backlight, warm sepia and dusty cream tones with muted oxblood accents; a thriving ancient-Roman city solid in the dusty haze behind, soft atmospheric perspective, gentle drifting dust, desaturated, highly detailed, large empty hazy sky above for text overlay. --ar 16:9 --style raw --stylize 250 --no text, watermark, frame, people in foreground, faces, bright colours, blue sky, modern objects, cartoon, anime, ruins, rubble, broken, derelict, decay
```

### `workshop`
```
A Roman siege workshop: half-built catapults, ballistae and a battering ram in timber frames, coils of rope, sawn beams and iron fittings, a hand-crane and sawdust, busy and ordered in the dusty light. Painterly digital concept art, moody and cinematic, monumental and orderly. Wide establishing shot, the structure reading dark against a glowing pale sky; strong hazy backlight, warm sepia and dusty cream tones with muted oxblood accents; a thriving ancient-Roman city solid in the dusty haze behind, soft atmospheric perspective, gentle drifting dust, desaturated, highly detailed, large empty hazy sky above for text overlay. --ar 16:9 --style raw --stylize 250 --no text, watermark, frame, people in foreground, faces, bright colours, blue sky, modern objects, cartoon, anime, ruins, rubble, broken, derelict, decay
```

### `residence`
```
A lord's fortified Roman residence wing: a dressed-stone manor with a warm hearth glimpsed through an arched window, oxblood banners, a planning table and maps for new settlements, retainers as faint shapes, columns at the steps. Painterly digital concept art, moody and cinematic, monumental and orderly. Wide establishing shot, the structure reading dark against a glowing pale sky; strong hazy backlight, warm sepia and dusty cream tones with muted oxblood accents; a thriving ancient-Roman city solid in the dusty haze behind, soft atmospheric perspective, gentle drifting dust, desaturated, highly detailed, large empty hazy sky above for text overlay. --ar 16:9 --style raw --stylize 250 --no text, watermark, frame, people in foreground, faces, bright colours, blue sky, modern objects, cartoon, anime, ruins, rubble, broken, derelict, decay
```

### `cranny`  *(interior style block)*
```
A hidden Roman cache below the village: a concealed vaulted cellar reached by a trapdoor, sacks and small chests of resources tucked in warm shadow, a single shaft of dusty light, secrecy and dim sepia gloom. Painterly digital concept art, moody and cinematic. Dim interior in warm sepia and dusty cream tones with muted oxblood accents, lit by a single narrow shaft of dusty light against deep sepia shadow; soft atmospheric haze, fine drifting dust, desaturated, highly detailed, dark empty areas for text overlay. --ar 16:9 --style raw --stylize 250 --no text, watermark, frame, people in foreground, faces, bright colours, blue sky, modern objects, cartoon, anime, ruins, rubble, broken, derelict, decay
```

### `outpost`
```
A Roman frontier outpost on a captured oasis: a dressed-stone watchtower and palisade beside hazy reed-fringed water in the wilds, an eagle standard and signal-brazier, a lone sentry, dust and pale light over the pool. Painterly digital concept art, moody and cinematic, monumental and orderly. Wide establishing shot, the structure reading dark against a glowing pale sky; strong hazy backlight, warm sepia and dusty cream tones with muted oxblood accents; a thriving ancient-Roman city solid in the dusty haze behind, soft atmospheric perspective, gentle drifting dust, desaturated, highly detailed, large empty hazy sky above for text overlay. --ar 16:9 --style raw --stylize 250 --no text, watermark, frame, people in foreground, faces, bright colours, blue sky, modern objects, cartoon, anime, ruins, rubble, broken, derelict, decay
```

### `town_hall`
```
A great Roman civic hall for triumphs: a large columned stone hall with a raised dais and long oxblood banners, trophies of war on the walls, braziers and faint celebration, grander and prouder, warm dusty light. Painterly digital concept art, moody and cinematic, monumental and orderly. Wide establishing shot, the structure reading dark against a glowing pale sky; strong hazy backlight, warm sepia and dusty cream tones with muted oxblood accents; a thriving ancient-Roman city solid in the dusty haze behind, soft atmospheric perspective, gentle drifting dust, desaturated, highly detailed, large empty hazy sky above for text overlay. --ar 16:9 --style raw --stylize 250 --no text, watermark, frame, people in foreground, faces, bright colours, blue sky, modern objects, cartoon, anime, ruins, rubble, broken, derelict, decay
```

### `palace`
```
The capital's Roman palace: a towering royal keep and columned throne-hall with tarnished bronze and gold accents, sweeping oxblood banners, armoured guards at the gate, commanding the village from on high, magnificent in the pale haze. Painterly digital concept art, moody and cinematic, monumental and orderly. Wide establishing shot, the structure reading dark against a glowing pale sky; strong hazy backlight, warm sepia and dusty cream tones with muted oxblood accents; a thriving ancient-Roman city solid in the dusty haze behind, soft atmospheric perspective, gentle drifting dust, desaturated, highly detailed, large empty hazy sky above for text overlay. --ar 16:9 --style raw --stylize 250 --no text, watermark, frame, people in foreground, faces, bright colours, blue sky, modern objects, cartoon, anime, ruins, rubble, broken, derelict, decay
```

### `treasury`  *(interior style block)*
```
A guarded Roman vault holding a captured artifact: a heavy dressed-stone treasury, an ancient relic on a pedestal radiating a faint eerie glow, iron-bound chests and chains, a single shaft of dusty light, deep sepia shadow. Painterly digital concept art, moody and cinematic. Dim interior in warm sepia and dusty cream tones with muted oxblood accents, lit by a single narrow shaft of dusty light against deep sepia shadow; soft atmospheric haze, fine drifting dust, desaturated, highly detailed, dark empty areas for text overlay. --ar 16:9 --style raw --stylize 250 --no text, watermark, frame, people in foreground, faces, bright colours, blue sky, modern objects, cartoon, anime, ruins, rubble, broken, derelict, decay
```

### `wonder`
```
The Roman Wonder of the World under construction: a colossal columned monument rising amid immense timber scaffolding and cranes, hundreds of tiny labourer-figures, an awe-and-dread silhouette against a vast pale hazy sky, the endgame of the war. Painterly digital concept art, moody and cinematic, monumental and orderly. Wide establishing shot, the structure reading dark against a glowing pale sky; strong hazy backlight, warm sepia and dusty cream tones with muted oxblood accents; a thriving ancient-Roman city solid in the dusty haze behind, soft atmospheric perspective, gentle drifting dust, desaturated, highly detailed, large empty hazy sky above for text overlay. --ar 16:9 --style raw --stylize 250 --no text, watermark, frame, people in foreground, faces, bright colours, blue sky, modern objects, cartoon, anime, ruins, rubble, broken, derelict, decay
```
