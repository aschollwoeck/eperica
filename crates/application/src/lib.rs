//! Eperica application layer — use-cases (commands) and the ports (traits) they depend on.
//!
//! Depends only on [`eperica_domain`]; the infrastructure layer implements the [`ports`]. Use-cases
//! are written against the ports so they can be tested with fakes, with no I/O.
#![forbid(unsafe_code)]

pub mod auth;
pub mod economy;
pub mod ports;
pub mod register;
pub mod scheduler;

#[cfg(test)]
mod tests;

/// Re-export of the domain crate, the layer this one builds upon.
pub use eperica_domain as domain;

pub use auth::{LoginError, authenticate};
pub use economy::{VillageEconomy, load_economy};
pub use ports::{
    AccountRepository, ActiveBuild, BuildRepository, DueBuild, DueEvent, EventStore, NewBuildOrder,
    NewUser, PasswordHasher, RepoError, UserRecord,
};
pub use register::{RegisterCommand, RegisterError, register};
pub use scheduler::process_due;
