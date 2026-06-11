//! Balance-data loading.
//!
//! Numeric and structural balance lives in `specs/balance/` as **data** (not hardcoded in logic, per
//! the constitution). This module embeds that data at compile time and parses it into pure domain
//! types, keeping the domain itself free of serialization concerns.

use eperica_domain::{
    BuildRules, BuildingKind, BuildingSlot, DomainError, EconomyRules, LevelSpec, ResearchSpec,
    ResourceAmounts, ResourceField, ResourceKind, SmithyRules, StartingVillage, TrainingRules,
    Tribe, UnitId, UnitRole, UnitRules, UnitSpec,
};
use serde::Deserialize;
use std::collections::HashMap;

/// Embedded starting-village balance data.
const STARTING_VILLAGE_TOML: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../specs/balance/starting-village.toml"
));

/// Embedded economy balance data.
const ECONOMY_TOML: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../specs/balance/economy.toml"
));

/// Embedded construction balance data.
const CONSTRUCTION_TOML: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../specs/balance/construction.toml"
));

/// Embedded unit balance data.
const UNITS_TOML: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../specs/balance/units.toml"
));

/// Errors that can occur while loading balance data.
#[derive(Debug, thiserror::Error)]
pub enum BalanceError {
    /// The balance file could not be parsed as TOML.
    #[error("failed to parse balance data: {0}")]
    Parse(#[from] toml::de::Error),
    /// An unknown resource name appeared in the data.
    #[error("unknown resource: {0}")]
    UnknownResource(String),
    /// An unknown building name appeared in the data.
    #[error("unknown building: {0}")]
    UnknownBuilding(String),
    /// An unknown unit role appeared in the data.
    #[error("unknown unit role: {0}")]
    UnknownUnitRole(String),
    /// The parsed data did not form a valid domain template.
    #[error(transparent)]
    Domain(DomainError),
}

#[derive(Deserialize)]
struct StartingVillageDto {
    fields: Vec<FieldDto>,
    buildings: Vec<BuildingDto>,
}

#[derive(Deserialize)]
struct FieldDto {
    resource: String,
    count: usize,
    level: u8,
}

#[derive(Deserialize)]
struct BuildingDto {
    building: String,
    level: u8,
}

/// Load the starting-village template from the embedded balance data.
///
/// # Errors
/// Returns [`BalanceError`] if the data cannot be parsed or does not form a valid template.
pub fn starting_village() -> Result<StartingVillage, BalanceError> {
    parse_starting_village(STARTING_VILLAGE_TOML)
}

fn parse_starting_village(toml_src: &str) -> Result<StartingVillage, BalanceError> {
    let dto: StartingVillageDto = toml::from_str(toml_src)?;

    let mut fields = Vec::new();
    for f in &dto.fields {
        let kind = parse_resource(&f.resource)?;
        fields.extend(std::iter::repeat_n(
            ResourceField {
                kind,
                level: f.level,
            },
            f.count,
        ));
    }

    let mut buildings = Vec::with_capacity(dto.buildings.len());
    for b in &dto.buildings {
        buildings.push(BuildingSlot {
            kind: parse_building(&b.building)?,
            level: b.level,
        });
    }

    StartingVillage::new(fields, buildings).map_err(BalanceError::Domain)
}

fn parse_resource(name: &str) -> Result<ResourceKind, BalanceError> {
    match name {
        "wood" => Ok(ResourceKind::Wood),
        "clay" => Ok(ResourceKind::Clay),
        "iron" => Ok(ResourceKind::Iron),
        "crop" => Ok(ResourceKind::Crop),
        other => Err(BalanceError::UnknownResource(other.to_owned())),
    }
}

fn parse_building(name: &str) -> Result<BuildingKind, BalanceError> {
    match name {
        "main_building" => Ok(BuildingKind::MainBuilding),
        "rally_point" => Ok(BuildingKind::RallyPoint),
        "warehouse" => Ok(BuildingKind::Warehouse),
        "granary" => Ok(BuildingKind::Granary),
        "barracks" => Ok(BuildingKind::Barracks),
        "academy" => Ok(BuildingKind::Academy),
        "smithy" => Ok(BuildingKind::Smithy),
        "stable" => Ok(BuildingKind::Stable),
        "workshop" => Ok(BuildingKind::Workshop),
        "residence" => Ok(BuildingKind::Residence),
        other => Err(BalanceError::UnknownBuilding(other.to_owned())),
    }
}

#[derive(Deserialize)]
struct EconomyDto {
    production: ProductionDto,
    population: PopulationDto,
    capacity: CapacityDto,
    starting_amounts: AmountsDto,
}

#[derive(Deserialize)]
struct ProductionDto {
    wood: Vec<i64>,
    clay: Vec<i64>,
    iron: Vec<i64>,
    crop: Vec<i64>,
}

#[derive(Deserialize)]
struct PopulationDto {
    field: Vec<i64>,
    /// Per-building tables keyed by building name.
    #[serde(flatten)]
    buildings: HashMap<String, Vec<i64>>,
}

#[derive(Deserialize)]
struct CapacityDto {
    warehouse: Vec<i64>,
    granary: Vec<i64>,
}

#[derive(Deserialize)]
struct AmountsDto {
    wood: i64,
    clay: i64,
    iron: i64,
    crop: i64,
}

/// Load the economy rules (production/population/capacity/starting amounts) from balance data.
///
/// # Errors
/// Returns [`BalanceError`] if the data cannot be parsed.
pub fn economy_rules() -> Result<EconomyRules, BalanceError> {
    let dto: EconomyDto = toml::from_str(ECONOMY_TOML)?;
    let mut building_population_per_level = HashMap::new();
    for (name, table) in dto.population.buildings {
        building_population_per_level.insert(parse_building(&name)?, table);
    }
    Ok(EconomyRules {
        wood_per_level: dto.production.wood,
        clay_per_level: dto.production.clay,
        iron_per_level: dto.production.iron,
        crop_per_level: dto.production.crop,
        field_population_per_level: dto.population.field,
        building_population_per_level,
        warehouse_capacity_per_level: dto.capacity.warehouse,
        granary_capacity_per_level: dto.capacity.granary,
        starting_amounts: ResourceAmounts {
            wood: dto.starting_amounts.wood,
            clay: dto.starting_amounts.clay,
            iron: dto.starting_amounts.iron,
            crop: dto.starting_amounts.crop,
        },
    })
}

#[derive(Deserialize)]
struct ConstructionDto {
    speed: SpeedDto,
    field: LevelSpecDto,
    buildings: BuildingsDto,
}

#[derive(Deserialize)]
struct SpeedDto {
    main_building_factor: Vec<f64>,
}

#[derive(Deserialize)]
struct CostDto {
    wood: Vec<i64>,
    clay: Vec<i64>,
    iron: Vec<i64>,
    crop: Vec<i64>,
}

#[derive(Deserialize)]
struct PrereqDto {
    building: String,
    level: u8,
}

#[derive(Deserialize)]
struct LevelSpecDto {
    time_secs: Vec<i64>,
    cost: CostDto,
    #[serde(default)]
    prerequisites: Vec<PrereqDto>,
}

#[derive(Deserialize)]
struct BuildingsDto {
    main_building: LevelSpecDto,
    rally_point: LevelSpecDto,
    warehouse: LevelSpecDto,
    granary: LevelSpecDto,
    barracks: LevelSpecDto,
    academy: LevelSpecDto,
    smithy: LevelSpecDto,
}

fn level_spec(dto: &LevelSpecDto) -> LevelSpec {
    let cost_per_level = (0..dto.time_secs.len())
        .map(|i| ResourceAmounts {
            wood: dto.cost.wood.get(i).copied().unwrap_or(0),
            clay: dto.cost.clay.get(i).copied().unwrap_or(0),
            iron: dto.cost.iron.get(i).copied().unwrap_or(0),
            crop: dto.cost.crop.get(i).copied().unwrap_or(0),
        })
        .collect();
    LevelSpec {
        cost_per_level,
        time_secs_per_level: dto.time_secs.clone(),
    }
}

fn prereqs(dto: &LevelSpecDto) -> Result<Vec<(BuildingKind, u8)>, BalanceError> {
    dto.prerequisites
        .iter()
        .map(|p| Ok((parse_building(&p.building)?, p.level)))
        .collect()
}

/// Load construction rules (costs, times, MB speed factors, prerequisites) from balance data.
///
/// # Errors
/// Returns [`BalanceError`] if the data cannot be parsed or names an unknown building.
pub fn build_rules() -> Result<BuildRules, BalanceError> {
    let dto: ConstructionDto = toml::from_str(CONSTRUCTION_TOML)?;
    let mut buildings = HashMap::new();
    let mut prerequisites = HashMap::new();
    for (kind, spec_dto) in [
        (BuildingKind::MainBuilding, &dto.buildings.main_building),
        (BuildingKind::RallyPoint, &dto.buildings.rally_point),
        (BuildingKind::Warehouse, &dto.buildings.warehouse),
        (BuildingKind::Granary, &dto.buildings.granary),
        (BuildingKind::Barracks, &dto.buildings.barracks),
        (BuildingKind::Academy, &dto.buildings.academy),
        (BuildingKind::Smithy, &dto.buildings.smithy),
    ] {
        buildings.insert(kind, level_spec(spec_dto));
        let pr = prereqs(spec_dto)?;
        if !pr.is_empty() {
            prerequisites.insert(kind, pr);
        }
    }
    Ok(BuildRules {
        field: level_spec(&dto.field),
        buildings,
        prerequisites,
        main_building_factor_per_level: dto.speed.main_building_factor,
    })
}

#[derive(Deserialize)]
struct UnitsDto {
    training: TrainingDto,
    smithy: SmithyDto,
    romans: TribeUnitsDto,
    teutons: TribeUnitsDto,
    gauls: TribeUnitsDto,
}

#[derive(Deserialize)]
struct TrainingDto {
    building_factor: Vec<f64>,
}

#[derive(Deserialize)]
struct SmithyDto {
    cost_permille_per_level: Vec<u32>,
    time_secs_per_level: Vec<i64>,
}

#[derive(Deserialize)]
struct TribeUnitsDto {
    units: Vec<UnitDto>,
}

#[derive(Deserialize)]
struct UnitDto {
    id: String,
    name: String,
    role: String,
    attack: u32,
    defense_infantry: u32,
    defense_cavalry: u32,
    speed: u32,
    carry_capacity: u32,
    crop_upkeep: u32,
    train_secs: i64,
    trained_in: String,
    cost: AmountsDto,
    research: Option<ResearchDto>,
}

#[derive(Deserialize)]
struct ResearchDto {
    time_secs: i64,
    cost: AmountsDto,
    requirements: Vec<PrereqDto>,
}

fn parse_role(name: &str) -> Result<UnitRole, BalanceError> {
    match name {
        "infantry" => Ok(UnitRole::Infantry),
        "cavalry" => Ok(UnitRole::Cavalry),
        "scout" => Ok(UnitRole::Scout),
        "siege" => Ok(UnitRole::Siege),
        "expansion" => Ok(UnitRole::Expansion),
        other => Err(BalanceError::UnknownUnitRole(other.to_owned())),
    }
}

fn amounts(dto: &AmountsDto) -> ResourceAmounts {
    ResourceAmounts {
        wood: dto.wood,
        clay: dto.clay,
        iron: dto.iron,
        crop: dto.crop,
    }
}

fn unit_spec(dto: &UnitDto) -> Result<UnitSpec, BalanceError> {
    let research = match &dto.research {
        None => None,
        Some(r) => Some(ResearchSpec {
            cost: amounts(&r.cost),
            time_secs: r.time_secs,
            requirements: r
                .requirements
                .iter()
                .map(|p| Ok((parse_building(&p.building)?, p.level)))
                .collect::<Result<_, BalanceError>>()?,
        }),
    };
    Ok(UnitSpec {
        id: UnitId(dto.id.clone()),
        name: dto.name.clone(),
        role: parse_role(&dto.role)?,
        attack: dto.attack,
        defense_infantry: dto.defense_infantry,
        defense_cavalry: dto.defense_cavalry,
        speed: dto.speed,
        carry_capacity: dto.carry_capacity,
        crop_upkeep: dto.crop_upkeep,
        cost: amounts(&dto.cost),
        train_secs: dto.train_secs,
        trained_in: parse_building(&dto.trained_in)?,
        research,
    })
}

/// Load the per-tribe unit rosters and Smithy upgrade tables from balance data.
///
/// Fails fast (004 AC4): parsing or [`UnitRules`] roster validation errors surface immediately.
///
/// # Errors
/// Returns [`BalanceError`] if the data cannot be parsed, names an unknown building/role, or does
/// not form complete rosters.
pub fn unit_rules() -> Result<UnitRules, BalanceError> {
    parse_unit_rules(UNITS_TOML)
}

fn parse_unit_rules(toml_src: &str) -> Result<UnitRules, BalanceError> {
    let dto: UnitsDto = toml::from_str(toml_src)?;
    let mut rosters = HashMap::new();
    for (tribe, tribe_dto) in [
        (Tribe::Romans, &dto.romans),
        (Tribe::Teutons, &dto.teutons),
        (Tribe::Gauls, &dto.gauls),
    ] {
        let roster: Vec<UnitSpec> = tribe_dto
            .units
            .iter()
            .map(unit_spec)
            .collect::<Result<_, _>>()?;
        rosters.insert(tribe, roster);
    }
    let smithy = SmithyRules {
        cost_permille_per_level: dto.smithy.cost_permille_per_level,
        time_secs_per_level: dto.smithy.time_secs_per_level,
    };
    let training = TrainingRules {
        building_factor_per_level: dto.training.building_factor,
    };
    UnitRules::new(rosters, smithy, training).map_err(BalanceError::Domain)
}

#[cfg(test)]
mod tests {
    use super::*;
    use eperica_domain::{BuildTarget, GameSpeed, ROSTER_SIZE, production_rates};

    #[test]
    fn loads_balanced_starting_village() {
        let sv = starting_village().expect("balance data loads");
        assert_eq!(sv.fields().len(), 18);

        let count = |k: ResourceKind| sv.fields().iter().filter(|f| f.kind == k).count();
        assert_eq!(count(ResourceKind::Wood), 4);
        assert_eq!(count(ResourceKind::Clay), 4);
        assert_eq!(count(ResourceKind::Iron), 4);
        assert_eq!(count(ResourceKind::Crop), 6);

        let bkinds: Vec<_> = sv.buildings().iter().map(|b| b.kind).collect();
        assert!(bkinds.contains(&BuildingKind::MainBuilding));
        assert!(bkinds.contains(&BuildingKind::RallyPoint));
    }

    #[test]
    fn starting_village_has_positive_economy() {
        // AC6: the starting village produces wood/clay/iron and has positive net crop.
        let sv = starting_village().expect("starting village");
        let rules = economy_rules().expect("economy rules");
        let rates = production_rates(
            sv.fields(),
            sv.buildings(),
            &rules,
            GameSpeed::new(1.0).unwrap(),
        );
        assert!(rates.wood > 0);
        assert!(rates.clay > 0);
        assert!(rates.iron > 0);
        assert!(rates.crop_net > 0, "net crop was {}", rates.crop_net);
    }

    #[test]
    fn loads_construction_rules() {
        let r = build_rules().expect("build rules");
        let field = BuildTarget::Field { slot: 0 };
        assert_eq!(r.max_level(field), 10);
        assert!(r.cost(field, 0).is_some());
        assert!(r.cost(field, 10).is_none()); // at max
        assert_eq!(
            r.prerequisites(BuildingKind::Warehouse),
            &[(BuildingKind::MainBuilding, 1)]
        );
        let warehouse = BuildTarget::Building {
            slot: 0,
            kind: BuildingKind::Warehouse,
        };
        assert!(r.cost(warehouse, 0).is_some());
    }

    #[test]
    fn military_buildings_have_spec_prerequisites() {
        // 004 AC5: Barracks <- MB>=3; Academy <- MB>=3 + Barracks>=3; Smithy <- MB>=3 + Academy>=1.
        let r = build_rules().expect("build rules");
        assert_eq!(
            r.prerequisites(BuildingKind::Barracks),
            &[(BuildingKind::MainBuilding, 3)]
        );
        assert_eq!(
            r.prerequisites(BuildingKind::Academy),
            &[(BuildingKind::MainBuilding, 3), (BuildingKind::Barracks, 3)]
        );
        assert_eq!(
            r.prerequisites(BuildingKind::Smithy),
            &[(BuildingKind::MainBuilding, 3), (BuildingKind::Academy, 1)]
        );
        for kind in [
            BuildingKind::Barracks,
            BuildingKind::Academy,
            BuildingKind::Smithy,
        ] {
            let target = BuildTarget::Building { slot: 0, kind };
            assert_eq!(r.max_level(target), 10, "{kind:?} levels");
        }
    }

    #[test]
    fn loads_complete_unit_rosters() {
        // 004 AC4: every tribe has a complete 10-unit roster with all attributes.
        let r = unit_rules().expect("unit rules load");
        for tribe in [Tribe::Romans, Tribe::Teutons, Tribe::Gauls] {
            let roster = r.roster(tribe);
            assert_eq!(roster.len(), ROSTER_SIZE, "{tribe:?}");
            // Exactly one tier-1 unit per tribe (researched by default, AC9).
            assert_eq!(
                roster.iter().filter(|u| u.researched_by_default()).count(),
                1,
                "{tribe:?}"
            );
            // Every researchable unit has a positive research cost and duration.
            for u in roster.iter().filter_map(|u| u.research.as_ref()) {
                assert!(u.time_secs > 0);
                assert!(u.cost.wood > 0);
            }
        }
        // The tier-1 units are the faithful ones.
        assert!(
            r.unit(Tribe::Romans, &UnitId("legionnaire".into()))
                .is_some()
        );
        assert!(
            r.unit(Tribe::Teutons, &UnitId("clubswinger".into()))
                .is_some()
        );
        assert!(r.unit(Tribe::Gauls, &UnitId("phalanx".into())).is_some());
        // Smithy tables cover 20 levels.
        assert_eq!(r.smithy.max_level(), 20);
    }

    #[test]
    fn incomplete_unit_data_fails_fast() {
        // 004 AC4: loading must fail when a roster is incomplete or a field is missing.
        let missing_tribe =
            "[smithy]\ncost_permille_per_level = [1500]\ntime_secs_per_level = [3600]\n";
        assert!(parse_unit_rules(missing_tribe).is_err());

        // A structurally-valid file whose Gauls roster is short must be rejected by validation.
        let full = UNITS_TOML;
        let truncated = &full[..full.rfind("[[gauls.units]]").expect("marker")];
        assert!(parse_unit_rules(truncated).is_err());
    }

    #[test]
    fn every_constructable_kind_has_population_data() {
        // A kind missing from the population map would silently contribute 0 population.
        let build = build_rules().expect("build rules");
        let economy = economy_rules().expect("economy rules");
        for kind in build.buildings.keys() {
            assert!(
                economy.building_population_per_level.contains_key(kind),
                "no population table for {kind:?}"
            );
        }
    }
}
