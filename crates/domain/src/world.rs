//! Core world value objects: game speed, map coordinates, and world configuration.

use crate::error::DomainError;
use std::time::Duration;

/// The server-configured time multiplier for a world (e.g. 1×, 3×, 5×).
///
/// Per the constitution (**P7**), every time-dependent value derives from a base design value scaled
/// by this multiplier — no wall-clock duration is hardcoded.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GameSpeed(f64);

impl GameSpeed {
    /// Create a game speed from a multiplier. Must be finite and strictly positive.
    ///
    /// # Errors
    /// Returns [`DomainError::InvalidGameSpeed`] if `multiplier` is not finite or not `> 0`.
    pub fn new(multiplier: f64) -> Result<Self, DomainError> {
        if multiplier.is_finite() && multiplier > 0.0 {
            Ok(Self(multiplier))
        } else {
            Err(DomainError::InvalidGameSpeed)
        }
    }

    /// The raw multiplier (e.g. `5.0` for a 5× world).
    pub fn multiplier(self) -> f64 {
        self.0
    }

    /// Scale a base design duration by this speed: a faster world shortens durations.
    ///
    /// `effective = base / speed` (a 5× world finishes a 1-hour build in 12 minutes).
    pub fn scale_duration(self, base: Duration) -> Duration {
        base.div_f64(self.0)
    }

    /// Scale a base hourly rate by this speed: a faster world produces proportionally faster.
    ///
    /// `effective = base × speed`.
    pub fn scale_rate(self, base_per_hour: f64) -> f64 {
        base_per_hour * self.0
    }
}

/// A position on the world-map grid: integer coordinates centered on the origin `(0, 0)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Coordinate {
    /// East–west axis.
    pub x: i32,
    /// North–south axis.
    pub y: i32,
}

impl Coordinate {
    /// Create a coordinate.
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// Whether this coordinate lies within a square world of the given `radius`
    /// (`-radius..=radius` on each axis). Uses `i64` math so `i32::MIN` cannot overflow on `abs`.
    pub fn in_bounds(self, radius: u32) -> bool {
        let r = i64::from(radius);
        i64::from(self.x).abs() <= r && i64::from(self.y).abs() <= r
    }

    /// The canonical in-bounds coordinate for a **toroidal** world of `radius`: each axis is
    /// wrapped into `-radius..=radius` (the map's far east is adjacent to the far west, GDD §7.2).
    /// Used to render a viewport seamlessly across the edge.
    pub fn wrapped(self, radius: u32) -> Coordinate {
        Coordinate::new(wrap_axis(self.x, radius), wrap_axis(self.y, radius))
    }
}

/// Grid width of a radius-`R` torus: coordinates span `-R..=R`, i.e. `2R + 1` cells per axis.
fn axis_width(radius: u32) -> i64 {
    2 * i64::from(radius) + 1
}

/// Wrap a single axis value into `-radius..=radius`.
fn wrap_axis(value: i32, radius: u32) -> i32 {
    let w = axis_width(radius);
    let r = i64::from(radius);
    // rem_euclid lands in 0..w; shift the upper half down to the negative side.
    let m = i64::from(value).rem_euclid(w);
    let wrapped = if m > r { m - w } else { m };
    i32::try_from(wrapped).unwrap_or(0)
}

/// The shortest **toroidal Euclidean** distance (in tiles) between two coordinates on a radius-`R`
/// world that wraps at its edges (GDD §7.2). Each axis takes the shorter of the direct or
/// wrapped gap; the result combines them as `√(dx² + dy²)`.
pub fn toroidal_distance(a: Coordinate, b: Coordinate, radius: u32) -> f64 {
    let w = axis_width(radius);
    let axis_gap = |p: i32, q: i32| -> i64 {
        let d = (i64::from(p) - i64::from(q)).abs();
        d.min(w - d)
    };
    let dx = axis_gap(a.x, b.x) as f64;
    let dy = axis_gap(a.y, b.y) as f64;
    (dx * dx + dy * dy).sqrt()
}

/// Unique identifier of a world instance (mapped to a UUID column by the infrastructure).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WorldId(pub u128);

/// Static, operator-set configuration for a single world instance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldConfig {
    /// The time multiplier for this world (P7).
    pub speed: GameSpeed,
    /// The map radius: valid coordinates are `-radius..=radius` on each axis.
    pub radius: u32,
}

impl WorldConfig {
    /// Create a world configuration.
    pub fn new(speed: GameSpeed, radius: u32) -> Self {
        Self { speed, radius }
    }
}

/// The coordinates forming the square ring at Chebyshev distance `ring` from the origin.
fn ring_coordinates(ring: i32) -> Vec<Coordinate> {
    if ring == 0 {
        return vec![Coordinate::new(0, 0)];
    }
    let mut coords = Vec::new();
    for x in -ring..=ring {
        coords.push(Coordinate::new(x, -ring));
        coords.push(Coordinate::new(x, ring));
    }
    for y in (-ring + 1)..ring {
        coords.push(Coordinate::new(-ring, y));
        coords.push(Coordinate::new(ring, y));
    }
    coords
}

/// Deterministic, finite enumeration of all coordinates within `radius` of the origin, ordered ring
/// by ring (nearest first).
///
/// Used to place a new village on the first free tile in a stable order (P6: deterministic). This is
/// the placeholder placement strategy for slice 001; map-aware placement arrives in slice 006.
pub fn coordinates_within(radius: u32) -> impl Iterator<Item = Coordinate> {
    let r = i32::try_from(radius).unwrap_or(i32::MAX);
    (0..=r).flat_map(ring_coordinates)
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- GameSpeed (AC5) ---

    #[test]
    fn speed_scales_duration_inversely() {
        let speed = GameSpeed::new(5.0).unwrap();
        // A 1-hour base build finishes in 12 minutes at 5×.
        assert_eq!(
            speed.scale_duration(Duration::from_secs(3600)),
            Duration::from_secs(720)
        );
    }

    #[test]
    fn speed_one_is_identity() {
        let speed = GameSpeed::new(1.0).unwrap();
        assert_eq!(
            speed.scale_duration(Duration::from_secs(3600)),
            Duration::from_secs(3600)
        );
        assert_eq!(speed.scale_rate(30.0), 30.0);
    }

    #[test]
    fn speed_scales_rate_proportionally() {
        let speed = GameSpeed::new(5.0).unwrap();
        // 30 wood/h at 5× becomes 150 wood/h.
        assert_eq!(speed.scale_rate(30.0), 150.0);
    }

    #[test]
    fn invalid_speeds_are_rejected() {
        for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert_eq!(GameSpeed::new(bad), Err(DomainError::InvalidGameSpeed));
        }
    }

    // --- Coordinate (AC3) ---

    #[test]
    fn coordinate_bounds_are_inclusive() {
        let radius = 500;
        assert!(Coordinate::new(0, 0).in_bounds(radius));
        assert!(Coordinate::new(500, 500).in_bounds(radius));
        assert!(Coordinate::new(-500, -500).in_bounds(radius));
    }

    #[test]
    fn coordinate_out_of_bounds_is_rejected() {
        let radius = 500;
        assert!(!Coordinate::new(501, 0).in_bounds(radius));
        assert!(!Coordinate::new(0, -501).in_bounds(radius));
        assert!(!Coordinate::new(i32::MIN, 0).in_bounds(radius));
    }

    // --- placement enumeration ---

    // --- toroidal distance & wrap (AC4) ---

    #[test]
    fn distance_is_plain_euclidean_when_not_wrapping() {
        let r = 200;
        assert_eq!(
            toroidal_distance(Coordinate::new(0, 0), Coordinate::new(3, 4), r),
            5.0
        );
        assert_eq!(
            toroidal_distance(Coordinate::new(0, 0), Coordinate::new(0, 0), r),
            0.0
        );
    }

    #[test]
    fn distance_is_symmetric_and_zero_iff_equal() {
        let r = 50;
        let a = Coordinate::new(-12, 7);
        let b = Coordinate::new(40, -45);
        assert_eq!(
            toroidal_distance(a, b, r),
            toroidal_distance(b, a, r),
            "symmetry"
        );
        assert_eq!(toroidal_distance(a, a, r), 0.0);
        assert!(toroidal_distance(a, b, r) > 0.0);
    }

    #[test]
    fn distance_uses_the_short_way_around_the_torus() {
        // radius 10 ⇒ width 21. The east edge (10) and west edge (-10) are adjacent (gap 1),
        // not 20 apart.
        let r = 10;
        let east = Coordinate::new(10, 0);
        let west = Coordinate::new(-10, 0);
        assert_eq!(toroidal_distance(east, west, r), 1.0);
        // Both axes wrap.
        let ne = Coordinate::new(10, 10);
        let sw = Coordinate::new(-10, -10);
        assert!((toroidal_distance(ne, sw, r) - 2f64.sqrt()).abs() < 1e-9);
    }

    #[test]
    fn wrapped_brings_coordinates_into_bounds() {
        let r = 10; // width 21, range -10..=10
        assert_eq!(Coordinate::new(11, 0).wrapped(r), Coordinate::new(-10, 0));
        assert_eq!(Coordinate::new(-11, 0).wrapped(r), Coordinate::new(10, 0));
        assert_eq!(
            Coordinate::new(10, -10).wrapped(r),
            Coordinate::new(10, -10)
        );
        assert_eq!(Coordinate::new(32, 0).wrapped(r), Coordinate::new(-10, 0)); // 32 - 21 - 21
        // Wrapping is idempotent and always lands in bounds.
        for x in [-100, -11, 0, 11, 100] {
            assert!(Coordinate::new(x, x).wrapped(r).in_bounds(r));
        }
    }

    #[test]
    fn coordinates_within_are_ordered_complete_and_unique() {
        let coords: Vec<_> = coordinates_within(1).collect();
        assert_eq!(coords[0], Coordinate::new(0, 0)); // origin first
        assert_eq!(coords.len(), 9); // (2*1 + 1)^2
        assert!(coords.iter().all(|c| c.in_bounds(1)));

        let mut unique = coords.clone();
        unique.sort_by_key(|c| (c.x, c.y));
        unique.dedup();
        assert_eq!(unique.len(), 9);
    }
}
