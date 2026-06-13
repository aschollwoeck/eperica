//! Eperica application layer — use-cases (commands) and the ports (traits) they depend on.
//!
//! Depends only on [`eperica_domain`]; the infrastructure layer implements the [`ports`]. Use-cases
//! are written against the ports so they can be tested with fakes, with no I/O.
#![forbid(unsafe_code)]

pub mod auth;
pub mod build;
pub mod combat;
pub mod culture;
pub mod economy;
pub mod map;
pub mod movement;
pub mod oasis;
pub mod ports;
pub mod register;
pub mod scheduler;
pub mod scouting;
pub mod settling;
pub mod starvation;
pub mod trade;
pub mod units;

#[cfg(test)]
mod tests;

/// Re-export of the domain crate, the layer this one builds upon.
pub use eperica_domain as domain;

pub use auth::{LoginError, authenticate};
pub use build::{BuildError, order_build, process_due_builds};
pub use combat::{CombatError, order_attack, process_due_combat};
pub use culture::{CultureView, load_culture, reanchor_culture};
pub use economy::{VillageEconomy, load_economy, pick_village, select_village, settle_amounts};
pub use map::{MapCell, Viewport, map_viewport, viewport_coords};
pub use movement::{MovementError, order_reinforcement, order_return, process_due_movements};
pub use oasis::{
    OasisError, order_oasis_attack, order_oasis_recall, order_oasis_reinforce,
    process_due_oasis_combat, process_due_oasis_regrow, process_due_oasis_reinforce,
};
pub use ports::{
    AccountRepository, ActiveBuild, ActiveTraining, ActiveUnitOrder, BattleApply, BattleReportView,
    BuildRepository, CombatRepository, ConquestRepository, CultureRepository, DueAttack, DueBuild,
    DueEvent, DueMovement, DueOasisAttack, DueOasisRegrow, DueOasisReinforce, DueScout, DueSettle,
    DueTrade, DueTraining, DueUnitOrder, EventStore, MovementRepository, MovementView,
    NewBattleReport, NewBuildOrder, NewOasisReport, NewScoutReport, NewTrainingOrder, NewUnitOrder,
    NewUser, OasisBattleApply, OasisOwnership, OasisReinforceOutcome, OasisRepository, OasisState,
    PasswordHasher, RazedBuilding, RepoError, ResourceWrite, ScoutApply, ScoutIntel,
    ScoutReportView, ScoutRepository, SettleApply, SettleOutcome, SettleRepository,
    StarvationRepository, StationedGroup, TradeRepository, TradeView, TrainingRepository,
    UnitOrderKind, UnitRepository, UserRecord, VillageMarker,
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
