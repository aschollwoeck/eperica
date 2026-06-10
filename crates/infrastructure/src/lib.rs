//! Eperica infrastructure layer — adapters implementing the application's ports.
//!
//! Depends on [`eperica_application`] (and transitively the domain). Concrete I/O lives here:
//! configuration loading, the database pool, migrations, and (in later tasks) repositories,
//! sessions, and the event store.
#![forbid(unsafe_code)]

pub mod balance;
pub mod config;
pub mod db;
pub mod event_store;
pub mod repo;
pub mod security;
pub mod world;

/// Re-export of the application crate, whose ports this layer implements.
pub use eperica_application as application;

pub use balance::{BalanceError, economy_rules, starting_village};
pub use config::{AppConfig, ConfigError};
pub use db::{MIGRATOR, create_pool, run_migrations};
pub use event_store::{PgEventStore, Scheduler, now};
pub use repo::PgAccountRepository;
pub use security::Argon2Hasher;
pub use world::ensure_world;
