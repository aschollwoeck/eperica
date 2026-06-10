//! The authentication use-case: verify credentials for login.

use crate::ports::{AccountRepository, PasswordHasher, UserRecord};

/// Why an authentication attempt failed.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LoginError {
    /// The username is unknown or the password is wrong (not distinguished, to avoid leaking which).
    #[error("invalid username or password")]
    InvalidCredentials,
    /// The account exists but its email is not yet confirmed.
    #[error("email not confirmed")]
    EmailNotConfirmed,
    /// A storage/backend failure.
    #[error("storage error: {0}")]
    Backend(String),
}

/// Authenticate a user by username + password (AC2).
///
/// Unknown user and wrong password both yield [`LoginError::InvalidCredentials`] so callers cannot
/// tell which accounts exist.
///
/// # Errors
/// See [`LoginError`].
pub async fn authenticate<R, H>(
    accounts: &R,
    hasher: &H,
    username: &str,
    password: &str,
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

    Ok(user)
}
