//! Resource economy — lazy production accrual (P1), storage capacity, and crop upkeep.
//!
//! All math is pure integer arithmetic over injected balance data ([`EconomyRules`]); there is no
//! background job — current amounts are computed on read from stored state + elapsed time (P1/P2).

use crate::building::BuildingKind;
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
pub fn production_rates(
    fields: &[ResourceField],
    buildings: &[BuildingSlot],
    troop_upkeep: i64,
    rules: &EconomyRules,
    speed: GameSpeed,
) -> ProductionRates {
    let base = |kind: ResourceKind| -> i64 {
        fields
            .iter()
            .filter(|f| f.kind == kind)
            .map(|f| rules.field_production(kind, f.level))
            .sum()
    };
    let crop_base = base(ResourceKind::Crop) - population(fields, buildings, rules) - troop_upkeep;
    ProductionRates {
        wood: scale(base(ResourceKind::Wood), speed),
        clay: scale(base(ResourceKind::Clay), speed),
        iron: scale(base(ResourceKind::Iron), speed),
        crop_net: scale(crop_base, speed),
    }
}

/// The hourly crop balance **before troop upkeep and speed scaling**: crop-field output minus
/// population. The starvation cull (005 AC7) compares this against the garrison's upkeep.
pub fn net_crop_base(
    fields: &[ResourceField],
    buildings: &[BuildingSlot],
    rules: &EconomyRules,
) -> i64 {
    let crop: i64 = fields
        .iter()
        .filter(|f| f.kind == ResourceKind::Crop)
        .map(|f| rules.field_production(ResourceKind::Crop, f.level))
        .sum();
    crop - population(fields, buildings, rules)
}

/// Storage capacities, derived from the highest Warehouse/Granary levels present (level 0 = base).
pub fn capacities(buildings: &[BuildingSlot], rules: &EconomyRules) -> Capacities {
    let level_of = |kind: BuildingKind| -> u8 {
        buildings
            .iter()
            .filter(|b| b.kind == kind)
            .map(|b| b.level)
            .max()
            .unwrap_or(0)
    };
    Capacities {
        warehouse: level_value(
            &rules.warehouse_capacity_per_level,
            level_of(BuildingKind::Warehouse),
        ),
        granary: level_value(
            &rules.granary_capacity_per_level,
            level_of(BuildingKind::Granary),
        ),
    }
}

/// Accrue a single resource: `(stored + rate·elapsed/3600)` clamped to `[0, capacity]`.
pub fn accrue(stored: i64, rate_per_hour: i64, elapsed_secs: i64, capacity: i64) -> i64 {
    let delta = rate_per_hour.saturating_mul(elapsed_secs.max(0)) / 3600;
    stored.saturating_add(delta).clamp(0, capacity)
}

/// Compute the current economy from stored amounts + elapsed time (the read path, P1/P2).
/// `troop_upkeep` is the garrison's total crop consumption per hour (005 AC6; 0 with no army).
pub fn compute_economy(
    stored: ResourceAmounts,
    elapsed_secs: i64,
    fields: &[ResourceField],
    buildings: &[BuildingSlot],
    troop_upkeep: i64,
    rules: &EconomyRules,
    speed: GameSpeed,
) -> Economy {
    let rates = production_rates(fields, buildings, troop_upkeep, rules, speed);
    let caps = capacities(buildings, rules);
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
        let r1 = production_rates(&fields, &[], 0, &rules(), GameSpeed::new(1.0).unwrap());
        let r2 = production_rates(&fields, &[], 0, &rules(), GameSpeed::new(2.0).unwrap());
        assert_eq!(r1.wood, 40); // 4 fields x 10
        assert_eq!(r2.wood, 80); // doubled
    }

    // --- AC4: net crop = production - population ---
    #[test]
    fn crop_net_subtracts_population() {
        let fields = vec![field(ResourceKind::Crop, 0); 6]; // 6 x 10 = 60
        let buildings = vec![
            BuildingSlot {
                kind: BuildingKind::MainBuilding,
                level: 1,
            }, // pop 2
            BuildingSlot {
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
        );
        assert_eq!(r.crop_net, 60 - 3);
    }

    #[test]
    fn crop_net_scales_with_speed() {
        // Both crop production and upkeep scale with speed, so net crop scales linearly (P7).
        let fields = vec![field(ResourceKind::Crop, 0); 6]; // 60 base
        let buildings = vec![BuildingSlot {
            kind: BuildingKind::MainBuilding,
            level: 1,
        }]; // pop 2
        let r1 = production_rates(
            &fields,
            &buildings,
            0,
            &rules(),
            GameSpeed::new(1.0).unwrap(),
        );
        let r2 = production_rates(
            &fields,
            &buildings,
            0,
            &rules(),
            GameSpeed::new(2.0).unwrap(),
        );
        assert_eq!(r1.crop_net, 58);
        assert_eq!(r2.crop_net, 2 * r1.crop_net);
    }

    // --- 005 AC6: troop upkeep reduces net crop ---
    #[test]
    fn crop_net_subtracts_troop_upkeep() {
        let fields = vec![field(ResourceKind::Crop, 0); 6]; // 60 base
        let r0 = production_rates(&fields, &[], 0, &rules(), GameSpeed::new(1.0).unwrap());
        let r25 = production_rates(&fields, &[], 25, &rules(), GameSpeed::new(1.0).unwrap());
        assert_eq!(r25.crop_net, r0.crop_net - 25);
        // Upkeep can push the net negative; it scales with speed like the rest (P7).
        let starving = production_rates(&fields, &[], 100, &rules(), GameSpeed::new(2.0).unwrap());
        assert_eq!(starving.crop_net, 2 * (60 - 100));
    }

    #[test]
    fn crop_net_can_be_negative() {
        let fields = vec![field(ResourceKind::Crop, 0)]; // 1 x 10 = 10
        let buildings = vec![BuildingSlot {
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
        );
        let b = compute_economy(
            stored,
            3600,
            &fields,
            &[],
            0,
            &rules(),
            GameSpeed::new(1.0).unwrap(),
        );
        assert_eq!(a, b);
        assert_eq!(a.amounts.wood, 140); // 100 + 40/h
    }
}
