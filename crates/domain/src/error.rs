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
    /// A starting-village template that does not have exactly the required number of resource fields.
    InvalidStartingVillage,
    /// Unit balance data that does not form valid rosters (see the message for the violated rule).
    InvalidUnitRules(&'static str),
    /// Map-generation balance data that is not valid (see the message for the violated rule).
    InvalidMapRules(&'static str),
    /// Merchant/trade balance data that is not valid (see the message for the violated rule).
    InvalidMerchantRules(&'static str),
}

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DomainError::InvalidGameSpeed => {
                write!(f, "game speed must be a finite, positive multiplier")
            }
            DomainError::InvalidStartingVillage => {
                write!(f, "a starting village must have exactly 18 resource fields")
            }
            DomainError::InvalidUnitRules(rule) => {
                write!(f, "invalid unit balance data: {rule}")
            }
            DomainError::InvalidMapRules(rule) => {
                write!(f, "invalid map balance data: {rule}")
            }
            DomainError::InvalidMerchantRules(rule) => {
                write!(f, "invalid merchant balance data: {rule}")
            }
        }
    }
}

impl std::error::Error for DomainError {}
