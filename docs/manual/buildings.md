# Building & upgrading

> This page explains your village's layout, every building you can construct, and how demolition
> works.

## The village centre

Your village has **22 centre slots** (numbered 0–21). Three are pre-assigned:

| Slot | Building | Notes |
|------|----------|-------|
| 0 | **Main Building** | Built at founding. Unique; can never be demolished. |
| 1 | **Rally Point** | Built at founding. Reserved — only the Rally Point may go here. |
| 11 | **Wall** | Reserved — only the Wall may go here. |

The remaining **19 slots** are general-purpose: each starts empty and you choose what to build
there from a menu. A village with no resources built on a slot shows a "build here" option when
you click it.

## The build queue

Your village builds **one thing at a time** in the centre lane (one field upgrade or building
per queue slot). Romans are the exception — they have a **dual queue** and can work on a
resource field and a centre building simultaneously.

Ordering a build costs resources immediately and starts a countdown. The new level takes effect
automatically when the timer finishes, even if you are logged out.

## Resource fields

Your village has 18 resource fields outside the centre — four each of wood, clay, and iron, and
six croplands. Each can be upgraded up to level 10 (level 20 in your **capital**). See
[Resources](resources.md) for production rates and upgrade costs.

## Building roster

Every building can be upgraded to a **maximum of level 10** (the Wonder of the World is the
exception, reaching level 100). Prerequisites are listed as the minimum level of another
building required before construction can start. "None" means you can build it on any empty
slot from the start.

### Main Building

**Purpose:** The hub of your village. Higher levels reduce the time needed to build and upgrade
everything else — a level-2 Main Building builds about 12% faster, level 5 about 56% faster,
level 10 about 2.7× faster than the baseline.

**Prerequisites:** None (built at founding).

**Multi-instance:** No — one per village.

**Notable:** Upgrading to level 10 unlocks demolition (see below), and is required for the
Treasury.

---

### Rally Point

**Purpose:** The command post for your troops. You send attacks, raids, reinforcements, and
scouting missions from here. Also shows incoming and outgoing troop movements.

**Prerequisites:** None (built at founding; slot 1 is reserved for it).

**Multi-instance:** No.

---

### Wall

**Purpose:** Adds a defensive bonus to every battle fought at your village. Higher Wall levels
increase the bonus.

**Prerequisites:** None.

**Multi-instance:** No (slot 11 is reserved for it).

---

### Warehouse

**Purpose:** Stores wood, clay, and iron. Base capacity without a Warehouse is 800 of each. A
level-1 Warehouse raises total capacity to 1 200; level 10 to 12 000.

**Prerequisites:** Main Building level 1.

**Multi-instance:** Yes — you may build Warehouses in multiple slots. **All their capacities add
together**, so two level-5 Warehouses hold twice what a single one does.

---

### Granary

**Purpose:** Stores crop. Identical capacity progression to the Warehouse (base 800, level 10 =
12 000).

**Prerequisites:** Main Building level 1.

**Multi-instance:** Yes — same stacking rule as the Warehouse.

---

### Cranny

**Purpose:** Hides a portion of your resources from raiders. Crop stored in the Cranny is also
protected.

**Prerequisites:** None.

**Multi-instance:** Yes — the protection from all your Crannies stacks.

---

### Marketplace

**Purpose:** Enables trade with other villages. Higher levels unlock more merchants — the faster
you can send and receive goods.

**Prerequisites:** Main Building level 1.

**Multi-instance:** No.

---

### Embassy

**Purpose:** Required to found or join an **alliance**. Level 1 lets you join an alliance;
level 3 lets you found one. The level that counts is your **highest** Embassy across all your
villages. The alliance member cap (60 by default) is a fixed world setting — not controlled by
Embassy level. See [Alliances & diplomacy](alliances.md).

**Prerequisites:** Main Building level 1.

**Multi-instance:** No.

---

### Barracks

**Purpose:** Trains your tribe's **infantry** units. Higher levels speed up training. The
Teutonic **Scout** also trains here (not in the Stable).

**Prerequisites:** Main Building level 3.

**Multi-instance:** No.

---

### Academy

**Purpose:** Researches new units so they can be trained. You must research a unit here before
it appears in the Barracks, Stable, or Workshop. Your tribe's tier-1 infantry unit and Settlers
do not require research.

**Prerequisites:** Main Building level 3, Barracks level 3.

**Multi-instance:** No.

---

### Smithy

**Purpose:** Upgrades a researched unit's combat strength. A unit's level can never exceed the
Smithy's own level.

**Prerequisites:** Main Building level 3, Academy level 1.

**Multi-instance:** No.

---

### Stable

**Purpose:** Trains your tribe's **cavalry** and **mounted scout** (Romans: Equites Legati;
Gauls: Pathfinder). Higher levels speed up training.

**Prerequisites:** Academy level 5, Smithy level 1.

**Multi-instance:** No.

---

### Workshop

**Purpose:** Trains **siege engines** — Rams and Catapults (or their tribe-specific
equivalents). Rams smash Walls; Catapults destroy buildings.

**Prerequisites:** Main Building level 5, Academy level 10.

**Multi-instance:** No.

---

### Town Hall

**Purpose:** Generates **culture points** (CP), which determine how many villages you may
found or hold. Higher levels produce more CP per hour.

**Prerequisites:** Main Building level 5, Academy level 10.

**Multi-instance:** No.

---

### Residence

**Purpose:** Lets you train **Settlers** (to found new villages) and **administrators** (to
conquer enemy villages). Also unlocks expansion village slots at certain Residence levels.

**Prerequisites:** Main Building level 5.

**Multi-instance:** No. A village may have a Residence **or** a Palace — not both.

---

### Palace

**Purpose:** Identical expansion function to the Residence, **plus** it designates this village
as your **capital** — which raises resource field caps to level 20. Only one Palace may exist
across all your villages at a time. The Palace **can never be demolished**.

**Prerequisites:** Main Building level 5.

**Multi-instance:** No (one per player, not one per village). Cannot coexist with a Residence in
the same village.

---

### Outpost

**Purpose:** Controls how many **oases** your village can occupy. A level-1 Outpost lets you
hold 1 oasis; higher levels unlock more (up to 6 at level 10). Without an Outpost you can
clear oasis animals but cannot occupy.

**Prerequisites:** Main Building level 3, Rally Point level 1.

**Multi-instance:** No.

---

### Treasury

**Purpose:** An **end-game building** required to capture and hold an **artifact**. The Treasury
level determines which artifact scope you can hold (small, large, or unique). See
[Artifacts & the Natars](artifacts.md).

**Prerequisites:** Main Building level 10.

**Multi-instance:** No.

---

## Demolition

Once your **Main Building reaches level 10**, you can demolish any built general-slot building
(including the Rally Point and Wall — but **not** the Main Building itself and not the Palace).

Demolition works **level by level**: each demolish order removes one level (e.g. a level-5
building becomes level 4). Demolishing the last level (from level 1 to 0) frees the slot so you
can build something else there.

Each demolish order:

- Is **free** — no resource cost.
- **Occupies the build lane** like a normal construction order, and takes roughly the build
  time of the level below, scaled by your Main Building level.
- Is issued from the building's page (a "Demolish" button appears when the condition is met).

To fully clear a building you need to demolish once per level — the game does not auto-continue.

## Roman dual queue

Roman players can run **two construction orders simultaneously**: one on a resource field and one
on a centre building. This is the key Roman economic advantage. The two lanes are independent —
a field upgrade in progress does not block a centre build, and vice versa.

## Tips

- Build a **Warehouse** and **Granary** early so production is not wasted once stores fill up.
- Upgrade your **Main Building** first to speed up everything that follows.
- Multiple **Warehouses** and **Granaries** are worth it in a mature village — their capacities
  add up.
- A **Town Hall** is essential for founding new villages; start working toward it once you have
  the Academy prerequisites met.
- Keep an **Outpost** if you have nearby oases worth occupying — the production bonus compounds
  quickly.

## See also

- [Resources](resources.md) — field production and storage.
- [Tribes, the Academy & the Smithy](tribes-and-units.md) — researching and upgrading units.
- [Oases](oases.md) — occupying oases with the Outpost.
- [Settling](settling.md) — culture points, Settlers, and the capital.
- [Artifacts & the Natars](artifacts.md) — the Treasury and artifact capture.
- [Player Manual index](README.md)
