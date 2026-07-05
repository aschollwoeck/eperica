# Training troops & feeding your army

Every unit you field has to be trained somewhere, and every unit you keep eats crop every hour.
This page covers where to train each kind of troop, and the upkeep rule that puts a natural
ceiling on army size.

## Training a batch

Units are trained at three buildings, each with its own queue:

- **Barracks** — infantry (and the Teuton foot scout).
- **Stable** — cavalry and mounted scouts (needs Academy 5 and a Smithy).
- **Workshop** — rams and catapults (needs Main Building 5 and Academy 10).

How to train:

1. Open the building from the village page and pick a **researched** unit (your first unit is
   always ready; research the rest in the Academy).
2. Enter how many to train and press **Train**. The **full cost** is paid up front.
3. Units finish **one at a time** — each joins your garrison the moment it completes, even while
   you are offline. The page shows how many remain and a countdown to the next one.

One batch runs per building at a time (the three buildings train in parallel); a higher building
level trains faster. Your garrison is listed on the village page.

## Crop upkeep — feed them or lose them

Every garrisoned unit eats crop every hour (its **upkeep**). Your net crop rate now reads:

> crop fields − population − army upkeep

If the net is **negative**, your crop store drains. The moment it hits **zero, troops starve**:
units die — the hungriest types first — until your income can feed what remains. This is the
natural limit on army size.

To avoid starvation: raise croplands, demolish nothing you need, or simply train fewer mouths.
Watch the crop line on the village page — it turns into a warning when your net is zero or
negative.

> **Warning:** Upkeep is not speed-scaled. On a faster world your fields produce more crop per
> real-time hour — but each unit eats exactly the same amount regardless of world speed. A large
> army is just as hungry on a 3× world as on a 1× one. Plan your army size against your cropland
> output, not the world speed.

## See also

- [Resources](resources.md) — crop production and the net-crop formula.
- [Tribes, the Academy & the Smithy](tribes-and-units.md) — each unit's exact upkeep and cost.
- [Building & upgrading](buildings.md) — Barracks, Stable, and Workshop prerequisites.
- [Player Manual index](README.md)
