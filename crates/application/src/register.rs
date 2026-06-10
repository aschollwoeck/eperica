//! The register use-case: create an account and its starting village.

use crate::ports::{AccountRepository, NewUser, PasswordHasher, RepoError, UserRecord};
use eperica_domain::StartingVillage;

/// Input to [`register`].
#[derive(Debug, Clone)]
pub struct RegisterCommand {
    /// Desired login name.
    pub username: String,
    /// Email address.
    pub email: String,
    /// Plaintext password (hashed before storage).
    pub password: String,
}

/// Why a registration attempt failed.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RegisterError {
    /// The username or email is already in use.
    #[error("username or email already taken")]
    Taken,
    /// No free tile remained for a starting village.
    #[error("the world is full")]
    WorldFull,
    /// A storage/backend failure.
    #[error("storage error: {0}")]
    Backend(String),
}

impl From<RepoError> for RegisterError {
    fn from(e: RepoError) -> Self {
        match e {
            RepoError::Duplicate => RegisterError::Taken,
            RepoError::WorldFull => RegisterError::WorldFull,
            RepoError::Backend(s) => RegisterError::Backend(s),
        }
    }
}

/// Register a new account, creating its starting village atomically (AC1, AC3).
///
/// `require_email_confirmation` controls whether the new account starts confirmed (AC1 / Decisions).
/// All work is server-side (P4); the caller supplies no identity or coordinate.
///
/// # Errors
/// See [`RegisterError`].
pub async fn register<R, H>(
    accounts: &R,
    hasher: &H,
    template: &StartingVillage,
    require_email_confirmation: bool,
    cmd: RegisterCommand,
) -> Result<UserRecord, RegisterError>
where
    R: AccountRepository,
    H: PasswordHasher,
{
    let password_hash = hasher.hash(&cmd.password)?;
    let new_user = NewUser {
        username: cmd.username,
        email: cmd.email,
        password_hash,
        email_confirmed: !require_email_confirmation,
    };
    Ok(accounts.create_account(new_user, template).await?)
}
