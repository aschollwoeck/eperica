# The world map

The world is one big shared grid of tiles, addressed by coordinates `(x|y)` with `(0|0)` at the
centre. It **wraps at the edges** — travel off the far east and you arrive in the far west — so
there is no real corner to hide in.

## Reading the map

Open the **Map** link from your village page. You'll see a grid centred on your village:

- **Green tiles are valleys** — the only tiles a village can sit on. Each valley has a fixed field
  layout (its woodcutters / clay pits / iron mines / croplands), shown when you hover it. Most are
  the balanced `4·4·4·6`; rare **croppers** like `3·3·3·9` or `1·1·1·15` trade other resources for
  huge crop output and are prime settling targets later.
- **Blue tiles are oases** — they grant a production bonus (hover to see it). Clear the animals
  and claim one through your Outpost to add its bonus to your village — see **[Oases](oases.md)**.
- **Red tiles are Natar** — special tiles reserved for the end-game.
- **★ marks a village.** Hover it for the owner's name; your own is highlighted. Who owns what and
  where is public — but a village's troops, resources, and defences stay hidden until you scout.

## Getting around

The map is a **drag-to-pan canvas** — click and drag in any direction to explore; tiles stream in as
you move. To jump straight to a known location, use the **"Go to x|y"** field: type the `x` and `y`
coordinates and press **Go**. The same map looks identical to every player and never changes — it is
generated once from the world's seed.

**Tile info cards** appear when you click a tile. Where an action makes sense the card offers it
directly — for example, clicking an occupied village lets you **send a merchant** there from your
Marketplace, and clicking a free valley lets you **settle** on it once you have settlers ready.
