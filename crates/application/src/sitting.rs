//! Account-sitting use-cases (030): an owner authorises trusted sitters; a sitter operates the owner's
//! account within limits, audited. Authorisation is **server-checked** (P4) and re-evaluated on every
//! request (so a revoke ends an in-progress sit).

use crate::ports::{AccountRepository, PlayerHit, RepoError, SitterActionView};
use eperica_domain::{PlayerId, Timestamp, account_blocked, can_grant_sitter};

/// Page size for the audit log (P11 — bounded).
pub const SITTER_LOG_LIMIT: i64 = 50;

/// Why a sitting action was rejected (030, server-enforced — P4).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SittingError {
    /// No player with that username.
    #[error("no such player")]
    NotFound,
    /// You cannot add yourself as a sitter.
    #[error("you cannot sit for yourself")]
    SelfSit,
    /// The sitter cap is reached.
    #[error("you already have the maximum number of sitters")]
    AtCap,
    /// The actor is not an authorised sitter of the owner (or the owner is unavailable).
    #[error("you are not authorised to sit for that player")]
    NotAuthorized,
    /// A backend/storage failure.
    #[error("storage error: {0}")]
    Backend(String),
}

impl From<RepoError> for SittingError {
    fn from(e: RepoError) -> Self {
        SittingError::Backend(e.to_string())
    }
}

/// Authorise the player named `sitter_username` to sit for `owner` (030 AC1). Owner-scoped — the caller
/// passes the session player. Rejects self, an unknown name, or exceeding the cap.
///
/// # Errors
/// [`SittingError::NotFound`], [`SittingError::SelfSit`], [`SittingError::AtCap`], or a backend error.
pub async fn grant_sitter<A>(
    accounts: &A,
    owner: PlayerId,
    sitter_username: &str,
) -> Result<(), SittingError>
where
    A: AccountRepository,
{
    let target = accounts
        .find_user_by_username(sitter_username.trim())
        .await?
        .ok_or(SittingError::NotFound)?;
    if target.id == owner {
        return Err(SittingError::SelfSit);
    }
    let count = usize::try_from(accounts.count_sitters(owner).await?).unwrap_or(usize::MAX);
    if !can_grant_sitter(owner, target.id, count) {
        return Err(SittingError::AtCap);
    }
    accounts.grant_sitter(owner, target.id).await?;
    Ok(())
}

/// Revoke a sitter authorisation (030 AC1) — owner-scoped, idempotent.
///
/// # Errors
/// A backend error.
pub async fn revoke_sitter<A>(
    accounts: &A,
    owner: PlayerId,
    sitter: PlayerId,
) -> Result<(), SittingError>
where
    A: AccountRepository,
{
    accounts.revoke_sitter(owner, sitter).await?;
    Ok(())
}

/// `owner`'s authorised sitters (030 AC1).
///
/// # Errors
/// A backend error.
pub async fn list_sitters<A>(accounts: &A, owner: PlayerId) -> Result<Vec<PlayerHit>, SittingError>
where
    A: AccountRepository,
{
    Ok(accounts.sitters_of(owner).await?)
}

/// The owners `sitter` may operate (030 AC2).
///
/// # Errors
/// A backend error.
pub async fn list_sitting_for<A>(
    accounts: &A,
    sitter: PlayerId,
) -> Result<Vec<PlayerHit>, SittingError>
where
    A: AccountRepository,
{
    Ok(accounts.sitting_for(sitter).await?)
}

/// `owner`'s sitter-action audit log (030 AC5).
///
/// # Errors
/// A backend error.
pub async fn sitter_log<A>(
    accounts: &A,
    owner: PlayerId,
) -> Result<Vec<SitterActionView>, SittingError>
where
    A: AccountRepository,
{
    Ok(accounts.sitter_actions(owner, SITTER_LOG_LIMIT).await?)
}

/// Whether `sitter` may currently operate `owner`'s account (030 AC2/AC3/AC6): an authorised sitter, and
/// the owner is **not** banned/suspended (a sanctioned account can't be operated via a sit).
///
/// # Errors
/// A backend error.
pub async fn authorize_sit<A>(
    accounts: &A,
    owner: PlayerId,
    sitter: PlayerId,
    now: Timestamp,
) -> Result<bool, SittingError>
where
    A: AccountRepository,
{
    if !accounts.is_sitter(owner, sitter).await? {
        return Ok(false);
    }
    let blocked = match accounts.find_user_by_id(owner).await? {
        Some(u) => account_blocked(u.banned_at, u.suspended_until, now),
        None => true, // owner gone ⇒ not operable
    };
    Ok(!blocked)
}

/// Record a sitter's mutating action on `owner`'s account (030 AC5).
///
/// # Errors
/// A backend error.
pub async fn record_sitter_action<A>(
    accounts: &A,
    owner: PlayerId,
    sitter: PlayerId,
    action: &str,
    now: Timestamp,
) -> Result<(), SittingError>
where
    A: AccountRepository,
{
    accounts
        .log_sitter_action(owner, sitter, action, now)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{NewUser, UserRecord};
    use async_trait::async_trait;
    use eperica_domain::{StartingVillage, Tribe};
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeAccounts {
        // (id, username, banned)
        users: Mutex<Vec<(u128, String, bool)>>,
        grants: Mutex<Vec<(u128, u128)>>, // (owner, sitter)
    }

    impl FakeAccounts {
        fn user(&self, id: u128, name: &str, banned: bool) {
            self.users
                .lock()
                .unwrap()
                .push((id, name.to_owned(), banned));
        }
    }

    fn rec(id: u128, banned: bool) -> UserRecord {
        UserRecord {
            id: PlayerId(id),
            username: format!("u{id}"),
            email: String::new(),
            password_hash: String::new(),
            email_confirmed: true,
            tribe: Tribe::Gauls,
            abandoned: false,
            is_moderator: false,
            is_admin: false,
            banned_at: if banned { Some(Timestamp(1)) } else { None },
            suspended_until: None,
        }
    }

    #[async_trait]
    impl AccountRepository for FakeAccounts {
        async fn create_account(
            &self,
            _u: NewUser,
            _t: &StartingVillage,
        ) -> Result<UserRecord, RepoError> {
            unimplemented!()
        }
        async fn find_user_by_username(
            &self,
            username: &str,
        ) -> Result<Option<UserRecord>, RepoError> {
            Ok(self
                .users
                .lock()
                .unwrap()
                .iter()
                .find(|(_, n, _)| n == username)
                .map(|(id, _, banned)| rec(*id, *banned)))
        }
        async fn find_user_by_id(&self, id: PlayerId) -> Result<Option<UserRecord>, RepoError> {
            Ok(self
                .users
                .lock()
                .unwrap()
                .iter()
                .find(|(uid, _, _)| *uid == id.0)
                .map(|(uid, _, banned)| rec(*uid, *banned)))
        }
        async fn villages_of(
            &self,
            _o: PlayerId,
        ) -> Result<Vec<eperica_domain::Village>, RepoError> {
            Ok(vec![])
        }
        async fn village_by_id(
            &self,
            _v: eperica_domain::VillageId,
        ) -> Result<Option<eperica_domain::Village>, RepoError> {
            Ok(None)
        }
        async fn stored_resources(
            &self,
            _v: eperica_domain::VillageId,
        ) -> Result<Option<(eperica_domain::ResourceAmounts, Timestamp)>, RepoError> {
            Ok(None)
        }
        async fn garrison(
            &self,
            _v: eperica_domain::VillageId,
        ) -> Result<eperica_domain::UnitCounts, RepoError> {
            Ok(Vec::new())
        }
        async fn villages_at(
            &self,
            _c: &[eperica_domain::Coordinate],
        ) -> Result<Vec<crate::ports::VillageMarker>, RepoError> {
            Ok(vec![])
        }
        async fn village_at(
            &self,
            _c: eperica_domain::Coordinate,
        ) -> Result<Option<eperica_domain::Village>, RepoError> {
            Ok(None)
        }
        async fn grant_sitter(&self, owner: PlayerId, sitter: PlayerId) -> Result<(), RepoError> {
            let mut g = self.grants.lock().unwrap();
            if !g.contains(&(owner.0, sitter.0)) {
                g.push((owner.0, sitter.0));
            }
            Ok(())
        }
        async fn revoke_sitter(&self, owner: PlayerId, sitter: PlayerId) -> Result<(), RepoError> {
            self.grants
                .lock()
                .unwrap()
                .retain(|g| *g != (owner.0, sitter.0));
            Ok(())
        }
        async fn is_sitter(&self, owner: PlayerId, sitter: PlayerId) -> Result<bool, RepoError> {
            Ok(self.grants.lock().unwrap().contains(&(owner.0, sitter.0)))
        }
        async fn count_sitters(&self, owner: PlayerId) -> Result<i64, RepoError> {
            Ok(self
                .grants
                .lock()
                .unwrap()
                .iter()
                .filter(|(o, _)| *o == owner.0)
                .count() as i64)
        }
    }

    #[tokio::test]
    async fn grant_rejects_self_unknown_and_overcap() {
        let a = FakeAccounts::default();
        a.user(1, "owner", false);
        a.user(2, "bob", false);
        a.user(3, "carol", false);
        a.user(4, "dave", false);
        a.user(5, "erin", false);
        // Unknown username.
        assert_eq!(
            grant_sitter(&a, PlayerId(1), "ghost").await.unwrap_err(),
            SittingError::NotFound
        );
        // Self.
        assert_eq!(
            grant_sitter(&a, PlayerId(1), "owner").await.unwrap_err(),
            SittingError::SelfSit
        );
        // Up to the cap (MAX_SITTERS = 3).
        grant_sitter(&a, PlayerId(1), "bob").await.unwrap();
        grant_sitter(&a, PlayerId(1), "carol").await.unwrap();
        grant_sitter(&a, PlayerId(1), "dave").await.unwrap();
        assert_eq!(
            grant_sitter(&a, PlayerId(1), "erin").await.unwrap_err(),
            SittingError::AtCap
        );
    }

    #[tokio::test]
    async fn authorize_requires_grant_and_unblocked_owner() {
        let a = FakeAccounts::default();
        a.user(1, "owner", false);
        a.user(9, "banned_owner", true);
        a.user(2, "sitter", false);
        let now = Timestamp(1000);
        // Not a sitter ⇒ false.
        assert!(
            !authorize_sit(&a, PlayerId(1), PlayerId(2), now)
                .await
                .unwrap()
        );
        // Granted ⇒ true.
        grant_sitter(&a, PlayerId(1), "sitter").await.unwrap();
        assert!(
            authorize_sit(&a, PlayerId(1), PlayerId(2), now)
                .await
                .unwrap()
        );
        // Granted but owner banned ⇒ false (can't operate a sanctioned account).
        grant_sitter(&a, PlayerId(9), "sitter").await.unwrap();
        assert!(
            !authorize_sit(&a, PlayerId(9), PlayerId(2), now)
                .await
                .unwrap()
        );
        // Revoke ⇒ de-authorised.
        revoke_sitter(&a, PlayerId(1), PlayerId(2)).await.unwrap();
        assert!(
            !authorize_sit(&a, PlayerId(1), PlayerId(2), now)
                .await
                .unwrap()
        );
    }
}
