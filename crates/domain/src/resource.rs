//! Resources — the four economic primitives of the game.

/// The four resources every village produces and spends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceKind {
    /// Wood (lumber) — general construction.
    Wood,
    /// Clay — general construction.
    Clay,
    /// Iron — construction, weighted toward military.
    Iron,
    /// Crop — construction and ongoing upkeep (population and troops).
    Crop,
}
