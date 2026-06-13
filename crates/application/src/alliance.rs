//! Alliance use-cases (015) — found / invite / accept / leave / expel / roles, the founder transfer,
//! disband, and the [`set_diplomacy`] stance machine. Each loads the actor's membership, runs the
//! **pure** domain role/rights/eligibility/diplomacy checks ([`eperica_domain::alliance`]), then calls
//! the [`AllianceRepository`]. Authority is enforced here, server-side (P4).

use crate::ports::{
    AllianceRepository, AlliedVillage, DiplomacyEntry, IncomingAttack, Membership, RepoError,
    RosterEntry,
};
use eperica_domain::AllianceRight;
use eperica_domain::{
    AllianceId, AllianceRole, AllianceRules, DiplomacyAction, DiplomacyError, DiplomacyStance,
    DiplomacyState, DiplomacyStatus, PlayerId, RightSet, VillageId, can_expel, has_right,
    next_stance,
};

/// Why an alliance command is rejected (server-enforced, P4).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AllianceError {
    /// The actor is not in any alliance (or not in the one they tried to act on).
    #[error("not a member of this alliance")]
    NotAMember,
    /// The actor is already in an alliance (cannot found/join another).
    #[error("already in an alliance")]
    AlreadyInAlliance,
    /// The target player to invite/act on is already in an alliance.
    #[error("that player is already in an alliance")]
    TargetInAlliance,
    /// The Embassy level is too low to found/join (AC1).
    #[error("Embassy level too low")]
    NotEligible,
    /// The alliance name or tag is already taken (AC2).
    #[error("that alliance name or tag is taken")]
    NameOrTagTaken,
    /// The actor lacks the right required for this action (AC6).
    #[error("you lack the right for this action")]
    MissingRight,
    /// The actor does not outrank the target (expel/manage — AC5/AC6).
    #[error("you do not outrank that member")]
    RankTooLow,
    /// The action cannot target oneself (expel/transfer).
    #[error("cannot target yourself")]
    SelfTarget,
    /// The alliance is at its membership cap (AC4).
    #[error("the alliance is full")]
    AtCap,
    /// There is no pending invitation to accept/decline.
    #[error("no pending invitation")]
    NoInvite,
    /// The invitee already has a pending invitation from this alliance.
    #[error("that player is already invited")]
    AlreadyInvited,
    /// The founder cannot leave without transferring or disbanding first (AC5).
    #[error("the founder must transfer or disband before leaving")]
    FounderMustTransfer,
    /// The target is not a member of the actor's alliance.
    #[error("that player is not in your alliance")]
    TargetNotInAlliance,
    /// A role change tried to set an invalid role (e.g. a second Founder — use transfer instead).
    #[error("invalid role assignment")]
    BadRole,
    /// An alliance cannot set diplomacy with itself (AC7).
    #[error("cannot set diplomacy with your own alliance")]
    SelfDiplomacy,
    /// Only the *other* alliance may accept a confederation proposal — not the side that offered it.
    #[error("cannot accept your own confederation proposal")]
    CannotAcceptOwnProposal,
    /// A rejected diplomacy transition (AC7).
    #[error(transparent)]
    Diplomacy(#[from] DiplomacyError),
    /// A backend/storage failure.
    #[error("storage error: {0}")]
    Backend(String),
}

impl From<RepoError> for AllianceError {
    fn from(e: RepoError) -> Self {
        match e {
            RepoError::Backend(m) => AllianceError::Backend(m),
            other => AllianceError::Backend(other.to_string()),
        }
    }
}

/// Found a new alliance: the actor must be alliance-less and hold Embassy ≥ found level (AC1/AC2).
///
/// # Errors
/// [`AllianceError::AlreadyInAlliance`], [`AllianceError::NotEligible`],
/// [`AllianceError::NameOrTagTaken`], or a backend error.
pub async fn found_alliance<R: AllianceRepository>(
    repo: &R,
    rules: &AllianceRules,
    founder: PlayerId,
    name: &str,
    tag: &str,
) -> Result<AllianceId, AllianceError> {
    if repo.alliance_of(founder).await?.is_some() {
        return Err(AllianceError::AlreadyInAlliance);
    }
    if !rules.can_found(repo.max_embassy_level(founder).await?) {
        return Err(AllianceError::NotEligible);
    }
    match repo.create_alliance(name, tag, founder).await {
        Ok(id) => Ok(id),
        Err(RepoError::Duplicate) => Err(AllianceError::NameOrTagTaken),
        Err(e) => Err(e.into()),
    }
}

/// Load the actor's membership, requiring they hold `right` (AC6). Returns the membership for further
/// checks.
async fn require_right<R: AllianceRepository>(
    repo: &R,
    actor: PlayerId,
    right: AllianceRight,
) -> Result<crate::ports::Membership, AllianceError> {
    let m = repo
        .alliance_of(actor)
        .await?
        .ok_or(AllianceError::NotAMember)?;
    if !has_right(m.role, m.rights, right) {
        return Err(AllianceError::MissingRight);
    }
    Ok(m)
}

/// Invite a player to the actor's alliance (needs the Invite right, AC3). The invitee must be
/// alliance-less.
///
/// # Errors
/// [`AllianceError::NotAMember`], [`AllianceError::MissingRight`],
/// [`AllianceError::TargetInAlliance`], [`AllianceError::AlreadyInvited`], or a backend error.
pub async fn invite_player<R: AllianceRepository>(
    repo: &R,
    actor: PlayerId,
    invitee: PlayerId,
) -> Result<(), AllianceError> {
    let m = require_right(repo, actor, AllianceRight::Invite).await?;
    if repo.alliance_of(invitee).await?.is_some() {
        return Err(AllianceError::TargetInAlliance);
    }
    match repo.insert_invite(m.alliance, invitee).await {
        Ok(()) => Ok(()),
        Err(RepoError::Duplicate) => Err(AllianceError::AlreadyInvited),
        Err(e) => Err(e.into()),
    }
}

/// Revoke a pending invitation the actor's alliance sent (needs the Invite right, AC3).
///
/// # Errors
/// [`AllianceError::NotAMember`], [`AllianceError::MissingRight`], or a backend error.
pub async fn revoke_invite<R: AllianceRepository>(
    repo: &R,
    actor: PlayerId,
    invitee: PlayerId,
) -> Result<(), AllianceError> {
    let m = require_right(repo, actor, AllianceRight::Invite).await?;
    repo.delete_invite(m.alliance, invitee).await?;
    Ok(())
}

/// Accept or decline an invitation to `alliance` (AC3). Accepting is gated on the invitee still being
/// alliance-less, holding Embassy ≥ join level, and the alliance being below the cap.
///
/// # Errors
/// [`AllianceError::NoInvite`], [`AllianceError::AlreadyInAlliance`], [`AllianceError::NotEligible`],
/// [`AllianceError::AtCap`], or a backend error.
pub async fn respond_invite<R: AllianceRepository>(
    repo: &R,
    rules: &AllianceRules,
    invitee: PlayerId,
    alliance: AllianceId,
    accept: bool,
) -> Result<(), AllianceError> {
    if !repo.has_invite(alliance, invitee).await? {
        return Err(AllianceError::NoInvite);
    }
    if !accept {
        repo.delete_invite(alliance, invitee).await?;
        return Ok(());
    }
    if repo.alliance_of(invitee).await?.is_some() {
        return Err(AllianceError::AlreadyInAlliance);
    }
    if !rules.can_join(repo.max_embassy_level(invitee).await?) {
        return Err(AllianceError::NotEligible);
    }
    // The guarded insert re-checks the cap atomically; the player_id PK re-checks "already in one".
    match repo
        .add_member(
            alliance,
            invitee,
            AllianceRole::Member,
            RightSet::empty(),
            rules.max_members,
        )
        .await
    {
        Ok(()) => {
            repo.delete_invite(alliance, invitee).await?;
            Ok(())
        }
        Err(RepoError::Conflict) => Err(AllianceError::AtCap),
        Err(RepoError::Duplicate) => Err(AllianceError::AlreadyInAlliance),
        Err(e) => Err(e.into()),
    }
}

/// Leave the actor's alliance. The founder must transfer or disband first (AC5).
///
/// # Errors
/// [`AllianceError::NotAMember`], [`AllianceError::FounderMustTransfer`], or a backend error.
pub async fn leave_alliance<R: AllianceRepository>(
    repo: &R,
    actor: PlayerId,
) -> Result<(), AllianceError> {
    let m = repo
        .alliance_of(actor)
        .await?
        .ok_or(AllianceError::NotAMember)?;
    if m.role == AllianceRole::Founder {
        return Err(AllianceError::FounderMustTransfer);
    }
    repo.remove_member(actor).await?;
    Ok(())
}

/// Expel a lower-ranked member (needs the Expel right + a strictly higher rank, AC5/AC6).
///
/// # Errors
/// [`AllianceError::NotAMember`], [`AllianceError::SelfTarget`],
/// [`AllianceError::TargetNotInAlliance`], [`AllianceError::MissingRight`]/[`RankTooLow`], or backend.
pub async fn expel_member<R: AllianceRepository>(
    repo: &R,
    actor: PlayerId,
    target: PlayerId,
) -> Result<(), AllianceError> {
    if actor == target {
        return Err(AllianceError::SelfTarget);
    }
    let me = repo
        .alliance_of(actor)
        .await?
        .ok_or(AllianceError::NotAMember)?;
    let them = repo
        .alliance_of(target)
        .await?
        .filter(|t| t.alliance == me.alliance)
        .ok_or(AllianceError::TargetNotInAlliance)?;
    if !can_expel(me.role, me.rights, them.role) {
        // Distinguish "no right" from "outranked" for a clearer message.
        return Err(if has_right(me.role, me.rights, AllianceRight::Expel) {
            AllianceError::RankTooLow
        } else {
            AllianceError::MissingRight
        });
    }
    repo.remove_member(target).await?;
    Ok(())
}

/// Set a member's role + rights (promote/demote, grant/revoke — AC6). The actor needs the ManageRoles
/// right and must strictly outrank **both** the target's current role and the new role; the new role
/// may not be Founder (use [`transfer_founder`]).
///
/// # Errors
/// [`AllianceError`] variants for the failed guard, or a backend error.
pub async fn set_member_role<R: AllianceRepository>(
    repo: &R,
    actor: PlayerId,
    target: PlayerId,
    new_role: AllianceRole,
    new_rights: RightSet,
) -> Result<(), AllianceError> {
    if actor == target {
        return Err(AllianceError::SelfTarget);
    }
    if new_role == AllianceRole::Founder {
        return Err(AllianceError::BadRole);
    }
    let me = require_right(repo, actor, AllianceRight::ManageRoles).await?;
    let them = repo
        .alliance_of(target)
        .await?
        .filter(|t| t.alliance == me.alliance)
        .ok_or(AllianceError::TargetNotInAlliance)?;
    // No privilege escalation: outrank the target now and the role being granted (so only a Founder can
    // mint Leaders; a ManageRoles leader can only adjust members).
    if !(me.role.outranks(them.role) && me.role.outranks(new_role)) {
        return Err(AllianceError::RankTooLow);
    }
    // A plain member holds no rights regardless of the bitset.
    let rights = if new_role == AllianceRole::Member {
        RightSet::empty()
    } else {
        new_rights
    };
    repo.set_member_role(me.alliance, target, new_role, rights)
        .await?;
    Ok(())
}

/// Transfer the founder role to another member of the actor's alliance (AC5/AC6). The actor must be the
/// Founder; the target must be a member of the same alliance and not the actor.
///
/// # Errors
/// [`AllianceError`] variants for the failed guard, or a backend error.
pub async fn transfer_founder<R: AllianceRepository>(
    repo: &R,
    actor: PlayerId,
    target: PlayerId,
) -> Result<(), AllianceError> {
    if actor == target {
        return Err(AllianceError::SelfTarget);
    }
    let me = repo
        .alliance_of(actor)
        .await?
        .ok_or(AllianceError::NotAMember)?;
    if me.role != AllianceRole::Founder {
        return Err(AllianceError::MissingRight);
    }
    repo.alliance_of(target)
        .await?
        .filter(|t| t.alliance == me.alliance)
        .ok_or(AllianceError::TargetNotInAlliance)?;
    repo.transfer_founder(me.alliance, actor, target).await?;
    Ok(())
}

/// Disband the actor's alliance (Founder only, AC5). Cascades to members, invitations, and diplomacy.
///
/// # Errors
/// [`AllianceError::NotAMember`], [`AllianceError::MissingRight`], or a backend error.
pub async fn disband_alliance<R: AllianceRepository>(
    repo: &R,
    actor: PlayerId,
) -> Result<(), AllianceError> {
    let me = repo
        .alliance_of(actor)
        .await?
        .ok_or(AllianceError::NotAMember)?;
    if me.role != AllianceRole::Founder {
        return Err(AllianceError::MissingRight);
    }
    repo.disband(me.alliance).await?;
    Ok(())
}

/// A diplomacy action a rights-holder takes toward another alliance (AC7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiplomacyCommand {
    /// Declare war — unilateral, immediate, overrides any confederation.
    DeclareWar,
    /// Offer a confederation (pending until the other side accepts).
    ProposeConfederation,
    /// Accept a confederation the other side proposed.
    AcceptConfederation,
    /// Cancel the current stance back to Neutral.
    Cancel,
}

/// Set the diplomacy stance between the actor's alliance and `other` (AC7). The actor needs the
/// Diplomacy right; the pair cannot be the same alliance; only the **counterpart** may accept a pending
/// confederation. The pure [`next_stance`] machine decides the transition; the repository stores the
/// normalised pair.
///
/// # Errors
/// [`AllianceError`] variants for the failed guard / [`DiplomacyError`], or a backend error.
pub async fn set_diplomacy<R: AllianceRepository>(
    repo: &R,
    actor: PlayerId,
    other: AllianceId,
    command: DiplomacyCommand,
) -> Result<(), AllianceError> {
    let me = require_right(repo, actor, AllianceRight::Diplomacy).await?;
    if me.alliance == other {
        return Err(AllianceError::SelfDiplomacy);
    }
    let state = repo.diplomacy_state(me.alliance, other).await?;
    let current = state.map(|(stance, status, _)| DiplomacyState { stance, status });
    let action = match command {
        DiplomacyCommand::DeclareWar => DiplomacyAction::DeclareWar,
        DiplomacyCommand::ProposeConfederation => DiplomacyAction::ProposeConfederation,
        DiplomacyCommand::Cancel => DiplomacyAction::Cancel,
        DiplomacyCommand::AcceptConfederation => {
            // Only the side that did *not* propose may accept.
            if let Some((DiplomacyStance::Confederation, DiplomacyStatus::Proposed, Some(by))) =
                state
                && by == me.alliance
            {
                return Err(AllianceError::CannotAcceptOwnProposal);
            }
            DiplomacyAction::AcceptConfederation
        }
    };
    match next_stance(current, action)? {
        Some(next) => {
            // A pending confederation records its proposer (the actor's alliance); everything else clears
            // the proposer (war, or an active confederation).
            let proposed_by = match (next.stance, next.status) {
                (DiplomacyStance::Confederation, DiplomacyStatus::Proposed) => Some(me.alliance),
                _ => None,
            };
            repo.set_diplomacy_state(me.alliance, other, next.stance, next.status, proposed_by)
                .await?;
        }
        None => repo.clear_diplomacy(me.alliance, other).await?,
    }
    Ok(())
}

/// The viewer's full alliance page (015 AC8/AC9/AC11), all scoped to **their** alliance + its one-hop
/// confederates — the visibility gate is structural (a viewer only ever sees their own set).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllianceOverview {
    /// The viewer's own membership (alliance + role + rights).
    pub membership: Membership,
    /// The alliance's name.
    pub name: String,
    /// The alliance's tag.
    pub tag: String,
    /// The viewer's alliance roster (members + roles/rights).
    pub roster: Vec<RosterEntry>,
    /// The viewer's alliance's diplomacy relationships.
    pub diplomacy: Vec<DiplomacyEntry>,
    /// Villages of fellow members **and** confederates — the shared village list (coords + names).
    pub allied_villages: Vec<AlliedVillage>,
    /// Incoming hostile movements against any allied village (target + ETA only).
    pub incoming: Vec<IncomingAttack>,
}

/// Assemble the viewer's alliance page (AC8/AC9/AC11), or `None` if they are in no alliance. Visibility
/// is gated structurally: only the viewer's alliance + its **active, one-hop** confederations are read.
///
/// # Errors
/// A backend error.
pub async fn alliance_view<R: AllianceRepository>(
    repo: &R,
    viewer: PlayerId,
) -> Result<Option<AllianceOverview>, AllianceError> {
    let Some(membership) = repo.alliance_of(viewer).await? else {
        return Ok(None);
    };
    let (name, tag) = repo
        .alliance_summary(membership.alliance)
        .await?
        .unwrap_or_default();
    let mut allied = vec![membership.alliance];
    allied.extend(repo.confederate_alliances(membership.alliance).await?);
    let roster = repo.roster(membership.alliance).await?;
    let diplomacy = repo.diplomacy_of(membership.alliance).await?;
    let allied_villages = repo.alliance_member_villages(&allied).await?;
    let village_ids: Vec<VillageId> = allied_villages.iter().map(|v| v.village).collect();
    let incoming = repo.incoming_against(&village_ids).await?;
    Ok(Some(AllianceOverview {
        membership,
        name,
        tag,
        roster,
        diplomacy,
        allied_villages,
        incoming,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{OutgoingInvite, PendingInvite};
    use async_trait::async_trait;
    use std::collections::{HashMap, HashSet};
    use std::sync::Mutex;

    fn rules() -> AllianceRules {
        AllianceRules {
            max_members: 3,
            join_embassy_level: 1,
            found_embassy_level: 3,
        }
    }

    /// In-memory `AllianceRepository` for use-case tests. Member rows keyed by player; invites a set of
    /// (alliance, invitee); embassy levels per player.
    type DiploState = (DiplomacyStance, DiplomacyStatus, Option<u128>);

    fn norm(a: AllianceId, b: AllianceId) -> (u128, u128) {
        if a.0 <= b.0 { (a.0, b.0) } else { (b.0, a.0) }
    }

    #[derive(Default)]
    struct FakeAlliance {
        members: Mutex<HashMap<u128, Membership>>,
        invites: Mutex<HashSet<(u128, u128)>>,
        embassy: Mutex<HashMap<u128, u8>>,
        next_id: Mutex<u128>,
        names: Mutex<HashSet<String>>,
        diplomacy: Mutex<HashMap<(u128, u128), DiploState>>,
    }

    impl FakeAlliance {
        fn with_embassy(levels: &[(u128, u8)]) -> Self {
            let f = FakeAlliance::default();
            *f.embassy.lock().unwrap() = levels.iter().copied().collect();
            *f.next_id.lock().unwrap() = 1;
            f
        }
        fn member(&self, p: u128) -> Option<Membership> {
            self.members.lock().unwrap().get(&p).cloned()
        }
    }

    #[async_trait]
    impl AllianceRepository for FakeAlliance {
        async fn max_embassy_level(&self, player: PlayerId) -> Result<u8, RepoError> {
            Ok(self
                .embassy
                .lock()
                .unwrap()
                .get(&player.0)
                .copied()
                .unwrap_or(0))
        }
        async fn alliance_of(&self, player: PlayerId) -> Result<Option<Membership>, RepoError> {
            Ok(self.member(player.0))
        }
        async fn member_count(&self, alliance: AllianceId) -> Result<u32, RepoError> {
            Ok(self
                .members
                .lock()
                .unwrap()
                .values()
                .filter(|m| m.alliance == alliance)
                .count() as u32)
        }
        async fn alliance_summary(
            &self,
            alliance: AllianceId,
        ) -> Result<Option<(String, String)>, RepoError> {
            Ok(Some((
                format!("a{}", alliance.0),
                format!("T{}", alliance.0),
            )))
        }
        async fn roster(&self, alliance: AllianceId) -> Result<Vec<RosterEntry>, RepoError> {
            Ok(self
                .members
                .lock()
                .unwrap()
                .iter()
                .filter(|(_, m)| m.alliance == alliance)
                .map(|(p, m)| RosterEntry {
                    player: PlayerId(*p),
                    name: format!("p{p}"),
                    role: m.role,
                    rights: m.rights,
                })
                .collect())
        }
        async fn create_alliance(
            &self,
            name: &str,
            tag: &str,
            founder: PlayerId,
        ) -> Result<AllianceId, RepoError> {
            let mut names = self.names.lock().unwrap();
            if !names.insert(name.to_owned()) || !names.insert(format!("tag:{tag}")) {
                return Err(RepoError::Duplicate);
            }
            let mut id = self.next_id.lock().unwrap();
            let aid = AllianceId(*id);
            *id += 1;
            self.members.lock().unwrap().insert(
                founder.0,
                Membership {
                    alliance: aid,
                    role: AllianceRole::Founder,
                    rights: RightSet::all(),
                },
            );
            Ok(aid)
        }
        async fn insert_invite(&self, a: AllianceId, p: PlayerId) -> Result<(), RepoError> {
            if !self.invites.lock().unwrap().insert((a.0, p.0)) {
                return Err(RepoError::Duplicate);
            }
            Ok(())
        }
        async fn delete_invite(&self, a: AllianceId, p: PlayerId) -> Result<(), RepoError> {
            self.invites.lock().unwrap().remove(&(a.0, p.0));
            Ok(())
        }
        async fn has_invite(&self, a: AllianceId, p: PlayerId) -> Result<bool, RepoError> {
            Ok(self.invites.lock().unwrap().contains(&(a.0, p.0)))
        }
        async fn pending_invites_for(&self, _p: PlayerId) -> Result<Vec<PendingInvite>, RepoError> {
            Ok(Vec::new())
        }
        async fn invites_of(&self, _a: AllianceId) -> Result<Vec<OutgoingInvite>, RepoError> {
            Ok(Vec::new())
        }
        async fn add_member(
            &self,
            alliance: AllianceId,
            player: PlayerId,
            role: AllianceRole,
            rights: RightSet,
            cap: u32,
        ) -> Result<(), RepoError> {
            let mut members = self.members.lock().unwrap();
            if members.contains_key(&player.0) {
                return Err(RepoError::Duplicate);
            }
            let count = members.values().filter(|m| m.alliance == alliance).count() as u32;
            if count >= cap {
                return Err(RepoError::Conflict);
            }
            members.insert(
                player.0,
                Membership {
                    alliance,
                    role,
                    rights,
                },
            );
            Ok(())
        }
        async fn remove_member(&self, player: PlayerId) -> Result<(), RepoError> {
            self.members.lock().unwrap().remove(&player.0);
            Ok(())
        }
        async fn set_member_role(
            &self,
            alliance: AllianceId,
            player: PlayerId,
            role: AllianceRole,
            rights: RightSet,
        ) -> Result<(), RepoError> {
            self.members.lock().unwrap().insert(
                player.0,
                Membership {
                    alliance,
                    role,
                    rights,
                },
            );
            Ok(())
        }
        async fn transfer_founder(
            &self,
            alliance: AllianceId,
            from: PlayerId,
            to: PlayerId,
        ) -> Result<(), RepoError> {
            let mut m = self.members.lock().unwrap();
            m.insert(
                from.0,
                Membership {
                    alliance,
                    role: AllianceRole::Member,
                    rights: RightSet::empty(),
                },
            );
            m.insert(
                to.0,
                Membership {
                    alliance,
                    role: AllianceRole::Founder,
                    rights: RightSet::all(),
                },
            );
            Ok(())
        }
        async fn disband(&self, alliance: AllianceId) -> Result<(), RepoError> {
            self.members
                .lock()
                .unwrap()
                .retain(|_, m| m.alliance != alliance);
            self.invites
                .lock()
                .unwrap()
                .retain(|(a, _)| *a != alliance.0);
            self.diplomacy
                .lock()
                .unwrap()
                .retain(|(lo, hi), _| *lo != alliance.0 && *hi != alliance.0);
            Ok(())
        }
        async fn diplomacy_state(
            &self,
            a: AllianceId,
            b: AllianceId,
        ) -> Result<Option<(DiplomacyStance, DiplomacyStatus, Option<AllianceId>)>, RepoError>
        {
            Ok(self
                .diplomacy
                .lock()
                .unwrap()
                .get(&norm(a, b))
                .map(|(s, st, by)| (*s, *st, by.map(AllianceId))))
        }
        async fn set_diplomacy_state(
            &self,
            a: AllianceId,
            b: AllianceId,
            stance: DiplomacyStance,
            status: DiplomacyStatus,
            proposed_by: Option<AllianceId>,
        ) -> Result<(), RepoError> {
            self.diplomacy
                .lock()
                .unwrap()
                .insert(norm(a, b), (stance, status, proposed_by.map(|p| p.0)));
            Ok(())
        }
        async fn clear_diplomacy(&self, a: AllianceId, b: AllianceId) -> Result<(), RepoError> {
            self.diplomacy.lock().unwrap().remove(&norm(a, b));
            Ok(())
        }
        async fn diplomacy_of(
            &self,
            alliance: AllianceId,
        ) -> Result<Vec<DiplomacyEntry>, RepoError> {
            Ok(self
                .diplomacy
                .lock()
                .unwrap()
                .iter()
                .filter(|((lo, hi), _)| *lo == alliance.0 || *hi == alliance.0)
                .map(|((lo, hi), (s, st, by))| {
                    let other = if *lo == alliance.0 { *hi } else { *lo };
                    DiplomacyEntry {
                        other: AllianceId(other),
                        other_name: format!("a{other}"),
                        other_tag: format!("T{other}"),
                        stance: *s,
                        status: *st,
                        proposed_by: by.map(AllianceId),
                    }
                })
                .collect())
        }
        async fn confederate_alliances(
            &self,
            alliance: AllianceId,
        ) -> Result<Vec<AllianceId>, RepoError> {
            Ok(self
                .diplomacy
                .lock()
                .unwrap()
                .iter()
                .filter(|((lo, hi), (s, st, _))| {
                    (*lo == alliance.0 || *hi == alliance.0)
                        && *s == DiplomacyStance::Confederation
                        && *st == DiplomacyStatus::Active
                })
                .map(|((lo, hi), _)| AllianceId(if *lo == alliance.0 { *hi } else { *lo }))
                .collect())
        }
        async fn alliance_member_villages(
            &self,
            _alliances: &[AllianceId],
        ) -> Result<Vec<AlliedVillage>, RepoError> {
            Ok(Vec::new())
        }
        async fn incoming_against(
            &self,
            _villages: &[VillageId],
        ) -> Result<Vec<IncomingAttack>, RepoError> {
            Ok(Vec::new())
        }
    }

    // AC1/AC2: found needs Embassy ≥ 3 and an alliance-less founder; duplicate name is rejected.
    #[tokio::test]
    async fn found_gates_on_embassy_and_uniqueness() {
        let repo = FakeAlliance::with_embassy(&[(1, 3), (2, 2)]);
        let r = rules();
        // Player 2 has only Embassy 2 → not eligible.
        assert_eq!(
            found_alliance(&repo, &r, PlayerId(2), "Knights", "KNI").await,
            Err(AllianceError::NotEligible)
        );
        // Player 1 founds successfully and becomes Founder.
        let aid = found_alliance(&repo, &r, PlayerId(1), "Knights", "KNI")
            .await
            .unwrap();
        assert_eq!(repo.member(1).unwrap().role, AllianceRole::Founder);
        // Founding again is rejected (already in an alliance).
        assert_eq!(
            found_alliance(&repo, &r, PlayerId(1), "Other", "OTH").await,
            Err(AllianceError::AlreadyInAlliance)
        );
        // A duplicate name is rejected.
        let repo2 = FakeAlliance::with_embassy(&[(9, 3)]);
        repo2.names.lock().unwrap().insert("Knights".to_owned());
        assert_eq!(
            found_alliance(&repo2, &r, PlayerId(9), "Knights", "ZZZ").await,
            Err(AllianceError::NameOrTagTaken)
        );
        let _ = aid;
    }

    // AC3/AC4: invite needs the right; accept needs Embassy ≥ 1, alliance-less, and a slot.
    #[tokio::test]
    async fn invite_and_accept_flow() {
        let repo = FakeAlliance::with_embassy(&[(1, 3), (2, 1), (3, 1), (4, 1), (5, 0)]);
        let r = rules();
        let aid = found_alliance(&repo, &r, PlayerId(1), "A", "A")
            .await
            .unwrap();
        // A non-member cannot invite.
        assert_eq!(
            invite_player(&repo, PlayerId(9), PlayerId(2)).await,
            Err(AllianceError::NotAMember)
        );
        // Founder invites player 2; player 2 accepts and joins.
        invite_player(&repo, PlayerId(1), PlayerId(2))
            .await
            .unwrap();
        respond_invite(&repo, &r, PlayerId(2), aid, true)
            .await
            .unwrap();
        assert_eq!(repo.member(2).unwrap().role, AllianceRole::Member);
        // Accepting without an invite is rejected.
        assert_eq!(
            respond_invite(&repo, &r, PlayerId(3), aid, true).await,
            Err(AllianceError::NoInvite)
        );
        // Embassy-0 invitee cannot accept.
        invite_player(&repo, PlayerId(1), PlayerId(5))
            .await
            .unwrap();
        assert_eq!(
            respond_invite(&repo, &r, PlayerId(5), aid, true).await,
            Err(AllianceError::NotEligible)
        );
        // Cap is 3: members are 1,2; invite+accept 3 fills it, 4 is rejected at the cap.
        invite_player(&repo, PlayerId(1), PlayerId(3))
            .await
            .unwrap();
        respond_invite(&repo, &r, PlayerId(3), aid, true)
            .await
            .unwrap();
        invite_player(&repo, PlayerId(1), PlayerId(4))
            .await
            .unwrap();
        assert_eq!(
            respond_invite(&repo, &r, PlayerId(4), aid, true).await,
            Err(AllianceError::AtCap)
        );
    }

    // AC5: the founder cannot leave; expel needs rank + right.
    #[tokio::test]
    async fn leave_and_expel_rank_rules() {
        let repo = FakeAlliance::with_embassy(&[(1, 3), (2, 1), (3, 1)]);
        let r = rules();
        let aid = found_alliance(&repo, &r, PlayerId(1), "A", "A")
            .await
            .unwrap();
        invite_player(&repo, PlayerId(1), PlayerId(2))
            .await
            .unwrap();
        respond_invite(&repo, &r, PlayerId(2), aid, true)
            .await
            .unwrap();
        invite_player(&repo, PlayerId(1), PlayerId(3))
            .await
            .unwrap();
        respond_invite(&repo, &r, PlayerId(3), aid, true)
            .await
            .unwrap();

        // The founder cannot leave.
        assert_eq!(
            leave_alliance(&repo, PlayerId(1)).await,
            Err(AllianceError::FounderMustTransfer)
        );
        // A member cannot expel (no right); the founder can.
        assert_eq!(
            expel_member(&repo, PlayerId(2), PlayerId(3)).await,
            Err(AllianceError::MissingRight)
        );
        // Cannot expel yourself.
        assert_eq!(
            expel_member(&repo, PlayerId(1), PlayerId(1)).await,
            Err(AllianceError::SelfTarget)
        );
        // The founder expels member 3.
        expel_member(&repo, PlayerId(1), PlayerId(3)).await.unwrap();
        assert!(repo.member(3).is_none());
        // A member can leave freely.
        leave_alliance(&repo, PlayerId(2)).await.unwrap();
        assert!(repo.member(2).is_none());
    }

    // AC6: only the founder can mint a leader; transfer hands over the founder role; disband clears all.
    #[tokio::test]
    async fn roles_transfer_and_disband() {
        let repo = FakeAlliance::with_embassy(&[(1, 3), (2, 1), (3, 1)]);
        let r = rules();
        let aid = found_alliance(&repo, &r, PlayerId(1), "A", "A")
            .await
            .unwrap();
        for p in [2, 3] {
            invite_player(&repo, PlayerId(1), PlayerId(p))
                .await
                .unwrap();
            respond_invite(&repo, &r, PlayerId(p), aid, true)
                .await
                .unwrap();
        }
        // The founder promotes player 2 to Leader with the Invite right.
        let invite_right = RightSet::empty().with(AllianceRight::Invite);
        set_member_role(
            &repo,
            PlayerId(1),
            PlayerId(2),
            AllianceRole::Leader,
            invite_right,
        )
        .await
        .unwrap();
        assert_eq!(repo.member(2).unwrap().role, AllianceRole::Leader);
        // The new leader (with Invite) cannot mint another Leader — only the founder can.
        assert_eq!(
            set_member_role(
                &repo,
                PlayerId(2),
                PlayerId(3),
                AllianceRole::Leader,
                invite_right
            )
            .await,
            Err(AllianceError::MissingRight)
        );
        // Even *with* manage-roles, a Leader cannot mint another Leader (no privilege escalation — they
        // do not outrank the Leader role); minting Leaders is effectively Founder-only.
        let manage = RightSet::empty().with(AllianceRight::ManageRoles);
        set_member_role(
            &repo,
            PlayerId(1),
            PlayerId(2),
            AllianceRole::Leader,
            manage,
        )
        .await
        .unwrap();
        assert_eq!(
            set_member_role(
                &repo,
                PlayerId(2),
                PlayerId(3),
                AllianceRole::Leader,
                manage
            )
            .await,
            Err(AllianceError::RankTooLow)
        );
        // Cannot set a Founder via role change.
        assert_eq!(
            set_member_role(
                &repo,
                PlayerId(1),
                PlayerId(3),
                AllianceRole::Founder,
                RightSet::empty()
            )
            .await,
            Err(AllianceError::BadRole)
        );
        // Transfer founder to player 2; player 1 becomes a plain member.
        transfer_founder(&repo, PlayerId(1), PlayerId(2))
            .await
            .unwrap();
        assert_eq!(repo.member(2).unwrap().role, AllianceRole::Founder);
        assert_eq!(repo.member(1).unwrap().role, AllianceRole::Member);
        // The old founder can no longer disband; the new founder can.
        assert_eq!(
            disband_alliance(&repo, PlayerId(1)).await,
            Err(AllianceError::MissingRight)
        );
        disband_alliance(&repo, PlayerId(2)).await.unwrap();
        assert!(repo.member(1).is_none() && repo.member(2).is_none() && repo.member(3).is_none());
    }

    // AC7: diplomacy — propose→accept (only the counterpart accepts), war overrides confederation,
    // war blocks a confederation proposal, cancel returns to neutral, and self-diplomacy is rejected.
    #[tokio::test]
    async fn diplomacy_propose_accept_war_cancel() {
        let repo = FakeAlliance::with_embassy(&[(1, 3), (2, 3)]);
        let r = rules();
        let a = found_alliance(&repo, &r, PlayerId(1), "A", "A")
            .await
            .unwrap();
        let b = found_alliance(&repo, &r, PlayerId(2), "B", "B")
            .await
            .unwrap();

        // Cannot set diplomacy with your own alliance.
        assert_eq!(
            set_diplomacy(&repo, PlayerId(1), a, DiplomacyCommand::DeclareWar).await,
            Err(AllianceError::SelfDiplomacy)
        );
        // A1 proposes a confederation to B; A1 cannot accept its own proposal.
        set_diplomacy(
            &repo,
            PlayerId(1),
            b,
            DiplomacyCommand::ProposeConfederation,
        )
        .await
        .unwrap();
        assert_eq!(
            set_diplomacy(&repo, PlayerId(1), b, DiplomacyCommand::AcceptConfederation).await,
            Err(AllianceError::CannotAcceptOwnProposal)
        );
        // Not yet active ⇒ not a confederate.
        assert!(repo.confederate_alliances(a).await.unwrap().is_empty());
        // B2 (the counterpart) accepts ⇒ active confederation both ways.
        set_diplomacy(&repo, PlayerId(2), a, DiplomacyCommand::AcceptConfederation)
            .await
            .unwrap();
        assert_eq!(repo.confederate_alliances(a).await.unwrap(), vec![b]);
        assert_eq!(repo.confederate_alliances(b).await.unwrap(), vec![a]);

        // Declaring war overrides the confederation; they are no longer confederates.
        set_diplomacy(&repo, PlayerId(1), b, DiplomacyCommand::DeclareWar)
            .await
            .unwrap();
        assert!(repo.confederate_alliances(a).await.unwrap().is_empty());
        // Cannot propose a confederation while at war.
        assert_eq!(
            set_diplomacy(
                &repo,
                PlayerId(2),
                a,
                DiplomacyCommand::ProposeConfederation
            )
            .await,
            Err(AllianceError::Diplomacy(
                DiplomacyError::WarBlocksConfederation
            ))
        );
        // Cancel ⇒ neutral (no stored stance).
        set_diplomacy(&repo, PlayerId(1), b, DiplomacyCommand::Cancel)
            .await
            .unwrap();
        assert!(repo.diplomacy_state(a, b).await.unwrap().is_none());
    }
}
