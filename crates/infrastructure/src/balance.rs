//! Balance-data loading.
//!
//! Numeric and structural balance lives in `specs/balance/` as **data** (not hardcoded in logic, per
//! the constitution). This module embeds that data at compile time and parses it into pure domain
//! types, keeping the domain itself free of serialization concerns.

use eperica_domain::{
    BuildRules, BuildingKind, BuildingSlot, DomainError, EconomyRules, LevelSpec, ResourceAmounts,
    ResourceField, ResourceKind, StartingVillage,
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

#[cfg(test)]
mod tests {
    use super::*;
    use eperica_domain::{BuildTarget, GameSpeed, production_rates};

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
