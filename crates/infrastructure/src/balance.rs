//! Balance-data loading.
//!
//! Numeric and structural balance lives in `specs/balance/` as **data** (not hardcoded in logic, per
//! the constitution). This module embeds that data at compile time and parses it into pure domain
//! types, keeping the domain itself free of serialization concerns.

use eperica_domain::{
    AchievementDef, AchievementId, AchievementKind, AllianceRules, ArtifactDef, ArtifactId,
    ArtifactKind, ArtifactScope, BuildRules, BuildingKind, BuildingSlot, CombatRules, CultureRules,
    DomainError, EconomyRules, FairPlayRules, FieldDistribution, LevelSpec, LifecycleRules,
    LoyaltyRules, MapRules, MedalCategory, MedalRules, MerchantProfile, MerchantRules, OasisBonus,
    OasisRules, QuestCondition, QuestDef, QuestId, QuestReward, RankingRules, ResearchSpec,
    ResourceAmounts, ResourceField, ResourceKind, Reward, ScoutRules, SiegeKind, SmithyRules,
    StartingVillage, TrainingRules, Tribe, UnitId, UnitRole, UnitRules, UnitSpec, WallProfile,
    Weighted, WonderRules, wonder_level_spec,
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

/// Embedded culture/expansion balance data.
const CULTURE_TOML: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../specs/balance/culture.toml"
));
const CONQUEST_TOML: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../specs/balance/conquest.toml"
));
const ALLIANCE_TOML: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../specs/balance/alliance.toml"
));
const RANKING_TOML: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../specs/balance/ranking.toml"
));
const MEDALS_TOML: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../specs/balance/medals.toml"
));
const ACHIEVEMENTS_TOML: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../specs/balance/achievements.toml"
));
const QUESTS_TOML: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../specs/balance/quests.toml"
));
const LIFECYCLE_TOML: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../specs/balance/lifecycle.toml"
));
const ARTIFACTS_TOML: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../specs/balance/artifacts.toml"
));
const WONDER_TOML: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../specs/balance/wonder.toml"
));
const FAIRPLAY_TOML: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../specs/balance/fairplay.toml"
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
    /// An unknown medal category appeared in the data (017).
    #[error("unknown medal category: {0}")]
    UnknownMedalCategory(String),
    /// An unknown achievement kind appeared in the data (017).
    #[error("unknown achievement kind: {0}")]
    UnknownAchievementKind(String),
    /// An unknown quest condition kind appeared in the data (018).
    #[error("unknown quest condition kind: {0}")]
    UnknownQuestCondition(String),
    /// A quest id appeared more than once in the chain (018) — ids must be unique (the completion PK).
    #[error("duplicate quest id: {0}")]
    DuplicateQuestId(String),
    /// An unknown artifact kind appeared in the data (020).
    #[error("unknown artifact kind: {0}")]
    UnknownArtifactKind(String),
    /// An unknown artifact scope appeared in the data (020).
    #[error("unknown artifact scope: {0}")]
    UnknownArtifactScope(String),
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
        "embassy" => Ok(BuildingKind::Embassy),
        "wall" => Ok(BuildingKind::Wall),
        "barracks" => Ok(BuildingKind::Barracks),
        "academy" => Ok(BuildingKind::Academy),
        "smithy" => Ok(BuildingKind::Smithy),
        "stable" => Ok(BuildingKind::Stable),
        "workshop" => Ok(BuildingKind::Workshop),
        "residence" => Ok(BuildingKind::Residence),
        "cranny" => Ok(BuildingKind::Cranny),
        "outpost" => Ok(BuildingKind::Outpost),
        "town_hall" => Ok(BuildingKind::TownHall),
        "palace" => Ok(BuildingKind::Palace),
        "treasury" => Ok(BuildingKind::Treasury),
        "wonder" => Ok(BuildingKind::Wonder),
        other => Err(BalanceError::UnknownBuilding(other.to_owned())),
    }
}

#[derive(Deserialize)]
struct EconomyDto {
    production: ProductionDto,
    population: PopulationDto,
    capacity: CapacityDto,
    outpost: OutpostEconomyDto,
    starting_amounts: AmountsDto,
}

#[derive(Deserialize)]
struct OutpostEconomyDto {
    capacity_per_level: Vec<u8>,
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
        outpost_capacity_per_level: dto.outpost.capacity_per_level,
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
    field: FieldSpecDto,
    buildings: BuildingsDto,
}

#[derive(Deserialize)]
struct FieldSpecDto {
    max_level: u8,
    capital_max_level: u8,
    #[serde(flatten)]
    spec: LevelSpecDto,
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
    embassy: LevelSpecDto,
    wall: LevelSpecDto,
    barracks: LevelSpecDto,
    academy: LevelSpecDto,
    smithy: LevelSpecDto,
    stable: LevelSpecDto,
    workshop: LevelSpecDto,
    cranny: LevelSpecDto,
    outpost: LevelSpecDto,
    town_hall: LevelSpecDto,
    residence: LevelSpecDto,
    palace: LevelSpecDto,
    treasury: LevelSpecDto,
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
        (BuildingKind::Embassy, &dto.buildings.embassy),
        (BuildingKind::Wall, &dto.buildings.wall),
        (BuildingKind::Barracks, &dto.buildings.barracks),
        (BuildingKind::Academy, &dto.buildings.academy),
        (BuildingKind::Smithy, &dto.buildings.smithy),
        (BuildingKind::Stable, &dto.buildings.stable),
        (BuildingKind::Workshop, &dto.buildings.workshop),
        (BuildingKind::Cranny, &dto.buildings.cranny),
        (BuildingKind::Outpost, &dto.buildings.outpost),
        (BuildingKind::TownHall, &dto.buildings.town_hall),
        (BuildingKind::Residence, &dto.buildings.residence),
        (BuildingKind::Palace, &dto.buildings.palace),
        (BuildingKind::Treasury, &dto.buildings.treasury),
    ] {
        buildings.insert(kind, level_spec(spec_dto));
        let pr = prereqs(spec_dto)?;
        if !pr.is_empty() {
            prerequisites.insert(kind, pr);
        }
    }
    // 021: the Wonder's 100-level construction curve is generated from `wonder.toml` (no 100-line table)
    // and merged in, so the Wonder reuses the whole 003 build path. No building prerequisite — the gate
    // (site control + held plan) lives in `order_wonder_build`.
    buildings.insert(BuildingKind::Wonder, wonder_level_spec(&wonder_rules()?));
    Ok(BuildRules {
        field: level_spec(&dto.field.spec),
        field_max_level: dto.field.max_level,
        capital_field_max_level: dto.field.capital_max_level,
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
struct CultureDto {
    base_cp_per_village: i64,
    town_hall_cp_per_level: Vec<i64>,
    cp_thresholds: Vec<i64>,
    expansion_slots_per_level: Vec<u32>,
    settlers_per_village: u32,
    settler_id: String,
}

/// Load the culture-point / expansion rules (013) from the embedded balance data.
///
/// # Errors
/// Returns [`BalanceError`] if the data cannot be parsed.
pub fn culture_rules() -> Result<CultureRules, BalanceError> {
    let dto: CultureDto = toml::from_str(CULTURE_TOML)?;
    Ok(CultureRules {
        base_cp_per_village: dto.base_cp_per_village,
        town_hall_cp_per_level: dto.town_hall_cp_per_level,
        cp_thresholds: dto.cp_thresholds,
        expansion_slots_per_level: dto.expansion_slots_per_level,
        settlers_per_village: dto.settlers_per_village,
        settler_id: dto.settler_id,
    })
}

#[derive(Deserialize)]
struct ConquestDto {
    starting_loyalty: i64,
    post_conquest_loyalty: i64,
    loyalty_regen_per_hour: i64,
    loyalty_drop_min: i64,
    loyalty_drop_max: i64,
    administrator_ids: Vec<String>,
}

/// Load the loyalty / conquest rules (014) from the embedded balance data.
///
/// # Errors
/// Returns [`BalanceError`] if the data cannot be parsed.
pub fn loyalty_rules() -> Result<LoyaltyRules, BalanceError> {
    let dto: ConquestDto = toml::from_str(CONQUEST_TOML)?;
    Ok(LoyaltyRules {
        starting_loyalty: dto.starting_loyalty,
        post_conquest_loyalty: dto.post_conquest_loyalty,
        regen_per_hour: dto.loyalty_regen_per_hour,
        drop_min: dto.loyalty_drop_min,
        drop_max: dto.loyalty_drop_max,
        administrator_ids: dto.administrator_ids,
    })
}

#[derive(Deserialize)]
struct AllianceDto {
    max_members: u32,
    join_embassy_level: u8,
    found_embassy_level: u8,
}

/// Load the alliance / diplomacy rules (015) from the embedded balance data.
///
/// # Errors
/// Returns [`BalanceError`] if the data cannot be parsed.
pub fn alliance_rules() -> Result<AllianceRules, BalanceError> {
    let dto: AllianceDto = toml::from_str(ALLIANCE_TOML)?;
    Ok(AllianceRules {
        max_members: dto.max_members,
        join_embassy_level: dto.join_embassy_level,
        found_embassy_level: dto.found_embassy_level,
    })
}

#[derive(Deserialize)]
struct RankingDto {
    /// Rolling leaderboard windows, in days (besides the implicit all-time window).
    windows_days: Vec<i64>,
    /// Maximum rows any leaderboard returns (P11).
    leaderboard_page_size: usize,
}

/// Load the ranking & statistics rules (016) from the embedded balance data: the leaderboard windows
/// and page bound (`ranking.toml`) combined with each unit's kill **point value** (from `units.toml`,
/// defaulting to crop upkeep — GDD §11.2).
///
/// # Errors
/// Returns [`BalanceError`] if either dataset cannot be parsed.
pub fn ranking_rules() -> Result<RankingRules, BalanceError> {
    let dto: RankingDto = toml::from_str(RANKING_TOML)?;
    let units = unit_rules()?;
    // Point values keyed by unit id (shared ids across tribes carry the same value, so the map
    // collapses them harmlessly — combat losses are keyed by id too).
    let mut point_value = HashMap::new();
    for tribe in [Tribe::Romans, Tribe::Teutons, Tribe::Gauls] {
        for spec in units.roster(tribe) {
            point_value.insert(spec.id.clone(), spec.point_value);
        }
    }
    Ok(RankingRules {
        point_value,
        windows_secs: dto.windows_days.iter().map(|d| d * 86_400).collect(),
        page_size: dto.leaderboard_page_size,
    })
}

#[derive(Deserialize)]
struct MedalsDto {
    period_secs: i64,
    medals_per_category: usize,
    categories: Vec<String>,
}

/// Load the weekly medal-settlement rules (017) from the embedded balance data.
///
/// # Errors
/// Returns [`BalanceError`] if the data cannot be parsed or names an unknown medal category.
pub fn medal_rules() -> Result<MedalRules, BalanceError> {
    let dto: MedalsDto = toml::from_str(MEDALS_TOML)?;
    let categories = dto
        .categories
        .iter()
        .map(|c| {
            MedalCategory::parse(c).ok_or_else(|| BalanceError::UnknownMedalCategory(c.clone()))
        })
        .collect::<Result<_, _>>()?;
    Ok(MedalRules {
        period_secs: dto.period_secs,
        per_category: dto.medals_per_category,
        categories,
    })
}

#[derive(Deserialize)]
struct LifecycleDto {
    protection: ProtectionDto,
    inactivity: InactivityDto,
}

#[derive(Deserialize)]
struct ProtectionDto {
    beginner_protection_secs: i64,
    population_threshold: i64,
}

#[derive(Deserialize)]
struct InactivityDto {
    inactive_after_secs: i64,
    abandon_after_secs: i64,
    sweep_interval_secs: i64,
}

/// Load the account-lifecycle rules (019, P7) from `lifecycle.toml`.
pub fn lifecycle_rules() -> Result<LifecycleRules, BalanceError> {
    let dto: LifecycleDto = toml::from_str(LIFECYCLE_TOML)?;
    Ok(LifecycleRules {
        beginner_protection_secs: dto.protection.beginner_protection_secs,
        protection_population_threshold: dto.protection.population_threshold,
        inactive_after_secs: dto.inactivity.inactive_after_secs,
        abandon_after_secs: dto.inactivity.abandon_after_secs,
        sweep_interval_secs: dto.inactivity.sweep_interval_secs,
    })
}

#[derive(Deserialize)]
struct FairPlayDto {
    rate_limit: RateLimitDto,
    sanctions: SanctionsDto,
    detection: DetectionDto,
}

#[derive(Deserialize)]
struct RateLimitDto {
    actions_per_window: u32,
    window_secs: i64,
    logins_per_window: u32,
}

#[derive(Deserialize)]
struct SanctionsDto {
    suspend_default_secs: i64,
}

#[derive(Deserialize)]
struct DetectionDto {
    ip_association_threshold: u32,
    inhuman_rate_threshold: u32,
}

/// Load the fair-play / anti-cheat rules (022, P7) from `fairplay.toml`.
///
/// # Errors
/// Returns [`BalanceError`] if the data cannot be parsed.
pub fn fair_play_rules() -> Result<FairPlayRules, BalanceError> {
    let dto: FairPlayDto = toml::from_str(FAIRPLAY_TOML)?;
    Ok(FairPlayRules {
        rate_limit_per_window: dto.rate_limit.actions_per_window,
        rate_window_secs: dto.rate_limit.window_secs,
        login_limit_per_window: dto.rate_limit.logins_per_window,
        suspend_default_secs: dto.sanctions.suspend_default_secs,
        ip_association_threshold: dto.detection.ip_association_threshold,
        inhuman_rate_threshold: dto.detection.inhuman_rate_threshold,
    })
}

/// The released artifact set plus the Treasury-level requirements and the Natar garrison spec (020).
#[derive(Debug, Clone, PartialEq)]
pub struct ArtifactCatalogue {
    /// The artifacts released at the artifact-release date.
    pub artifacts: Vec<ArtifactDef>,
    /// Treasury level required to hold a small/large/unique artifact.
    pub treasury_small: u8,
    pub treasury_large: u8,
    pub treasury_unique: u8,
    /// The seeded Natar defensive garrison: `base_count + per_index * villageIndex` of `unit`.
    pub garrison_unit: String,
    pub garrison_base_count: i64,
    pub garrison_per_index: i64,
}

#[derive(Deserialize)]
struct ArtifactsDto {
    treasury: TreasuryDto,
    garrison: GarrisonDto,
    artifacts: Vec<ArtifactDto>,
}

#[derive(Deserialize)]
struct TreasuryDto {
    small: u8,
    large: u8,
    unique: u8,
}

#[derive(Deserialize)]
struct GarrisonDto {
    unit: String,
    base_count: i64,
    per_index: i64,
}

#[derive(Deserialize)]
struct ArtifactDto {
    id: String,
    kind: String,
    scope: String,
    magnitude: f64,
}

fn parse_artifact_kind(s: &str) -> Result<ArtifactKind, BalanceError> {
    match s {
        "speed" => Ok(ArtifactKind::Speed),
        "storage" => Ok(ArtifactKind::Storage),
        "sustenance" => Ok(ArtifactKind::Sustenance),
        "trainer" => Ok(ArtifactKind::Trainer),
        "architect" => Ok(ArtifactKind::Architect),
        "eyes" => Ok(ArtifactKind::Eyes),
        "confuser" => Ok(ArtifactKind::Confuser),
        "fool" => Ok(ArtifactKind::Fool),
        other => Err(BalanceError::UnknownArtifactKind(other.to_owned())),
    }
}

fn parse_artifact_scope(s: &str) -> Result<ArtifactScope, BalanceError> {
    match s {
        "small" => Ok(ArtifactScope::Small),
        "large" => Ok(ArtifactScope::Large),
        "unique" => Ok(ArtifactScope::Unique),
        other => Err(BalanceError::UnknownArtifactScope(other.to_owned())),
    }
}

/// Load the artifact catalogue (020, P7) from `artifacts.toml`, fail-fast on unknown kind/scope or a
/// duplicate id.
pub fn artifact_catalogue() -> Result<ArtifactCatalogue, BalanceError> {
    let dto: ArtifactsDto = toml::from_str(ARTIFACTS_TOML)?;
    let mut seen = std::collections::HashSet::with_capacity(dto.artifacts.len());
    let artifacts = dto
        .artifacts
        .iter()
        .map(|a| {
            if !seen.insert(a.id.as_str()) {
                return Err(BalanceError::UnknownArtifactKind(format!(
                    "duplicate artifact id: {}",
                    a.id
                )));
            }
            Ok(ArtifactDef {
                id: ArtifactId(a.id.clone()),
                kind: parse_artifact_kind(&a.kind)?,
                scope: parse_artifact_scope(&a.scope)?,
                magnitude: a.magnitude,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ArtifactCatalogue {
        artifacts,
        treasury_small: dto.treasury.small,
        treasury_large: dto.treasury.large,
        treasury_unique: dto.treasury.unique,
        garrison_unit: dto.garrison.unit,
        garrison_base_count: dto.garrison.base_count,
        garrison_per_index: dto.garrison.per_index,
    })
}

#[derive(Deserialize)]
struct WonderDto {
    wonder: WonderInnerDto,
}

#[derive(Deserialize)]
struct WonderInnerDto {
    base_cost: AmountsDto,
    cost_ratio: f64,
    base_time_secs: i64,
    time_ratio: f64,
    plan_count: u32,
    site_count: u32,
    garrison: GarrisonDto,
}

/// Load the Wonder-of-the-World rules (021, P7) from `wonder.toml`.
pub fn wonder_rules() -> Result<WonderRules, BalanceError> {
    let dto: WonderDto = toml::from_str(WONDER_TOML)?;
    let w = dto.wonder;
    Ok(WonderRules {
        base_cost: ResourceAmounts {
            wood: w.base_cost.wood,
            clay: w.base_cost.clay,
            iron: w.base_cost.iron,
            crop: w.base_cost.crop,
        },
        cost_ratio: w.cost_ratio,
        base_time_secs: w.base_time_secs,
        time_ratio: w.time_ratio,
        plan_count: w.plan_count,
        site_count: w.site_count,
        garrison_unit: w.garrison.unit,
        garrison_base_count: w.garrison.base_count,
        garrison_per_index: w.garrison.per_index,
    })
}

#[derive(Deserialize)]
struct AchievementsDto {
    achievements: Vec<AchievementDto>,
}

#[derive(Deserialize)]
struct AchievementDto {
    id: String,
    kind: String,
    #[serde(default)]
    threshold: i64,
    #[serde(default)]
    reward: RewardDto,
}

#[derive(Deserialize, Default)]
struct RewardDto {
    culture: Option<i64>,
    wood: Option<i64>,
    clay: Option<i64>,
    iron: Option<i64>,
    crop: Option<i64>,
}

fn parse_achievement_kind(s: &str) -> Result<AchievementKind, BalanceError> {
    Ok(match s {
        "second_village" => AchievementKind::SecondVillage,
        "defensive_wins" => AchievementKind::DefensiveWins,
        "first_oasis" => AchievementKind::FirstOasis,
        "population" => AchievementKind::Population,
        "research_all_units" => AchievementKind::ResearchAllUnits,
        other => return Err(BalanceError::UnknownAchievementKind(other.to_owned())),
    })
}

fn parse_reward(dto: &RewardDto) -> Reward {
    if let Some(cp) = dto.culture {
        Reward::Culture(cp)
    } else if dto.wood.is_some() || dto.clay.is_some() || dto.iron.is_some() || dto.crop.is_some() {
        Reward::Resources(ResourceAmounts {
            wood: dto.wood.unwrap_or(0),
            clay: dto.clay.unwrap_or(0),
            iron: dto.iron.unwrap_or(0),
            crop: dto.crop.unwrap_or(0),
        })
    } else {
        Reward::None
    }
}

/// Load the achievement catalogue (017) from the embedded balance data.
///
/// # Errors
/// Returns [`BalanceError`] if the data cannot be parsed or names an unknown achievement kind.
pub fn achievement_catalogue() -> Result<Vec<AchievementDef>, BalanceError> {
    let dto: AchievementsDto = toml::from_str(ACHIEVEMENTS_TOML)?;
    dto.achievements
        .iter()
        .map(|a| {
            Ok(AchievementDef {
                id: AchievementId(a.id.clone()),
                kind: parse_achievement_kind(&a.kind)?,
                threshold: a.threshold,
                reward: parse_reward(&a.reward),
            })
        })
        .collect()
}

#[derive(Deserialize)]
struct QuestsDto {
    quests: Vec<QuestDto>,
}

#[derive(Deserialize)]
struct QuestDto {
    id: String,
    description: String,
    condition: QuestConditionDto,
    #[serde(default)]
    reward: QuestRewardDto,
}

#[derive(Deserialize)]
struct QuestConditionDto {
    kind: String,
    #[serde(default)]
    level: u8,
    building: Option<String>,
    #[serde(default)]
    population: i64,
}

#[derive(Deserialize, Default)]
struct QuestRewardDto {
    #[serde(default)]
    wood: i64,
    #[serde(default)]
    clay: i64,
    #[serde(default)]
    iron: i64,
    #[serde(default)]
    crop: i64,
    #[serde(default)]
    culture: i64,
    troop_unit: Option<String>,
    troop_count: Option<u32>,
}

fn parse_quest_condition(dto: &QuestConditionDto) -> Result<QuestCondition, BalanceError> {
    Ok(match dto.kind.as_str() {
        "field_level" => QuestCondition::FieldLevel(dto.level),
        "building_level" => {
            let building = dto.building.as_deref().ok_or_else(|| {
                BalanceError::UnknownQuestCondition("building_level: no building".into())
            })?;
            QuestCondition::BuildingLevel(parse_building(building)?, dto.level)
        }
        "train_troops" => QuestCondition::TrainTroops,
        "send_raid" => QuestCondition::SendRaid,
        "population" => QuestCondition::Population(dto.population),
        other => return Err(BalanceError::UnknownQuestCondition(other.to_owned())),
    })
}

fn quest_reward(dto: &QuestRewardDto) -> QuestReward {
    QuestReward {
        resources: ResourceAmounts {
            wood: dto.wood,
            clay: dto.clay,
            iron: dto.iron,
            crop: dto.crop,
        },
        culture: dto.culture,
        troops: match (&dto.troop_unit, dto.troop_count) {
            (Some(unit), Some(count)) => Some((UnitId(unit.clone()), count)),
            _ => None,
        },
    }
}

/// Load the ordered onboarding quest chain (018) from the embedded balance data.
///
/// # Errors
/// Returns [`BalanceError`] if the data cannot be parsed or names an unknown condition/building.
pub fn quest_chain() -> Result<Vec<QuestDef>, BalanceError> {
    let dto: QuestsDto = toml::from_str(QUESTS_TOML)?;
    let mut seen = std::collections::HashSet::with_capacity(dto.quests.len());
    dto.quests
        .iter()
        .map(|q| {
            if !seen.insert(q.id.as_str()) {
                return Err(BalanceError::DuplicateQuestId(q.id.clone()));
            }
            Ok(QuestDef {
                id: QuestId(q.id.clone()),
                description: q.description.clone(),
                condition: parse_quest_condition(&q.condition)?,
                reward: quest_reward(&q.reward),
            })
        })
        .collect()
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
    regrow_secs: i64,
    regrow_per_step: u32,
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
    /// Ranking kill value (016, P7). Absent ⇒ defaults to `crop_upkeep` (faithful population value).
    #[serde(default)]
    point_value: Option<i64>,
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
        point_value: dto.point_value.unwrap_or(i64::from(dto.crop_upkeep)),
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
        point_value: 0,
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
        regrow_secs: dto.oasis_garrison.regrow_secs,
        regrow_per_step: dto.oasis_garrison.regrow_per_step,
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
            eperica_domain::OasisBonus::default(),
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
        assert_eq!(r.max_level(field), 10); // the normal field cap
        assert!(r.cost(field, 0).is_some());
        // 013 AC10: the field cost table runs to the capital cap; a capital may build past 10.
        assert_eq!(r.field_max_level(false), 10);
        assert_eq!(r.field_max_level(true), 20);
        assert!(r.field_max_level(true) > r.field_max_level(false));
        assert!(
            r.cost(field, 10).is_some(),
            "capital level-11 field has a cost"
        );
        assert!(
            r.cost(field, 20).is_none(),
            "the table ends at the capital cap"
        );
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
    fn loads_embassy_building() {
        // 015 AC1: the Embassy is an ordinary infrastructure building — Main Building L1 prereq, a
        // 10-level cost/time table, and no exclusivity. It loads into the build catalog like any other.
        let r = build_rules().expect("build rules");
        let embassy = BuildTarget::Building {
            slot: 0,
            kind: BuildingKind::Embassy,
        };
        assert_eq!(
            r.prerequisites(BuildingKind::Embassy),
            &[(BuildingKind::MainBuilding, 1)]
        );
        assert_eq!(r.max_level(embassy), 10, "Embassy level cap");
        assert!(r.cost(embassy, 0).is_some(), "level-1 Embassy has a cost");
        assert!(
            r.cost(embassy, 9).is_some() && r.cost(embassy, 10).is_none(),
            "the Embassy cost table ends at the cap"
        );
        // Costs rise with level (the table is monotonic in wood).
        assert!(r.cost(embassy, 1).unwrap().wood > r.cost(embassy, 0).unwrap().wood);
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
    fn loads_outpost_building_and_capacity() {
        // 012 AC6: the Outpost is a constructable building with spec prerequisites, and its capacity
        // table rises with level (level 0 holds none; higher levels hold more).
        let r = build_rules().expect("build rules");
        let outpost = BuildTarget::Building {
            slot: 0,
            kind: BuildingKind::Outpost,
        };
        assert_eq!(r.max_level(outpost), 10);
        assert!(r.cost(outpost, 0).is_some());
        assert_eq!(
            r.prerequisites(BuildingKind::Outpost),
            &[
                (BuildingKind::MainBuilding, 3),
                (BuildingKind::RallyPoint, 1)
            ]
        );
        let economy = economy_rules().expect("economy rules");
        assert_eq!(economy.outpost_capacity(0), 0, "no Outpost holds no oasis");
        assert!(economy.outpost_capacity(1) >= 1);
        assert!(economy.outpost_capacity(10) > economy.outpost_capacity(1));
    }

    #[test]
    fn loads_culture_rules_and_town_hall() {
        // 013 AC1/AC2: culture balance loads; the Town Hall is a constructable building whose CP rate
        // rises with level; the CP gate + slots are well-formed.
        use eperica_domain::{allowed_villages, cp_allows, culture_rate};
        let r = culture_rules().expect("culture rules");
        assert!(r.base_cp_per_village > 0);
        assert_eq!(r.town_hall_cp(0), 0, "no Town Hall adds nothing");
        assert!(r.town_hall_cp(10) > r.town_hall_cp(1));
        assert_eq!(r.cp_thresholds[1], 0, "the first village is free");
        // A Town Hall raises the player's rate.
        assert!(culture_rate(&[3], &r) > culture_rate(&[0], &r));
        // The CP gate and the combined gate behave.
        assert_eq!(cp_allows(0, &r), 1);
        assert!(cp_allows(1_000_000, &r) >= 2);
        assert_eq!(
            allowed_villages(1_000_000, &[], &r),
            1,
            "no Residence ⇒ home only"
        );

        // Town Hall / Residence / Palace are buildable (constructable + have population data).
        let build = build_rules().expect("build rules");
        let economy = economy_rules().expect("economy rules");
        for kind in [
            BuildingKind::TownHall,
            BuildingKind::Residence,
            BuildingKind::Palace,
        ] {
            let target = BuildTarget::Building { slot: 0, kind };
            assert_eq!(build.max_level(target), 10, "{kind:?}");
            assert!(build.cost(target, 0).is_some(), "{kind:?}");
            assert!(
                economy.building_population_per_level.contains_key(&kind),
                "{kind:?} population"
            );
        }
        // 013 AC3: a Residence/Palace grants expansion slots, rising with level.
        assert_eq!(r.slots_at(0), 0);
        assert!(r.slots_at(10) > r.slots_at(1));
    }

    #[test]
    fn loads_alliance_rules() {
        // 015 AC1/AC2/AC3/AC4: alliance balance loads and is well-formed — a positive cap, and the
        // join gate no higher than the found gate (join ≤ found, both > 0 in faithful play).
        let r = alliance_rules().expect("alliance rules");
        assert!(r.max_members > 0);
        assert!(r.join_embassy_level >= 1);
        assert!(r.found_embassy_level >= r.join_embassy_level);
        // The gates behave: L0 ⇒ neither, the join level ⇒ join only, the found level ⇒ both.
        assert!(!r.can_join(0) && !r.can_found(0));
        assert!(r.can_join(r.join_embassy_level) && !r.can_found(r.join_embassy_level - 1));
        assert!(r.can_found(r.found_embassy_level));
        assert!(r.at_cap(r.max_members) && !r.at_cap(r.max_members - 1));
    }

    #[test]
    fn loads_loyalty_rules() {
        // 014 AC1/AC3: loyalty balance loads and is well-formed — a fresh village starts at the max,
        // a conquered one resets lower, loyalty regenerates, and the administrator drop range is sane.
        use eperica_domain::{MAX_LOYALTY, regenerate_loyalty};
        let r = loyalty_rules().expect("loyalty rules");
        assert_eq!(r.starting_loyalty, MAX_LOYALTY);
        assert!(r.post_conquest_loyalty < r.starting_loyalty);
        assert!(r.regen_per_hour > 0);
        assert!(r.drop_min > 0 && r.drop_min <= r.drop_max);
        // Regeneration accrues toward the maximum and clamps there.
        let speed = eperica_domain::GameSpeed::new(1.0).unwrap();
        assert!(regenerate_loyalty(50, 3600, &r, speed) > 50);
        assert_eq!(regenerate_loyalty(100, 36_000, &r, speed), MAX_LOYALTY);

        // 014 AC2: each administrator id names a real roster unit that **fights** (Expansion role,
        // attack > 0) and trains in a Residence/Palace — one per tribe.
        assert!(!r.administrator_ids.is_empty());
        let units = unit_rules().expect("unit rules");
        for tribe in [Tribe::Romans, Tribe::Teutons, Tribe::Gauls] {
            let admins: Vec<_> = units
                .roster(tribe)
                .iter()
                .filter(|s| r.is_administrator(&s.id))
                .collect();
            assert_eq!(admins.len(), 1, "exactly one administrator for {tribe:?}");
            let a = admins[0];
            assert_eq!(a.role, UnitRole::Expansion);
            assert!(a.attack > 0, "the administrator fights ({})", a.id.as_str());
            assert_eq!(a.trained_in, BuildingKind::Residence);
        }
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
    fn loads_ranking_rules_with_point_values_defaulting_to_upkeep() {
        // 016: windows + page bound load, and each unit's kill point value defaults to its crop
        // upkeep (the faithful population value) when no explicit `point_value` is given.
        let r = ranking_rules().expect("ranking rules load");
        assert_eq!(r.windows_secs, vec![7 * 86_400, 30 * 86_400]);
        assert_eq!(r.page_size, 100);
        let units = unit_rules().expect("unit rules");
        for tribe in [Tribe::Romans, Tribe::Teutons, Tribe::Gauls] {
            for spec in units.roster(tribe) {
                assert_eq!(
                    r.unit_value(&spec.id),
                    i64::from(spec.crop_upkeep),
                    "{:?}/{}",
                    tribe,
                    spec.id.0
                );
            }
        }
        // An unknown unit (e.g. a wild animal) carries no point value.
        assert_eq!(r.unit_value(&UnitId("elephant".into())), 0);
    }

    #[test]
    fn loads_medal_and_achievement_balance() {
        // 017: the weekly-settlement rules + achievement catalogue load fail-fast with the seed data.
        let m = medal_rules().expect("medal rules load");
        assert_eq!(m.period_secs, 604_800);
        assert_eq!(m.per_category, 3);
        assert!(m.categories.contains(&MedalCategory::Climber));
        assert_eq!(m.categories.len(), 7);

        let cat = achievement_catalogue().expect("achievement catalogue load");
        assert_eq!(cat.len(), 5);
        let pop = cat
            .iter()
            .find(|a| a.kind == AchievementKind::Population)
            .expect("population achievement");
        assert_eq!(pop.threshold, 1000);
        assert_eq!(
            pop.reward,
            Reward::Resources(ResourceAmounts {
                wood: 500,
                clay: 500,
                iron: 500,
                crop: 500,
            })
        );
        let second = cat
            .iter()
            .find(|a| a.kind == AchievementKind::SecondVillage)
            .expect("second-village achievement");
        assert_eq!(second.reward, Reward::Culture(50));
    }

    #[test]
    fn loads_quest_chain() {
        // 018: the ordered onboarding chain loads fail-fast with the seed data.
        let chain = quest_chain().expect("quest chain load");
        assert_eq!(chain.first().expect("first quest").id.0, "upgrade_field");
        assert!(chain.len() >= 5);
        // The warehouse quest carries a building-level condition.
        let wh = chain
            .iter()
            .find(|q| q.id.0 == "build_warehouse")
            .expect("warehouse quest");
        assert_eq!(
            wh.condition,
            eperica_domain::QuestCondition::BuildingLevel(BuildingKind::Warehouse, 1)
        );
        assert!(wh.reward.resources.wood > 0);
    }

    #[test]
    fn loads_artifact_catalogue() {
        // 020: the artifact set loads fail-fast with valid kinds/scopes and unique ids.
        let cat = artifact_catalogue().expect("artifact catalogue load");
        assert!(cat.artifacts.len() >= 8, "at least one of every kind");
        assert!(
            cat.treasury_small < cat.treasury_unique,
            "unique needs a higher Treasury"
        );
        assert!(cat.garrison_base_count > 0);
        // Every scope is represented.
        use eperica_domain::ArtifactScope::*;
        for scope in [Small, Large, Unique] {
            assert!(
                cat.artifacts.iter().any(|a| a.scope == scope),
                "scope {scope:?} present"
            );
        }
    }

    #[test]
    fn loads_wonder_rules_and_curve() {
        // 021: the Wonder rules load and generate a 100-level construction curve in BuildRules.
        let w = wonder_rules().expect("wonder rules load");
        assert!(w.cost_ratio > 1.0 && w.time_ratio > 1.0);
        assert!(w.plan_count > 0 && w.site_count > 0);
        let rules = build_rules().expect("build rules");
        let spec = rules
            .buildings
            .get(&eperica_domain::BuildingKind::Wonder)
            .expect("wonder in build rules");
        assert_eq!(spec.max_level(), eperica_domain::MAX_WONDER_LEVEL);
    }

    #[test]
    fn loads_lifecycle_rules() {
        // 019: the protection + inactivity timings load fail-fast and are positive.
        let r = lifecycle_rules().expect("lifecycle rules load");
        assert!(r.beginner_protection_secs > 0);
        assert!(r.protection_population_threshold > 0);
        assert!(r.inactive_after_secs > 0);
        assert!(
            r.abandon_after_secs > r.inactive_after_secs,
            "abandon is later than inactive"
        );
        assert!(r.sweep_interval_secs > 0);
    }

    #[test]
    fn loads_fair_play_rules() {
        // 022: the rate limits, suspension default, and detection thresholds load and are positive.
        let r = fair_play_rules().expect("fair-play rules load");
        assert!(r.rate_limit_per_window > 0);
        assert!(r.rate_window_secs > 0);
        assert!(r.login_limit_per_window > 0);
        assert!(r.suspend_default_secs > 0);
        assert!(r.ip_association_threshold > 0);
        assert!(r.inhuman_rate_threshold > 0);
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
