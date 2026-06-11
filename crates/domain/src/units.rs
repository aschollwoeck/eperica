//! Units — per-tribe unit definitions, Academy research, and Smithy upgrade rules.
//!
//! Pure rules over injected balance data ([`UnitRules`]); all numbers live in the balance dataset.
//! Training the units is slice 005; combat use of the stats is slice 009.

use crate::building::BuildingKind;
use crate::construction::building_levels_met;
use crate::economy::ResourceAmounts;
use crate::error::DomainError;
use crate::village::{BuildingSlot, Tribe};
use crate::world::GameSpeed;
use std::collections::{HashMap, HashSet};

/// Number of unit types in every tribe's roster (GDD §6).
pub const ROSTER_SIZE: usize = 10;

/// Highest Smithy upgrade level any unit can reach (also capped by the Smithy's own level).
pub const MAX_UNIT_LEVEL: u8 = 20;

/// Stable identifier of a unit type within its tribe (slug from balance data, e.g. `legionnaire`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UnitId(pub String);

impl UnitId {
    /// The id as the slug string used in forms, URLs, and storage.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Broad role of a unit type (GDD §6.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitRole {
    Infantry,
    Cavalry,
    Scout,
    Siege,
    Expansion,
}

/// Cost, duration, and building requirements for researching a unit type in the Academy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResearchSpec {
    pub cost: ResourceAmounts,
    /// Base research duration in seconds (before world speed).
    pub time_secs: i64,
    /// Building levels the village must have to start the research.
    pub requirements: Vec<(BuildingKind, u8)>,
}

/// A unit type definition carrying the GDD §6.2 attributes. All values are balance data.
#[derive(Debug, Clone)]
pub struct UnitSpec {
    pub id: UnitId,
    /// Display name (e.g. "Legionnaire").
    pub name: String,
    pub role: UnitRole,
    pub attack: u32,
    pub defense_infantry: u32,
    pub defense_cavalry: u32,
    /// Map speed in fields per hour (before world speed).
    pub speed: u32,
    /// Loot carried per unit.
    pub carry_capacity: u32,
    /// Crop consumed per hour while alive (GDD §2.2).
    pub crop_upkeep: u32,
    /// Training cost per unit.
    pub cost: ResourceAmounts,
    /// Base training duration in seconds (before world speed; used from 005).
    pub train_secs: i64,
    /// The building whose queue trains this unit (005).
    pub trained_in: BuildingKind,
    /// `None` marks the tribe's tier-1 unit: researched from the start, no order needed (AC9).
    pub research: Option<ResearchSpec>,
}

impl UnitSpec {
    /// Whether this unit type needs no Academy research (the tribe's tier-1 unit).
    pub fn researched_by_default(&self) -> bool {
        self.research.is_none()
    }
}

/// Smithy upgrade balance. Index 0 = the upgrade that reaches level 1.
#[derive(Debug, Clone)]
pub struct SmithyRules {
    /// Upgrade cost as permille of the unit's training cost, per target level.
    pub cost_permille_per_level: Vec<u32>,
    /// Base upgrade duration in seconds per target level (before world speed).
    pub time_secs_per_level: Vec<i64>,
}

impl SmithyRules {
    /// Highest reachable unit level (length of the cost table, ≤ [`MAX_UNIT_LEVEL`]).
    pub fn max_level(&self) -> u8 {
        u8::try_from(self.cost_per_level_len())
            .unwrap_or(u8::MAX)
            .min(MAX_UNIT_LEVEL)
    }

    fn cost_per_level_len(&self) -> usize {
        self.cost_permille_per_level
            .len()
            .min(self.time_secs_per_level.len())
    }

    /// Cost to raise `unit` from `current_level` to the next; `None` if at max.
    pub fn upgrade_cost(&self, unit: &UnitSpec, current_level: u8) -> Option<ResourceAmounts> {
        if current_level >= self.max_level() {
            return None;
        }
        let permille = i64::from(*self.cost_permille_per_level.get(current_level as usize)?);
        let part = |base: i64| (base * permille) / 1000;
        Some(ResourceAmounts {
            wood: part(unit.cost.wood),
            clay: part(unit.cost.clay),
            iron: part(unit.cost.iron),
            crop: part(unit.cost.crop),
        })
    }

    /// Base duration (seconds, before world speed) to raise a unit from `current_level`.
    pub fn base_time_secs(&self, current_level: u8) -> Option<i64> {
        if current_level >= self.max_level() {
            return None;
        }
        self.time_secs_per_level
            .get(current_level as usize)
            .copied()
    }
}

/// All unit balance data, validated on construction.
#[derive(Debug, Clone)]
pub struct UnitRules {
    rosters: HashMap<Tribe, Vec<UnitSpec>>,
    pub smithy: SmithyRules,
}

impl UnitRules {
    /// Build validated unit rules.
    ///
    /// # Errors
    /// Returns [`DomainError::InvalidUnitRules`] unless every tribe has exactly [`ROSTER_SIZE`]
    /// units with per-tribe-unique ids and exactly one research-free (tier-1) unit, and the
    /// Smithy tables are non-empty and of equal length (a mismatch would silently lower the cap).
    pub fn new(
        rosters: HashMap<Tribe, Vec<UnitSpec>>,
        smithy: SmithyRules,
    ) -> Result<Self, DomainError> {
        if smithy.cost_permille_per_level.is_empty()
            || smithy.cost_permille_per_level.len() != smithy.time_secs_per_level.len()
        {
            return Err(DomainError::InvalidUnitRules(
                "smithy cost/time tables must be non-empty and the same length",
            ));
        }
        for tribe in [Tribe::Romans, Tribe::Teutons, Tribe::Gauls] {
            let roster = rosters
                .get(&tribe)
                .ok_or(DomainError::InvalidUnitRules("missing tribe roster"))?;
            if roster.len() != ROSTER_SIZE {
                return Err(DomainError::InvalidUnitRules(
                    "a tribe roster must have exactly 10 units",
                ));
            }
            let ids: HashSet<&str> = roster.iter().map(|u| u.id.as_str()).collect();
            if ids.len() != roster.len() {
                return Err(DomainError::InvalidUnitRules(
                    "unit ids must be unique within a tribe",
                ));
            }
            if roster.iter().filter(|u| u.researched_by_default()).count() != 1 {
                return Err(DomainError::InvalidUnitRules(
                    "each tribe must have exactly one tier-1 (research-free) unit",
                ));
            }
        }
        Ok(Self { rosters, smithy })
    }

    /// The tribe's full roster, in balance order.
    pub fn roster(&self, tribe: Tribe) -> &[UnitSpec] {
        self.rosters.get(&tribe).map_or(&[], Vec::as_slice)
    }

    /// Look up one unit type of a tribe.
    pub fn unit(&self, tribe: Tribe, id: &UnitId) -> Option<&UnitSpec> {
        self.roster(tribe).iter().find(|u| &u.id == id)
    }
}

/// Why a research order is denied (beyond affordability, which callers check separately).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResearchDenied {
    /// The unit is already researched here (tier-1 counts as researched).
    AlreadyResearched,
    /// The village has no Academy.
    NoAcademy,
    /// A building-level requirement of the unit is unmet.
    RequirementsUnmet,
}

/// Whether `unit` may be researched in a village with `buildings`, given it `is_researched` or not.
///
/// # Errors
/// Returns the [`ResearchDenied`] reason when research may not start (AC6/AC7).
pub fn can_research(
    unit: &UnitSpec,
    is_researched: bool,
    buildings: &[BuildingSlot],
) -> Result<(), ResearchDenied> {
    if is_researched || unit.researched_by_default() {
        return Err(ResearchDenied::AlreadyResearched);
    }
    if !building_levels_met(&[(BuildingKind::Academy, 1)], buildings) {
        return Err(ResearchDenied::NoAcademy);
    }
    let spec = unit
        .research
        .as_ref()
        .ok_or(ResearchDenied::AlreadyResearched)?;
    if !building_levels_met(&spec.requirements, buildings) {
        return Err(ResearchDenied::RequirementsUnmet);
    }
    Ok(())
}

/// Why a Smithy upgrade order is denied (beyond affordability).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpgradeDenied {
    /// The unit is not researched in this village.
    NotResearched,
    /// The village has no Smithy.
    NoSmithy,
    /// The unit is at the balance max level.
    AtMaxLevel,
    /// The unit's level has caught up with the Smithy's building level.
    SmithyLevelTooLow,
}

/// Whether a researched unit at `current_level` may be upgraded in a village with `buildings`.
///
/// # Errors
/// Returns the [`UpgradeDenied`] reason when the upgrade may not start (AC10/AC11).
pub fn can_upgrade(
    unit: &UnitSpec,
    is_researched: bool,
    current_level: u8,
    buildings: &[BuildingSlot],
    rules: &SmithyRules,
) -> Result<(), UpgradeDenied> {
    if !is_researched && !unit.researched_by_default() {
        return Err(UpgradeDenied::NotResearched);
    }
    let smithy_level = buildings
        .iter()
        .filter(|b| b.kind == BuildingKind::Smithy)
        .map(|b| b.level)
        .max()
        .unwrap_or(0);
    if smithy_level == 0 {
        return Err(UpgradeDenied::NoSmithy);
    }
    if current_level >= rules.max_level() {
        return Err(UpgradeDenied::AtMaxLevel);
    }
    if current_level >= smithy_level {
        return Err(UpgradeDenied::SmithyLevelTooLow);
    }
    Ok(())
}

/// Effective duration after applying world speed (P7), never below 1 second.
pub fn scaled_time_secs(base_secs: i64, speed: GameSpeed) -> i64 {
    ((base_secs as f64 / speed.multiplier()).round() as i64).max(1)
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

    fn unit(id: &str, research: Option<ResearchSpec>) -> UnitSpec {
        UnitSpec {
            id: UnitId(id.to_owned()),
            name: id.to_owned(),
            role: UnitRole::Infantry,
            attack: 40,
            defense_infantry: 35,
            defense_cavalry: 50,
            speed: 6,
            carry_capacity: 50,
            crop_upkeep: 1,
            cost: amounts(100),
            train_secs: 1600,
            trained_in: BuildingKind::Barracks,
            research,
        }
    }

    fn researchable(id: &str, requirements: Vec<(BuildingKind, u8)>) -> UnitSpec {
        unit(
            id,
            Some(ResearchSpec {
                cost: amounts(500),
                time_secs: 3600,
                requirements,
            }),
        )
    }

    fn smithy_rules() -> SmithyRules {
        SmithyRules {
            cost_permille_per_level: vec![1500, 1900, 2400],
            time_secs_per_level: vec![3600, 4500, 5600],
        }
    }

    fn roster() -> Vec<UnitSpec> {
        let mut units = vec![unit("tier1", None)];
        for i in 1..ROSTER_SIZE {
            units.push(researchable(&format!("u{i}"), vec![]));
        }
        units
    }

    fn rules() -> UnitRules {
        let rosters = HashMap::from([
            (Tribe::Romans, roster()),
            (Tribe::Teutons, roster()),
            (Tribe::Gauls, roster()),
        ]);
        UnitRules::new(rosters, smithy_rules()).expect("valid rules")
    }

    fn slots(pairs: &[(BuildingKind, u8)]) -> Vec<BuildingSlot> {
        pairs
            .iter()
            .map(|&(kind, level)| BuildingSlot { kind, level })
            .collect()
    }

    // --- AC4: validation ---
    #[test]
    fn rejects_incomplete_rosters() {
        let mut rosters = HashMap::from([(Tribe::Romans, roster()), (Tribe::Teutons, roster())]);
        assert!(UnitRules::new(rosters.clone(), smithy_rules()).is_err()); // missing Gauls

        let mut short = roster();
        short.pop();
        rosters.insert(Tribe::Gauls, short);
        assert!(UnitRules::new(rosters.clone(), smithy_rules()).is_err()); // 9 units

        let mut two_tier1 = roster();
        two_tier1[1] = unit("second_tier1", None);
        rosters.insert(Tribe::Gauls, two_tier1);
        assert!(UnitRules::new(rosters.clone(), smithy_rules()).is_err()); // two tier-1

        rosters.insert(Tribe::Gauls, roster());
        // Mismatched smithy tables would silently lower the level cap — rejected at load.
        let lopsided = SmithyRules {
            cost_permille_per_level: vec![1500, 1900],
            time_secs_per_level: vec![3600],
        };
        assert!(UnitRules::new(rosters.clone(), lopsided).is_err());

        assert!(UnitRules::new(rosters, smithy_rules()).is_ok());
    }

    #[test]
    fn looks_up_units_by_tribe_and_id() {
        let r = rules();
        assert_eq!(r.roster(Tribe::Romans).len(), ROSTER_SIZE);
        assert!(r.unit(Tribe::Romans, &UnitId("tier1".into())).is_some());
        assert!(r.unit(Tribe::Romans, &UnitId("nope".into())).is_none());
    }

    // --- AC9: tier-1 needs no research ---
    #[test]
    fn tier1_is_researched_by_default() {
        let tier1 = unit("tier1", None);
        assert!(tier1.researched_by_default());
        let academy = slots(&[(BuildingKind::Academy, 1)]);
        assert_eq!(
            can_research(&tier1, false, &academy),
            Err(ResearchDenied::AlreadyResearched)
        );
    }

    // --- AC6/AC7: research gates ---
    #[test]
    fn research_gates() {
        let u = researchable("u1", vec![(BuildingKind::Smithy, 1)]);
        let no_academy = slots(&[(BuildingKind::MainBuilding, 3)]);
        assert_eq!(
            can_research(&u, false, &no_academy),
            Err(ResearchDenied::NoAcademy)
        );

        let academy_only = slots(&[(BuildingKind::Academy, 1)]);
        assert_eq!(
            can_research(&u, false, &academy_only),
            Err(ResearchDenied::RequirementsUnmet)
        );

        let ready = slots(&[(BuildingKind::Academy, 1), (BuildingKind::Smithy, 1)]);
        assert_eq!(can_research(&u, false, &ready), Ok(()));
        assert_eq!(
            can_research(&u, true, &ready),
            Err(ResearchDenied::AlreadyResearched)
        );
    }

    // --- AC10/AC11: upgrade gates ---
    #[test]
    fn upgrade_gates() {
        let u = researchable("u1", vec![]);
        let r = smithy_rules();

        let smithy3 = slots(&[(BuildingKind::Smithy, 3)]);
        assert_eq!(
            can_upgrade(&u, false, 0, &smithy3, &r),
            Err(UpgradeDenied::NotResearched)
        );
        assert_eq!(
            can_upgrade(&u, true, 0, &slots(&[]), &r),
            Err(UpgradeDenied::NoSmithy)
        );
        assert_eq!(can_upgrade(&u, true, 0, &smithy3, &r), Ok(()));
        assert_eq!(can_upgrade(&u, true, 2, &smithy3, &r), Ok(()));
        // Level caught up with the Smithy's level.
        let smithy1 = slots(&[(BuildingKind::Smithy, 1)]);
        assert_eq!(
            can_upgrade(&u, true, 1, &smithy1, &r),
            Err(UpgradeDenied::SmithyLevelTooLow)
        );
        // Balance max level (3-entry table).
        let smithy9 = slots(&[(BuildingKind::Smithy, 9)]);
        assert_eq!(
            can_upgrade(&u, true, 3, &smithy9, &r),
            Err(UpgradeDenied::AtMaxLevel)
        );
        // Tier-1 is upgradable without a research row.
        let tier1 = unit("tier1", None);
        assert_eq!(can_upgrade(&tier1, false, 0, &smithy3, &r), Ok(()));
    }

    #[test]
    fn upgrade_cost_scales_with_unit_cost_and_level() {
        let r = smithy_rules();
        let u = unit("tier1", None); // cost 100 each
        assert_eq!(r.upgrade_cost(&u, 0), Some(amounts(150))); // 1500‰
        assert_eq!(r.upgrade_cost(&u, 2), Some(amounts(240))); // 2400‰
        assert_eq!(r.upgrade_cost(&u, 3), None); // at max
        assert_eq!(r.base_time_secs(1), Some(4500));
        assert_eq!(r.base_time_secs(3), None);
    }

    // --- AC14: world speed scales research/upgrade durations ---
    #[test]
    fn speed_scales_durations() {
        let s1 = GameSpeed::new(1.0).unwrap();
        let s2 = GameSpeed::new(2.0).unwrap();
        assert_eq!(scaled_time_secs(3600, s1), 3600);
        assert_eq!(scaled_time_secs(3600, s2), 1800);
        assert_eq!(scaled_time_secs(1, GameSpeed::new(10.0).unwrap()), 1);
    }
}
