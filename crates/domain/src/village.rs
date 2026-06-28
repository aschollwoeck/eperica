//! Villages — the player's settlements: identity, the resource fields, and the center buildings.

use crate::artifact::ArtifactEffects;
use crate::building::BuildingKind;
use crate::error::DomainError;
use crate::map::{FieldDistribution, OasisBonus};
use crate::resource::ResourceKind;
use crate::world::Coordinate;

/// Number of resource-field slots every village has.
pub const RESOURCE_FIELD_COUNT: usize = 18;

/// Unique identifier of a player (a 128-bit value; the infrastructure maps it to a UUID column).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlayerId(pub u128);

/// Unique identifier of a village.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VillageId(pub u128);

/// The three playable tribes, chosen once at registration (004 AC1/AC2; GDD §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tribe {
    /// Balanced, expensive all-rounders.
    Romans,
    /// Cheap, aggressive raiders.
    Teutons,
    /// Fast, defensive specialists.
    Gauls,
}

impl Tribe {
    /// The stable lowercase identifier used in forms, URLs, and storage.
    pub fn slug(self) -> &'static str {
        match self {
            Tribe::Romans => "romans",
            Tribe::Teutons => "teutons",
            Tribe::Gauls => "gauls",
        }
    }

    /// Parse a [`slug`](Self::slug); `None` for anything else (server-side validation, P4).
    pub fn from_slug(s: &str) -> Option<Self> {
        match s {
            "romans" => Some(Tribe::Romans),
            "teutons" => Some(Tribe::Teutons),
            "gauls" => Some(Tribe::Gauls),
            _ => None,
        }
    }
}

/// One resource-field slot (a single woodcutter, clay pit, iron mine, or cropland) at a level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceField {
    /// Which resource this field produces.
    pub kind: ResourceKind,
    /// The field's level.
    pub level: u8,
}

/// One occupied center-building slot: a building of `kind` at `level`, sitting in centre `slot`
/// (110). Slots are `0..`[`VILLAGE_BUILDING_SLOTS`](crate::building::VILLAGE_BUILDING_SLOTS); a village
/// holds only its occupied slots (an absent slot number is an empty build spot).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildingSlot {
    /// The centre slot this building occupies (110).
    pub slot: u8,
    /// Which building occupies the slot.
    pub kind: BuildingKind,
    /// The building's level.
    pub level: u8,
}

/// A validated template for a freshly-founded village (its values come from balance data).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartingVillage {
    fields: Vec<ResourceField>,
    buildings: Vec<BuildingSlot>,
}

impl StartingVillage {
    /// Create a starting-village template.
    ///
    /// # Errors
    /// Returns [`DomainError::InvalidStartingVillage`] unless there are exactly
    /// [`RESOURCE_FIELD_COUNT`] resource fields.
    pub fn new(
        fields: Vec<ResourceField>,
        buildings: Vec<BuildingSlot>,
    ) -> Result<Self, DomainError> {
        if fields.len() != RESOURCE_FIELD_COUNT {
            return Err(DomainError::InvalidStartingVillage);
        }
        Ok(Self { fields, buildings })
    }

    /// The template's resource fields.
    pub fn fields(&self) -> &[ResourceField] {
        &self.fields
    }

    /// The template's starting buildings.
    pub fn buildings(&self) -> &[BuildingSlot] {
        &self.buildings
    }
}

/// A player's settlement on the map.
#[derive(Debug, Clone, PartialEq)]
pub struct Village {
    /// Unique identity.
    pub id: VillageId,
    /// The owning player.
    pub owner: PlayerId,
    /// The map tile this village occupies.
    pub coordinate: Coordinate,
    /// The village's tribe (`None` until tribe selection — slice 004).
    pub tribe: Option<Tribe>,
    /// The 18 resource-field slots.
    pub fields: Vec<ResourceField>,
    /// The center buildings.
    pub buildings: Vec<BuildingSlot>,
    /// The summed per-resource production bonus from the oases this village occupies (012, AC8).
    /// Folded into the village read (the repository derives it from the village's owned oases) so it
    /// rides every economy computation; zero for a village holding no oases.
    pub oasis_bonus: OasisBonus,
    /// Whether this village is the player's **capital** (013, §3.4) — set by building a Palace. The
    /// capital may raise its resource fields past the normal cap; folded into the village read.
    pub is_capital: bool,
    /// Whether this is a **Natar NPC** village (020) — an artifact vault, not ownable (conquest never
    /// transfers it) and excluded from boards/stats.
    pub is_natar: bool,
    /// Whether this Natar village is a **Wonder construction site** (021) — unlike an artifact vault, a
    /// site **is** conquerable, so an alliance can take it and build the Wonder of the World on it.
    pub is_wonder_site: bool,
    /// The artifact effects in force for this village (020) — its own small holdings plus the
    /// account's large/unique, aggregated on read (the oasis-bonus pattern). `NONE` for no holdings
    /// (and for Natar/NPC villages, which never benefit from the artifacts they guard).
    pub artifact_effects: ArtifactEffects,
}

impl Village {
    /// Found a new village for `owner` at `coordinate`, carrying the owner's `tribe` (004). The 18
    /// resource fields are built from the `distribution` of the valley tile being settled (006);
    /// the center `buildings` come from the starting template. Server-side callers supply the
    /// identity, coordinate, and tile (P4); the domain never invents them.
    pub fn found(
        id: VillageId,
        owner: PlayerId,
        coordinate: Coordinate,
        tribe: Tribe,
        distribution: FieldDistribution,
        template: &StartingVillage,
    ) -> Self {
        Self {
            id,
            owner,
            coordinate,
            tribe: Some(tribe),
            fields: distribution.fields(),
            buildings: template.buildings().to_vec(),
            oasis_bonus: OasisBonus::default(),
            is_capital: false,
            is_natar: false,
            is_wonder_site: false,
            artifact_effects: ArtifactEffects::NONE,
        }
    }

    /// Whether this village can be **conquered** (ownership transferred by an administrator attack).
    /// A capital is immune (013/014 AC5) and a Natar artifact vault is never ownable (020 AC2), but a
    /// Natar **Wonder site** is conquerable so an alliance can take it and build the Wonder (021 AC3).
    pub fn is_conquerable(&self) -> bool {
        !self.is_capital && (!self.is_natar || self.is_wonder_site)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn balanced_template() -> StartingVillage {
        let mut fields = Vec::new();
        for kind in [ResourceKind::Wood, ResourceKind::Clay, ResourceKind::Iron] {
            fields.extend(std::iter::repeat_n(ResourceField { kind, level: 0 }, 4));
        }
        fields.extend(std::iter::repeat_n(
            ResourceField {
                kind: ResourceKind::Crop,
                level: 0,
            },
            6,
        ));
        StartingVillage::new(
            fields,
            vec![
                BuildingSlot {
                    slot: 0,
                    kind: BuildingKind::MainBuilding,
                    level: 1,
                },
                BuildingSlot {
                    slot: 1,
                    kind: BuildingKind::RallyPoint,
                    level: 1,
                },
            ],
        )
        .expect("balanced template is valid")
    }

    #[test]
    fn founded_village_has_18_fields_and_core_buildings() {
        // The fields come from the settled valley's distribution (006), the buildings from the
        // template.
        let cropper = FieldDistribution::new(3, 3, 3, 9).unwrap();
        let v = Village::found(
            VillageId(1),
            PlayerId(42),
            Coordinate::new(0, 0),
            Tribe::Gauls,
            cropper,
            &balanced_template(),
        );
        assert_eq!(v.fields.len(), RESOURCE_FIELD_COUNT);
        assert_eq!(
            v.fields
                .iter()
                .filter(|f| f.kind == ResourceKind::Crop)
                .count(),
            9
        );
        assert_eq!(v.owner, PlayerId(42));
        assert_eq!(v.coordinate, Coordinate::new(0, 0));
        assert_eq!(v.tribe, Some(Tribe::Gauls));
        let kinds: Vec<_> = v.buildings.iter().map(|b| b.kind).collect();
        assert!(kinds.contains(&BuildingKind::MainBuilding));
        assert!(kinds.contains(&BuildingKind::RallyPoint));
    }

    #[test]
    fn conquerability_follows_capital_natar_and_wonder_site() {
        let mut v = Village::found(
            VillageId(1),
            PlayerId(1),
            Coordinate::new(0, 0),
            Tribe::Romans,
            FieldDistribution::new(4, 4, 4, 6).unwrap(),
            &balanced_template(),
        );
        // A normal village is conquerable (014).
        assert!(v.is_conquerable());
        // A capital is immune (014 AC5).
        v.is_capital = true;
        assert!(!v.is_conquerable());
        v.is_capital = false;
        // An artifact vault (Natar, not a site) is never ownable (020 AC2).
        v.is_natar = true;
        assert!(!v.is_conquerable());
        // A Natar Wonder site IS conquerable (021 AC3).
        v.is_wonder_site = true;
        assert!(v.is_conquerable());
    }

    #[test]
    fn starting_village_rejects_wrong_field_count() {
        let too_few = vec![ResourceField {
            kind: ResourceKind::Wood,
            level: 0,
        }];
        assert_eq!(
            StartingVillage::new(too_few, vec![]),
            Err(DomainError::InvalidStartingVillage)
        );
    }
}
