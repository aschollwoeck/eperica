//! Balance-data loading.
//!
//! Numeric and structural balance lives in `specs/balance/` as **data** (not hardcoded in logic, per
//! the constitution). This module embeds that data at compile time and parses it into pure domain
//! types, keeping the domain itself free of serialization concerns.

use eperica_domain::{
    BuildRules, BuildingKind, BuildingSlot, CombatRules, DomainError, EconomyRules,
    FieldDistribution, LevelSpec, MapRules, MerchantProfile, MerchantRules, OasisBonus, OasisRules,
    ResearchSpec, ResourceAmounts, ResourceField, ResourceKind, ScoutRules, SiegeKind, SmithyRules,
    StartingVillage, TrainingRules, Tribe, UnitId, UnitRole, UnitRules, UnitSpec, WallProfile,
    Weighted,
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

/// Embedded world-map balance data.
const MAP_TOML: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../specs/balance/map.toml"
));

/// Embedded trade/merchant balance data.
const TRADE_TOML: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../specs/balance/trade.toml"
));

/// Embedded combat balance data.
const COMBAT_TOML: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../specs/balance/combat.toml"
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
        "marketplace" => Ok(BuildingKind::Marketplace),
        "wall" => Ok(BuildingKind::Wall),
        "barracks" => Ok(BuildingKind::Barracks),
        "academy" => Ok(BuildingKind::Academy),
        "smithy" => Ok(BuildingKind::Smithy),
        "stable" => Ok(BuildingKind::Stable),
        "workshop" => Ok(BuildingKind::Workshop),
        "residence" => Ok(BuildingKind::Residence),
        "cranny" => Ok(BuildingKind::Cranny),
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
    marketplace: LevelSpecDto,
    wall: LevelSpecDto,
    barracks: LevelSpecDto,
    academy: LevelSpecDto,
    smithy: LevelSpecDto,
    stable: LevelSpecDto,
    workshop: LevelSpecDto,
    cranny: LevelSpecDto,
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
        (BuildingKind::Marketplace, &dto.buildings.marketplace),
        (BuildingKind::Wall, &dto.buildings.wall),
        (BuildingKind::Barracks, &dto.buildings.barracks),
        (BuildingKind::Academy, &dto.buildings.academy),
        (BuildingKind::Smithy, &dto.buildings.smithy),
        (BuildingKind::Stable, &dto.buildings.stable),
        (BuildingKind::Workshop, &dto.buildings.workshop),
        (BuildingKind::Cranny, &dto.buildings.cranny),
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
struct TradeDto {
    merchants: MerchantsDto,
    tribes: TradeTribesDto,
}

#[derive(Deserialize)]
struct MerchantsDto {
    per_level: Vec<u32>,
}

#[derive(Deserialize)]
struct TradeTribesDto {
    romans: MerchantProfileDto,
    teutons: MerchantProfileDto,
    gauls: MerchantProfileDto,
}

#[derive(Deserialize)]
struct MerchantProfileDto {
    capacity: u32,
    speed: u32,
}

impl From<&MerchantProfileDto> for MerchantProfile {
    fn from(dto: &MerchantProfileDto) -> Self {
        MerchantProfile {
            capacity: dto.capacity,
            speed: dto.speed,
        }
    }
}

/// Load the merchant/trade rules (per-tribe capacity + speed, merchants per Marketplace level) from
/// the embedded balance data.
///
/// # Errors
/// Returns [`BalanceError`] if the data cannot be parsed or does not form valid merchant rules.
pub fn merchant_rules() -> Result<MerchantRules, BalanceError> {
    let dto: TradeDto = toml::from_str(TRADE_TOML)?;
    let profiles = HashMap::from([
        (Tribe::Romans, MerchantProfile::from(&dto.tribes.romans)),
        (Tribe::Teutons, MerchantProfile::from(&dto.tribes.teutons)),
        (Tribe::Gauls, MerchantProfile::from(&dto.tribes.gauls)),
    ]);
    MerchantRules::new(profiles, dto.merchants.per_level).map_err(BalanceError::Domain)
}

#[derive(Deserialize)]
struct CombatDto {
    loss_exponent: f64,
    luck_range: f64,
    morale_exponent: f64,
    base_defense: f64,
    smithy_bonus_per_level: f64,
    catapult_durability: f64,
    walls: CombatWallsDto,
    scouting: ScoutingDto,
    loot: LootDto,
}

#[derive(Deserialize)]
struct LootDto {
    teuton_cranny_bypass: f64,
    cranny_protection_per_level: Vec<i64>,
}

#[derive(Deserialize)]
struct ScoutingDto {
    loss_exponent: f64,
}

#[derive(Deserialize)]
struct CombatWallsDto {
    romans: WallDto,
    teutons: WallDto,
    gauls: WallDto,
}

#[derive(Deserialize)]
struct WallDto {
    bonus_per_level: Vec<f64>,
    ram_durability: f64,
}

impl From<&WallDto> for WallProfile {
    fn from(dto: &WallDto) -> Self {
        WallProfile {
            bonus_per_level: dto.bonus_per_level.clone(),
            ram_durability: dto.ram_durability,
        }
    }
}

/// Load the combat rules (loss/luck/morale/base scalars + per-tribe Wall profiles) from balance data.
///
/// # Errors
/// Returns [`BalanceError`] if the data cannot be parsed.
pub fn combat_rules() -> Result<CombatRules, BalanceError> {
    let dto: CombatDto = toml::from_str(COMBAT_TOML)?;
    Ok(CombatRules {
        loss_exponent: dto.loss_exponent,
        luck_range: dto.luck_range,
        morale_exponent: dto.morale_exponent,
        base_defense: dto.base_defense,
        smithy_bonus_per_level: dto.smithy_bonus_per_level,
        catapult_durability: dto.catapult_durability,
        cranny_bypass_teuton: dto.loot.teuton_cranny_bypass,
        cranny_protection_per_level: dto.loot.cranny_protection_per_level,
        walls: HashMap::from([
            (Tribe::Romans, WallProfile::from(&dto.walls.romans)),
            (Tribe::Teutons, WallProfile::from(&dto.walls.teutons)),
            (Tribe::Gauls, WallProfile::from(&dto.walls.gauls)),
        ]),
    })
}

/// Load the scouting/espionage rules (the attacking-scout loss exponent) from combat balance (010).
///
/// # Errors
/// Returns [`BalanceError`] if the data cannot be parsed.
pub fn scout_rules() -> Result<ScoutRules, BalanceError> {
    let dto: CombatDto = toml::from_str(COMBAT_TOML)?;
    Ok(ScoutRules {
        loss_exponent: dto.scouting.loss_exponent,
    })
}

#[derive(Deserialize)]
struct UnitsDto {
    training: TrainingDto,
    smithy: SmithyDto,
    romans: TribeUnitsDto,
    teutons: TribeUnitsDto,
    gauls: TribeUnitsDto,
    /// Oasis defenders (012) — defence-only, not a tribe.
    #[serde(default)]
    wild_animals: Vec<WildAnimalDto>,
    /// Seeded oasis-garrison generation balance (012).
    oasis_garrison: OasisGarrisonDto,
}

#[derive(Deserialize)]
struct WildAnimalDto {
    id: String,
    name: String,
    defense_infantry: u32,
    defense_cavalry: u32,
}

#[derive(Deserialize)]
struct OasisGarrisonDto {
    base_count: u32,
    extra_per_step: u32,
    tiles_per_step: u32,
    max_count: u32,
    tiles_per_tier: u32,
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
    /// Espionage / counter-espionage strength (010); absent (⇒ 0) for every non-Scout unit.
    #[serde(default)]
    scouting: u32,
    speed: u32,
    carry_capacity: u32,
    crop_upkeep: u32,
    train_secs: i64,
    trained_in: String,
    cost: AmountsDto,
    research: Option<ResearchDto>,
    /// Siege target for siege units (`"ram"`/`"catapult"`); absent for all other roles.
    #[serde(default)]
    siege: Option<String>,
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
        scouting: dto.scouting,
        speed: dto.speed,
        carry_capacity: dto.carry_capacity,
        crop_upkeep: dto.crop_upkeep,
        cost: amounts(&dto.cost),
        train_secs: dto.train_secs,
        trained_in: parse_building(&dto.trained_in)?,
        research,
        siege_kind: parse_siege(dto.siege.as_deref())?,
    })
}

fn parse_siege(s: Option<&str>) -> Result<Option<SiegeKind>, BalanceError> {
    match s {
        None => Ok(None),
        Some("ram") => Ok(Some(SiegeKind::Ram)),
        Some("catapult") => Ok(Some(SiegeKind::Catapult)),
        Some(other) => Err(BalanceError::UnknownUnitRole(format!("siege:{other}"))),
    }
}

/// Build a wild-animal [`UnitSpec`] (012): a defence-only oasis guard with no offence, no upkeep,
/// no cost, and no training — every non-defensive attribute is zero/empty.
fn wild_animal_spec(dto: &WildAnimalDto) -> UnitSpec {
    UnitSpec {
        id: UnitId(dto.id.clone()),
        name: dto.name.clone(),
        role: UnitRole::Wild,
        attack: 0,
        defense_infantry: dto.defense_infantry,
        defense_cavalry: dto.defense_cavalry,
        scouting: 0,
        speed: 0,
        carry_capacity: 0,
        crop_upkeep: 0,
        cost: ResourceAmounts::default(),
        train_secs: 0,
        trained_in: BuildingKind::Barracks,
        research: None,
        siege_kind: None,
    }
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
    let wild_animals: Vec<UnitSpec> = dto.wild_animals.iter().map(wild_animal_spec).collect();
    UnitRules::new(rosters, smithy, training)
        .map(|r| r.with_wild_animals(wild_animals))
        .map_err(BalanceError::Domain)
}

/// Load the seeded oasis-garrison generation rules (012) from unit balance data.
///
/// # Errors
/// Returns [`BalanceError`] if the data cannot be parsed.
pub fn oasis_rules() -> Result<OasisRules, BalanceError> {
    let dto: UnitsDto = toml::from_str(UNITS_TOML)?;
    Ok(OasisRules {
        base_count: dto.oasis_garrison.base_count,
        extra_per_step: dto.oasis_garrison.extra_per_step,
        tiles_per_step: dto.oasis_garrison.tiles_per_step,
        max_count: dto.oasis_garrison.max_count,
        tiles_per_tier: dto.oasis_garrison.tiles_per_tier,
    })
}

#[derive(Deserialize)]
struct MapDto {
    oasis_permille: u32,
    natar_permille: u32,
    distributions: Vec<DistributionDto>,
    oasis_bonuses: Vec<OasisBonusDto>,
}

#[derive(Deserialize)]
struct DistributionDto {
    wood: u8,
    clay: u8,
    iron: u8,
    crop: u8,
    weight: u32,
}

#[derive(Deserialize)]
struct OasisBonusDto {
    wood: u8,
    clay: u8,
    iron: u8,
    crop: u8,
    weight: u32,
}

/// Load the world-map generation rules (densities + weighted distribution/bonus tables).
///
/// Fails fast (006 AC2): parsing or [`MapRules`] validation (e.g. a distribution not summing to 18)
/// surfaces immediately.
///
/// # Errors
/// Returns [`BalanceError`] if the data cannot be parsed or does not form valid map rules.
pub fn map_rules() -> Result<MapRules, BalanceError> {
    parse_map_rules(MAP_TOML)
}

fn parse_map_rules(toml_src: &str) -> Result<MapRules, BalanceError> {
    let dto: MapDto = toml::from_str(toml_src)?;
    let mut distributions = Vec::with_capacity(dto.distributions.len());
    for d in &dto.distributions {
        distributions.push(Weighted {
            value: FieldDistribution::new(d.wood, d.clay, d.iron, d.crop)
                .map_err(BalanceError::Domain)?,
            weight: d.weight,
        });
    }
    let oasis_bonuses = dto
        .oasis_bonuses
        .iter()
        .map(|b| Weighted {
            value: OasisBonus {
                wood: b.wood,
                clay: b.clay,
                iron: b.iron,
                crop: b.crop,
            },
            weight: b.weight,
        })
        .collect();
    MapRules::new(
        dto.oasis_permille,
        dto.natar_permille,
        distributions,
        oasis_bonuses,
    )
    .map_err(BalanceError::Domain)
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
            0,
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
    fn troop_buildings_have_spec_prerequisites() {
        // 005 AC1: Stable <- Academy>=5 + Smithy>=1; Workshop <- MB>=5 + Academy>=10.
        let r = build_rules().expect("build rules");
        assert_eq!(
            r.prerequisites(BuildingKind::Stable),
            &[(BuildingKind::Academy, 5), (BuildingKind::Smithy, 1)]
        );
        assert_eq!(
            r.prerequisites(BuildingKind::Workshop),
            &[(BuildingKind::MainBuilding, 5), (BuildingKind::Academy, 10)]
        );
        for kind in [BuildingKind::Stable, BuildingKind::Workshop] {
            let target = BuildTarget::Building { slot: 0, kind };
            assert_eq!(r.max_level(target), 10, "{kind:?} levels");
        }
        // The training factor table loaded and is usable (T1/T3).
        let units = unit_rules().expect("unit rules");
        assert!(units.training.building_factor(10) > units.training.building_factor(1));
    }

    #[test]
    fn loads_valid_map_rules() {
        // 006 AC2: the shipped map balance loads, every valley distribution sums to 18, and the
        // origin region has valleys to place on.
        use eperica_domain::{Coordinate, TileKind, WorldMap, coordinates_within};
        let rules = map_rules().expect("map rules load");
        let map = WorldMap::new(0xDEAD_BEEF, 50, rules);
        for c in coordinates_within(50) {
            if let Some(TileKind::Valley(d)) = map.tile_at(c) {
                assert_eq!(d.sum(), 18, "{d:?} at {c:?}");
            }
        }
        let valleys = coordinates_within(5).filter(|c| map.is_valley(*c)).count();
        assert!(valleys > 5, "only {valleys} valleys near the origin");
        // A different seed yields a different map.
        let other = WorldMap::new(1, 50, map_rules().unwrap());
        assert!(
            (-10..=10).any(|x| map.tile_at(Coordinate::new(x, 0))
                != other.tile_at(Coordinate::new(x, 0)))
        );
    }

    #[test]
    fn bad_map_distribution_fails_fast() {
        // A distribution not summing to 18 is rejected at load (006 AC2).
        let bad = "oasis_permille = 100\nnatar_permille = 10\n\
            [[distributions]]\nwood=4\nclay=4\niron=4\ncrop=4\nweight=1\n\
            [[oasis_bonuses]]\nwood=25\nclay=0\niron=0\ncrop=0\nweight=1\n";
        assert!(parse_map_rules(bad).is_err());
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
    fn loads_wild_animals_and_oasis_rules() {
        // 012: the wild-animal roster loads as defence-only guards and the oasis-garrison balance
        // produces a non-empty, distance-scaled garrison.
        use eperica_domain::{Coordinate, oasis_garrison};
        let units = unit_rules().expect("unit rules");
        let animals = units.wild_animal_roster();
        assert!(!animals.is_empty(), "wild-animal roster must be populated");
        for a in animals {
            assert_eq!(a.role, UnitRole::Wild);
            assert_eq!(a.attack, 0, "wild animals have no offence");
            assert!(
                a.defense_infantry > 0 || a.defense_cavalry > 0,
                "{} must defend",
                a.id.0
            );
            assert_eq!(a.crop_upkeep, 0);
        }
        let rules = oasis_rules().expect("oasis rules");
        assert!(rules.base_count > 0);
        let near = oasis_garrison(7, Coordinate::new(1, 0), animals, &rules);
        let far = oasis_garrison(7, Coordinate::new(60, 0), animals, &rules);
        assert!(!near.is_empty(), "origin oasis must hold animals");
        let total = |g: &eperica_domain::UnitCounts| g.iter().map(|(_, n)| *n).sum::<u32>();
        assert!(total(&far) >= total(&near));
        assert!(total(&far) <= rules.max_count);
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
    fn loads_combat_rules() {
        // 009: combat scalars are present and every tribe has a Wall profile.
        let r = combat_rules().expect("combat rules");
        assert!(r.loss_exponent > 1.0);
        assert!((0.0..=1.0).contains(&r.luck_range));
        assert_eq!(r.smithy_factor(0), 1.0);
        assert!(r.smithy_factor(10) > 1.0);
        // 011 siege/loot balance: catapult durability + Cranny protection load and rise with level.
        assert!(r.catapult_durability > 0.0);
        assert!((0.0..=1.0).contains(&r.cranny_bypass_teuton));
        assert_eq!(r.cranny_capacity(0), 0);
        assert!(r.cranny_capacity(10) > r.cranny_capacity(1));
        // A Wall raises defence: resolving with a wall hurts the attacker more than without.
        use eperica_domain::{AttackMode, AttackPower, BattleInput, resolve_battle};
        let base = BattleInput {
            attack: AttackPower {
                infantry: 500.0,
                cavalry: 0.0,
                ram: 0.0,
            },
            def_infantry: 400.0,
            def_cavalry: 0.0,
            wall_tribe: Tribe::Romans,
            wall_level: 0,
            attacker_pop: 100,
            defender_pop: 100,
        };
        let walled = BattleInput {
            wall_level: 10,
            ..base
        };
        let a = resolve_battle(AttackMode::Raid, base, &r, 1.0);
        let b = resolve_battle(AttackMode::Raid, walled, &r, 1.0);
        assert!(b.attacker_loss_frac > a.attacker_loss_frac);
    }

    #[test]
    fn loads_scout_rules_and_scout_strengths() {
        // 010: the espionage loss exponent loads and is a real power-law (> 1).
        let s = scout_rules().expect("scout rules");
        assert!(s.loss_exponent > 1.0);

        // Every tribe's Scout-role unit carries a positive `scouting`; non-scouts stay at 0.
        let units = unit_rules().expect("unit rules");
        for tribe in [Tribe::Romans, Tribe::Teutons, Tribe::Gauls] {
            let roster = units.roster(tribe);
            let scouts: Vec<_> = roster
                .iter()
                .filter(|u| u.role == UnitRole::Scout)
                .collect();
            assert_eq!(scouts.len(), 1, "{tribe:?} should have one scout");
            assert!(scouts[0].scouting > 0, "{tribe:?} scout needs scouting > 0");
            for u in roster.iter().filter(|u| u.role != UnitRole::Scout) {
                assert_eq!(u.scouting, 0, "non-scout {} must have scouting 0", u.id.0);
            }
        }
    }

    #[test]
    fn loads_merchant_rules() {
        // 008: every tribe has a positive merchant profile and the per-level table is non-empty.
        let r = merchant_rules().expect("merchant rules");
        assert_eq!(r.merchants_total(0), 0);
        assert!(r.merchants_total(1) >= 1);
        assert_eq!(r.profile(Tribe::Teutons).capacity, 1000);
        assert_eq!(r.profile(Tribe::Gauls).speed, 24);
        assert!(r.profile(Tribe::Romans).capacity > 0);
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
