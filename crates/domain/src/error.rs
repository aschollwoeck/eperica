//! Domain error types.
//!
//! The pure core defines its own errors and does **not** depend on external error crates (P3).

use std::fmt;

/// Errors produced by domain rules and value-object construction.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DomainError {
    /// A game speed that is not a finite, strictly-positive multiplier.
    InvalidGameSpeed,
}

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DomainError::InvalidGameSpeed => {
                write!(f, "game speed must be a finite, positive multiplier")
            }
        }
    }
}

impl std::error::Error for DomainError {}
