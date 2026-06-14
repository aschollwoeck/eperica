//! The authentication use-case: verify credentials for login.

use crate::ports::{AccountRepository, PasswordHasher, UserRecord};
use eperica_domain::{Timestamp, account_blocked};

/// Why an authentication attempt failed.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LoginError {
    /// The username is unknown or the password is wrong (not distinguished, to avoid leaking which).
    #[error("invalid username or password")]
    InvalidCredentials,
    /// The account exists but its email is not yet confirmed.
    #[error("email not confirmed")]
    EmailNotConfirmed,
    /// The account has been abandoned by the inactivity sweep (019 AC8) — retired and cannot log in.
    #[error("account abandoned")]
    Abandoned,
    /// The account is **sanctioned** (022 AC5) — banned, or suspended and not yet expired.
    #[error("account sanctioned")]
    Sanctioned,
    /// A storage/backend failure.
    #[error("storage error: {0}")]
    Backend(String),
}

/// Authenticate a user by username + password (AC2).
///
/// Unknown user and wrong password both yield [`LoginError::InvalidCredentials`] so callers cannot
/// tell which accounts exist. A sanctioned account (banned, or suspended and not yet expired at `now`)
/// is rejected with [`LoginError::Sanctioned`] (022 AC5).
///
/// # Errors
/// See [`LoginError`].
pub async fn authenticate<R, H>(
    accounts: &R,
    hasher: &H,
    username: &str,
    password: &str,
    now: Timestamp,
) -> Result<UserRecord, LoginError>
where
    R: AccountRepository,
    H: PasswordHasher,
{
    let user = accounts
        .find_user_by_username(username)
        .await
        .map_err(|e| LoginError::Backend(e.to_string()))?;

    let Some(user) = user else {
        return Err(LoginError::InvalidCredentials);
    };

    let verified = hasher
        .verify(password, &user.password_hash)
        .map_err(|e| LoginError::Backend(e.to_string()))?;
    if !verified {
        return Err(LoginError::InvalidCredentials);
    }

    if !user.email_confirmed {
        return Err(LoginError::EmailNotConfirmed);
    }

    if user.abandoned {
        return Err(LoginError::Abandoned);
    }

    if account_blocked(user.banned_at, user.suspended_until, now) {
        return Err(LoginError::Sanctioned);
    }

    Ok(user)
}
