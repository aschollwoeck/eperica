//! Account-lifecycle rules (019): beginner's protection and the inactivity → abandonment lifecycle.
//! Pure (P3) — no I/O. Time-based durations **scale by world speed** (P7); the abandonment sweep reuses
//! the medal period arithmetic ([`crate::medals::period_start`]) for a state-driven, reproducible cadence.

use crate::event::Timestamp;
use crate::medals::period_start;
use crate::units::scaled_time_secs;
use crate::world::GameSpeed;

/// Tunable lifecycle balance (P7). All `_secs` are **base** durations; the time-based ones are scaled by
/// world speed at use. `sweep_interval_secs` is the abandonment-sweep cadence (the period length).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleRules {
    /// Beginner's-protection window granted at spawn (base seconds, speed-scaled).
    pub beginner_protection_secs: i64,
    /// Population at which a protected player is "established" and protection ends early.
    pub protection_population_threshold: i64,
    /// Idle time after which a player is **inactive** (farmable / greyed) — base seconds, speed-scaled.
    pub inactive_after_secs: i64,
    /// Idle time after which a player is **abandoned** (villages freed) — base seconds, speed-scaled.
    pub abandon_after_secs: i64,
    /// The abandonment-sweep cadence / period length (real-time seconds).
    pub sweep_interval_secs: i64,
}

/// Whether a player is currently under beginner's protection: a protection instant exists and is still
/// in the future. (A `None` window — never granted or already ended — is unprotected.)
pub fn is_protected(protected_until: Option<Timestamp>, now: Timestamp) -> bool {
    matches!(protected_until, Some(t) if now.0 < t.0)
}

/// The instant beginner's protection should expire for a player spawning at `now` (speed-scaled, P7).
/// Timestamps are milliseconds; the scaled window is in seconds.
pub fn protection_expiry(now: Timestamp, base_secs: i64, speed: GameSpeed) -> Timestamp {
    Timestamp(now.0 + scaled_time_secs(base_secs, speed) * 1000)
}

/// Whether a player whose last activity was `last_activity` is **inactive** (farmable) at `now` — idle
/// longer than the speed-scaled `inactive_after_secs`.
pub fn is_inactive(
    last_activity: Timestamp,
    now: Timestamp,
    inactive_after_secs: i64,
    speed: GameSpeed,
) -> bool {
    now.0 - last_activity.0 > scaled_time_secs(inactive_after_secs, speed) * 1000
}

/// The activity cutoff for the abandonment sweep of period `P`: accounts whose `last_activity` is
/// **before** this instant are abandoned in period `P`. Anchored to the period boundary
/// (`period_start(P+1) − scaled(abandon_after_secs)`) so a given period always abandons the same set
/// from the same persisted activity data (P2/P6).
pub fn abandon_cutoff(
    period: i64,
    world_start: Timestamp,
    sweep_interval_secs: i64,
    abandon_after_secs: i64,
    speed: GameSpeed,
) -> Timestamp {
    let boundary = period_start(period + 1, world_start, sweep_interval_secs);
    Timestamp(boundary.0 - scaled_time_secs(abandon_after_secs, speed) * 1000)
}

/// Whether a protected player has grown enough that protection should end early (AC4).
pub fn protection_ended_by_population(population: i64, threshold: i64) -> bool {
    population >= threshold
}

#[cfg(test)]
mod tests {
    use super::*;

    fn speed(x: f64) -> GameSpeed {
        GameSpeed::new(x).unwrap()
    }

    #[test]
    fn protection_active_expired_and_never() {
        let now = Timestamp(10_000);
        assert!(
            is_protected(Some(Timestamp(10_001)), now),
            "future window ⇒ protected"
        );
        assert!(
            !is_protected(Some(Timestamp(10_000)), now),
            "equal instant ⇒ not protected"
        );
        assert!(
            !is_protected(Some(Timestamp(9_999)), now),
            "past window ⇒ not protected"
        );
        assert!(!is_protected(None, now), "no window ⇒ not protected");
    }

    #[test]
    fn protection_window_scales_with_speed() {
        let now = Timestamp(0);
        // base 1000s at 1× ⇒ 1000s; at 2× ⇒ 500s (faster server, shorter protection).
        assert_eq!(
            protection_expiry(now, 1000, speed(1.0)),
            Timestamp(1_000_000)
        );
        assert_eq!(protection_expiry(now, 1000, speed(2.0)), Timestamp(500_000));
    }

    #[test]
    fn inactivity_threshold_edge_and_scaling() {
        let last = Timestamp(0);
        // base 100s at 1×: idle of exactly 100s is not yet inactive; 100.001s is.
        assert!(!is_inactive(last, Timestamp(100_000), 100, speed(1.0)));
        assert!(is_inactive(last, Timestamp(100_001), 100, speed(1.0)));
        // at 2× the threshold halves to 50s.
        assert!(!is_inactive(last, Timestamp(50_000), 100, speed(2.0)));
        assert!(is_inactive(last, Timestamp(50_001), 100, speed(2.0)));
    }

    #[test]
    fn abandon_cutoff_anchored_to_period_boundary() {
        let world_start = Timestamp(0);
        // sweep interval 1000s ⇒ period 0 ends at 1000s; abandon_after 200s ⇒ cutoff = 1000s − 200s.
        let cut = abandon_cutoff(0, world_start, 1000, 200, speed(1.0));
        assert_eq!(cut, Timestamp(800_000));
        // period 1 ends at 2000s ⇒ cutoff = 2000s − 200s.
        assert_eq!(
            abandon_cutoff(1, world_start, 1000, 200, speed(1.0)),
            Timestamp(1_800_000)
        );
        // at 2× the abandon window halves (100s), so the cutoff moves later.
        assert_eq!(
            abandon_cutoff(0, world_start, 1000, 200, speed(2.0)),
            Timestamp(900_000)
        );
    }

    #[test]
    fn population_threshold_ends_protection() {
        assert!(!protection_ended_by_population(199, 200));
        assert!(protection_ended_by_population(200, 200));
        assert!(protection_ended_by_population(201, 200));
    }
}
