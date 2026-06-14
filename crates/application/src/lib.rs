//! Eperica application layer — use-cases (commands) and the ports (traits) they depend on.
//!
//! Depends only on [`eperica_domain`]; the infrastructure layer implements the [`ports`]. Use-cases
//! are written against the ports so they can be tested with fakes, with no I/O.
#![forbid(unsafe_code)]

pub mod achievements;
pub mod alliance;
pub mod artifact;
pub mod auth;
pub mod build;
pub mod combat;
pub mod comms;
pub mod culture;
pub mod economy;
pub mod fairplay;
pub mod lifecycle;
pub mod map;
pub mod medals;
pub mod movement;
pub mod oasis;
pub mod ports;
pub mod profile;
pub mod quests;
pub mod ranking;
pub mod register;
pub mod scheduler;
pub mod scouting;
pub mod settling;
pub mod starvation;
pub mod trade;
pub mod units;
pub mod wonder;

#[cfg(test)]
mod tests;

/// Re-export of the domain crate, the layer this one builds upon.
pub use eperica_domain as domain;

pub use achievements::evaluate_achievements;
pub use alliance::{
    AllianceError, AllianceOverview, DiplomacyCommand, alliance_view, disband_alliance,
    expel_member, found_alliance, invite_player, leave_alliance, respond_invite, revoke_invite,
    set_diplomacy, set_member_role, transfer_founder,
};
pub use artifact::{ReleaseSpec, process_due_artifact_release};
pub use auth::{LoginError, authenticate};
pub use build::{BuildError, order_build, process_due_builds};
pub use combat::{CombatError, order_attack, process_due_combat};
pub use comms::{
    CommsError, conversation_list, dm_key, dm_pair_key, open_chat, open_dm, parse_dm_key,
    send_chat, send_dm, unread_badge,
};
pub use culture::{CultureView, load_culture, reanchor_culture};
pub use economy::{VillageEconomy, load_economy, pick_village, select_village, settle_amounts};
pub use fairplay::{
    AccountSignals, ModerationError, account_signals, check_rate_limit, file_report,
    resolve_report, review_queue, sanction_account,
};
pub use lifecycle::{end_protection_if_established, process_due_lifecycle};
pub use map::{MapCell, Viewport, map_viewport, viewport_coords};
pub use medals::process_due_medal_settlement;
pub use movement::{MovementError, order_reinforcement, order_return, process_due_movements};
pub use oasis::{
    OasisError, order_oasis_attack, order_oasis_recall, order_oasis_reinforce,
    process_due_oasis_combat, process_due_oasis_regrow, process_due_oasis_reinforce,
};
pub use ports::{
    AccountRepository, AchievementRepository, ActiveBuild, ActiveTraining, ActiveUnitOrder,
    AllianceLeaderboardRow, AllianceRepository, AllianceStats, AlliedVillage, ArtifactCapture,
    ArtifactRepository, BattleApply, BattleReportView, BoardScope, BuildRepository,
    CombatRepository, CommsRepository, ConflictMetric, ConquestRepository, ConquestTransfer,
    ConversationSummary, CultureRepository, DefenderContribution, DefenderReport, DiplomacyEntry,
    DueAttack, DueBuild, DueEvent, DueMovement, DueOasisAttack, DueOasisRegrow, DueOasisReinforce,
    DueScout, DueSettle, DueTrade, DueTraining, DueUnitOrder, EventStore, HeldArtifact,
    IncomingAttack, LeaderboardRow, LifecycleRepository, LoyaltyApply, MedalAward, MedalRepository,
    MedalSubjectKind, MedalView, Membership, MessageView, ModerationRepository, MovementRepository,
    MovementView, NewBattleReport, NewBuildOrder, NewOasisReport, NewScoutReport, NewTrainingOrder,
    NewUnitOrder, NewUser, OasisBattleApply, OasisOwnership, OasisReinforceOutcome,
    OasisRepository, OasisState, OutgoingInvite, PasswordHasher, PendingInvite, PlanCapture,
    PlayerStats, ProfileView, QuestRepository, RankingRepository, RazedBuilding,
    ReinforcementReturn, RepoError, ReportView, ResourceWrite, RosterEntry, ScoutApply, ScoutIntel,
    ScoutReportView, ScoutRepository, SettleApply, SettleOutcome, SettleRepository,
    StarvationRepository, StationedGroup, TradeRepository, TradeView, TrainingRepository,
    UnitOrderKind, UnitRepository, UserRecord, VillageMarker, WonderOutcome, WonderRepository,
    WonderStanding,
};
pub use profile::{ProfileError, edit_bio, view_profile};
pub use quests::evaluate_quests;
pub use ranking::{
    RankingError, Window, alliance_conflict_leaderboard, alliance_population_leaderboard,
    alliance_statistics, climbers_leaderboard, conflict_leaderboard, player_statistics,
    population_history, population_leaderboard, reinforcement_reports,
};
pub use register::{RegisterCommand, RegisterError, register};
pub use scheduler::process_due;
pub use scouting::{ScoutError, gather_intel, order_scout, process_due_scouts};
pub use settling::{SettleError, order_settle, process_due_settles};
pub use starvation::{process_due_starvation, sync_starvation_check, sync_starvation_checks};
pub use trade::{TradeError, order_trade, process_due_trades};
pub use units::{
    ResearchError, TrainError, UpgradeError, order_research, order_smithy_upgrade, order_train,
    process_due_training, process_due_unit_orders,
};
pub use wonder::{
    WonderError, WonderReleaseSpec, order_wonder_build, process_due_wonder_release,
    process_due_wonder_victory,
};
