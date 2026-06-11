//! The register use-case: create an account and its starting village.

use crate::ports::{AccountRepository, NewUser, PasswordHasher, RepoError, UserRecord};
use eperica_domain::{StartingVillage, Tribe};

/// Input to [`register`].
#[derive(Debug, Clone)]
pub struct RegisterCommand {
    /// Desired login name.
    pub username: String,
    /// Email address.
    pub email: String,
    /// Plaintext password (hashed before storage).
    pub password: String,
    /// The chosen tribe, as its slug (validated server-side, 004 AC1).
    pub tribe: String,
}

/// Why a registration attempt failed.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RegisterError {
    /// The submitted details were invalid (bad username, email, or password). Server-enforced (P4),
    /// never relying on client-side form constraints.
    #[error("{0}")]
    Invalid(String),
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
            // Registration performs no optimistic settle; treat a conflict as a backend anomaly.
            RepoError::Conflict | RepoError::Backend(_) => RegisterError::Backend(e.to_string()),
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
    let tribe = validate(&cmd)?;
    let password_hash = hasher.hash(&cmd.password)?;
    let new_user = NewUser {
        username: cmd.username.trim().to_owned(),
        email: cmd.email.trim().to_owned(),
        password_hash,
        email_confirmed: !require_email_confirmation,
        tribe,
    };
    Ok(accounts.create_account(new_user, template).await?)
}

/// Maximum allowed username length.
const MAX_USERNAME_LEN: usize = 32;
/// Minimum allowed password length.
const MIN_PASSWORD_LEN: usize = 8;

/// Validate registration input server-side (P4); returns the parsed tribe (004 AC1).
fn validate(cmd: &RegisterCommand) -> Result<Tribe, RegisterError> {
    let username = cmd.username.trim();
    if username.is_empty() || username.chars().count() > MAX_USERNAME_LEN {
        return Err(RegisterError::Invalid(format!(
            "username must be 1–{MAX_USERNAME_LEN} characters"
        )));
    }
    if !is_valid_email(cmd.email.trim()) {
        return Err(RegisterError::Invalid(
            "a valid email address is required".to_owned(),
        ));
    }
    if cmd.password.chars().count() < MIN_PASSWORD_LEN {
        return Err(RegisterError::Invalid(format!(
            "password must be at least {MIN_PASSWORD_LEN} characters"
        )));
    }
    Tribe::from_slug(cmd.tribe.trim())
        .ok_or_else(|| RegisterError::Invalid("choose a tribe to play".to_owned()))
}

/// Minimal email shape check: exactly one `@`, non-empty local part, a dotted domain, no whitespace.
fn is_valid_email(s: &str) -> bool {
    if s.contains(char::is_whitespace) {
        return false;
    }
    let mut parts = s.split('@');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(local), Some(domain), None) => {
            !local.is_empty()
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
        }
        _ => false,
    }
}
