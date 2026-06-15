//! Administrator use-cases (036 — M9 multi-world & administration): the `/admin` console's gated reads
//! and role grants. All actions are gated on the elevated **Administrator** role (`require_admin`),
//! server-authoritative (P4); the pure `domain` crate is untouched (this is identity/role + I/O).

use crate::ports::{
    AccountRepository, AdminAccount, AdminOverview, AdminRepository, ModerationRepository,
    RepoError,
};
use eperica_domain::PlayerId;

/// Why an admin action was rejected (036).
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AdminError {
    /// The actor is not an administrator.
    #[error("not authorized")]
    NotAuthorized,
    /// The target account does not exist.
    #[error("account not found")]
    NotFound,
    /// An admin tried to remove their **own** Administrator role (anti-lockout, AC3).
    #[error("you cannot remove your own administrator role")]
    SelfDemotion,
    /// A storage/backend failure.
    #[error("storage error: {0}")]
    Backend(String),
}

impl From<RepoError> for AdminError {
    fn from(e: RepoError) -> Self {
        AdminError::Backend(e.to_string())
    }
}

/// Whether `actor` holds the Administrator role (036 AC1) — the gate for every admin-only action.
///
/// # Errors
/// [`AdminError::NotAuthorized`] for a non-admin; otherwise a backend error.
pub async fn require_admin<A>(accounts: &A, actor: PlayerId) -> Result<(), AdminError>
where
    A: AccountRepository,
{
    match accounts.find_user_by_id(actor).await? {
        Some(u) if u.is_admin => Ok(()),
        _ => Err(AdminError::NotAuthorized),
    }
}

/// The read-only world + server overview for the console (036 AC4). Admin-gated.
///
/// # Errors
/// [`AdminError::NotAuthorized`] for a non-admin; otherwise a backend error.
pub async fn admin_overview<A, D>(
    accounts: &A,
    admin: &D,
    actor: PlayerId,
) -> Result<AdminOverview, AdminError>
where
    A: AccountRepository,
    D: AdminRepository,
{
    require_admin(accounts, actor).await?;
    Ok(admin.admin_overview().await?)
}

/// The console's account listing (036 AC3). Admin-gated. Newest accounts first, capped at `limit`.
///
/// # Errors
/// [`AdminError::NotAuthorized`] for a non-admin; otherwise a backend error.
pub async fn list_accounts<A, D>(
    accounts: &A,
    admin: &D,
    actor: PlayerId,
    limit: i64,
) -> Result<Vec<AdminAccount>, AdminError>
where
    A: AccountRepository,
    D: AdminRepository,
{
    require_admin(accounts, actor).await?;
    Ok(admin.recent_accounts(limit).await?)
}

/// Grant or revoke an elevated role (Moderator or Administrator) on `subject` (036 AC3). Admin-gated.
/// Refuses to remove the actor's **own** Administrator role (anti-lockout). Idempotent.
///
/// # Errors
/// [`AdminError::NotAuthorized`] for a non-admin; [`AdminError::SelfDemotion`] when removing your own
/// admin role; [`AdminError::NotFound`] if the subject does not exist; otherwise a backend error.
pub async fn set_role<A, M, D>(
    accounts: &A,
    moderation: &M,
    admin: &D,
    actor: PlayerId,
    subject: PlayerId,
    role: ElevatedRole,
    grant: bool,
) -> Result<(), AdminError>
where
    A: AccountRepository,
    M: ModerationRepository,
    D: AdminRepository,
{
    require_admin(accounts, actor).await?;
    if role == ElevatedRole::Admin && !grant && actor == subject {
        return Err(AdminError::SelfDemotion);
    }
    // Confirm the subject exists, so the console reports a clear error rather than a silent no-op.
    if accounts.find_user_by_id(subject).await?.is_none() {
        return Err(AdminError::NotFound);
    }
    match role {
        ElevatedRole::Moderator => moderation.set_moderator(subject, grant).await?,
        ElevatedRole::Admin => admin.set_admin(subject, grant).await?,
    }
    Ok(())
}

/// An elevated role an admin can grant/revoke from the console (036 AC3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElevatedRole {
    Moderator,
    Admin,
}

impl ElevatedRole {
    /// Parse the form slug.
    #[must_use]
    pub fn from_slug(s: &str) -> Option<Self> {
        match s {
            "moderator" => Some(ElevatedRole::Moderator),
            "admin" => Some(ElevatedRole::Admin),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::UserRecord;
    use async_trait::async_trait;
    use eperica_domain::Tribe;
    use std::sync::Mutex;

    /// A minimal account+admin fake: a roster of (id, is_moderator, is_admin) with recorded role writes.
    #[derive(Default)]
    struct Fake {
        users: Mutex<Vec<UserRecord>>,
        set_admin_calls: Mutex<Vec<(u128, bool)>>,
        set_mod_calls: Mutex<Vec<(u128, bool)>>,
    }

    fn user(id: u128, is_admin: bool) -> UserRecord {
        UserRecord {
            id: PlayerId(id),
            username: format!("u{id}"),
            email: format!("u{id}@e.test"),
            password_hash: "x".to_owned(),
            email_confirmed: true,
            tribe: Tribe::Gauls,
            abandoned: false,
            is_moderator: false,
            is_admin,
            banned_at: None,
            suspended_until: None,
        }
    }

    #[async_trait]
    impl AccountRepository for Fake {
        async fn find_user_by_id(&self, id: PlayerId) -> Result<Option<UserRecord>, RepoError> {
            Ok(self
                .users
                .lock()
                .unwrap()
                .iter()
                .find(|u| u.id == id)
                .cloned())
        }

        // The remaining required methods are unused by the admin use-cases under test — minimal stubs.
        async fn create_account(
            &self,
            _user: crate::ports::NewUser,
            _template: &eperica_domain::StartingVillage,
        ) -> Result<UserRecord, RepoError> {
            Err(RepoError::Backend("unused".to_owned()))
        }
        async fn find_user_by_username(
            &self,
            _username: &str,
        ) -> Result<Option<UserRecord>, RepoError> {
            Ok(None)
        }
        async fn villages_of(
            &self,
            _owner: PlayerId,
        ) -> Result<Vec<eperica_domain::Village>, RepoError> {
            Ok(Vec::new())
        }
        async fn village_by_id(
            &self,
            _village: eperica_domain::VillageId,
        ) -> Result<Option<eperica_domain::Village>, RepoError> {
            Ok(None)
        }
        async fn stored_resources(
            &self,
            _village: eperica_domain::VillageId,
        ) -> Result<Option<(eperica_domain::ResourceAmounts, eperica_domain::Timestamp)>, RepoError>
        {
            Ok(None)
        }
        async fn garrison(
            &self,
            _village: eperica_domain::VillageId,
        ) -> Result<eperica_domain::UnitCounts, RepoError> {
            Ok(eperica_domain::UnitCounts::default())
        }
        async fn villages_at(
            &self,
            _coords: &[eperica_domain::Coordinate],
        ) -> Result<Vec<crate::ports::VillageMarker>, RepoError> {
            Ok(Vec::new())
        }
        async fn village_at(
            &self,
            _coord: eperica_domain::Coordinate,
        ) -> Result<Option<eperica_domain::Village>, RepoError> {
            Ok(None)
        }
    }

    #[async_trait]
    impl ModerationRepository for Fake {
        async fn set_moderator(&self, p: PlayerId, on: bool) -> Result<(), RepoError> {
            self.set_mod_calls.lock().unwrap().push((p.0, on));
            Ok(())
        }
    }

    #[async_trait]
    impl AdminRepository for Fake {
        async fn set_admin(&self, p: PlayerId, on: bool) -> Result<(), RepoError> {
            self.set_admin_calls.lock().unwrap().push((p.0, on));
            Ok(())
        }
    }

    fn fake(users: Vec<UserRecord>) -> Fake {
        Fake {
            users: Mutex::new(users),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn non_admin_is_rejected() {
        let f = fake(vec![user(1, false)]);
        assert_eq!(
            require_admin(&f, PlayerId(1)).await,
            Err(AdminError::NotAuthorized)
        );
        // A missing actor is also unauthorized (not a panic).
        assert_eq!(
            require_admin(&f, PlayerId(99)).await,
            Err(AdminError::NotAuthorized)
        );
    }

    #[tokio::test]
    async fn admin_can_grant_and_revoke_roles() {
        let f = fake(vec![user(1, true), user(2, false)]);
        // Grant moderator + admin to subject 2.
        set_role(
            &f,
            &f,
            &f,
            PlayerId(1),
            PlayerId(2),
            ElevatedRole::Moderator,
            true,
        )
        .await
        .unwrap();
        set_role(
            &f,
            &f,
            &f,
            PlayerId(1),
            PlayerId(2),
            ElevatedRole::Admin,
            true,
        )
        .await
        .unwrap();
        assert_eq!(*f.set_mod_calls.lock().unwrap(), vec![(2, true)]);
        assert_eq!(*f.set_admin_calls.lock().unwrap(), vec![(2, true)]);
    }

    #[tokio::test]
    async fn admin_cannot_remove_own_admin_role() {
        let f = fake(vec![user(1, true)]);
        assert_eq!(
            set_role(
                &f,
                &f,
                &f,
                PlayerId(1),
                PlayerId(1),
                ElevatedRole::Admin,
                false,
            )
            .await,
            Err(AdminError::SelfDemotion)
        );
        // But an admin *may* drop their own Moderator role (not a lockout).
        set_role(
            &f,
            &f,
            &f,
            PlayerId(1),
            PlayerId(1),
            ElevatedRole::Moderator,
            false,
        )
        .await
        .unwrap();
        assert_eq!(*f.set_mod_calls.lock().unwrap(), vec![(1, false)]);
        // No admin write happened.
        assert!(f.set_admin_calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn granting_a_missing_subject_is_not_found() {
        let f = fake(vec![user(1, true)]);
        assert_eq!(
            set_role(
                &f,
                &f,
                &f,
                PlayerId(1),
                PlayerId(42),
                ElevatedRole::Moderator,
                true,
            )
            .await,
            Err(AdminError::NotFound)
        );
    }

    // A non-admin actor cannot move roles at all (gate runs before the self/exists checks).
    #[tokio::test]
    async fn non_admin_cannot_set_roles() {
        let f = fake(vec![user(1, false), user(2, false)]);
        assert_eq!(
            set_role(
                &f,
                &f,
                &f,
                PlayerId(1),
                PlayerId(2),
                ElevatedRole::Admin,
                true,
            )
            .await,
            Err(AdminError::NotAuthorized)
        );
        assert!(f.set_admin_calls.lock().unwrap().is_empty());
    }
}
