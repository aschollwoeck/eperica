//! Construction — build targets, costs, times, and prerequisites (pure rules over injected balance).

use crate::building::BuildingKind;
use crate::economy::ResourceAmounts;
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
    /// Spec shared by all resource fields (its cost/time tables run to the **capital** cap).
    pub field: LevelSpec,
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
    pub fn cost(&self, target: BuildTarget, current_level: u8) -> Option<ResourceAmounts> {
        self.spec(target)?.cost(current_level)
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
            kind: BuildingKind::MainBuilding,
            level: 1,
        }];
        assert!(prerequisites_met(BuildingKind::Warehouse, &with_mb, &r));
        // A field has no prerequisites.
        assert!(prerequisites_met(BuildingKind::RallyPoint, &none, &r));
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
