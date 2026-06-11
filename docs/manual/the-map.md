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
- **Blue tiles are oases** — they grant a production bonus (hover to see it). You'll be able to
  clear and occupy them in a later update.
- **Red tiles are Natar** — special tiles reserved for the end-game.
- **★ marks a village.** Hover it for the owner's name; your own is highlighted. Who owns what and
  where is public — but a village's troops, resources, and defences stay hidden until you scout.

## Getting around

Use **↑ North / ← West / East → / ↓ South** to shift the view a full screen, or type an `x` and `y`
and press **Go** to jump anywhere. The same map looks identical to every player and never changes —
it is generated once from the world's seed.
