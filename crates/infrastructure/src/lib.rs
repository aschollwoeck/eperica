//! Eperica infrastructure layer — adapters implementing the application's ports.
//!
//! Depends on [`eperica_application`] (and transitively the domain). Concrete I/O — database access
//! (SQLx/Postgres), sessions, the event store — is added here in later tasks.
#![forbid(unsafe_code)]

/// Re-export of the application crate, whose ports this layer implements.
pub use eperica_application as application;
