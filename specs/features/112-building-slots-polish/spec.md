# Feature 112 — slot-based buildings: polish & a demolition correctness fix

Follow-ups to 110/111.

## AC1 — The Palace can't be demolished (capital integrity)
The Palace designates the **capital** (013) and grants conquest immunity. Demolishing it left `is_capital`
set with no Palace — an inconsistent state. The Palace joins the Main Building as **non-demolishable**
(`is_demolishable` false). Relocating the capital is still done by building a Palace elsewhere (which
reassigns the capital, 013 AC9). The Residence stays demolishable (it doesn't touch `is_capital`).

## AC2 — Unbuilt buildings are built from a free slot, not a fixed-slot button
A building's kind page (a functional page like the Academy, or the generic `/building/{kind}`) still renders
when the building isn't built yet (e.g. the Academy explains its requirement), but its panel now offers a
**"Build on a free slot"** link to the village plan instead of a Build button that posted to a legacy fixed
slot (which could collide with whatever the player put there). Construction happens via the empty-slot build
menu (110). Resource **fields** (positional) keep their in-place build form; built buildings keep their
upgrade form.

## AC3 — The village plan reads as a deliberate layout
The 22-slot centre is arranged to read clearly within the fortress walls (reserved Rally Point / Wall
emphasised; empty build spots are an obvious, inviting affordance) rather than a flat grid.

## Out of scope
- Level-by-level demolition — **now implemented in slice 113**.
