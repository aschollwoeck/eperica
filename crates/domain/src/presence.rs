//! Player presence + profile-bio rules (025). Pure (P3) — no I/O.
//!
//! Presence is a reproducible read of the 019 `last_activity` signal vs. the current time against a config
//! window (P1/P2/P7) — never a stored flag. It is **wall-clock** (real-time): how recently a human was at
//! the keyboard does not scale with game speed.

use crate::event::Timestamp;

/// Max length (characters) of a profile bio.
pub const MAX_BIO: usize = 500;

/// A player's presence, derived from their `last_activity`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    /// Active within the online window.
    Online,
    /// Not currently online; last active at this instant.
    LastSeen(Timestamp),
}

/// Compute presence: **Online** iff `now − last_activity ≤ online_window_secs`, else **LastSeen**
/// (025 AC3). Wall-clock; deterministic from persisted state.
pub fn presence(last_activity: Timestamp, now: Timestamp, online_window_secs: i64) -> Presence {
    if now.0 - last_activity.0 <= online_window_secs.max(0) * 1000 {
        Presence::Online
    } else {
        Presence::LastSeen(last_activity)
    }
}

/// Whether a profile `bio` is valid to save (025 AC1) — within [`MAX_BIO`] characters. An empty bio is
/// allowed (clearing the bio).
pub fn valid_bio(bio: &str) -> bool {
    bio.trim().chars().count() <= MAX_BIO
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presence_online_window_boundary() {
        let now = Timestamp(10_000_000);
        let window = 600; // 10 minutes
        // Exactly at the window edge ⇒ still online.
        assert_eq!(
            presence(Timestamp(now.0 - window * 1000), now, window),
            Presence::Online
        );
        // Just inside ⇒ online.
        assert_eq!(
            presence(Timestamp(now.0 - 1000), now, window),
            Presence::Online
        );
        // Just past ⇒ last seen.
        let stale = Timestamp(now.0 - window * 1000 - 1);
        assert_eq!(presence(stale, now, window), Presence::LastSeen(stale));
    }

    #[test]
    fn bio_bounds() {
        assert!(valid_bio(""));
        assert!(valid_bio("Founder of the Iron Pact."));
        assert!(valid_bio(&"x".repeat(MAX_BIO)));
        assert!(!valid_bio(&"x".repeat(MAX_BIO + 1)));
    }
}
