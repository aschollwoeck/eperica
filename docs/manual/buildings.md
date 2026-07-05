# Building & upgrading

Your village grows one building and one field at a time. This page covers the village layout, how
the build queue works, and what every building is for — full prerequisites, max levels, and costs
for each one live in the generated reference, linked below.

## The village centre

Your village has **22 centre slots** (numbered 0–21). Three are pre-assigned:

| Slot | Building | Notes |
|------|----------|-------|
| 0 | **Main Building** | Built at founding. Unique; can never be demolished. |
| 1 | **Rally Point** | Built at founding. Reserved — only the Rally Point may go here. |
| 11 | **Wall** | Reserved — only the Wall may go here. |

The remaining **19 slots** are general-purpose: each starts empty, and clicking it opens a "build
here" menu of everything you're eligible to construct there.

Outside the centre sit your **18 resource fields** — four each of wood, clay, and iron, and six
croplands. See [Resources](resources.md) for their production rates and upgrade costs.

## The build queue

Click a slot or field and press **Upgrade** (or **Build** on an empty one). Resources are spent
immediately and a countdown starts; the new level applies on its own, even while you're offline.

Your village builds **one thing at a time** in the centre lane. **Romans are the exception** —
their dual queue runs a resource field and a centre building **simultaneously**, the tribe's
signature economic edge. The two lanes are independent: a field upgrade never blocks a centre
build, or the other way round.

## Demolishing a building

Once your **Main Building reaches level 10**, a **Demolish** button appears on any built
general-slot building — including the Rally Point and Wall, but never the Main Building or the
Palace. Demolition runs **level by level**: each order removes exactly one level and takes the
removed level's own build time (reduced by your Main Building level), occupying the build lane
like a normal construction order. It's always **free** — no resource cost. Demolishing the last
level frees the slot for something else; to clear a building fully, demolish it once per level.

## What each building is for

### Command & defence

- **Main Building** — the hub; higher levels build and upgrade everything else faster. Unlocks
  demolition and the Treasury path at level 10.
- **Rally Point** — send attacks, raids, reinforcements, and scouts from here; also lists your
  movements in flight.
- **Wall** — multiplies your defence in every battle fought at your village; each tribe's Wall has
  its own bonus and toughness against rams — see [Attacking & defending](combat.md).

### Economy

- **Warehouse** — stores wood, clay, and iron; build several and their capacities **add together**.
- **Granary** — stores crop, with the same stacking rule as the Warehouse.
- **Cranny** — hides a slice of your resources from raiders; stacks across multiple Crannies.
- **Marketplace** — trade with other villages; its level sets how many merchants you have.

### Military

- **Barracks** — trains your tribe's infantry (and the Teuton foot Scout).
- **Academy** — researches new units before they can be trained anywhere; your tier-1 infantry and
  Settlers need no research.
- **Smithy** — upgrades a researched unit's combat strength, level by level, capped at the
  Smithy's own level.
- **Stable** — trains cavalry and mounted scouts.
- **Workshop** — trains Rams (break Walls) and Catapults (break buildings).

### Expansion & diplomacy

- **Town Hall** — generates culture points, which gate how many villages you may hold.
- **Residence** — trains Settlers and administrators, and grants expansion slots.
- **Palace** — everything the Residence does, **plus** it marks that village your **capital**
  (higher field caps, unconquerable). Only one Palace at a time; a village holds a Residence *or*
  a Palace, never both.
- **Embassy** — required to found (level 3) or join (level 1) an **alliance**; your highest
  Embassy across all villages counts. See [Alliances & diplomacy](alliances.md).
- **Outpost** — lets you occupy oases; its level caps how many you can hold. See [Oases](oases.md).

### End-game

- **Treasury** — required to capture and hold an **artifact**; its level determines which artifact
  scope (small/large/unique) you can keep. See [Artifacts & the Natars](artifacts.md).

Every building's exact prerequisites, max level, and per-level costs are generated straight from
the world's rules — see [all buildings & prerequisites](/manual/reference/buildings).

## Tips

- Build a **Warehouse** and **Granary** early so production isn't wasted once stores fill up.
- Upgrade your **Main Building** first — it speeds up everything that follows.
- Multiple **Warehouses** and **Granaries** pay off in a mature village; their capacities stack.
- Start toward a **Town Hall** as soon as your Academy prerequisites allow — you'll want culture
  points flowing before you're ready to found a second village.
- Keep an **Outpost** if you have oases nearby worth occupying — the bonus compounds quickly.

## See also

- [Resources](resources.md) — field production and storage in detail.
- [Tribes, the Academy & the Smithy](tribes-and-units.md) — researching and upgrading units.
- [Oases](oases.md) — occupying oases with the Outpost.
- [Settling](settling.md) — culture points, Settlers, and the capital.
- [Artifacts & the Natars](artifacts.md) — the Treasury and artifact capture.
- [Player Manual index](README.md)
