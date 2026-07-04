//! Deterministic bot persona derived from username via inline FNV-1a 64-bit hash.
//!
//! `std::DefaultHasher` is per-process-random (since Rust 1.36) and therefore
//! unusable for AC4 determinism across process restarts.  We inline the FNV-1a
//! algorithm (64-bit variant) with a fixed offset basis and prime so the hash
//! is stable across all processes, platforms, and Rust versions.
//!
//! Persona fields are derived from a chain of FNV-1a hashes: each subsequent
//! hash re-hashes the previous hash's little-endian bytes, producing independent-
//! looking distributions for each field without reusing overlapping hash bits.

// ---------------------------------------------------------------------------
// FNV-1a 64-bit — inline, const-seeded, deterministic
// ---------------------------------------------------------------------------

/// FNV-1a 64-bit offset basis.
const FNV_OFFSET: u64 = 14_695_981_039_346_656_037;
/// FNV-1a 64-bit prime.
const FNV_PRIME: u64 = 1_099_511_628_211;

/// Inline FNV-1a 64-bit hash.  Deterministic, const-seeded, byte-loop only.
/// No std hash traits; no per-process randomness.
///
/// `pub(crate)` so `runner.rs` can seed its jitter from the same algorithm without
/// duplicating the constants.
pub(crate) fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut h = FNV_OFFSET;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(FNV_PRIME);
    }
    h
}

// ---------------------------------------------------------------------------
// Persona
// ---------------------------------------------------------------------------

/// Deterministic personality profile for one bot, derived from its username.
///
/// Every field is within the documented range; derivation uses an FNV-1a chain
/// so each trait is drawn from a different position in hash space.
#[derive(Debug, Clone, PartialEq)]
pub struct Persona {
    /// UTC hour (0–23) at which the activity window opens.
    pub window_start_hour: u8,
    /// Activity-window length in hours (8–16 inclusive).  May wrap past midnight.
    pub window_len_hours: u8,
    /// Minimum inter-tick interval in seconds.  Drawn from 180–719; always < tick_max_secs.
    pub tick_min_secs: u32,
    /// Maximum inter-tick interval in seconds.  Drawn from 720–900; always > tick_min_secs.
    pub tick_max_secs: u32,
    /// Aggression level: 0 = passive / builds only; 3 = raids aggressively.
    pub aggression: u8,
    /// Maximum raid radius in tiles, measured as Chebyshev distance.  5–10 inclusive.
    /// Bounded by the map-window clamp (docs/agent-api.md r=10 maximum).
    pub raid_range: u8,
}

impl Persona {
    /// Derive a deterministic `Persona` from `name`.
    ///
    /// Hash chain:
    ///   h0 = FNV-1a(name bytes)
    ///   h1 = FNV-1a(h0 as little-endian u64)
    ///   …h5
    ///
    /// Field derivation (each from a distinct h_N):
    /// - `window_start_hour` ← h0 % 24
    /// - `window_len_hours`  ← (h1 % 9) + 8   → 8..=16
    /// - `tick_min_secs`     ← (h2 % 540) + 180 → 180..=719  \
    /// - `tick_max_secs`     ← (h3 % 181) + 720 → 720..=900  / split guarantees min < max
    /// - `aggression`        ← h4 % 4           → 0..=3
    /// - `raid_range`        ← (h5 % 6) + 5     → 5..=10 — the map-window clamp bounds it
    pub fn from_name(name: &str) -> Self {
        let h0 = fnv1a_64(name.as_bytes());
        let h1 = fnv1a_64(&h0.to_le_bytes());
        let h2 = fnv1a_64(&h1.to_le_bytes());
        let h3 = fnv1a_64(&h2.to_le_bytes());
        let h4 = fnv1a_64(&h3.to_le_bytes());
        let h5 = fnv1a_64(&h4.to_le_bytes());

        Persona {
            window_start_hour: (h0 % 24) as u8,
            window_len_hours: ((h1 % 9) + 8) as u8,
            // tick_min drawn from [180, 719] and tick_max from [720, 900] so the
            // invariant min < max holds unconditionally without clamping.
            tick_min_secs: ((h2 % 540) + 180) as u32,
            tick_max_secs: ((h3 % 181) + 720) as u32,
            aggression: (h4 % 4) as u8,
            raid_range: ((h5 % 6) + 5) as u8,
        }
    }

    /// Returns `true` when `hour_utc` (0–23) falls inside this persona's activity window.
    ///
    /// Handles windows that wrap past midnight; e.g., start=22, len=6 covers
    /// hours 22, 23, 0, 1, 2, 3.
    pub fn in_window(&self, hour_utc: u8) -> bool {
        let start = self.window_start_hour as u32;
        let len = self.window_len_hours as u32;
        let hour = hour_utc as u32;

        if start + len <= 24 {
            // Non-wrapping window.
            hour >= start && hour < start + len
        } else {
            // Wrapping window: active [start, 24) ∪ [0, end).
            let end = (start + len) % 24;
            hour >= start || hour < end
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Pin exact personas for 3 fixed names using hardcoded literal values.
    ///
    /// Any change to the FNV-1a hash chain or derivation formula (field modulus, offset,
    /// cast) will cause this test to fail — the literals must be updated intentionally.
    /// Computed by running the inline FNV-1a reference outside the test harness.
    #[test]
    fn pinned_personas_match_reference() {
        let alpha = Persona::from_name("alpha");
        assert_eq!(alpha.window_start_hour, 3, "alpha window_start_hour");
        assert_eq!(alpha.window_len_hours, 14, "alpha window_len_hours");
        assert_eq!(alpha.tick_min_secs, 582, "alpha tick_min_secs");
        assert_eq!(alpha.tick_max_secs, 763, "alpha tick_max_secs");
        assert_eq!(alpha.aggression, 2, "alpha aggression");
        assert_eq!(alpha.raid_range, 7, "alpha raid_range");

        let bravo = Persona::from_name("bravo_bot");
        assert_eq!(bravo.window_start_hour, 9, "bravo_bot window_start_hour");
        assert_eq!(bravo.window_len_hours, 9, "bravo_bot window_len_hours");
        assert_eq!(bravo.tick_min_secs, 285, "bravo_bot tick_min_secs");
        assert_eq!(bravo.tick_max_secs, 791, "bravo_bot tick_max_secs");
        assert_eq!(bravo.aggression, 2, "bravo_bot aggression");
        assert_eq!(bravo.raid_range, 8, "bravo_bot raid_range");

        let charlie = Persona::from_name("charlie_42");
        assert_eq!(
            charlie.window_start_hour, 16,
            "charlie_42 window_start_hour"
        );
        assert_eq!(charlie.window_len_hours, 10, "charlie_42 window_len_hours");
        assert_eq!(charlie.tick_min_secs, 653, "charlie_42 tick_min_secs");
        assert_eq!(charlie.tick_max_secs, 819, "charlie_42 tick_max_secs");
        assert_eq!(charlie.aggression, 2, "charlie_42 aggression");
        assert_eq!(charlie.raid_range, 6, "charlie_42 raid_range");
    }

    /// Different names must produce different personas (probabilistic; three pairs).
    #[test]
    fn different_names_produce_different_personas() {
        let a = Persona::from_name("alpha");
        let b = Persona::from_name("beta");
        let c = Persona::from_name("gamma");
        // All three should differ on at least one field.
        assert_ne!(a, b, "alpha vs beta should differ");
        assert_ne!(b, c, "beta vs gamma should differ");
        assert_ne!(a, c, "alpha vs gamma should differ");
    }

    /// Same name must always produce the same persona (determinism).
    #[test]
    fn same_name_deterministic() {
        let p1 = Persona::from_name("determinism_test");
        let p2 = Persona::from_name("determinism_test");
        assert_eq!(p1, p2);
    }

    /// Sweep 200 generated names; every field must be within its documented range.
    #[test]
    fn all_fields_in_range_over_200_names() {
        for i in 0u32..200 {
            let name = format!("bot_{i:04}");
            let p = Persona::from_name(&name);
            assert!(
                p.window_start_hour < 24,
                "{name}: window_start_hour out of range"
            );
            assert!(
                p.window_len_hours >= 8 && p.window_len_hours <= 16,
                "{name}: window_len_hours {}",
                p.window_len_hours
            );
            assert!(
                p.tick_min_secs >= 180 && p.tick_min_secs <= 719,
                "{name}: tick_min_secs {}",
                p.tick_min_secs
            );
            assert!(
                p.tick_max_secs >= 720 && p.tick_max_secs <= 900,
                "{name}: tick_max_secs {}",
                p.tick_max_secs
            );
            assert!(
                p.tick_min_secs < p.tick_max_secs,
                "{name}: tick_min {} >= tick_max {}",
                p.tick_min_secs,
                p.tick_max_secs
            );
            assert!(p.aggression <= 3, "{name}: aggression out of range");
            assert!(
                p.raid_range >= 5 && p.raid_range <= 10,
                "{name}: raid_range {} (expected 5..=10)",
                p.raid_range
            );
        }
    }

    // -------------------------------------------------------------------------
    // in_window tests
    // -------------------------------------------------------------------------

    fn persona_with_window(start: u8, len: u8) -> Persona {
        Persona {
            window_start_hour: start,
            window_len_hours: len,
            tick_min_secs: 300,
            tick_max_secs: 720,
            aggression: 1,
            raid_range: 8,
        }
    }

    /// Non-wrapping window: 8–16 (start=8, len=8).
    #[test]
    fn window_non_wrapping_boundaries() {
        let p = persona_with_window(8, 8); // active 8..16
        assert!(p.in_window(8), "window start is inside");
        assert!(p.in_window(12), "mid-window");
        assert!(p.in_window(15), "last hour inside");
        assert!(!p.in_window(16), "first hour after window");
        assert!(!p.in_window(7), "just before window start");
        assert!(!p.in_window(0), "midnight outside");
        assert!(!p.in_window(23), "23:00 outside");
    }

    /// Non-wrapping full window of 16 hours: 0–16.
    #[test]
    fn window_non_wrapping_from_midnight() {
        let p = persona_with_window(0, 16); // active 0..16
        assert!(p.in_window(0));
        assert!(p.in_window(15));
        assert!(!p.in_window(16));
        assert!(!p.in_window(23));
    }

    /// Wrapping window: start=22, len=8 → active 22, 23, 0, 1, 2, 3, 4, 5.
    #[test]
    fn window_wrapping_around_midnight() {
        let p = persona_with_window(22, 8);
        // Inside: 22, 23, 0..=5
        for h in [22u8, 23, 0, 1, 2, 3, 4, 5] {
            assert!(
                p.in_window(h),
                "hour {h} should be inside wrap-around window"
            );
        }
        // Outside: 6..=21
        for h in 6u8..=21 {
            assert!(
                !p.in_window(h),
                "hour {h} should be outside wrap-around window"
            );
        }
    }

    /// Wrapping window: start=20, len=16 → active 20, 21, 22, 23, 0..=11.
    #[test]
    fn window_large_wrapping() {
        let p = persona_with_window(20, 16); // covers 20..24 + 0..12
        for h in [20u8, 21, 22, 23, 0, 5, 11] {
            assert!(p.in_window(h), "hour {h} should be inside");
        }
        for h in 12u8..=19 {
            assert!(!p.in_window(h), "hour {h} should be outside");
        }
    }
}
