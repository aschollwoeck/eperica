//! Alliance & membership use-cases (015) — found / invite / accept / leave / expel / roles, plus the
//! founder transfer and disband. Each loads the actor's membership, runs the **pure** domain
//! role/rights/eligibility checks ([`eperica_domain::alliance`]), then calls the [`AllianceRepository`].
//! Authority is enforced here, server-side (P4); the diplomacy use-cases land in T4.

use crate::ports::{AllianceRepository, RepoError};
use eperica_domain::AllianceRight;
use eperica_domain::{
    AllianceId, AllianceRole, AllianceRules, PlayerId, RightSet, can_expel, has_right,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{Membership, OutgoingInvite, PendingInvite, RosterEntry};
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
    #[derive(Default)]
    struct FakeAlliance {
        members: Mutex<HashMap<u128, Membership>>,
        invites: Mutex<HashSet<(u128, u128)>>,
        embassy: Mutex<HashMap<u128, u8>>,
        next_id: Mutex<u128>,
        names: Mutex<HashSet<String>>,
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
            Ok(())
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
}
