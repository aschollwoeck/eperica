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

/// Largest training batch a single order may request (005; a server-side sanity bound — resources
/// are the real constraint).
pub const MAX_TRAINING_BATCH: u32 = 9999;

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

/// What a siege unit targets in combat (009/011). Non-siege units carry `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiegeKind {
    /// A ram — reduces the target's effective Wall level before the defence bonus (009).
    Ram,
    /// A catapult — damages targeted buildings (siege effect lands in 011; fights as a unit in 009).
    Catapult,
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
    /// Espionage **and** counter-espionage strength (010); `0` for every non-Scout unit. A scout uses
    /// this both when spying (attacking) and when countering enemy scouts (defending).
    pub scouting: u32,
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
    /// For siege units, what they target in combat (009/011); `None` for all other roles.
    pub siege_kind: Option<SiegeKind>,
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

/// Training-speed balance (005). One shared table for all troop buildings.
#[derive(Debug, Clone)]
pub struct TrainingRules {
    /// Training-speed multiplier by training-building level (≥ 1; higher ⇒ faster). Index = level;
    /// clamped to the last entry.
    pub building_factor_per_level: Vec<f64>,
}

impl TrainingRules {
    /// The factor at `level` (clamped to the table).
    pub fn building_factor(&self, level: u8) -> f64 {
        self.building_factor_per_level
            .get(level as usize)
            .or_else(|| self.building_factor_per_level.last())
            .copied()
            .unwrap_or(1.0)
    }
}

/// All unit balance data, validated on construction.
#[derive(Debug, Clone)]
pub struct UnitRules {
    rosters: HashMap<Tribe, Vec<UnitSpec>>,
    pub smithy: SmithyRules,
    pub training: TrainingRules,
}

impl UnitRules {
    /// Build validated unit rules.
    ///
    /// # Errors
    /// Returns [`DomainError::InvalidUnitRules`] unless every tribe has exactly [`ROSTER_SIZE`]
    /// units with per-tribe-unique ids and exactly one research-free (tier-1) unit, the Smithy
    /// tables are non-empty and of equal length (a mismatch would silently lower the cap), and
    /// the training factor table is non-empty.
    pub fn new(
        rosters: HashMap<Tribe, Vec<UnitSpec>>,
        smithy: SmithyRules,
        training: TrainingRules,
    ) -> Result<Self, DomainError> {
        if smithy.cost_permille_per_level.is_empty()
            || smithy.cost_permille_per_level.len() != smithy.time_secs_per_level.len()
        {
            return Err(DomainError::InvalidUnitRules(
                "smithy cost/time tables must be non-empty and the same length",
            ));
        }
        if training.building_factor_per_level.is_empty() {
            return Err(DomainError::InvalidUnitRules(
                "the training factor table must not be empty",
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
        Ok(Self {
            rosters,
            smithy,
            training,
        })
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

/// The buildings whose training queues exist in slice 005. `trained_in` kinds outside this set
/// (Residence — settlers/administrators) are trainable in later slices.
fn trains_here(kind: BuildingKind) -> bool {
    matches!(
        kind,
        BuildingKind::Barracks | BuildingKind::Stable | BuildingKind::Workshop
    )
}

/// Why a training batch is denied (beyond affordability and queue occupancy).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrainDenied {
    /// The unit is not researched in this village.
    NotResearched,
    /// The unit's training building is not built in this village.
    BuildingMissing,
    /// The unit trains in a building no slice has made trainable yet (e.g. the Residence).
    BuildingUnavailable,
    /// The batch size is outside `1..=MAX_TRAINING_BATCH`.
    CountOutOfRange,
}

/// Whether a batch of `count` × `unit` may be trained in a village with `buildings` (005 AC2/AC3).
///
/// # Errors
/// Returns the [`TrainDenied`] reason when training may not start.
pub fn can_train(
    unit: &UnitSpec,
    is_researched: bool,
    count: u32,
    buildings: &[BuildingSlot],
) -> Result<(), TrainDenied> {
    if !is_researched && !unit.researched_by_default() {
        return Err(TrainDenied::NotResearched);
    }
    if count == 0 || count > MAX_TRAINING_BATCH {
        return Err(TrainDenied::CountOutOfRange);
    }
    if !trains_here(unit.trained_in) {
        return Err(TrainDenied::BuildingUnavailable);
    }
    if !building_levels_met(&[(unit.trained_in, 1)], buildings) {
        return Err(TrainDenied::BuildingMissing);
    }
    Ok(())
}

/// The full cost of a batch (`count × unitCost`, saturating).
pub fn batch_cost(unit: &UnitSpec, count: u32) -> ResourceAmounts {
    let n = i64::from(count);
    ResourceAmounts {
        wood: unit.cost.wood.saturating_mul(n),
        clay: unit.cost.clay.saturating_mul(n),
        iron: unit.cost.iron.saturating_mul(n),
        crop: unit.cost.crop.saturating_mul(n),
    }
}

/// Per-unit training duration after the building factor and world speed (005 AC4, P7; ≥ 1 s).
pub fn per_unit_time_secs(
    train_secs: i64,
    building_level: u8,
    rules: &TrainingRules,
    speed: GameSpeed,
) -> i64 {
    let divisor = speed.multiplier() * rules.building_factor(building_level);
    ((train_secs as f64 / divisor).round() as i64).max(1)
}

fn upkeep_of(roster: &[UnitSpec], unit: &UnitId) -> i64 {
    roster
        .iter()
        .find(|s| &s.id == unit)
        .map_or(0, |s| i64::from(s.crop_upkeep))
}

/// Per-unit-type counts — a garrison or a casualty list (005).
pub type UnitCounts = Vec<(UnitId, u32)>;

/// The garrison's total crop consumption per hour (005 AC6). Unknown unit ids count 0.
pub fn garrison_upkeep(garrison: &[(UnitId, u32)], roster: &[UnitSpec]) -> i64 {
    garrison
        .iter()
        .map(|(unit, count)| upkeep_of(roster, unit).saturating_mul(i64::from(*count)))
        .sum()
}

/// Starve a garrison down to a sustainable size (005 AC7, GDD §2.2).
///
/// `net_without_troops` is the village's hourly crop balance **before** troop upkeep (fields −
/// population; pre-speed-scaling — the sign is what matters). Units die deterministically:
/// repeatedly one unit of the garrisoned type with the **highest `cropUpkeep`** (ties: roster
/// order) until `net_without_troops − upkeep(remaining) ≥ 0` or no upkeep-bearing unit remains.
///
/// Returns `(survivors, casualties)`; both omit zero counts.
pub fn starve(
    garrison: &[(UnitId, u32)],
    roster: &[UnitSpec],
    net_without_troops: i64,
) -> (UnitCounts, UnitCounts) {
    let mut remaining: UnitCounts = garrison.to_vec();
    // Kill order: highest upkeep first, ties by roster order (kept stable by sorting on the
    // roster index). Unknown ids sort last and are never culled (upkeep 0).
    let roster_index = |unit: &UnitId| {
        roster
            .iter()
            .position(|s| &s.id == unit)
            .unwrap_or(usize::MAX)
    };
    remaining.sort_by_key(|(unit, _)| (-upkeep_of(roster, unit), roster_index(unit)));

    let mut deficit = garrison_upkeep(garrison, roster) - net_without_troops;
    let mut casualties = Vec::new();
    for (unit, count) in &mut remaining {
        if deficit <= 0 {
            break;
        }
        let upkeep = upkeep_of(roster, unit);
        if upkeep <= 0 {
            break; // only upkeep-bearing units can starve (or help)
        }
        // Killing one at a time from this (currently highest-upkeep) type is equivalent to the
        // bulk count below.
        let needed = u32::try_from(deficit.div_euclid(upkeep) + i64::from(deficit % upkeep != 0))
            .unwrap_or(u32::MAX);
        let killed = needed.min(*count);
        deficit -= upkeep.saturating_mul(i64::from(killed));
        *count -= killed;
        casualties.push((unit.clone(), killed));
    }
    remaining.retain(|(_, count)| *count > 0);
    (remaining, casualties)
}

/// Seconds until the crop store empties at the current (negative) net rate; `None` when net ≥ 0.
/// An already-empty store depletes "now" (0 s). Rounded up so the check never fires early.
pub fn depletion_secs(crop_stored: i64, net_rate_per_hour: i64) -> Option<i64> {
    if net_rate_per_hour >= 0 {
        return None;
    }
    let deficit = -net_rate_per_hour;
    let scaled = crop_stored.max(0).saturating_mul(3600);
    // Manual ceiling division — immune to the `scaled + deficit - 1` overflow when saturated.
    Some(scaled / deficit + i64::from(scaled % deficit != 0))
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
            scouting: 0,
            speed: 6,
            carry_capacity: 50,
            crop_upkeep: 1,
            cost: amounts(100),
            train_secs: 1600,
            trained_in: BuildingKind::Barracks,
            research,
            siege_kind: None,
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

    fn training_rules() -> TrainingRules {
        TrainingRules {
            building_factor_per_level: vec![1.0, 1.0, 1.25, 1.5],
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
        UnitRules::new(rosters, smithy_rules(), training_rules()).expect("valid rules")
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
        assert!(UnitRules::new(rosters.clone(), smithy_rules(), training_rules()).is_err()); // missing Gauls

        let mut short = roster();
        short.pop();
        rosters.insert(Tribe::Gauls, short);
        assert!(UnitRules::new(rosters.clone(), smithy_rules(), training_rules()).is_err()); // 9 units

        let mut two_tier1 = roster();
        two_tier1[1] = unit("second_tier1", None);
        rosters.insert(Tribe::Gauls, two_tier1);
        assert!(UnitRules::new(rosters.clone(), smithy_rules(), training_rules()).is_err()); // two tier-1

        rosters.insert(Tribe::Gauls, roster());
        // Mismatched smithy tables would silently lower the level cap — rejected at load.
        let lopsided = SmithyRules {
            cost_permille_per_level: vec![1500, 1900],
            time_secs_per_level: vec![3600],
        };
        assert!(UnitRules::new(rosters.clone(), lopsided, training_rules()).is_err());

        assert!(UnitRules::new(rosters, smithy_rules(), training_rules()).is_ok());
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

    // --- 005 AC3 (domain side): training gates ---
    #[test]
    fn training_gates() {
        let infantry = unit("tier1", None); // trained_in: Barracks
        let barracks = slots(&[(BuildingKind::Barracks, 1)]);

        assert_eq!(can_train(&infantry, false, 5, &barracks), Ok(()));
        // Unresearched (non-tier-1).
        let researchable = researchable("u1", vec![]);
        assert_eq!(
            can_train(&researchable, false, 5, &barracks),
            Err(TrainDenied::NotResearched)
        );
        assert_eq!(can_train(&researchable, true, 5, &barracks), Ok(()));
        // Count out of range.
        assert_eq!(
            can_train(&infantry, false, 0, &barracks),
            Err(TrainDenied::CountOutOfRange)
        );
        assert_eq!(
            can_train(&infantry, false, MAX_TRAINING_BATCH + 1, &barracks),
            Err(TrainDenied::CountOutOfRange)
        );
        // Training building absent.
        assert_eq!(
            can_train(&infantry, false, 5, &slots(&[(BuildingKind::Stable, 1)])),
            Err(TrainDenied::BuildingMissing)
        );
        // Residence-trained units are unavailable until 013.
        let mut settler = unit("settler", None);
        settler.trained_in = BuildingKind::Residence;
        assert_eq!(
            can_train(&settler, true, 1, &slots(&[(BuildingKind::Residence, 1)])),
            Err(TrainDenied::BuildingUnavailable)
        );
    }

    // --- 005 AC2: batch cost ---
    #[test]
    fn batch_cost_multiplies_unit_cost() {
        let u = unit("tier1", None); // cost 100 each
        assert_eq!(batch_cost(&u, 1), amounts(100));
        assert_eq!(batch_cost(&u, 25), amounts(2500));
    }

    // --- 005 AC4: building level and world speed scale training ---
    #[test]
    fn training_time_scales_with_building_level_and_speed() {
        let r = training_rules(); // factors [1.0, 1.0, 1.25, 1.5]
        let s1 = GameSpeed::new(1.0).unwrap();
        let t1 = per_unit_time_secs(1000, 1, &r, s1);
        let t2 = per_unit_time_secs(1000, 2, &r, s1);
        let t3 = per_unit_time_secs(1000, 3, &r, s1);
        assert_eq!(t1, 1000);
        assert!(t2 < t1 && t3 < t2, "{t1} {t2} {t3}");
        // Levels beyond the table clamp to the last factor.
        assert_eq!(per_unit_time_secs(1000, 9, &r, s1), t3);
        // World speed scales proportionally (P7).
        let s2 = GameSpeed::new(2.0).unwrap();
        assert_eq!(per_unit_time_secs(1000, 1, &r, s2), 500);
        assert_eq!(per_unit_time_secs(1, 9, &r, s2), 1); // floor at 1 s
    }

    // --- 005 AC6: garrison upkeep ---
    #[test]
    fn garrison_upkeep_sums_per_unit_consumption() {
        let mut heavy = unit("knight", None);
        heavy.crop_upkeep = 3;
        let light = unit("militia", None); // upkeep 1
        let roster = vec![light.clone(), heavy.clone()];
        let garrison = vec![(UnitId("militia".into()), 10), (UnitId("knight".into()), 4)];
        assert_eq!(garrison_upkeep(&garrison, &roster), 10 + 12);
        // Unknown ids count 0.
        assert_eq!(garrison_upkeep(&[(UnitId("ghost".into()), 99)], &roster), 0);
    }

    // --- 005 AC7: the starvation cull is deterministic, highest-upkeep first ---
    #[test]
    fn starve_culls_highest_upkeep_first_until_sustainable() {
        let mut heavy = unit("knight", None);
        heavy.crop_upkeep = 3;
        let light = unit("militia", None); // upkeep 1
        let roster = vec![light.clone(), heavy.clone()];
        let garrison = vec![(UnitId("militia".into()), 10), (UnitId("knight".into()), 4)];
        // Upkeep 22, net without troops 12 => deficit 10: kill 4 knights (12), done.
        let (survivors, casualties) = starve(&garrison, &roster, 12);
        assert_eq!(casualties, vec![(UnitId("knight".into()), 4)]);
        assert_eq!(survivors, vec![(UnitId("militia".into()), 10)]);

        // Deficit 15: 4 knights (12) + 3 militia.
        let (survivors, casualties) = starve(&garrison, &roster, 7);
        assert_eq!(
            casualties,
            vec![(UnitId("knight".into()), 4), (UnitId("militia".into()), 3)]
        );
        assert_eq!(survivors, vec![(UnitId("militia".into()), 7)]);

        // Ties broken by roster order: two upkeep-1 types — the earlier roster entry dies first.
        let militia2 = unit("levy", None);
        let roster2 = vec![light.clone(), militia2];
        let garrison2 = vec![(UnitId("levy".into()), 5), (UnitId("militia".into()), 5)];
        let (_, casualties) = starve(&garrison2, &roster2, 7); // deficit 3
        assert_eq!(casualties, vec![(UnitId("militia".into()), 3)]);

        // Net never recoverable (buildings alone overconsume): everything dies, nothing panics.
        let (survivors, _) = starve(&garrison, &roster, -5);
        assert!(survivors.is_empty());

        // No deficit: nothing dies.
        let (survivors, casualties) = starve(&garrison, &roster, 22);
        assert!(casualties.is_empty());
        assert_eq!(survivors.len(), 2);
    }

    // --- 005 AC7: depletion scheduling ---
    #[test]
    fn depletion_time_rounds_up_and_handles_signs() {
        assert_eq!(depletion_secs(100, -100), Some(3600)); // exactly one hour
        assert_eq!(depletion_secs(100, -101), Some(3565)); // ceil(360000/101)
        assert_eq!(depletion_secs(0, -10), Some(0)); // already empty
        assert_eq!(depletion_secs(100, 0), None);
        assert_eq!(depletion_secs(100, 5), None);
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
