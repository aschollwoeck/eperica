//! Eperica infrastructure layer — adapters implementing the application's ports.
//!
//! Depends on [`eperica_application`] (and transitively the domain). Concrete I/O lives here:
//! configuration loading, the database pool, migrations, and (in later tasks) repositories,
//! sessions, and the event store.
#![forbid(unsafe_code)]

pub mod balance;
pub mod comms_live;
pub mod config;
pub mod db;
pub mod event_store;
pub mod perf;
pub mod repo;
pub mod security;
pub mod world;
pub mod world_rules;

/// Re-export of the application crate, whose ports this layer implements.
pub use eperica_application as application;

pub use balance::{
    ArtifactCatalogue, BalanceError, achievement_catalogue, alliance_rules, artifact_catalogue,
    build_rules, combat_rules, culture_rules, economy_rules, fair_play_rules, lifecycle_rules,
    loyalty_rules, map_rules, medal_rules, merchant_rules, oasis_rules, quest_chain, ranking_rules,
    scout_rules, starting_village, unit_rules, wonder_rules,
};
pub use comms_live::{
    ChatHub, LiveMessage, LiveNotification, NotificationHub, run_chat_listener,
    run_notification_listener,
};
pub use config::{AppConfig, ConfigError};
pub use db::{MIGRATOR, create_pool, run_migrations};
pub use event_store::{PgEventStore, Scheduler, now};
pub use repo::PgAccountRepository;
pub use security::Argon2Hasher;
/// Re-exported so downstream crates (the web registry, 041) can name the pool type without a direct
/// `sqlx` dependency.
pub use sqlx::PgPool;
pub use world::{
    World, all_worlds, create_world, ensure_world, ensure_world_with_release, world_by_id,
};
pub use world_rules::{WorldRules, load_world_rules};
