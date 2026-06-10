//! Buildings that occupy a village's center slots.

/// A type of center building. Extended in later slices; `#[non_exhaustive]` so adding variants is
/// not a breaking change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum BuildingKind {
    /// Speeds construction; required by most other buildings.
    MainBuilding,
    /// Required to send and return troops; present from founding.
    RallyPoint,
}
