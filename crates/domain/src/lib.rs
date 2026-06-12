//! Eperica domain layer — the pure game core.
//!
//! Holds entities, value objects, and game rules. Per the project constitution (**P3**) this crate
//! has **no I/O, framework, or database dependencies** and is unit-testable in isolation. Game
//! modules grow here slice by slice.
#![forbid(unsafe_code)]

pub mod building;
pub mod combat;
pub mod construction;
pub mod economy;
pub mod error;
pub mod event;
pub mod map;
pub mod movement;
pub mod oasis;
pub mod resource;
pub mod scouting;
pub mod trade;
pub mod units;
pub mod village;
pub mod world;

pub use building::BuildingKind;
pub use combat::{
    AttackMode, AttackPower, BattleInput, BattleOutcome, CombatRules, WallProfile, add_defense,
    apply_losses, attack_power, carry_capacity_total, catapult_power, cranny_protection,
    loot_split, luck_factor, razed_levels, resolve_battle,
};
pub use construction::{
    BuildRules, BuildTarget, LevelSpec, QueueLane, build_time_secs, building_levels_met,
    can_afford, debit, prerequisites_met, queue_lane,
};
pub use economy::{
    Capacities, Economy, EconomyRules, ProductionRates, ResourceAmounts, accrue, capacities,
    compute_economy, net_crop_base, population, production_rates,
};
pub use error::DomainError;
pub use event::{EventKind, ScheduledEvent, Timestamp};
pub use map::{FieldDistribution, MapRules, OasisBonus, TileKind, Weighted, WorldMap};
pub use movement::{MovementKind, slowest_speed, travel_time_secs, travel_time_secs_floored};
pub use oasis::{OasisRules, oasis_garrison};
pub use resource::ResourceKind;
pub use scouting::{ScoutOutcome, ScoutRules, ScoutTarget, resolve_scouting, scouting_power};
pub use trade::{
    MerchantProfile, MerchantRules, TradeKind, bundle_is_empty, bundle_total, deposit_capped,
    merchants_required,
};
pub use units::{
    MAX_TRAINING_BATCH, MAX_UNIT_LEVEL, ROSTER_SIZE, ResearchDenied, ResearchSpec, SiegeKind,
    SmithyRules, TrainDenied, TrainingRules, UnitCounts, UnitId, UnitRole, UnitRules, UnitSpec,
    UpgradeDenied, batch_cost, can_research, can_train, can_upgrade, depletion_secs,
    garrison_upkeep, per_unit_time_secs, scaled_time_secs, starve,
};
pub use village::{
    BuildingSlot, PlayerId, RESOURCE_FIELD_COUNT, ResourceField, StartingVillage, Tribe, Village,
    VillageId,
};
pub use world::{
    Coordinate, GameSpeed, WorldConfig, WorldId, coordinates_within, toroidal_distance,
};
