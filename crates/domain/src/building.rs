//! Buildings that occupy a village's center slots.

/// Number of center building slots every village has (110). Slots are `0..VILLAGE_BUILDING_SLOTS`:
/// 20 general slots plus the two reserved special positions (Rally Point, Wall). The Main Building
/// occupies a general slot (slot 0) from founding. Mirrors [`crate::village::RESOURCE_FIELD_COUNT`].
pub const VILLAGE_BUILDING_SLOTS: u8 = 22;

/// The reserved centre slot for the Main Building (default-built, unique, non-demolishable).
pub const MAIN_BUILDING_SLOT: u8 = 0;
/// The reserved centre slot for the Rally Point (default-built).
pub const RALLY_POINT_SLOT: u8 = 1;
/// The reserved centre slot for the Wall. (Reserved-slot numbers are pinned to the pre-110
/// `building_slot` values so existing villages migrate without moving any row.)
pub const WALL_SLOT: u8 = 11;

/// The kind reserved to `slot`, if it is a reserved special position; `None` for a general slot.
/// A reserved slot accepts only its kind, and that kind builds only there.
pub fn reserved_kind(slot: u8) -> Option<BuildingKind> {
    match slot {
        MAIN_BUILDING_SLOT => Some(BuildingKind::MainBuilding),
        RALLY_POINT_SLOT => Some(BuildingKind::RallyPoint),
        WALL_SLOT => Some(BuildingKind::Wall),
        _ => None,
    }
}

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
    /// Gates alliance membership (015): level 1 to join an alliance, level 3 to found one.
    Embassy,
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
    /// Designates the player's **capital** and trains settlers/administrators; at most one per player.
    /// The Residence is its non-capital counterpart. (Loyalty defence + conquest are 014.)
    Palace,
    /// Required to hold an **artifact** (020, GDD §6/§11.3); its level gates which artifact scopes a
    /// village may hold. One artifact per village.
    Treasury,
    /// The **Wonder of the World** (021, GDD §11.3): built only at a conquered Natar Wonder site while
    /// the alliance holds a plan; the first alliance to raise it to level 100 wins the round.
    Wonder,
}

impl BuildingKind {
    /// The reserved centre slot this kind must occupy, if any (110). The Main Building, Rally Point,
    /// and Wall have fixed positions; every other kind is placed on a free general slot chosen by the
    /// player. Inverse of [`reserved_kind`].
    pub fn reserved_slot(self) -> Option<u8> {
        match self {
            BuildingKind::MainBuilding => Some(MAIN_BUILDING_SLOT),
            BuildingKind::RallyPoint => Some(RALLY_POINT_SLOT),
            BuildingKind::Wall => Some(WALL_SLOT),
            _ => None,
        }
    }

    /// How many of this kind a village may hold (110). `None` = unlimited (bounded only by the
    /// available free slots); `Some(1)` = one per village. The Warehouse, Granary, and Cranny may be
    /// built repeatedly — their effects stack (storage/protection sum); every other kind is unique.
    pub fn max_instances(self) -> Option<u32> {
        match self {
            BuildingKind::Warehouse | BuildingKind::Granary | BuildingKind::Cranny => None,
            _ => Some(1),
        }
    }

    /// Whether this kind may be built more than once per village (the inverse of "unique").
    pub fn is_multi(self) -> bool {
        self.max_instances().is_none()
    }

    /// Whether this kind can be **demolished** to free its slot (110, AC6). The Main Building is the
    /// construction hub and is never demolished; everything else (including the reserved Rally Point
    /// and Wall) can be torn down.
    pub fn is_demolishable(self) -> bool {
        self != BuildingKind::MainBuilding
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_slots_round_trip() {
        assert_eq!(
            reserved_kind(MAIN_BUILDING_SLOT),
            Some(BuildingKind::MainBuilding)
        );
        assert_eq!(
            reserved_kind(RALLY_POINT_SLOT),
            Some(BuildingKind::RallyPoint)
        );
        assert_eq!(reserved_kind(WALL_SLOT), Some(BuildingKind::Wall));
        assert_eq!(reserved_kind(5), None);
        assert_eq!(
            BuildingKind::RallyPoint.reserved_slot(),
            Some(RALLY_POINT_SLOT)
        );
        assert_eq!(BuildingKind::Marketplace.reserved_slot(), None);
    }

    #[test]
    fn multiplicity_and_demolition_flags() {
        for k in [
            BuildingKind::Warehouse,
            BuildingKind::Granary,
            BuildingKind::Cranny,
        ] {
            assert!(k.is_multi(), "{k:?} stacks");
            assert_eq!(k.max_instances(), None);
        }
        assert_eq!(BuildingKind::Marketplace.max_instances(), Some(1));
        assert!(!BuildingKind::Marketplace.is_multi());
        // the Main Building is never demolished; everything else can be.
        assert!(!BuildingKind::MainBuilding.is_demolishable());
        assert!(BuildingKind::Warehouse.is_demolishable());
        assert!(BuildingKind::RallyPoint.is_demolishable());
    }
}
