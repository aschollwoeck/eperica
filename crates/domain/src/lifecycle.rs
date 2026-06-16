//! Account-lifecycle rules (019): beginner's protection and the inactivity → abandonment lifecycle.
//! Pure (P3) — no I/O. **Beginner's protection** scales by world speed (P7); the **inactivity → abandonment**
//! windows are measured in **real wall-clock time** (054) — they gate on a real person's login recency
//! (`last_activity`), not in-game time, so a faster server does not shorten them. The abandonment sweep
//! reuses the medal period arithmetic ([`crate::medals::period_start`]) for a state-driven, reproducible
//! cadence.

use crate::event::Timestamp;
use crate::medals::period_start;
use crate::units::scaled_time_secs;
use crate::world::GameSpeed;

/// Tunable lifecycle balance (P7). `beginner_protection_secs` is a **base** duration scaled by world speed;
/// the inactivity/abandonment `_secs` are **real-time** wall-clock durations (054). `sweep_interval_secs`
/// is the abandonment-sweep cadence (the period length).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleRules {
    /// Beginner's-protection window granted at spawn (base seconds, speed-scaled).
    pub beginner_protection_secs: i64,
    /// Population at which a protected player is "established" and protection ends early.
    pub protection_population_threshold: i64,
    /// Idle time after which a player is **inactive** (farmable / greyed) — **real-time** seconds (054).
    pub inactive_after_secs: i64,
    /// Idle time after which a player is **abandoned** (villages freed) — **real-time** seconds (054).
    pub abandon_after_secs: i64,
    /// The abandonment-sweep cadence / period length (real-time seconds).
    pub sweep_interval_secs: i64,
    /// Presence "online" window (real-time seconds): active within this ⇒ shown online (025, P7).
    pub presence_online_secs: i64,
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
/// longer than `inactive_after_secs` in **real wall-clock time** (054). Not speed-scaled: a real person's
/// absence is measured against the real clock, so a faster server does not grey them sooner.
pub fn is_inactive(last_activity: Timestamp, now: Timestamp, inactive_after_secs: i64) -> bool {
    now.0 - last_activity.0 > inactive_after_secs * 1000
}

/// The activity cutoff for the abandonment sweep of period `P`: accounts whose `last_activity` is
/// **before** this instant are abandoned in period `P`. Anchored to the period boundary
/// (`period_start(P+1) − abandon_after_secs`) so a given period always abandons the same set from the same
/// persisted activity data (P2/P6). The window is **real-time** (054) — not speed-scaled.
pub fn abandon_cutoff(
    period: i64,
    world_start: Timestamp,
    sweep_interval_secs: i64,
    abandon_after_secs: i64,
) -> Timestamp {
    let boundary = period_start(period + 1, world_start, sweep_interval_secs);
    Timestamp(boundary.0 - abandon_after_secs * 1000)
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
    fn inactivity_threshold_edge_is_real_time() {
        let last = Timestamp(0);
        // 100s real: idle of exactly 100s is not yet inactive; 100.001s is.
        assert!(!is_inactive(last, Timestamp(100_000), 100));
        assert!(is_inactive(last, Timestamp(100_001), 100));
    }

    #[test]
    fn abandon_cutoff_anchored_to_period_boundary() {
        let world_start = Timestamp(0);
        // sweep interval 1000s ⇒ period 0 ends at 1000s; abandon_after 200s ⇒ cutoff = 1000s − 200s.
        let cut = abandon_cutoff(0, world_start, 1000, 200);
        assert_eq!(cut, Timestamp(800_000));
        // period 1 ends at 2000s ⇒ cutoff = 2000s − 200s.
        assert_eq!(
            abandon_cutoff(1, world_start, 1000, 200),
            Timestamp(1_800_000)
        );
    }

    /// 054: inactivity and abandonment are measured in real wall-clock time — world speed must not change
    /// them (unlike beginner protection, which still scales). These take no `speed`, so the property is
    /// structural; this guards the intent against a regression that re-introduces scaling.
    #[test]
    fn inactivity_and_abandonment_are_speed_independent() {
        let last = Timestamp(0);
        // 7 "days" (in this test's units) of idle is inactive regardless of any world speed — the function
        // has no speed input, so the same real elapsed time always yields the same verdict.
        assert!(is_inactive(last, Timestamp(700_001), 700));
        assert!(!is_inactive(last, Timestamp(699_999), 700));
        // The abandon cutoff depends only on real-time inputs (period, sweep interval, abandon window).
        assert_eq!(
            abandon_cutoff(0, Timestamp(0), 1000, 200),
            Timestamp(800_000)
        );
        // Protection, by contrast, DOES still scale with speed (019 unchanged) — see
        // `protection_window_scales_with_speed`.
    }

    #[test]
    fn population_threshold_ends_protection() {
        assert!(!protection_ended_by_population(199, 200));
        assert!(protection_ended_by_population(200, 200));
        assert!(protection_ended_by_population(201, 200));
    }
}
