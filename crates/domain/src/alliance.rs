//! Alliances & diplomacy (GDD §10) — the pure group rules behind the social/political layer.
//!
//! An **alliance** groups players; each member holds an [`AllianceRole`] and (for leaders) a granular
//! [`RightSet`]. Eligibility to **join**/**found** is gated by the Embassy level (015 AC1) against
//! injected [`AllianceRules`] (P7). Between two alliances, a [`DiplomacyStance`] (war / confederation,
//! with neutral the absence) evolves through the [`next_stance`] state machine (AC7). Everything here is
//! **pure** over roles, rights, levels, counts, and balance — no I/O (P3). Persistence (who is in which
//! alliance, the normalised diplomacy pair) lives in infrastructure; authority is enforced in the
//! application from these predicates (P4).

/// Stable identity of an alliance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AllianceId(pub u128);

/// A member's rank within an alliance. Higher rank outranks lower (`Founder > Leader > Member`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllianceRole {
    /// Full control; exactly one per alliance. Implicitly holds every [`AllianceRight`].
    Founder,
    /// Holds an explicit, granular [`RightSet`] — only what was granted.
    Leader,
    /// Belongs to the alliance; no management rights.
    Member,
}

impl AllianceRole {
    /// Rank for outranking comparisons (`Founder` highest).
    #[must_use]
    fn rank(self) -> u8 {
        match self {
            AllianceRole::Founder => 2,
            AllianceRole::Leader => 1,
            AllianceRole::Member => 0,
        }
    }

    /// Whether `self` strictly outranks `other` (needed to expel / manage another member).
    #[must_use]
    pub fn outranks(self, other: AllianceRole) -> bool {
        self.rank() > other.rank()
    }
}

/// A granular management right held by a leader (the founder implicitly holds all of them).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllianceRight {
    /// Invite players and revoke pending invitations.
    Invite,
    /// Expel a lower-ranked member.
    Expel,
    /// Set / accept / cancel diplomacy stances.
    Diplomacy,
    /// Post the alliance announcement.
    Announce,
    /// Promote/demote leaders and grant/revoke their rights.
    ManageRoles,
}

impl AllianceRight {
    /// Every right, for iteration / `RightSet::all`.
    pub const ALL: [AllianceRight; 5] = [
        AllianceRight::Invite,
        AllianceRight::Expel,
        AllianceRight::Diplomacy,
        AllianceRight::Announce,
        AllianceRight::ManageRoles,
    ];

    /// The single bit this right occupies in a [`RightSet`].
    #[must_use]
    fn bit(self) -> u8 {
        1 << (self as u8)
    }
}

/// A compact set of [`AllianceRight`]s (a bitset persisted as a small integer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RightSet(u8);

impl RightSet {
    /// The empty set (an ordinary member / a leader with nothing granted yet).
    #[must_use]
    pub const fn empty() -> Self {
        RightSet(0)
    }

    /// Every right set (what the founder effectively holds).
    #[must_use]
    pub fn all() -> Self {
        AllianceRight::ALL
            .into_iter()
            .fold(RightSet(0), |s, r| s.with(r))
    }

    /// Rebuild from a persisted bit pattern (unknown high bits are ignored on read via [`Self::bits`]).
    #[must_use]
    pub const fn from_bits(bits: u8) -> Self {
        RightSet(bits)
    }

    /// The raw bit pattern, for persistence.
    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// Whether `right` is in the set.
    #[must_use]
    pub fn contains(self, right: AllianceRight) -> bool {
        self.0 & right.bit() != 0
    }

    /// A copy with `right` added.
    #[must_use]
    pub fn with(self, right: AllianceRight) -> Self {
        RightSet(self.0 | right.bit())
    }

    /// A copy with `right` removed.
    #[must_use]
    pub fn without(self, right: AllianceRight) -> Self {
        RightSet(self.0 & !right.bit())
    }
}

/// Whether a member with `role` + granted `rights` holds `right`. The founder holds every right
/// regardless of the bitset; a member holds none; a leader holds exactly what was granted (AC6).
#[must_use]
pub fn has_right(role: AllianceRole, rights: RightSet, right: AllianceRight) -> bool {
    match role {
        AllianceRole::Founder => true,
        AllianceRole::Leader => rights.contains(right),
        AllianceRole::Member => false,
    }
}

/// Whether an actor (`actor_role` + `actor_rights`) may **expel** a member of `target_role` (AC5/AC6):
/// they must hold the [`AllianceRight::Expel`] right **and** strictly outrank the target (so a leader
/// cannot expel the founder or a fellow leader; no one expels an equal). Expelling **self** is rejected
/// by identity in the application, not here.
#[must_use]
pub fn can_expel(
    actor_role: AllianceRole,
    actor_rights: RightSet,
    target_role: AllianceRole,
) -> bool {
    has_right(actor_role, actor_rights, AllianceRight::Expel) && actor_role.outranks(target_role)
}

/// Balance for alliances (P7): the membership cap and the Embassy levels that gate joining/founding.
#[derive(Debug, Clone)]
pub struct AllianceRules {
    /// Maximum members an alliance may hold (faithful default ~60).
    pub max_members: u32,
    /// Highest Embassy level (across a player's villages) required to **join** an alliance.
    pub join_embassy_level: u8,
    /// Highest Embassy level required to **found** an alliance.
    pub found_embassy_level: u8,
}

impl AllianceRules {
    /// Whether a player whose highest Embassy is `embassy_level` may **found** an alliance (AC1/AC2).
    #[must_use]
    pub fn can_found(&self, embassy_level: u8) -> bool {
        embassy_level >= self.found_embassy_level
    }

    /// Whether a player whose highest Embassy is `embassy_level` may **join** an alliance (AC1/AC3).
    #[must_use]
    pub fn can_join(&self, embassy_level: u8) -> bool {
        embassy_level >= self.join_embassy_level
    }

    /// Whether an alliance already holding `member_count` members is **at the cap** — a further join is
    /// rejected (AC4).
    #[must_use]
    pub fn at_cap(&self, member_count: u32) -> bool {
        member_count >= self.max_members
    }
}

/// A pairwise diplomatic stance between two alliances. **Neutral** is the *absence* of any stance
/// (`Option::None`), so it is not a variant here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiplomacyStance {
    /// Formal hostility — declared **unilaterally**, immediately mutual (AC7). Enables war statistics
    /// (016), not built here.
    War,
    /// Trusted allies — requires **mutual consent** (propose → accept) before it is `Active` (AC7).
    /// Grants the confederation visibility/defence effects (AC8/AC9).
    Confederation,
}

/// Whether a stance has taken effect or is still awaiting the counterpart's consent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiplomacyStatus {
    /// A confederation offer awaiting the other alliance's acceptance.
    Proposed,
    /// In effect (war is always `Active`; a confederation once accepted).
    Active,
}

/// The current stance for a pair: the stance + its status. `None` (Neutral) is represented outside this
/// type (an absent row / `Option::None`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiplomacyState {
    pub stance: DiplomacyStance,
    pub status: DiplomacyStatus,
}

/// An action a diplomacy-rights holder takes on a pair (AC7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiplomacyAction {
    /// Declare war — unilateral, immediate, overriding any confederation.
    DeclareWar,
    /// Offer a confederation (becomes `Proposed` until the counterpart accepts).
    ProposeConfederation,
    /// Accept a pending confederation proposal (makes it `Active`).
    AcceptConfederation,
    /// Cancel the current stance back to Neutral.
    Cancel,
}

/// Why a diplomacy transition is rejected. (The pure core defines its own errors — no external error
/// crate, P3.)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiplomacyError {
    /// A confederation cannot be proposed while the pair is at war (cancel the war first, AC7).
    WarBlocksConfederation,
    /// There is no pending confederation proposal to accept.
    NothingToAccept,
}

impl core::fmt::Display for DiplomacyError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            DiplomacyError::WarBlocksConfederation => write!(f, "cannot confederate while at war"),
            DiplomacyError::NothingToAccept => write!(f, "no confederation proposal to accept"),
        }
    }
}

impl std::error::Error for DiplomacyError {}

/// The pure diplomacy state machine (AC7): given the pair's `current` state (`None` = Neutral) and an
/// `action`, return the next state (`None` = Neutral). Transitions are **idempotent** (re-declaring the
/// active stance is a no-op, not an error); **war is unilateral** and overrides a confederation;
/// **confederation** needs propose → accept; **cancel** returns to Neutral. *Who* may accept (only the
/// counterpart, never the proposer) is an identity check enforced in the application, not here.
///
/// # Errors
/// [`DiplomacyError::WarBlocksConfederation`] when proposing a confederation while at war;
/// [`DiplomacyError::NothingToAccept`] when accepting with no pending proposal.
pub fn next_stance(
    current: Option<DiplomacyState>,
    action: DiplomacyAction,
) -> Result<Option<DiplomacyState>, DiplomacyError> {
    use DiplomacyAction as A;
    use DiplomacyStance::{Confederation, War};
    use DiplomacyStatus::{Active, Proposed};
    match action {
        // War is unilateral and immediate; it overrides any prior stance (exclusivity) and re-declaring
        // is a no-op.
        A::DeclareWar => Ok(Some(DiplomacyState {
            stance: War,
            status: Active,
        })),
        A::ProposeConfederation => match current {
            Some(DiplomacyState { stance: War, .. }) => Err(DiplomacyError::WarBlocksConfederation),
            // Already confederated (active or proposed) ⇒ idempotent no-op.
            Some(
                state @ DiplomacyState {
                    stance: Confederation,
                    ..
                },
            ) => Ok(Some(state)),
            None => Ok(Some(DiplomacyState {
                stance: Confederation,
                status: Proposed,
            })),
        },
        A::AcceptConfederation => match current {
            Some(DiplomacyState {
                stance: Confederation,
                status: Proposed,
            }) => Ok(Some(DiplomacyState {
                stance: Confederation,
                status: Active,
            })),
            // Accepting an already-active confederation is a harmless no-op.
            Some(
                state @ DiplomacyState {
                    stance: Confederation,
                    status: Active,
                },
            ) => Ok(Some(state)),
            _ => Err(DiplomacyError::NothingToAccept),
        },
        // Cancel is idempotent: any stance (or Neutral) → Neutral.
        A::Cancel => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // AC6: the rights truth table — founder holds all, member none, leader only what is granted.
    #[test]
    fn rights_follow_role_and_grant() {
        let none = RightSet::empty();
        let some = RightSet::empty()
            .with(AllianceRight::Invite)
            .with(AllianceRight::Diplomacy);
        // Founder holds every right regardless of the bitset.
        for r in AllianceRight::ALL {
            assert!(
                has_right(AllianceRole::Founder, none, r),
                "founder holds {r:?}"
            );
        }
        // Member holds none, whatever bits are set.
        for r in AllianceRight::ALL {
            assert!(!has_right(AllianceRole::Member, RightSet::all(), r));
        }
        // Leader holds exactly the granted bits.
        assert!(has_right(AllianceRole::Leader, some, AllianceRight::Invite));
        assert!(has_right(
            AllianceRole::Leader,
            some,
            AllianceRight::Diplomacy
        ));
        assert!(!has_right(AllianceRole::Leader, some, AllianceRight::Expel));
        assert!(!has_right(
            AllianceRole::Leader,
            some,
            AllianceRight::ManageRoles
        ));
    }

    #[test]
    fn rightset_roundtrips_through_bits() {
        let s = RightSet::empty()
            .with(AllianceRight::Expel)
            .with(AllianceRight::Announce);
        assert_eq!(RightSet::from_bits(s.bits()), s);
        assert!(
            s.without(AllianceRight::Expel)
                .contains(AllianceRight::Announce)
        );
        assert!(
            !s.without(AllianceRight::Expel)
                .contains(AllianceRight::Expel)
        );
        // `all()` contains every right and round-trips.
        let all = RightSet::all();
        assert!(AllianceRight::ALL.into_iter().all(|r| all.contains(r)));
    }

    // AC5/AC6: expel needs the Expel right AND a strictly higher rank.
    #[test]
    fn expel_needs_right_and_higher_rank() {
        let expel = RightSet::empty().with(AllianceRight::Expel);
        // Founder (implicit all-rights) can expel leaders and members.
        assert!(can_expel(
            AllianceRole::Founder,
            RightSet::empty(),
            AllianceRole::Leader
        ));
        assert!(can_expel(
            AllianceRole::Founder,
            RightSet::empty(),
            AllianceRole::Member
        ));
        // A leader with the right can expel a member but not a fellow leader or the founder.
        assert!(can_expel(AllianceRole::Leader, expel, AllianceRole::Member));
        assert!(!can_expel(
            AllianceRole::Leader,
            expel,
            AllianceRole::Leader
        ));
        assert!(!can_expel(
            AllianceRole::Leader,
            expel,
            AllianceRole::Founder
        ));
        // Without the right, a leader cannot expel anyone.
        assert!(!can_expel(
            AllianceRole::Leader,
            RightSet::empty(),
            AllianceRole::Member
        ));
        // A member never can.
        assert!(!can_expel(
            AllianceRole::Member,
            RightSet::all(),
            AllianceRole::Member
        ));
    }

    // AC1/AC2/AC3/AC4: the Embassy-level eligibility gates and the cap.
    #[test]
    fn eligibility_and_cap_gates() {
        let rules = AllianceRules {
            max_members: 60,
            join_embassy_level: 1,
            found_embassy_level: 3,
        };
        assert!(!rules.can_join(0) && !rules.can_found(0)); // no Embassy ⇒ neither
        assert!(rules.can_join(1) && !rules.can_found(1)); // L1 ⇒ join only
        assert!(rules.can_join(3) && rules.can_found(3)); // L3 ⇒ both
        assert!(!rules.at_cap(59));
        assert!(rules.at_cap(60)); // the 61st cannot join
        assert!(rules.at_cap(61));
    }

    // AC7: the diplomacy state machine.
    #[test]
    fn diplomacy_state_machine() {
        let war = DiplomacyState {
            stance: DiplomacyStance::War,
            status: DiplomacyStatus::Active,
        };
        let proposed = DiplomacyState {
            stance: DiplomacyStance::Confederation,
            status: DiplomacyStatus::Proposed,
        };
        let confed = DiplomacyState {
            stance: DiplomacyStance::Confederation,
            status: DiplomacyStatus::Active,
        };

        // War is unilateral & immediate from Neutral, and idempotent.
        assert_eq!(
            next_stance(None, DiplomacyAction::DeclareWar),
            Ok(Some(war))
        );
        assert_eq!(
            next_stance(Some(war), DiplomacyAction::DeclareWar),
            Ok(Some(war))
        );
        // War overrides a confederation (exclusivity).
        assert_eq!(
            next_stance(Some(confed), DiplomacyAction::DeclareWar),
            Ok(Some(war))
        );

        // Confederation: propose → accept; re-propose / re-accept idempotent.
        assert_eq!(
            next_stance(None, DiplomacyAction::ProposeConfederation),
            Ok(Some(proposed))
        );
        assert_eq!(
            next_stance(Some(proposed), DiplomacyAction::AcceptConfederation),
            Ok(Some(confed))
        );
        assert_eq!(
            next_stance(Some(confed), DiplomacyAction::ProposeConfederation),
            Ok(Some(confed))
        );
        assert_eq!(
            next_stance(Some(confed), DiplomacyAction::AcceptConfederation),
            Ok(Some(confed))
        );

        // Cannot confederate while at war; nothing to accept when not proposed.
        assert_eq!(
            next_stance(Some(war), DiplomacyAction::ProposeConfederation),
            Err(DiplomacyError::WarBlocksConfederation)
        );
        assert_eq!(
            next_stance(None, DiplomacyAction::AcceptConfederation),
            Err(DiplomacyError::NothingToAccept)
        );
        assert_eq!(
            next_stance(Some(war), DiplomacyAction::AcceptConfederation),
            Err(DiplomacyError::NothingToAccept)
        );

        // Cancel any stance (or Neutral) → Neutral, idempotent.
        assert_eq!(next_stance(Some(war), DiplomacyAction::Cancel), Ok(None));
        assert_eq!(next_stance(Some(confed), DiplomacyAction::Cancel), Ok(None));
        assert_eq!(next_stance(None, DiplomacyAction::Cancel), Ok(None));
    }
}
