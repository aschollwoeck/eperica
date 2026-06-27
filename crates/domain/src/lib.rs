//! Eperica domain layer — the pure game core.
//!
//! Holds entities, value objects, and game rules. Per the project constitution (**P3**) this crate
//! has **no I/O, framework, or database dependencies** and is unit-testable in isolation. Game
//! modules grow here slice by slice.
#![forbid(unsafe_code)]

pub mod achievements;
pub mod alliance;
pub mod artifact;
pub mod building;
pub mod combat;
pub mod comms;
pub mod construction;
pub mod culture;
pub mod economy;
pub mod error;
pub mod event;
pub mod fairplay;
pub mod forum;
pub mod lifecycle;
pub mod loyalty;
pub mod map;
pub mod medals;
pub mod movement;
pub mod notification;
pub mod oasis;
pub mod presence;
pub mod quest;
pub mod ranking;
pub mod resource;
pub mod scouting;
pub mod search;
pub mod sitter;
pub mod trade;
pub mod units;
pub mod village;
pub mod wonder;
pub mod world;

pub use achievements::{
    AchievementDef, AchievementId, AchievementKind, PlayerProgress, Reward, met, newly_earned,
};
pub use alliance::{
    AllianceId, AllianceRight, AllianceRole, AllianceRules, DiplomacyAction, DiplomacyError,
    DiplomacyStance, DiplomacyState, DiplomacyStatus, RightSet, can_expel, has_right, next_stance,
};
pub use artifact::{
    ArtifactDef, ArtifactEffects, ArtifactId, ArtifactKind, ArtifactScope, aggregate_effects,
    can_capture, fool_resolved, required_treasury_level,
};
pub use building::BuildingKind;
pub use combat::{
    AttackMode, AttackPower, BattleInput, BattleOutcome, CombatRules, WallProfile, add_defense,
    apply_losses, attack_power, carry_capacity_total, catapult_power, cranny_protection,
    loot_split, luck_factor, razed_levels, resolve_battle,
};
pub use comms::{ChatChannel, MAX_MESSAGE, can_access_channel, valid_body};
pub use construction::{
    BuildRules, BuildTarget, LevelSpec, QueueLane, build_time_secs, building_levels_met,
    can_afford, debit, prerequisites_met, queue_lane,
};
pub use culture::{
    CultureRules, allowed_villages, cp_allows, culture_rate, expansion_slots, settle_value,
};
pub use economy::{
    Capacities, Economy, EconomyRules, ProductionRates, ResourceAmounts, accrue, capacities,
    compute_economy, net_crop_base, population, production_rates,
};
pub use error::DomainError;
pub use event::{EventKind, ScheduledEvent, Timestamp};
pub use fairplay::{
    FairPlayRules, ReportReason, SanctionKind, account_blocked, inhuman_action_rate,
    shared_ip_flagged,
};
pub use forum::{MAX_THREAD_TITLE, valid_thread_title};
pub use lifecycle::{
    LifecycleRules, abandon_cutoff, is_inactive, is_protected, protection_ended_by_population,
    protection_expiry,
};
pub use loyalty::{
    ConquestOutcome, LoyaltyRules, MAX_LOYALTY, administrator_count, administrator_drop,
    conquest_outcome, regenerate_loyalty,
};
pub use map::{FieldDistribution, MapRules, OasisBonus, TileKind, Weighted, WorldMap};
pub use medals::{MedalCategory, MedalRules, period_index, period_start, rank_top};
pub use movement::{MovementKind, slowest_speed, travel_time_secs, travel_time_secs_floored};
pub use notification::NotificationKind;
pub use oasis::{OasisRules, oasis_garrison, regrow_step};
pub use presence::{MAX_BIO, Presence, presence, valid_bio};
pub use quest::{
    QuestCondition, QuestDef, QuestId, QuestProgress, QuestReward, current_quest, newly_completed,
    quest_met,
};
pub use ranking::{RankingRules, apportion};
pub use resource::ResourceKind;
pub use scouting::{ScoutOutcome, ScoutRules, ScoutTarget, resolve_scouting, scouting_power};
pub use search::parse_coordinate;
pub use sitter::{MAX_SITTERS, can_grant_sitter};
pub use trade::{
    MerchantProfile, MerchantRules, TradeKind, bundle_is_empty, bundle_total, deposit_capped,
    merchants_required,
};
pub use units::{
    MAX_TRAINING_BATCH, MAX_UNIT_LEVEL, ROSTER_SIZE, ResearchDenied, ResearchSpec, SiegeKind,
    SmithyRules, TrainDenied, TrainingRules, UnitCounts, UnitId, UnitRole, UnitRules, UnitSpec,
    UpgradeDenied, batch_cost, can_research, can_train, can_upgrade, depletion_secs,
    garrison_upkeep, per_unit_time_secs, scaled_time_secs, starve, training_building_level,
};
pub use village::{
    BuildingSlot, PlayerId, RESOURCE_FIELD_COUNT, ResourceField, StartingVillage, Tribe, Village,
    VillageId,
};
pub use wonder::{MAX_WONDER_LEVEL, WonderRules, wonder_complete, wonder_level_spec};
pub use world::{
    Coordinate, GameSpeed, Quadrant, WorldConfig, WorldId, coordinates_within, quadrant,
    toroidal_distance,
};
