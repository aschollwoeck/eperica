//! Fair-play & anti-cheat rules (022, GDD §12.5): the sanction state machine, report reasons, and the
//! reproducible detection predicates. Pure (P3) — no I/O.
//!
//! Sanction state is **computed on read** (P1): whether an account is blocked *now* is a function of its
//! `banned_at`/`suspended_until` and the current time, so a suspension simply expires with no sweep.
//! Detection predicates turn a reproducible count (P2/P6) into an advisory flag against a config
//! threshold (P7) — they never sanction on their own.

use crate::event::Timestamp;

/// A moderator action on an account (022 AC4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SanctionKind {
    /// A recorded warning — no block.
    Warn,
    /// A temporary block until a timestamp (the default window is config, P7).
    Suspend,
    /// A permanent block.
    Ban,
}

impl SanctionKind {
    /// The persisted/form string for this kind.
    pub fn as_str(self) -> &'static str {
        match self {
            SanctionKind::Warn => "warn",
            SanctionKind::Suspend => "suspend",
            SanctionKind::Ban => "ban",
        }
    }

    /// Parse a sanction kind from its string, or `None` if unknown.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "warn" => Some(SanctionKind::Warn),
            "suspend" => Some(SanctionKind::Suspend),
            "ban" => Some(SanctionKind::Ban),
            _ => None,
        }
    }
}

/// Why a player reported an account (022 AC2) — a fixed set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportReason {
    /// Pushing / running several accounts.
    Pushing,
    /// Bot / automation.
    Botting,
    /// Abuse / harassment.
    Abuse,
    /// Anything else (see the note).
    Other,
}

impl ReportReason {
    /// The persisted/form string for this reason.
    pub fn as_str(self) -> &'static str {
        match self {
            ReportReason::Pushing => "pushing",
            ReportReason::Botting => "botting",
            ReportReason::Abuse => "abuse",
            ReportReason::Other => "other",
        }
    }

    /// Parse a report reason from its string, or `None` if unknown.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "pushing" => Some(ReportReason::Pushing),
            "botting" => Some(ReportReason::Botting),
            "abuse" => Some(ReportReason::Abuse),
            "other" => Some(ReportReason::Other),
            _ => None,
        }
    }
}

/// Whether an account is **blocked now** (022 AC5): a ban always blocks; a suspension blocks until its
/// instant passes. The single source of truth for both the login block and the action guard (P1/P4).
pub fn account_blocked(
    banned_at: Option<Timestamp>,
    suspended_until: Option<Timestamp>,
    now: Timestamp,
) -> bool {
    banned_at.is_some() || suspended_until.is_some_and(|until| now.0 < until.0)
}

/// Tunable fair-play limits + detection thresholds (022, P7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FairPlayRules {
    /// Max mutating actions per window for one player before rejection (429).
    pub rate_limit_per_window: u32,
    /// The rate-limit window length (seconds). Wall-clock — abuse is real-time, not speed-scaled.
    pub rate_window_secs: i64,
    /// Max login attempts per window for one IP (brute-force guard).
    pub login_limit_per_window: u32,
    /// Default suspension length (seconds) when a moderator suspends without an explicit window.
    pub suspend_default_secs: i64,
    /// Accounts sharing a registration IP at/above this count raise the shared-IP signal.
    pub ip_association_threshold: u32,
    /// A windowed action count at/above this raises the inhuman-action-rate signal.
    pub inhuman_rate_threshold: u32,
}

/// Whether the shared-registration-IP signal is raised for an `association_count` (022 AC7).
pub fn shared_ip_flagged(association_count: u32, rules: &FairPlayRules) -> bool {
    association_count >= rules.ip_association_threshold
}

/// Whether the inhuman-action-rate signal is raised for the peak windowed action count (022 AC7).
pub fn inhuman_action_rate(max_window_count: u32, rules: &FairPlayRules) -> bool {
    max_window_count >= rules.inhuman_rate_threshold
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> FairPlayRules {
        FairPlayRules {
            rate_limit_per_window: 30,
            rate_window_secs: 60,
            login_limit_per_window: 10,
            suspend_default_secs: 86_400,
            ip_association_threshold: 3,
            inhuman_rate_threshold: 100,
        }
    }

    #[test]
    fn block_transitions() {
        let now = Timestamp(1_000_000);
        // None ⇒ not blocked.
        assert!(!account_blocked(None, None, now));
        // Banned ⇒ always blocked, regardless of suspension.
        assert!(account_blocked(Some(Timestamp(1)), None, now));
        // Suspended into the future ⇒ blocked; expired ⇒ not.
        assert!(account_blocked(None, Some(Timestamp(now.0 + 1)), now));
        assert!(!account_blocked(None, Some(Timestamp(now.0 - 1)), now));
        // The boundary: a suspension exactly at `now` has expired (now < until is false).
        assert!(!account_blocked(None, Some(now), now));
    }

    #[test]
    fn detection_predicates_trip_at_threshold() {
        let r = rules();
        assert!(!shared_ip_flagged(2, &r));
        assert!(shared_ip_flagged(3, &r));
        assert!(!inhuman_action_rate(99, &r));
        assert!(inhuman_action_rate(100, &r));
    }

    #[test]
    fn string_round_trips() {
        for k in [SanctionKind::Warn, SanctionKind::Suspend, SanctionKind::Ban] {
            assert_eq!(SanctionKind::parse(k.as_str()), Some(k));
        }
        assert_eq!(SanctionKind::parse("nope"), None);
        for r in [
            ReportReason::Pushing,
            ReportReason::Botting,
            ReportReason::Abuse,
            ReportReason::Other,
        ] {
            assert_eq!(ReportReason::parse(r.as_str()), Some(r));
        }
        assert_eq!(ReportReason::parse("nope"), None);
    }
}
