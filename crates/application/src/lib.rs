//! Eperica application layer — use-cases (commands) and the ports (traits) they depend on.
//!
//! Depends only on [`eperica_domain`]; the infrastructure layer implements the [`ports`]. Use-cases
//! are written against the ports so they can be tested with fakes, with no I/O.
#![forbid(unsafe_code)]

pub mod auth;
pub mod build;
pub mod economy;
pub mod map;
pub mod movement;
pub mod ports;
pub mod register;
pub mod scheduler;
pub mod starvation;
pub mod units;

#[cfg(test)]
mod tests;

/// Re-export of the domain crate, the layer this one builds upon.
pub use eperica_domain as domain;

pub use auth::{LoginError, authenticate};
pub use build::{BuildError, order_build, process_due_builds};
pub use economy::{VillageEconomy, load_economy};
pub use map::{MapCell, Viewport, map_viewport, viewport_coords};
pub use movement::{MovementError, order_reinforcement, order_return, process_due_movements};
pub use ports::{
    AccountRepository, ActiveBuild, ActiveTraining, ActiveUnitOrder, BuildRepository, DueBuild,
    DueEvent, DueMovement, DueTrade, DueTraining, DueUnitOrder, EventStore, MovementRepository,
    MovementView, NewBuildOrder, NewTrainingOrder, NewUnitOrder, NewUser, PasswordHasher,
    RepoError, StarvationRepository, StationedGroup, TradeRepository, TradeView,
    TrainingRepository, UnitOrderKind, UnitRepository, UserRecord, VillageMarker,
};
pub use register::{RegisterCommand, RegisterError, register};
pub use scheduler::process_due;
pub use starvation::{process_due_starvation, sync_starvation_check, sync_starvation_checks};
pub use units::{
    ResearchError, TrainError, UpgradeError, order_research, order_smithy_upgrade, order_train,
    process_due_training, process_due_unit_orders,
};
