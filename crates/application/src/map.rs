//! The map-view use-case (006 AC7): assemble the viewport grid of tiles around a center, wrapping
//! at the world's edges and overlaying public village markers. Pure over the [`WorldMap`] and the
//! markers the caller fetched.

use crate::ports::VillageMarker;
use eperica_domain::{Coordinate, TileKind, WorldMap};

/// One rendered cell of the map view.
#[derive(Debug, Clone)]
pub struct MapCell {
    /// The canonical (in-bounds) coordinate this cell shows.
    pub coordinate: Coordinate,
    /// The tile's terrain.
    pub tile: TileKind,
    /// A village on this tile, if any (its owner is public — GDD §7.3).
    pub marker: Option<VillageMarker>,
}

/// A square viewport of the map, north (high `y`) at the top, each row west→east.
#[derive(Debug, Clone)]
pub struct Viewport {
    /// The center coordinate the view is built around.
    pub center: Coordinate,
    /// `2·half + 1` rows of `2·half + 1` cells.
    pub rows: Vec<Vec<MapCell>>,
}

/// The canonical coordinates a viewport of `half`-cell radius around `center` covers (deduped) —
/// the caller fetches markers for exactly these (006 AC7).
pub fn viewport_coords(center: Coordinate, half: i32, radius: u32) -> Vec<Coordinate> {
    let mut coords = Vec::new();
    for dy in -half..=half {
        for dx in -half..=half {
            let c = Coordinate::new(center.x.saturating_add(dx), center.y.saturating_add(dy))
                .wrapped(radius);
            if !coords.contains(&c) {
                coords.push(c);
            }
        }
    }
    coords
}

/// Build the viewport grid: each cell's terrain from `map`, with any matching marker overlaid.
pub fn map_viewport(
    map: &WorldMap,
    center: Coordinate,
    half: i32,
    markers: &[VillageMarker],
) -> Viewport {
    let radius = map.radius();
    let mut rows = Vec::with_capacity((2 * half + 1).max(0) as usize);
    // North (higher y) at the top.
    for dy in (-half..=half).rev() {
        let mut row = Vec::with_capacity((2 * half + 1).max(0) as usize);
        for dx in -half..=half {
            let coord = Coordinate::new(center.x.saturating_add(dx), center.y.saturating_add(dy))
                .wrapped(radius);
            // `tile_at` is `Some` for any in-bounds coordinate, which the wrap guarantees.
            let tile = map.tile_at(coord).unwrap_or(TileKind::Natar);
            let marker = markers.iter().find(|m| m.coordinate == coord).cloned();
            row.push(MapCell {
                coordinate: coord,
                tile,
                marker,
            });
        }
        rows.push(row);
    }
    Viewport {
        center: center.wrapped(radius),
        rows,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eperica_domain::{FieldDistribution, MapRules, OasisBonus, Weighted};

    fn map(radius: u32) -> WorldMap {
        let rules = MapRules::new(
            100,
            20,
            vec![Weighted {
                value: FieldDistribution::new(4, 4, 4, 6).unwrap(),
                weight: 1,
            }],
            vec![Weighted {
                value: OasisBonus {
                    wood: 25,
                    clay: 0,
                    iron: 0,
                    crop: 0,
                },
                weight: 1,
            }],
        )
        .unwrap();
        WorldMap::new(7, radius, rules)
    }

    #[test]
    fn viewport_is_square_and_centered() {
        let v = map_viewport(&map(50), Coordinate::new(3, -2), 4, &[]);
        assert_eq!(v.rows.len(), 9);
        assert!(v.rows.iter().all(|r| r.len() == 9));
        // Center cell is the middle of the middle row.
        assert_eq!(v.rows[4][4].coordinate, Coordinate::new(3, -2));
        // Top-left is NW of center; north is higher y.
        assert_eq!(v.rows[0][0].coordinate, Coordinate::new(-1, 2));
    }

    #[test]
    fn viewport_wraps_at_the_edge() {
        // radius 5 (width 11); centering on the east edge shows the far-west column seamlessly.
        let v = map_viewport(&map(5), Coordinate::new(5, 0), 2, &[]);
        let mid = &v.rows[2]; // dy = 0 row, west→east
        assert_eq!(mid[0].coordinate, Coordinate::new(3, 0));
        assert_eq!(mid[2].coordinate, Coordinate::new(5, 0));
        assert_eq!(mid[3].coordinate, Coordinate::new(-5, 0)); // wrapped
        assert_eq!(mid[4].coordinate, Coordinate::new(-4, 0));
    }

    #[test]
    fn markers_overlay_their_tiles() {
        let here = Coordinate::new(0, 0);
        let markers = vec![VillageMarker {
            coordinate: here,
            owner_name: "alice".to_owned(),
            alliance_tag: None,
            owner_last_activity: eperica_domain::Timestamp(0),
        }];
        let v = map_viewport(&map(50), here, 1, &markers);
        assert_eq!(
            v.rows[1][1].marker.as_ref().map(|m| m.owner_name.as_str()),
            Some("alice")
        );
        // No marker on the neighbours.
        assert!(v.rows[0][0].marker.is_none());
    }

    #[test]
    fn viewport_coords_are_canonical_and_deduped() {
        // On a tiny world the window wraps onto itself; coords stay in-bounds and unique.
        let coords = viewport_coords(Coordinate::new(0, 0), 3, 2);
        assert!(coords.iter().all(|c| c.in_bounds(2)));
        let mut sorted = coords.clone();
        sorted.sort_by_key(|c| (c.x, c.y));
        sorted.dedup();
        assert_eq!(sorted.len(), coords.len());
    }
}
