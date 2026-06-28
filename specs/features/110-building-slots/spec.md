# Feature 110 — slot-based village buildings (multiplicity, stacked effects, demolition)

## Why

Today every building kind maps to one **fixed slot** (`building_slot(kind)` → 0–18), so a village holds at
most one of each kind and the village plan positions plots by kind. This was an explicit simplification in
slice 003 (*"Building slots are fixed per kind … dynamic slots are later work"*). This slice realizes the
village-centre model the game design already commits to (`game-design.md §3.2`: *"~20 general slots plus fixed
special positions for the Rally Point and Wall"*): a fixed set of build **spots**, a few default/reserved
buildings, and **free spots** where the player chooses what to build — including **multiple** Warehouses /
Granaries / Crannies whose effects **add up**.

This supersedes the 003 "fixed slot per kind" decision and the `002`/`011` "the warehouse cap / a Cranny"
singular phrasing.

## Decisions (confirmed)

1. **Layout — faithful.** 22 village-centre slots (`0..=21`): **20 general** + **2 reserved** (Rally Point,
   Wall). The 18 resource fields are unchanged and separate.
2. **Multi-instance — Warehouse, Granary, Cranny**, bounded only by free slots (no per-kind cap). Every other
   kind is one-per-village.
3. **Migrate in place** — existing buildings keep their current slot numbers; no reset, no data loss.
4. **Demolition included** — Main-Building-gated; frees a slot to rebuild.

## Model

- **Slot space.** A village centre has `VILLAGE_BUILDING_SLOTS = 22` spots, `slot ∈ 0..=21`.
  - **Reserved slots** hold only their kind and that kind goes only there: **Rally Point → slot 1**,
    **Wall → slot 11** (kept at today's numbers so existing rows migrate untouched).
  - **Main Building → slot 0**, default-built at level 1, unique, non-demolishable.
  - The remaining **19 slots** plus slot 0 = **20 general slots**; general slots accept any non-reserved,
    non-Main-Building kind subject to the rules below.
- **`BuildingSlot` gains `slot: u8`.** `Village.buildings: Vec<BuildingSlot>` is sparse — only built slots
  are present; an absent slot is empty. (Slot is read from the existing `village_buildings.slot` column.)
- **Multiplicity.** `building_max_instances(kind)`: `Warehouse | Granary | Cranny → None` (slot-bounded);
  every other kind → `1`. Residence ⟷ Palace stay mutually exclusive (013).
- **Stacked effects (the correctness core, P2).** For a village's building set:
  - **Warehouse capacity** = Σ over Warehouse instances of `warehouse_capacity(level)`.
  - **Granary capacity** = Σ over Granary instances of `granary_capacity(level)`.
  - **Cranny protection** = Σ over Cranny instances of `cranny_capacity(level)` (per resource type).
  - Unique buildings are unchanged (one instance → its level): Main Building speed, Marketplace merchants,
    Smithy/training levels, Wall bonus, Town Hall CP, Residence/Palace slots, Outpost capacity, Embassy, etc.
  - Population already sums per building and stays correct for multiple instances.
  - Base capacity (no Warehouse/Granary built) still applies when the sum is zero.

## Acceptance criteria

- **AC1 — Fixed slots.** A village has exactly 22 centre slots. Slot 0 = Main Building (default, unique,
  non-demolishable); slot 1 = Rally Point (default-built); slot 11 = the Wall slot. The 18 fields are
  unchanged.
- **AC2 — Build on a chosen empty slot.** Clicking an empty general slot offers the buildable kinds (excludes
  kinds already at their max instances, kinds whose requirements are unmet, and the reserved/Main-Building
  kinds); choosing one places **that** kind in **that** slot. The slot is **client-supplied but
  server-validated** (P4): owned village, slot in range, slot empty, kind allowed at that slot (reserved-slot
  rules + multiplicity + requirements + Residence/Palace exclusivity). An illegal placement is rejected and
  changes nothing.
- **AC3 — Reserved slots.** The Rally Point and Wall build only on their reserved slot, and those slots accept
  only that kind. A request to put any other kind on a reserved slot, or Rally/Wall on a general slot, is
  rejected.
- **AC4 — Multiple instances stack.** A village may build a second (third, …) Warehouse / Granary / Cranny on
  any free general slot. Total storage capacity is the **sum** of every Warehouse's capacity at its level
  (Granary likewise for crop; Cranny likewise for protection). No other kind can be built more than once.
- **AC5 — Upgrade by slot.** Clicking a built slot upgrades the building **in that slot** (the existing
  per-slot upgrade path, now keyed by slot rather than re-derived from kind). Costs/gates/queue (003) unchanged.
- **AC6 — Demolition.** When the Main Building is at least the configured demolition level, a built general
  slot (and the Rally Point / Wall, but **not** the Main Building) can be demolished, which clears the slot
  back to empty (server-validated, server-authoritative, due-processed via the build queue like a build).
  A freed general slot becomes buildable again.
- **AC7 — Slot-keyed plan.** The village plan renders the 22 centre slots at fixed positions **by slot**
  (not by kind); each slot shows its building + level, or an empty "build here" affordance; reserved slots are
  visually distinct. Multiple Warehouses each appear in their own slot.
- **AC8 — Migration safety.** Existing villages keep every current building at its current slot and continue
  to render and function; the storage capacity of a single-Warehouse village is unchanged from before.

## Roles
- **Player** — builds/upgrades/demolishes on slots in their own villages only (P4). No new role; the existing
  village-ownership checks gate every action. **Visitor** — no access (redirect to login), as today.

## Constitution
- **P1** — demolition is a due-timestamped event in the build queue, processed when due (not ticked).
- **P2/P4** — placement, multiplicity, reserved-slot, and demolition rules are validated server-side; the
  client never decides a slot's legality. Effect sums are pure-domain and unit-tested.
- **P7** — slot count is a structural constant (like `RESOURCE_FIELD_COUNT = 18`); no wall-clock values added.
- **P11** — reads stay O(buildings) (≤22); summing is trivial.

## Out of scope
- Per-kind instance caps and "Great Warehouse/Granary" variants (free-slot-bounded only, by decision).
- Level-by-level demolition — **now implemented in slice 113** (each order removes one level).
- Changing the resource-field model, the build queue/lanes (003), or balance curve values.
- A drag-to-rearrange village layout (slots are at fixed positions).

## Risks
- **Effect aggregation is correctness-critical (P2):** the `.max()/.find()` → `sum` change for
  storage/cranny must be covered by domain tests, including the single-instance case (must equal today's
  value) and the base-capacity floor.
- **Build-flow inversion (P4):** the slot moves from server-derived to client-supplied; the new validation
  must be airtight against placing on an occupied/foreign/reserved slot or exceeding multiplicity.
- **Reserved-slot numbers** (Rally 1, Wall 11) are pinned to today's `building_slot` values so existing rows
  need no data migration; a DB CHECK (optional) would assert it.
