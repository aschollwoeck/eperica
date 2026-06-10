//! Eperica application layer — use-cases (commands) and the ports (traits) the domain needs.
//!
//! Depends only on [`eperica_domain`]; the infrastructure layer implements its ports. No concrete
//! I/O lives here beyond port definitions.
#![forbid(unsafe_code)]

/// Re-export of the domain crate, the layer this one builds upon.
pub use eperica_domain as domain;
