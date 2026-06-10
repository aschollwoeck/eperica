//! Eperica infrastructure layer — adapters implementing the application's ports.
//!
//! Depends on [`eperica_application`] (and transitively the domain). Concrete I/O lives here:
//! configuration loading, the database pool, migrations, and (in later tasks) repositories,
//! sessions, and the event store.
#![forbid(unsafe_code)]

pub mod config;
pub mod db;

/// Re-export of the application crate, whose ports this layer implements.
pub use eperica_application as application;

pub use config::{AppConfig, ConfigError};
pub use db::{MIGRATOR, create_pool, run_migrations};
