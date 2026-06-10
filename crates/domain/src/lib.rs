//! Eperica domain layer — the pure game core.
//!
//! Holds entities, value objects, and game rules. Per the project constitution (**P3**) this crate
//! has **no I/O, framework, or database dependencies** and is unit-testable in isolation. Game
//! modules grow here slice by slice.
#![forbid(unsafe_code)]

pub mod building;
pub mod construction;
pub mod economy;
pub mod error;
pub mod event;
pub mod resource;
pub mod village;
pub mod world;

pub use building::BuildingKind;
pub use construction::{
    BuildRules, BuildTarget, LevelSpec, build_time_secs, can_afford, debit, prerequisites_met,
};
pub use economy::{
    Capacities, Economy, EconomyRules, ProductionRates, ResourceAmounts, accrue, capacities,
    compute_economy, population, production_rates,
};
pub use error::DomainError;
pub use event::{EventKind, ScheduledEvent, Timestamp};
pub use resource::ResourceKind;
pub use village::{
    BuildingSlot, PlayerId, RESOURCE_FIELD_COUNT, ResourceField, StartingVillage, Tribe, Village,
    VillageId,
};
pub use world::{Coordinate, GameSpeed, WorldConfig, WorldId, coordinates_within};
