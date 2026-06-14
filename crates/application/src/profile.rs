//! Profile use-cases (025): editing one's own bio and reading a public profile. Server-authoritative
//! (P4) — the edit is owner-scoped by construction (the caller passes the authenticated player as the
//! subject; there is no target id to forge).

use crate::ports::{AccountRepository, ProfileView, RepoError};
use eperica_domain::{PlayerId, valid_bio};

/// Why a profile action was rejected (025).
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ProfileError {
    /// The bio exceeds the length cap.
    #[error("invalid bio")]
    Invalid,
    /// The profile does not exist.
    #[error("not found")]
    NotFound,
    /// A storage/backend failure.
    #[error("storage error: {0}")]
    Backend(String),
}

impl From<RepoError> for ProfileError {
    fn from(e: RepoError) -> Self {
        ProfileError::Backend(e.to_string())
    }
}

/// Set `owner`'s own bio (025 AC1) — validated + trimmed. Owner-scoped: callers pass the authenticated
/// session player, so a player can only edit their own profile (P4).
///
/// # Errors
/// [`ProfileError::Invalid`] if the bio is too long; otherwise a backend error.
pub async fn edit_bio<A>(accounts: &A, owner: PlayerId, bio: &str) -> Result<(), ProfileError>
where
    A: AccountRepository,
{
    if !valid_bio(bio) {
        return Err(ProfileError::Invalid);
    }
    accounts.set_bio(owner, bio.trim()).await?;
    Ok(())
}

/// A player's public profile (025 AC2), or [`ProfileError::NotFound`].
///
/// # Errors
/// See [`ProfileError`].
pub async fn view_profile<A>(accounts: &A, player: PlayerId) -> Result<ProfileView, ProfileError>
where
    A: AccountRepository,
{
    accounts
        .profile_of(player)
        .await?
        .ok_or(ProfileError::NotFound)
}
