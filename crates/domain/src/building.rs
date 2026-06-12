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
    /// Enables trade; its level sets how many merchants the village has (008).
    Marketplace,
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
    /// Trains settlers/administrators and gates expansion (013). Known so unit definitions can
    /// reference it; constructable in 013.
    Residence,
    /// Boosts the garrison's defence; reduced by rams in combat (009). Tribe-flavoured by balance.
    Wall,
    /// Hides a per-level quantity of each resource from looting (011); Teutons partially bypass it.
    Cranny,
    /// Garrisons troops on a captured oasis; its level sets how many oases a village may hold (012).
    Outpost,
    /// Produces culture points that gate expansion (013, GDD §11.1).
    TownHall,
}
