//! Account-sitting rules (030). Pure (P3) — no I/O.
//!
//! An owner authorises trusted players to operate their account; the *who* is persisted, but the simple
//! eligibility rule (not yourself, under the cap) lives here.

use crate::village::PlayerId;

/// Maximum number of sitters an owner may authorise at once.
pub const MAX_SITTERS: usize = 3;

/// Whether `owner` may grant `target` as a sitter (030 AC1): a different player, and not past
/// [`MAX_SITTERS`] given the owner's `current_count`.
pub fn can_grant_sitter(owner: PlayerId, target: PlayerId, current_count: usize) -> bool {
    owner != target && current_count < MAX_SITTERS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cannot_grant_self() {
        assert!(!can_grant_sitter(PlayerId(1), PlayerId(1), 0));
    }

    #[test]
    fn respects_the_cap() {
        assert!(can_grant_sitter(PlayerId(1), PlayerId(2), MAX_SITTERS - 1));
        assert!(!can_grant_sitter(PlayerId(1), PlayerId(2), MAX_SITTERS));
        assert!(!can_grant_sitter(PlayerId(1), PlayerId(2), MAX_SITTERS + 1));
    }

    #[test]
    fn allows_a_distinct_player_under_cap() {
        assert!(can_grant_sitter(PlayerId(1), PlayerId(2), 0));
    }
}
