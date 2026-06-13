//! Troop movement & travel time (GDD §8) — pure rules over the unit roster and world speed.
//!
//! A movement's travel time is `distance ÷ (effectiveSpeed × worldSpeed)`, where `effectiveSpeed`
//! is the **slowest** unit's map speed (so a slow unit paces the whole army). Distance is the
//! toroidal map distance (006). The engine that schedules and applies movements lives in the
//! application/infrastructure layers; this module only computes the timing.

use crate::units::{UnitId, UnitSpec};
use crate::world::GameSpeed;

/// What a movement does on arrival (this slice — non-combat).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MovementKind {
    /// Troops travel to another village and station there to defend it.
    Reinforce,
    /// Stationed troops travel home and rejoin the source garrison.
    Return,
    /// Troops travel to an enemy village to fight to destroy (009).
    Attack,
    /// Troops travel to an enemy village to fight to plunder (009).
    Raid,
    /// Scouts travel to spy on a village; espionage resolves separately, no main battle (010).
    Scout,
    /// Troops travel to an **oasis tile** to clear its animals and (if able) occupy it (012).
    OasisAttack,
    /// Troops travel to reinforce one of the owner's **own** occupied oases (012).
    OasisReinforce,
    /// Settlers travel to a free valley to **found** a new village (013).
    Settle,
}

/// The slowest map speed among the unit types present in `troops` (fields/hour), or `None` if the
/// composition is empty or none of its types are in the roster (GDD §8.1).
pub fn slowest_speed(troops: &[(UnitId, u32)], roster: &[UnitSpec]) -> Option<u32> {
    troops
        .iter()
        .filter(|(_, count)| *count > 0)
        .filter_map(|(unit, _)| roster.iter().find(|s| &s.id == unit).map(|s| s.speed))
        .min()
}

/// Travel time in seconds for a movement of distance `distance` tiles paced by `slowest_speed`
/// fields/hour on a world of `speed` (P7). Never below 1 second.
pub fn travel_time_secs(distance: f64, slowest_speed: u32, speed: GameSpeed) -> i64 {
    let fields_per_hour = f64::from(slowest_speed.max(1)) * speed.multiplier();
    ((distance / fields_per_hour) * 3600.0).round() as i64
}

/// Travel time, floored at 1 second (used everywhere a real movement is scheduled).
pub fn travel_time_secs_floored(distance: f64, slowest_speed: u32, speed: GameSpeed) -> i64 {
    travel_time_secs(distance, slowest_speed, speed).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::building::BuildingKind;
    use crate::economy::ResourceAmounts;
    use crate::units::UnitRole;

    fn unit(id: &str, speed: u32) -> UnitSpec {
        UnitSpec {
            id: UnitId(id.to_owned()),
            name: id.to_owned(),
            role: UnitRole::Infantry,
            attack: 1,
            defense_infantry: 1,
            defense_cavalry: 1,
            scouting: 0,
            speed,
            carry_capacity: 0,
            crop_upkeep: 1,
            point_value: 1,
            cost: ResourceAmounts {
                wood: 1,
                clay: 1,
                iron: 1,
                crop: 1,
            },
            train_secs: 1,
            trained_in: BuildingKind::Barracks,
            research: None,
            siege_kind: None,
        }
    }

    fn speed(m: f64) -> GameSpeed {
        GameSpeed::new(m).unwrap()
    }

    #[test]
    fn slowest_speed_picks_the_minimum_present_type() {
        let roster = vec![unit("inf", 6), unit("cav", 14), unit("ram", 4)];
        // Infantry + cavalry => paced by infantry (6).
        let mix = vec![(UnitId("inf".into()), 10), (UnitId("cav".into()), 5)];
        assert_eq!(slowest_speed(&mix, &roster), Some(6));
        // Adding a ram (4) slows the whole movement.
        let with_ram = vec![(UnitId("cav".into()), 5), (UnitId("ram".into()), 1)];
        assert_eq!(slowest_speed(&with_ram, &roster), Some(4));
        // Zero-count entries are ignored; an empty/unknown mix has no speed.
        assert_eq!(slowest_speed(&[(UnitId("inf".into()), 0)], &roster), None);
        assert_eq!(slowest_speed(&[], &roster), None);
    }

    #[test]
    fn travel_time_scales_with_distance_speed_and_pace() {
        // 6 fields/h, speed 1: 6 tiles take exactly 1 hour.
        assert_eq!(travel_time_secs(6.0, 6, speed(1.0)), 3600);
        // Twice the distance, twice the time.
        assert_eq!(travel_time_secs(12.0, 6, speed(1.0)), 7200);
        // A 2× world halves it (P7).
        assert_eq!(travel_time_secs(6.0, 6, speed(2.0)), 1800);
        // A slower unit (3 fields/h) doubles it.
        assert_eq!(travel_time_secs(6.0, 3, speed(1.0)), 7200);
    }

    #[test]
    fn travel_time_floors_at_one_second() {
        // A sub-second movement rounds to 0; the floored variant keeps it ≥ 1.
        // 0.0008 tiles ÷ 6 fields/h × 3600 = 0.48 s → rounds to 0.
        assert_eq!(travel_time_secs(0.000_8, 6, speed(1.0)), 0);
        assert_eq!(travel_time_secs_floored(0.000_8, 6, speed(1.0)), 1);
        assert_eq!(travel_time_secs_floored(0.0, 6, speed(1.0)), 1);
    }
}
