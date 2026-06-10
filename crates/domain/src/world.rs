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
}

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
}
