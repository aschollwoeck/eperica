//! Buildings that occupy a village's center slots.

/// A type of center building. Extended in later slices — exhaustive on purpose so that adding a
/// variant produces a compile error everywhere it must be handled (e.g. persistence mapping).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuildingKind {
    /// Speeds construction; required by most other buildings.
    MainBuilding,
    /// Required to send and return troops; present from founding.
    RallyPoint,
    /// Stores wood, clay, and iron; higher levels raise their capacity.
    Warehouse,
    /// Stores crop; higher levels raise its capacity.
    Granary,
    /// Trains infantry (005); required by the Academy.
    Barracks,
    /// Researches unit types so they can be trained.
    Academy,
    /// Upgrades unit types' combat strength in levels.
    Smithy,
    /// Trains cavalry (005). Known so research requirements can reference it; constructable in 005.
    Stable,
    /// Builds siege engines (005). Known so research requirements can reference it; constructable in 005.
    Workshop,
}
