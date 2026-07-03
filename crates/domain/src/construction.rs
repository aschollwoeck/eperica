//! Construction — build targets, costs, times, and prerequisites (pure rules over injected balance).

use crate::building::{BuildingKind, VILLAGE_BUILDING_SLOTS, reserved_kind};
use crate::economy::ResourceAmounts;
use crate::resource::ResourceKind;
use crate::village::{BuildingSlot, Tribe};
use crate::world::GameSpeed;
use std::collections::HashMap;

/// What a build order acts on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildTarget {
    /// Upgrade the resource field in this slot.
    Field { slot: u8 },
    /// Upgrade (or construct, if the slot is empty) the building of this kind in this slot.
    Building { slot: u8, kind: BuildingKind },
}

/// Cost/time for a sequence of levels. `cost_per_level[i]` is the cost to reach level `i + 1`.
#[derive(Debug, Clone)]
pub struct LevelSpec {
    pub cost_per_level: Vec<ResourceAmounts>,
    pub time_secs_per_level: Vec<i64>,
}

impl LevelSpec {
    /// Highest reachable level (length of the cost table).
    pub fn max_level(&self) -> u8 {
        u8::try_from(self.cost_per_level.len()).unwrap_or(u8::MAX)
    }
    fn cost(&self, current_level: u8) -> Option<ResourceAmounts> {
        self.cost_per_level.get(current_level as usize).copied()
    }
    fn time(&self, current_level: u8) -> Option<i64> {
        self.time_secs_per_level
            .get(current_level as usize)
            .copied()
    }
}

/// Injected construction balance.
#[derive(Debug, Clone)]
pub struct BuildRules {
    /// Spec shared by wood/clay/iron resource fields (its cost/time tables run to the **capital** cap).
    pub field: LevelSpec,
    /// Cropland's own upgrade **cost** table (faithful Travian: cheaper crop, 7:9:7:2 ratio). Croplands
    /// share `field`'s time and level caps — only the per-resource cost differs, so this is just the cost.
    pub crop_field_cost: Vec<ResourceAmounts>,
    /// The normal resource-field level cap (a non-capital village; 013 §3.4).
    pub field_max_level: u8,
    /// The resource-field level cap for a **capital** village (> `field_max_level`).
    pub capital_field_max_level: u8,
    /// Per-building-kind specs.
    pub buildings: HashMap<BuildingKind, LevelSpec>,
    /// Building prerequisites: to build/upgrade the key, each `(kind, level)` must be met.
    pub prerequisites: HashMap<BuildingKind, Vec<(BuildingKind, u8)>>,
    /// Construction-speed multiplier by Main Building level (≥ 1.0; higher ⇒ faster). Index = level.
    pub main_building_factor_per_level: Vec<f64>,
}

impl BuildRules {
    fn spec(&self, target: BuildTarget) -> Option<&LevelSpec> {
        match target {
            BuildTarget::Field { .. } => Some(&self.field),
            BuildTarget::Building { kind, .. } => self.buildings.get(&kind),
        }
    }

    /// Cost to raise `target` from `current_level` to the next; `None` if at max or unknown.
    ///
    /// For resource fields this returns the **shared** (wood/clay/iron) table; use [`Self::field_cost`]
    /// where the field's resource is known so croplands charge their own table.
    pub fn cost(&self, target: BuildTarget, current_level: u8) -> Option<ResourceAmounts> {
        self.spec(target)?.cost(current_level)
    }

    /// Cost to raise a resource field of `resource` from `current_level` to the next — croplands use their
    /// own cheaper table; wood/clay/iron fields share `field`. `None` at/above the table's end. Callers
    /// with the acting village (order-time debit + upgrade display) route field costs through this so the
    /// charged amount matches the shown amount (P4).
    #[must_use]
    pub fn field_cost(&self, resource: ResourceKind, current_level: u8) -> Option<ResourceAmounts> {
        let table = if resource == ResourceKind::Crop {
            &self.crop_field_cost
        } else {
            &self.field.cost_per_level
        };
        table.get(current_level as usize).copied()
    }

    /// Base build time (seconds, before speed/MB) for the next level; `None` if at max or unknown.
    pub fn base_time_secs(&self, target: BuildTarget, current_level: u8) -> Option<i64> {
        self.spec(target)?.time(current_level)
    }

    /// Max level for a target; 0 if unknown. Resource fields use the **normal** field cap (the
    /// capital's raised cap is [`BuildRules::field_max_level`]); buildings use their table length.
    pub fn max_level(&self, target: BuildTarget) -> u8 {
        match target {
            BuildTarget::Field { .. } => self.field_max_level,
            BuildTarget::Building { kind, .. } => {
                self.buildings.get(&kind).map_or(0, LevelSpec::max_level)
            }
        }
    }

    /// The resource-field level cap for a village, raised for the **capital** (013 AC10, §3.4).
    #[must_use]
    pub fn field_max_level(&self, is_capital: bool) -> u8 {
        if is_capital {
            self.capital_field_max_level
        } else {
            self.field_max_level
        }
    }

    /// Main Building speed factor at `mb_level` (clamped to the table).
    pub fn main_building_factor(&self, mb_level: u8) -> f64 {
        self.main_building_factor_per_level
            .get(mb_level as usize)
            .or_else(|| self.main_building_factor_per_level.last())
            .copied()
            .unwrap_or(1.0)
    }

    /// Prerequisites for constructing/upgrading a building kind.
    pub fn prerequisites(&self, kind: BuildingKind) -> &[(BuildingKind, u8)] {
        self.prerequisites.get(&kind).map_or(&[][..], Vec::as_slice)
    }
}

/// Effective build time after applying world speed and the Main Building factor (≥ 1 second).
pub fn build_time_secs(base_secs: i64, mb_level: u8, rules: &BuildRules, speed: GameSpeed) -> i64 {
    let divisor = speed.multiplier() * rules.main_building_factor(mb_level);
    ((base_secs as f64 / divisor).round() as i64).max(1)
}

/// Whether `amounts` covers `cost`.
pub fn can_afford(amounts: ResourceAmounts, cost: ResourceAmounts) -> bool {
    amounts.wood >= cost.wood
        && amounts.clay >= cost.clay
        && amounts.iron >= cost.iron
        && amounts.crop >= cost.crop
}

/// Subtract `cost` from `amounts` (caller must have checked affordability).
pub fn debit(amounts: ResourceAmounts, cost: ResourceAmounts) -> ResourceAmounts {
    ResourceAmounts {
        wood: amounts.wood - cost.wood,
        clay: amounts.clay - cost.clay,
        iron: amounts.iron - cost.iron,
        crop: amounts.crop - cost.crop,
    }
}

/// The build-queue lane an order occupies. At most one active order may hold a lane (enforced by
/// the persistence layer even under races, P4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueLane {
    /// The single lane shared by all targets (non-Roman tribes).
    All,
    /// The Roman resource-field lane.
    Field,
    /// The Roman center-building lane.
    Building,
}

/// The lane a build order of `tribe` for `target` occupies — the Roman trait (004 AC13): Romans
/// run one field order and one center-building order in parallel; other tribes have one lane.
pub fn queue_lane(tribe: Tribe, target: BuildTarget) -> QueueLane {
    match (tribe, target) {
        (Tribe::Romans, BuildTarget::Field { .. }) => QueueLane::Field,
        (Tribe::Romans, BuildTarget::Building { .. }) => QueueLane::Building,
        _ => QueueLane::All,
    }
}

/// Whether every `(kind, level)` requirement is satisfied by the village's `buildings`.
pub fn building_levels_met(
    requirements: &[(BuildingKind, u8)],
    buildings: &[BuildingSlot],
) -> bool {
    requirements.iter().all(|(req_kind, req_level)| {
        buildings
            .iter()
            .any(|b| b.kind == *req_kind && b.level >= *req_level)
    })
}

/// Whether all prerequisites for `kind` are satisfied by the village's `buildings`.
pub fn prerequisites_met(
    kind: BuildingKind,
    buildings: &[BuildingSlot],
    rules: &BuildRules,
) -> bool {
    building_levels_met(rules.prerequisites(kind), buildings)
}

/// The building occupying centre `slot`, if any (110).
pub fn building_at(buildings: &[BuildingSlot], slot: u8) -> Option<&BuildingSlot> {
    buildings.iter().find(|b| b.slot == slot)
}

/// The empty centre slots — candidate spots for new construction (110).
pub fn free_slots(buildings: &[BuildingSlot]) -> Vec<u8> {
    (0..VILLAGE_BUILDING_SLOTS)
        .filter(|s| building_at(buildings, *s).is_none())
        .collect()
}

/// Why a kind may not be newly placed on a centre slot (110, AC2/AC3/AC4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlacementError {
    /// The slot number is outside `0..VILLAGE_BUILDING_SLOTS`.
    OutOfRange,
    /// The slot already holds a building (build on an empty slot; upgrade the one in place).
    Occupied,
    /// The slot is reserved for a different kind (only the Rally Point may sit on its slot, etc.).
    SlotReserved,
    /// This kind has its own reserved slot and may not be placed elsewhere (Main Building/Rally/Wall).
    WrongSlot,
    /// The village already holds the maximum number of this (unique) kind.
    MaxInstances,
}

/// Whether `kind` may be **newly built** on empty centre `slot`, given the village's current
/// `buildings` (110, AC2/AC3/AC4). Pure slot/multiplicity validation; cross-kind exclusivity
/// (Residence ⟷ Palace), prerequisites, and affordability are the caller's job. Upgrades never go
/// through this — they act on the kind already in the slot.
pub fn can_place(
    buildings: &[BuildingSlot],
    slot: u8,
    kind: BuildingKind,
) -> Result<(), PlacementError> {
    if slot >= VILLAGE_BUILDING_SLOTS {
        return Err(PlacementError::OutOfRange);
    }
    if building_at(buildings, slot).is_some() {
        return Err(PlacementError::Occupied);
    }
    // A reserved slot accepts only its kind; a kind with a reserved slot goes only there.
    if let Some(rk) = reserved_kind(slot)
        && rk != kind
    {
        return Err(PlacementError::SlotReserved);
    }
    if let Some(rs) = kind.reserved_slot()
        && rs != slot
    {
        return Err(PlacementError::WrongSlot);
    }
    if let Some(max) = kind.max_instances() {
        let count = buildings.iter().filter(|b| b.kind == kind).count() as u32;
        if count >= max {
            return Err(PlacementError::MaxInstances);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn amounts(n: i64) -> ResourceAmounts {
        ResourceAmounts {
            wood: n,
            clay: n,
            iron: n,
            crop: n,
        }
    }

    fn rules() -> BuildRules {
        let field = LevelSpec {
            cost_per_level: vec![amounts(40), amounts(100), amounts(250)],
            time_secs_per_level: vec![600, 1200, 2400],
        };
        let mut buildings = HashMap::new();
        buildings.insert(
            BuildingKind::Warehouse,
            LevelSpec {
                cost_per_level: vec![amounts(50), amounts(120)],
                time_secs_per_level: vec![800, 1500],
            },
        );
        let mut prerequisites = HashMap::new();
        prerequisites.insert(
            BuildingKind::Warehouse,
            vec![(BuildingKind::MainBuilding, 1)],
        );
        BuildRules {
            field,
            // Cropland cost distinct from the shared field cost, so `field_cost` differentiation is testable.
            crop_field_cost: vec![amounts(20), amounts(60), amounts(150)],
            field_max_level: 3,
            capital_field_max_level: 5,
            buildings,
            prerequisites,
            main_building_factor_per_level: vec![1.0, 1.2, 1.5],
        }
    }

    #[test]
    fn cost_and_max_level() {
        let r = rules();
        let f = BuildTarget::Field { slot: 0 };
        assert_eq!(r.cost(f, 0), Some(amounts(40))); // level 0 -> 1
        assert_eq!(r.cost(f, 2), Some(amounts(250))); // level 2 -> 3
        assert_eq!(r.cost(f, 3), None); // beyond the table
        assert_eq!(r.max_level(f), 3); // the normal field cap
    }

    // Croplands charge their own cost table; wood/clay/iron fields share `field` (== the generic `cost`).
    #[test]
    fn field_cost_differentiates_cropland() {
        let r = rules();
        let f = BuildTarget::Field { slot: 0 };
        // Wood/clay/iron fields: field_cost matches the shared table (and the generic cost()).
        for res in [ResourceKind::Wood, ResourceKind::Clay, ResourceKind::Iron] {
            assert_eq!(r.field_cost(res, 0), Some(amounts(40)));
            assert_eq!(r.field_cost(res, 0), r.cost(f, 0));
        }
        // Cropland: its own (cheaper) table, distinct from the shared one.
        assert_eq!(r.field_cost(ResourceKind::Crop, 0), Some(amounts(20)));
        assert_eq!(r.field_cost(ResourceKind::Crop, 2), Some(amounts(150)));
        assert_ne!(r.field_cost(ResourceKind::Crop, 0), r.cost(f, 0));
        assert_eq!(r.field_cost(ResourceKind::Crop, 3), None); // beyond the table
    }

    // 013 AC10: a capital may raise its fields past the normal cap; a non-capital may not.
    #[test]
    fn capital_raises_the_field_cap() {
        let r = rules();
        assert_eq!(r.field_max_level(false), 3, "normal field cap");
        assert_eq!(r.field_max_level(true), 5, "capital field cap");
        assert!(r.field_max_level(true) > r.field_max_level(false));
    }

    #[test]
    fn affordability_and_debit() {
        assert!(can_afford(amounts(100), amounts(40)));
        assert!(!can_afford(amounts(30), amounts(40)));
        assert_eq!(debit(amounts(100), amounts(40)), amounts(60));
    }

    #[test]
    fn main_building_speeds_construction() {
        // AC6: higher Main Building level => strictly shorter build time.
        let r = rules();
        let s = GameSpeed::new(1.0).unwrap();
        let t0 = build_time_secs(1200, 0, &r, s);
        let t1 = build_time_secs(1200, 1, &r, s);
        let t2 = build_time_secs(1200, 2, &r, s);
        assert!(t1 < t0 && t2 < t1, "{t0} {t1} {t2}");
    }

    #[test]
    fn speed_scales_construction() {
        // AC7: higher world speed => proportionally shorter build time.
        let r = rules();
        let t1 = build_time_secs(1200, 0, &r, GameSpeed::new(1.0).unwrap());
        let t2 = build_time_secs(1200, 0, &r, GameSpeed::new(2.0).unwrap());
        assert_eq!(t1, 1200);
        assert_eq!(t2, 600);
    }

    #[test]
    fn prerequisites_gate() {
        // AC4: Warehouse needs Main Building >= 1.
        let r = rules();
        let none: Vec<BuildingSlot> = vec![];
        assert!(!prerequisites_met(BuildingKind::Warehouse, &none, &r));
        let with_mb = vec![BuildingSlot {
            slot: 0,
            kind: BuildingKind::MainBuilding,
            level: 1,
        }];
        assert!(prerequisites_met(BuildingKind::Warehouse, &with_mb, &r));
        // A field has no prerequisites.
        assert!(prerequisites_met(BuildingKind::RallyPoint, &none, &r));
    }

    // --- 110: slot placement ---

    fn bld(slot: u8, kind: BuildingKind, level: u8) -> BuildingSlot {
        BuildingSlot { slot, kind, level }
    }

    #[test]
    fn placement_accepts_a_free_slot_and_rejects_occupied_or_out_of_range() {
        let v = vec![
            bld(0, BuildingKind::MainBuilding, 1),
            bld(1, BuildingKind::RallyPoint, 1),
        ];
        assert_eq!(can_place(&v, 2, BuildingKind::Marketplace), Ok(()));
        assert_eq!(
            can_place(&v, 0, BuildingKind::Marketplace),
            Err(PlacementError::Occupied)
        );
        assert_eq!(
            can_place(&v, VILLAGE_BUILDING_SLOTS, BuildingKind::Marketplace),
            Err(PlacementError::OutOfRange)
        );
    }

    #[test]
    fn reserved_slots_bind_their_kind_both_ways() {
        use crate::building::WALL_SLOT;
        let v = vec![bld(0, BuildingKind::MainBuilding, 1)];
        // the Rally Point slot (1) accepts only the Rally Point...
        assert_eq!(
            can_place(&v, 1, BuildingKind::Marketplace),
            Err(PlacementError::SlotReserved)
        );
        assert_eq!(can_place(&v, 1, BuildingKind::RallyPoint), Ok(()));
        // ...and the Wall builds only on its reserved slot, never a general one.
        assert_eq!(
            can_place(&v, 2, BuildingKind::Wall),
            Err(PlacementError::WrongSlot)
        );
        assert_eq!(can_place(&v, WALL_SLOT, BuildingKind::Wall), Ok(()));
    }

    #[test]
    fn multi_instance_kinds_stack_unique_kinds_do_not() {
        let mut v = vec![
            bld(0, BuildingKind::MainBuilding, 1),
            bld(2, BuildingKind::Marketplace, 1),
        ];
        // a second Marketplace (unique) is rejected.
        assert_eq!(
            can_place(&v, 3, BuildingKind::Marketplace),
            Err(PlacementError::MaxInstances)
        );
        // multiple Warehouses are allowed on free slots.
        v.push(bld(4, BuildingKind::Warehouse, 1));
        assert_eq!(can_place(&v, 5, BuildingKind::Warehouse), Ok(()));
        v.push(bld(5, BuildingKind::Warehouse, 1));
        assert_eq!(can_place(&v, 6, BuildingKind::Warehouse), Ok(()));
    }

    #[test]
    fn free_slots_lists_every_empty_centre_slot() {
        let v = vec![
            bld(0, BuildingKind::MainBuilding, 1),
            bld(1, BuildingKind::RallyPoint, 1),
            bld(5, BuildingKind::Warehouse, 1),
        ];
        let free = free_slots(&v);
        assert_eq!(free.len() as u8, VILLAGE_BUILDING_SLOTS - 3);
        assert!(!free.contains(&0) && !free.contains(&1) && !free.contains(&5));
        assert!(free.contains(&2) && free.contains(&(VILLAGE_BUILDING_SLOTS - 1)));
    }

    #[test]
    fn romans_get_a_lane_per_target_category() {
        // 004 AC13: Romans build a field and a center building in parallel; others do not.
        let field = BuildTarget::Field { slot: 3 };
        let building = BuildTarget::Building {
            slot: 0,
            kind: BuildingKind::MainBuilding,
        };
        assert_eq!(queue_lane(Tribe::Romans, field), QueueLane::Field);
        assert_eq!(queue_lane(Tribe::Romans, building), QueueLane::Building);
        for tribe in [Tribe::Teutons, Tribe::Gauls] {
            assert_eq!(queue_lane(tribe, field), QueueLane::All);
            assert_eq!(queue_lane(tribe, building), QueueLane::All);
        }
    }
}
