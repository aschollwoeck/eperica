//! The world map — seeded, generate-on-read terrain (GDD §7, P6).
//!
//! The whole map is a pure function of the world's `seed`: `tile_at(coord)` mixes `(seed, x, y)`
//! into a hash and buckets it by balance weights. No tile is ever stored; mutable state (villages
//! now, oasis occupation later) lives in its own tables and is layered on top. Selection is
//! integer-only, so the map is bit-reproducible across platforms.

use crate::error::DomainError;
use crate::resource::ResourceKind;
use crate::village::{RESOURCE_FIELD_COUNT, ResourceField};
use crate::world::{Coordinate, toroidal_distance};

/// How the 18 resource fields of a valley are split across the four resources (GDD §3.1). The four
/// counts always sum to [`RESOURCE_FIELD_COUNT`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldDistribution {
    pub wood: u8,
    pub clay: u8,
    pub iron: u8,
    pub crop: u8,
}

impl FieldDistribution {
    /// Create a distribution.
    ///
    /// # Errors
    /// Returns [`DomainError::InvalidMapRules`] unless the counts sum to [`RESOURCE_FIELD_COUNT`].
    pub fn new(wood: u8, clay: u8, iron: u8, crop: u8) -> Result<Self, DomainError> {
        let d = Self {
            wood,
            clay,
            iron,
            crop,
        };
        if d.sum() == RESOURCE_FIELD_COUNT {
            Ok(d)
        } else {
            Err(DomainError::InvalidMapRules(
                "a field distribution must sum to 18",
            ))
        }
    }

    /// Total number of fields (must be [`RESOURCE_FIELD_COUNT`]).
    pub fn sum(self) -> usize {
        usize::from(self.wood)
            + usize::from(self.clay)
            + usize::from(self.iron)
            + usize::from(self.crop)
    }

    /// The 18 level-0 resource fields this valley founds a village with, grouped by resource in a
    /// deterministic order (wood, clay, iron, crop).
    pub fn fields(self) -> Vec<ResourceField> {
        let mut fields = Vec::with_capacity(RESOURCE_FIELD_COUNT);
        for (kind, count) in [
            (ResourceKind::Wood, self.wood),
            (ResourceKind::Clay, self.clay),
            (ResourceKind::Iron, self.iron),
            (ResourceKind::Crop, self.crop),
        ] {
            for _ in 0..count {
                fields.push(ResourceField { kind, level: 0 });
            }
        }
        fields
    }
}

/// An oasis's percentage production bonus by resource (most entries zero) — GDD §7.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OasisBonus {
    pub wood: u8,
    pub clay: u8,
    pub iron: u8,
    pub crop: u8,
}

/// What occupies a single map tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileKind {
    /// An occupiable valley with its field layout.
    Valley(FieldDistribution),
    /// An oasis granting a production bonus (occupied via the Outpost — slice 012).
    Oasis(OasisBonus),
    /// A reserved Natar/special tile (end-game — §11); terrain only for now.
    Natar,
}

/// A balance entry with a selection weight.
#[derive(Debug, Clone, Copy)]
pub struct Weighted<T> {
    pub value: T,
    pub weight: u32,
}

/// Injected map-generation balance: tile-kind densities and the weighted distribution/bonus tables.
#[derive(Debug, Clone)]
pub struct MapRules {
    /// Per-mille of tiles that are oases.
    oasis_permille: u32,
    /// Per-mille of tiles that are Natar (after oases).
    natar_permille: u32,
    /// Weighted valley field distributions (each summing to 18).
    distributions: Vec<Weighted<FieldDistribution>>,
    /// Weighted oasis bonuses.
    oasis_bonuses: Vec<Weighted<OasisBonus>>,
}

impl MapRules {
    /// Build validated map rules.
    ///
    /// # Errors
    /// Returns [`DomainError::InvalidMapRules`] if the densities leave no room for valleys, a table
    /// is empty, any weight is zero, or any distribution does not sum to 18.
    pub fn new(
        oasis_permille: u32,
        natar_permille: u32,
        distributions: Vec<Weighted<FieldDistribution>>,
        oasis_bonuses: Vec<Weighted<OasisBonus>>,
    ) -> Result<Self, DomainError> {
        if oasis_permille + natar_permille >= 1000 {
            return Err(DomainError::InvalidMapRules(
                "oasis + natar density must leave room for valleys (< 1000 permille)",
            ));
        }
        if distributions.is_empty() || oasis_bonuses.is_empty() {
            return Err(DomainError::InvalidMapRules(
                "distribution and oasis-bonus tables must be non-empty",
            ));
        }
        if distributions.iter().any(|w| w.weight == 0)
            || oasis_bonuses.iter().any(|w| w.weight == 0)
        {
            return Err(DomainError::InvalidMapRules(
                "table weights must be positive",
            ));
        }
        for d in &distributions {
            if d.value.sum() != RESOURCE_FIELD_COUNT {
                return Err(DomainError::InvalidMapRules(
                    "a field distribution must sum to 18",
                ));
            }
        }
        Ok(Self {
            oasis_permille,
            natar_permille,
            distributions,
            oasis_bonuses,
        })
    }
}

/// A seeded, generate-on-read world map: the pure terrain over a persisted `seed` and `radius`.
#[derive(Debug, Clone)]
pub struct WorldMap {
    seed: u64,
    radius: u32,
    rules: MapRules,
}

impl WorldMap {
    /// Create a world map from its persisted seed, radius, and balance rules.
    pub fn new(seed: u64, radius: u32, rules: MapRules) -> Self {
        Self {
            seed,
            radius,
            rules,
        }
    }

    /// The map radius (coordinates span `-radius..=radius`).
    pub fn radius(&self) -> u32 {
        self.radius
    }

    /// The tile at `coord`, or `None` if it is out of bounds (P6 deterministic; AC1).
    pub fn tile_at(&self, coord: Coordinate) -> Option<TileKind> {
        if !coord.in_bounds(self.radius) {
            return None;
        }
        let h = mix(self.seed, coord.x, coord.y);
        let kind_roll = (h % 1000) as u32;
        let table_roll = splitmix64(h);
        Some(if kind_roll < self.rules.oasis_permille {
            TileKind::Oasis(*pick(&self.rules.oasis_bonuses, table_roll))
        } else if kind_roll < self.rules.oasis_permille + self.rules.natar_permille {
            TileKind::Natar
        } else {
            TileKind::Valley(*pick(&self.rules.distributions, table_roll))
        })
    }

    /// Whether `coord` is an occupiable valley (false out of bounds, on oases, and on Natar) —
    /// used by village placement (AC5).
    pub fn is_valley(&self, coord: Coordinate) -> bool {
        matches!(self.tile_at(coord), Some(TileKind::Valley(_)))
    }

    /// The shortest wrapped distance between two tiles on this map (AC4).
    pub fn distance(&self, a: Coordinate, b: Coordinate) -> f64 {
        toroidal_distance(a, b, self.radius)
    }
}

/// One round of the SplitMix64 finalizer — a fast, well-distributed integer hash.
fn splitmix64(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Deterministically mix `(seed, x, y)` into a hash. `x`/`y` are bit-cast to unsigned so negative
/// coordinates are handled uniformly; `y` is rotated into the high bits so it cannot collide with
/// `x`.
fn mix(seed: u64, x: i32, y: i32) -> u64 {
    let h = splitmix64(seed);
    let h = splitmix64(h ^ u64::from(x as u32));
    splitmix64(h ^ u64::from(y as u32).rotate_left(32))
}

/// Pick a weighted entry by a roll. The table is non-empty with positive weights (validated), so
/// the fallback is unreachable.
fn pick<T>(items: &[Weighted<T>], roll: u64) -> &T {
    let total: u64 = items.iter().map(|w| u64::from(w.weight)).sum();
    let mut target = roll % total;
    for item in items {
        let w = u64::from(item.weight);
        if target < w {
            return &item.value;
        }
        target -= w;
    }
    &items[items.len() - 1].value
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> MapRules {
        MapRules::new(
            100, // 10% oasis
            20,  // 2% natar
            vec![
                Weighted {
                    value: FieldDistribution::new(4, 4, 4, 6).unwrap(),
                    weight: 90,
                },
                Weighted {
                    value: FieldDistribution::new(3, 3, 3, 9).unwrap(),
                    weight: 9,
                },
                Weighted {
                    value: FieldDistribution::new(1, 1, 1, 15).unwrap(),
                    weight: 1,
                },
            ],
            vec![
                Weighted {
                    value: OasisBonus {
                        wood: 25,
                        clay: 0,
                        iron: 0,
                        crop: 0,
                    },
                    weight: 3,
                },
                Weighted {
                    value: OasisBonus {
                        wood: 0,
                        clay: 0,
                        iron: 0,
                        crop: 50,
                    },
                    weight: 1,
                },
            ],
        )
        .expect("valid rules")
    }

    fn map(seed: u64) -> WorldMap {
        WorldMap::new(seed, 50, rules())
    }

    // --- AC1: deterministic, seed-dependent terrain ---
    #[test]
    fn terrain_is_deterministic_and_seed_dependent() {
        let m = map(12345);
        let c = Coordinate::new(7, -3);
        assert_eq!(m.tile_at(c), m.tile_at(c)); // same seed+coord → same tile

        // A different seed changes a material fraction of a sampled region.
        let a = map(1);
        let b = map(2);
        let mut differ = 0;
        let mut total = 0;
        for x in -20..=20 {
            for y in -20..=20 {
                let c = Coordinate::new(x, y);
                total += 1;
                if a.tile_at(c) != b.tile_at(c) {
                    differ += 1;
                }
            }
        }
        assert!(
            differ * 4 > total,
            "seeds barely differed: {differ}/{total}"
        );

        // Out of bounds → no tile.
        assert_eq!(m.tile_at(Coordinate::new(51, 0)), None);
        assert_eq!(m.tile_at(Coordinate::new(0, -51)), None);
    }

    // --- AC2: valley distributions are valid and from the table ---
    #[test]
    fn valleys_have_valid_distributions_from_the_table() {
        let m = map(999);
        let allowed = [
            FieldDistribution::new(4, 4, 4, 6).unwrap(),
            FieldDistribution::new(3, 3, 3, 9).unwrap(),
            FieldDistribution::new(1, 1, 1, 15).unwrap(),
        ];
        for x in -50..=50 {
            for y in -50..=50 {
                if let Some(TileKind::Valley(d)) = m.tile_at(Coordinate::new(x, y)) {
                    assert_eq!(d.sum(), RESOURCE_FIELD_COUNT);
                    assert!(allowed.contains(&d), "{d:?} not in the table");
                }
            }
        }
    }

    #[test]
    fn distribution_builds_eighteen_level_zero_fields() {
        let d = FieldDistribution::new(4, 4, 4, 6).unwrap();
        let fields = d.fields();
        assert_eq!(fields.len(), 18);
        assert!(fields.iter().all(|f| f.level == 0));
        assert_eq!(
            fields
                .iter()
                .filter(|f| f.kind == ResourceKind::Crop)
                .count(),
            6
        );
        assert_eq!(
            fields
                .iter()
                .filter(|f| f.kind == ResourceKind::Wood)
                .count(),
            4
        );
    }

    // --- AC3: densities roughly track the configured permille ---
    #[test]
    fn oasis_and_natar_densities_track_the_config() {
        let m = map(2024);
        let (mut oasis, mut natar, mut valley) = (0u32, 0u32, 0u32);
        for x in -50..=50 {
            for y in -50..=50 {
                match m.tile_at(Coordinate::new(x, y)) {
                    Some(TileKind::Oasis(_)) => oasis += 1,
                    Some(TileKind::Natar) => natar += 1,
                    Some(TileKind::Valley(_)) => valley += 1,
                    None => {}
                }
            }
        }
        let total = (oasis + natar + valley) as f64;
        // Configured 100‰ / 20‰; allow ±30‰ absolute over this sample.
        assert!(
            (oasis as f64 / total - 0.100).abs() < 0.03,
            "oasis {oasis}/{total}"
        );
        assert!(
            (natar as f64 / total - 0.020).abs() < 0.03,
            "natar {natar}/{total}"
        );
        assert!(valley > oasis + natar, "valleys should dominate");
    }

    // --- AC2: fail-fast validation ---
    #[test]
    fn invalid_rules_are_rejected() {
        // Distribution not summing to 18.
        assert!(FieldDistribution::new(4, 4, 4, 4).is_err());
        // Densities with no room for valleys.
        assert!(
            MapRules::new(
                600,
                500,
                vec![Weighted {
                    value: FieldDistribution::new(4, 4, 4, 6).unwrap(),
                    weight: 1
                }],
                vec![Weighted {
                    value: OasisBonus {
                        wood: 25,
                        clay: 0,
                        iron: 0,
                        crop: 0
                    },
                    weight: 1
                }],
            )
            .is_err()
        );
        // Empty distribution table.
        assert!(
            MapRules::new(
                0,
                0,
                vec![],
                vec![Weighted {
                    value: OasisBonus {
                        wood: 25,
                        clay: 0,
                        iron: 0,
                        crop: 0
                    },
                    weight: 1
                }],
            )
            .is_err()
        );
    }

    // --- AC5 support: there are valleys near the origin to place on ---
    #[test]
    fn the_origin_region_has_valleys() {
        let m = map(42);
        let valleys = crate::world::coordinates_within(5)
            .filter(|c| m.is_valley(*c))
            .count();
        assert!(valleys > 10, "only {valleys} valleys near the origin");
    }
}
