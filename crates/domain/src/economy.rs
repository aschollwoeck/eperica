//! Resource economy — lazy production accrual (P1), storage capacity, and crop upkeep.
//!
//! All math is pure integer arithmetic over injected balance data ([`EconomyRules`]); there is no
//! background job — current amounts are computed on read from stored state + elapsed time (P1/P2).

use crate::building::BuildingKind;
use crate::map::OasisBonus;
use crate::resource::ResourceKind;
use crate::village::{BuildingSlot, ResourceField};
use crate::world::GameSpeed;
use std::collections::HashMap;

/// Stored resource amounts (integer units).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ResourceAmounts {
    pub wood: i64,
    pub clay: i64,
    pub iron: i64,
    pub crop: i64,
}

/// Hourly production rates (units/hour). `crop_net` is already net of upkeep and may be negative.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProductionRates {
    pub wood: i64,
    pub clay: i64,
    pub iron: i64,
    pub crop_net: i64,
}

/// Storage capacities: `warehouse` caps wood/clay/iron, `granary` caps crop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capacities {
    pub warehouse: i64,
    pub granary: i64,
}

/// A village's computed economy at an instant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Economy {
    pub amounts: ResourceAmounts,
    pub rates: ProductionRates,
    pub capacities: Capacities,
}

/// Injected balance data driving the economy (pure data; values come from the balance dataset).
#[derive(Debug, Clone)]
pub struct EconomyRules {
    /// Hourly production by field level, per resource (index = level; clamped to the last entry).
    pub wood_per_level: Vec<i64>,
    pub clay_per_level: Vec<i64>,
    pub iron_per_level: Vec<i64>,
    pub crop_per_level: Vec<i64>,
    /// Population added per resource-field level.
    pub field_population_per_level: Vec<i64>,
    /// Population added per building level, by kind. A kind absent from the map contributes 0
    /// (the balance loader is responsible for covering every constructable kind).
    pub building_population_per_level: HashMap<BuildingKind, Vec<i64>>,
    /// Warehouse capacity by Warehouse level (index 0 = base, i.e. no Warehouse).
    pub warehouse_capacity_per_level: Vec<i64>,
    /// Granary capacity by Granary level (index 0 = base, i.e. no Granary).
    pub granary_capacity_per_level: Vec<i64>,
    /// How many oases an Outpost may hold, by Outpost level (index = level; 012). Level 0 (no
    /// Outpost) holds none.
    pub outpost_capacity_per_level: Vec<u8>,
    /// Stored amounts a new village starts with.
    pub starting_amounts: ResourceAmounts,
}

impl EconomyRules {
    /// The number of oases a village whose Outpost is at `level` may occupy (012, **AC6**). Clamped
    /// to the table; an empty table or level 0 holds none.
    #[must_use]
    pub fn outpost_capacity(&self, level: u8) -> u8 {
        if self.outpost_capacity_per_level.is_empty() {
            return 0;
        }
        let idx = (level as usize).min(self.outpost_capacity_per_level.len() - 1);
        self.outpost_capacity_per_level[idx]
    }
}

fn level_value(table: &[i64], level: u8) -> i64 {
    table
        .get(level as usize)
        .copied()
        .unwrap_or_else(|| table.last().copied().unwrap_or(0))
}

impl EconomyRules {
    fn field_production(&self, kind: ResourceKind, level: u8) -> i64 {
        let table = match kind {
            ResourceKind::Wood => &self.wood_per_level,
            ResourceKind::Clay => &self.clay_per_level,
            ResourceKind::Iron => &self.iron_per_level,
            ResourceKind::Crop => &self.crop_per_level,
        };
        level_value(table, level)
    }

    fn building_population(&self, kind: BuildingKind, level: u8) -> i64 {
        self.building_population_per_level
            .get(&kind)
            .map_or(0, |table| level_value(table, level))
    }

    /// A single resource field's **gross, pre-oasis** hourly production at `level`, scaled by `speed` — the
    /// per-field base [`production_rates`] sums before applying any oasis bonus. Used to show an upgrade's
    /// effect (e.g. a crop field's new rate) without recomputing the whole village.
    #[must_use]
    pub fn field_production_per_hour(
        &self,
        kind: ResourceKind,
        level: u8,
        speed: GameSpeed,
    ) -> i64 {
        scale(self.field_production(kind, level), speed)
    }

    /// Population contributed by a resource field at `level` (clamped to the table).
    #[must_use]
    pub fn field_population(&self, level: u8) -> i64 {
        level_value(&self.field_population_per_level, level)
    }

    /// Population contributed by a building of `kind` at `level` (0 for an unknown kind / level 0).
    #[must_use]
    pub fn building_population_at(&self, kind: BuildingKind, level: u8) -> i64 {
        self.building_population(kind, level)
    }

    /// Warehouse storage capacity (wood/clay/iron) at `level` (level 0 = the base, no Warehouse).
    #[must_use]
    pub fn warehouse_capacity(&self, level: u8) -> i64 {
        level_value(&self.warehouse_capacity_per_level, level)
    }

    /// Granary storage capacity (crop) at `level` (level 0 = the base, no Granary).
    #[must_use]
    pub fn granary_capacity(&self, level: u8) -> i64 {
        level_value(&self.granary_capacity_per_level, level)
    }
}

/// Total village population — each point consumes 1 crop/hour.
pub fn population(
    fields: &[ResourceField],
    buildings: &[BuildingSlot],
    rules: &EconomyRules,
) -> i64 {
    let from_fields: i64 = fields
        .iter()
        .map(|f| level_value(&rules.field_population_per_level, f.level))
        .sum();
    let from_buildings: i64 = buildings
        .iter()
        .map(|b| rules.building_population(b.kind, b.level))
        .sum();
    from_fields + from_buildings
}

/// Apply `speed` to a base per-hour value, rounded to the nearest integer.
fn scale(base: i64, speed: GameSpeed) -> i64 {
    (base as f64 * speed.multiplier()).round() as i64
}

/// Hourly production rates for a village's fields/buildings at the given world speed (P7).
/// `troop_upkeep` is the garrison's total crop consumption per hour (005 AC6; 0 with no army).
/// `oasis_bonus` is the summed per-resource production bonus from the oases the village holds (012,
/// AC8): it boosts the **gross** field output of each resource (floor), before population/upkeep are
/// subtracted from crop.
///
/// 114: **production scales with world speed, but consumption (village population + troop upkeep) is
/// FIXED** — `crop_net = output×speed − population − upkeep` — so a fast world's scaled output dominates
/// the fixed upkeep and crop is abundant (faithful Travian). At speed 1× this is identical to before.
pub fn production_rates(
    fields: &[ResourceField],
    buildings: &[BuildingSlot],
    troop_upkeep: i64,
    rules: &EconomyRules,
    speed: GameSpeed,
    oasis_bonus: OasisBonus,
) -> ProductionRates {
    // Boost a gross hourly rate by a per-resource oasis percentage (integer floor).
    let boosted = |base: i64, pct: u8| -> i64 { base + base * i64::from(pct) / 100 };
    let base = |kind: ResourceKind| -> i64 {
        fields
            .iter()
            .filter(|f| f.kind == kind)
            .map(|f| rules.field_production(kind, f.level))
            .sum()
    };
    let crop_gross = boosted(base(ResourceKind::Crop), oasis_bonus.crop);
    ProductionRates {
        wood: scale(boosted(base(ResourceKind::Wood), oasis_bonus.wood), speed),
        clay: scale(boosted(base(ResourceKind::Clay), oasis_bonus.clay), speed),
        iron: scale(boosted(base(ResourceKind::Iron), oasis_bonus.iron), speed),
        // 114: scale only the field output; population + upkeep are fixed (not speed-scaled).
        crop_net: scale(crop_gross, speed) - population(fields, buildings, rules) - troop_upkeep,
    }
}

/// Storage capacities (110): the **sum** over every instance of the kind, so multiple Warehouses /
/// Granaries stack. With none built, the level-0 base capacity applies. A single instance equals the
/// pre-110 value (`level_value` at that level), so existing villages are unchanged.
pub fn capacities(buildings: &[BuildingSlot], rules: &EconomyRules) -> Capacities {
    let sum_for = |kind: BuildingKind, curve: &[i64]| -> i64 {
        let mut levels = buildings.iter().filter(|b| b.kind == kind).peekable();
        if levels.peek().is_none() {
            level_value(curve, 0)
        } else {
            levels.map(|b| level_value(curve, b.level)).sum()
        }
    };
    Capacities {
        warehouse: sum_for(BuildingKind::Warehouse, &rules.warehouse_capacity_per_level),
        granary: sum_for(BuildingKind::Granary, &rules.granary_capacity_per_level),
    }
}

/// Accrue a single resource: `(stored + rate·elapsed/3600)` clamped to `[0, capacity]`.
pub fn accrue(stored: i64, rate_per_hour: i64, elapsed_secs: i64, capacity: i64) -> i64 {
    let delta = rate_per_hour.saturating_mul(elapsed_secs.max(0)) / 3600;
    stored.saturating_add(delta).clamp(0, capacity)
}

/// Compute the current economy from stored amounts + elapsed time (the read path, P1/P2).
/// `troop_upkeep` is the garrison's total crop consumption per hour (005 AC6; 0 with no army).
#[allow(clippy::too_many_arguments)]
pub fn compute_economy(
    stored: ResourceAmounts,
    elapsed_secs: i64,
    fields: &[ResourceField],
    buildings: &[BuildingSlot],
    troop_upkeep: i64,
    rules: &EconomyRules,
    speed: GameSpeed,
    oasis_bonus: OasisBonus,
    storage_factor: f64,
) -> Economy {
    let rates = production_rates(fields, buildings, troop_upkeep, rules, speed, oasis_bonus);
    let base = capacities(buildings, rules);
    // 020: a Storage artifact multiplies warehouse/granary capacity (1.0 = no artifact).
    let caps = Capacities {
        warehouse: (base.warehouse as f64 * storage_factor) as i64,
        granary: (base.granary as f64 * storage_factor) as i64,
    };
    let amounts = ResourceAmounts {
        wood: accrue(stored.wood, rates.wood, elapsed_secs, caps.warehouse),
        clay: accrue(stored.clay, rates.clay, elapsed_secs, caps.warehouse),
        iron: accrue(stored.iron, rates.iron, elapsed_secs, caps.warehouse),
        crop: accrue(stored.crop, rates.crop_net, elapsed_secs, caps.granary),
    };
    Economy {
        amounts,
        rates,
        capacities: caps,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> EconomyRules {
        EconomyRules {
            wood_per_level: vec![10, 20, 40],
            clay_per_level: vec![10, 20, 40],
            iron_per_level: vec![10, 20, 40],
            crop_per_level: vec![10, 20, 40],
            field_population_per_level: vec![0, 1, 2],
            building_population_per_level: HashMap::from([
                (BuildingKind::MainBuilding, vec![0, 2, 3]),
                (BuildingKind::RallyPoint, vec![0, 1, 1]),
                (BuildingKind::Warehouse, vec![0, 1, 1]),
                (BuildingKind::Granary, vec![0, 1, 1]),
            ]),
            warehouse_capacity_per_level: vec![800, 1200, 1700],
            granary_capacity_per_level: vec![800, 1200, 1700],
            outpost_capacity_per_level: vec![0, 1, 1],
            starting_amounts: ResourceAmounts {
                wood: 750,
                clay: 750,
                iron: 750,
                crop: 750,
            },
        }
    }

    fn field(kind: ResourceKind, level: u8) -> ResourceField {
        ResourceField { kind, level }
    }

    // --- 031: per-level effect accessors (for the upgrade-effect display) ---
    #[test]
    fn level_accessors_report_next_level_values() {
        let r = rules();
        let s = GameSpeed::new(2.0).unwrap();
        // Field production is the table value × speed; clamps past the table end.
        assert_eq!(r.field_production_per_hour(ResourceKind::Wood, 0, s), 20);
        assert_eq!(r.field_production_per_hour(ResourceKind::Wood, 1, s), 40);
        assert_eq!(r.field_production_per_hour(ResourceKind::Wood, 9, s), 80); // clamped to 40 × 2
        // Population + capacity tables, clamped.
        assert_eq!(r.field_population(2), 2);
        assert_eq!(r.field_population(9), 2);
        assert_eq!(r.building_population_at(BuildingKind::MainBuilding, 1), 2);
        assert_eq!(r.warehouse_capacity(2), 1700);
        assert_eq!(r.granary_capacity(0), 800);
    }

    // --- AC1: accrual ---
    #[test]
    fn accrues_linearly_over_time() {
        assert_eq!(accrue(100, 30, 3600, 800), 130);
        assert_eq!(accrue(100, 30, 7200, 800), 160);
        assert_eq!(accrue(100, 30, 0, 800), 100);
    }

    // --- AC3: capacity / overflow ---
    #[test]
    fn clamps_at_capacity() {
        assert_eq!(accrue(790, 30, 3600, 800), 800); // 820 -> capped
    }

    // --- 110: storage capacity sums over multiple Warehouses/Granaries (single instance unchanged) ---
    #[test]
    fn capacity_sums_over_multiple_storage_buildings() {
        let r = rules();
        let bld = |slot, kind, level| BuildingSlot { slot, kind, level };
        // no Warehouse -> the level-0 base capacity.
        assert_eq!(capacities(&[], &r).warehouse, r.warehouse_capacity(0));
        // one Warehouse -> exactly its level value (== pre-110 behaviour).
        let one = vec![bld(2, BuildingKind::Warehouse, 2)];
        assert_eq!(capacities(&one, &r).warehouse, r.warehouse_capacity(2));
        // two Warehouses on different slots -> the sum of each level's capacity.
        let two = vec![
            bld(2, BuildingKind::Warehouse, 2),
            bld(3, BuildingKind::Warehouse, 1),
        ];
        assert_eq!(
            capacities(&two, &r).warehouse,
            r.warehouse_capacity(2) + r.warehouse_capacity(1)
        );
    }

    // --- AC4: negative crop drains, floored at zero ---
    #[test]
    fn negative_rate_drains_then_floors() {
        assert_eq!(accrue(100, -10, 3600, 800), 90);
        assert_eq!(accrue(5, -10, 3600, 800), 0);
    }

    // --- AC2: speed scales production ---
    #[test]
    fn production_scales_with_speed() {
        let fields: Vec<_> = (0..4).map(|_| field(ResourceKind::Wood, 0)).collect();
        let r1 = production_rates(
            &fields,
            &[],
            0,
            &rules(),
            GameSpeed::new(1.0).unwrap(),
            OasisBonus::default(),
        );
        let r2 = production_rates(
            &fields,
            &[],
            0,
            &rules(),
            GameSpeed::new(2.0).unwrap(),
            OasisBonus::default(),
        );
        assert_eq!(r1.wood, 40); // 4 fields x 10
        assert_eq!(r2.wood, 80); // doubled
    }

    // --- AC4: net crop = production - population ---
    #[test]
    fn crop_net_subtracts_population() {
        let fields = vec![field(ResourceKind::Crop, 0); 6]; // 6 x 10 = 60
        let buildings = vec![
            BuildingSlot {
                slot: 0,
                kind: BuildingKind::MainBuilding,
                level: 1,
            }, // pop 2
            BuildingSlot {
                slot: 0,
                kind: BuildingKind::RallyPoint,
                level: 1,
            }, // pop 1
        ];
        let r = production_rates(
            &fields,
            &buildings,
            0,
            &rules(),
            GameSpeed::new(1.0).unwrap(),
            OasisBonus::default(),
        );
        assert_eq!(r.crop_net, 60 - 3);
    }

    #[test]
    fn crop_output_scales_with_speed_but_population_is_fixed() {
        // 114: field output scales with world speed, but population is FIXED — so net crop is
        // output×speed − population (it does NOT simply double; it exceeds 2× by the unscaled population).
        let fields = vec![field(ResourceKind::Crop, 0); 6]; // 60 gross
        let buildings = vec![BuildingSlot {
            slot: 0,
            kind: BuildingKind::MainBuilding,
            level: 1,
        }]; // pop 2
        let r1 = production_rates(
            &fields,
            &buildings,
            0,
            &rules(),
            GameSpeed::new(1.0).unwrap(),
            OasisBonus::default(),
        );
        let r2 = production_rates(
            &fields,
            &buildings,
            0,
            &rules(),
            GameSpeed::new(2.0).unwrap(),
            OasisBonus::default(),
        );
        assert_eq!(r1.crop_net, 60 - 2); // 58, unchanged at 1×
        assert_eq!(r2.crop_net, 60 * 2 - 2); // 118: output doubled, population fixed
        assert!(
            r2.crop_net > 2 * r1.crop_net,
            "fixed population makes net exceed a linear 2× (118 > 116)"
        );
    }

    // --- 005 AC6: troop upkeep reduces net crop ---
    #[test]
    fn crop_net_subtracts_troop_upkeep() {
        let fields = vec![field(ResourceKind::Crop, 0); 6]; // 60 base
        let r0 = production_rates(
            &fields,
            &[],
            0,
            &rules(),
            GameSpeed::new(1.0).unwrap(),
            OasisBonus::default(),
        );
        let r25 = production_rates(
            &fields,
            &[],
            25,
            &rules(),
            GameSpeed::new(1.0).unwrap(),
            OasisBonus::default(),
        );
        assert_eq!(r25.crop_net, r0.crop_net - 25);
        // 114: upkeep is FIXED (not speed-scaled), so at 2× the scaled output (120) covers a 100 upkeep
        // with room to spare — fast worlds support far larger armies than 1×.
        let fast = production_rates(
            &fields,
            &[],
            100,
            &rules(),
            GameSpeed::new(2.0).unwrap(),
            OasisBonus::default(),
        );
        assert_eq!(fast.crop_net, 60 * 2 - 100); // 20, still positive
    }

    // --- 114 AC3: fixed upkeep ⇒ a fast world supports an army that would starve at 1× ---
    #[test]
    fn fast_world_supports_an_army_that_starves_at_base_speed() {
        let fields = vec![field(ResourceKind::Crop, 0); 6]; // 60 gross
        let at_1x = production_rates(
            &fields,
            &[],
            100,
            &rules(),
            GameSpeed::new(1.0).unwrap(),
            OasisBonus::default(),
        )
        .crop_net;
        let at_5x = production_rates(
            &fields,
            &[],
            100,
            &rules(),
            GameSpeed::new(5.0).unwrap(),
            OasisBonus::default(),
        )
        .crop_net;
        assert!(at_1x < 0, "the same garrison starves at 1× ({at_1x})"); // 60 − 100 = −40
        assert!(at_5x > 0, "but thrives once output scales ({at_5x})"); // 300 − 100 = 200
    }

    #[test]
    fn crop_net_can_be_negative() {
        let fields = vec![field(ResourceKind::Crop, 0)]; // 1 x 10 = 10
        let buildings = vec![BuildingSlot {
            slot: 0,
            kind: BuildingKind::MainBuilding,
            level: 2,
        }]; // pop 3
        // population also includes the single field (level 0 -> 0). net = 10 - 3 = 7 (still positive)
        // add many high-pop fields to force negative:
        let mut fields = fields;
        fields.extend(std::iter::repeat_n(field(ResourceKind::Wood, 2), 10)); // 10 x pop 2 = 20
        let r = production_rates(
            &fields,
            &buildings,
            0,
            &rules(),
            GameSpeed::new(1.0).unwrap(),
            OasisBonus::default(),
        );
        assert!(
            r.crop_net < 0,
            "expected negative net crop, got {}",
            r.crop_net
        );
    }

    // --- AC5: pure & reproducible ---
    #[test]
    fn compute_is_reproducible() {
        let fields = vec![field(ResourceKind::Wood, 0); 4];
        let stored = ResourceAmounts {
            wood: 100,
            clay: 0,
            iron: 0,
            crop: 0,
        };
        let a = compute_economy(
            stored,
            3600,
            &fields,
            &[],
            0,
            &rules(),
            GameSpeed::new(1.0).unwrap(),
            OasisBonus::default(),
            1.0,
        );
        let b = compute_economy(
            stored,
            3600,
            &fields,
            &[],
            0,
            &rules(),
            GameSpeed::new(1.0).unwrap(),
            OasisBonus::default(),
            1.0,
        );
        assert_eq!(a, b);
        assert_eq!(a.amounts.wood, 140); // 100 + 40/h
    }

    // --- 012 AC8: an oasis production bonus boosts gross output, before pop/upkeep and scaling ---
    #[test]
    fn oasis_bonus_boosts_gross_production() {
        let speed = GameSpeed::new(1.0).unwrap();
        let wood = vec![field(ResourceKind::Wood, 0); 4]; // 4 x 10 = 40 gross
        let none = production_rates(&wood, &[], 0, &rules(), speed, OasisBonus::default());
        let plus25 = production_rates(
            &wood,
            &[],
            0,
            &rules(),
            speed,
            OasisBonus {
                wood: 25,
                ..Default::default()
            },
        );
        assert_eq!(none.wood, 40);
        assert_eq!(plus25.wood, 50, "40 + 25% = 50");
        assert_eq!(plus25.clay, 0, "the bonus is per-resource");

        // A crop bonus boosts the gross crop *before* population/upkeep subtract — so it lifts net
        // crop by the full bonus on gross (not a percentage of the already-reduced net).
        let crop = vec![field(ResourceKind::Crop, 0); 6]; // 60 gross
        let buildings = vec![BuildingSlot {
            slot: 0,
            kind: BuildingKind::MainBuilding,
            level: 1,
        }]; // pop 2
        let c0 = production_rates(&crop, &buildings, 0, &rules(), speed, OasisBonus::default());
        let c50 = production_rates(
            &crop,
            &buildings,
            0,
            &rules(),
            speed,
            OasisBonus {
                crop: 50,
                ..Default::default()
            },
        );
        // gross 60 → 90; the +30 lands entirely on net crop (population unchanged).
        assert_eq!(c50.crop_net - c0.crop_net, 30);
    }
}
