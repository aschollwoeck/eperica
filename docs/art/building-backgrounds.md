# Art prompts — building backgrounds & unit portraits

AI image prompts for the game's art, all in the landing page's "Grim Forged Steel" look (cold gunmetal night,
torch/ember warmth, tarnished brass, heavy vignette). Two families:

- **[Building backgrounds](#building-backgrounds--ai-image-prompts)** — one 16:9 image per building, shown as a
  **darkened background** behind that building's page.
- **[Troop & unit portraits](#troop--unit-art--ai-image-prompts)** — one figure portrait per unit, for the
  roster / training cards.

# Building backgrounds — AI image prompts

One image per building, shown as a **darkened background** behind that building's page.

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
