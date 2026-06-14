//! Wonder use-cases (021): the one-time release of Wonder plans + conquerable sites at the world's
//! Wonder-release date, and the round-victory check (first alliance to a complete Wonder).

use crate::ports::{RepoError, WonderRepository};
use eperica_domain::{Timestamp, wonder_complete};

/// The release parameters (mirrors [`eperica_domain::WonderRules`]; passed in so the use-case stays
/// I/O-free).
pub struct WonderReleaseSpec<'a> {
    /// How many capturable plans to materialize.
    pub plan_count: u32,
    /// How many conquerable Wonder sites to materialize.
    pub site_count: u32,
    /// The Natar garrison unit + strength.
    pub garrison_unit: &'a str,
    pub garrison_base_count: i64,
    pub garrison_per_index: i64,
}

/// Release the Wonder plans + sites if the world has reached its Wonder-release date (021 AC1).
/// Idempotent — a no-op before the date or once released. Returns the number materialized.
///
/// # Errors
/// [`RepoError::Backend`] on storage failure.
pub async fn process_due_wonder_release<R>(
    repo: &R,
    release_at: Option<Timestamp>,
    now: Timestamp,
    spec: &WonderReleaseSpec<'_>,
) -> Result<usize, RepoError>
where
    R: WonderRepository,
{
    let Some(release_at) = release_at else {
        return Ok(0); // no release scheduled
    };
    if now.0 < release_at.0 {
        return Ok(0); // not yet due
    }
    repo.release_wonder(
        release_at,
        now,
        spec.plan_count,
        spec.site_count,
        spec.garrison_unit,
        spec.garrison_base_count,
        spec.garrison_per_index,
    )
    .await
}

/// Record the round's victory if any alliance has a complete Wonder (021 AC6). The standings are
/// highest-first, so the leader is the candidate; victory is recorded at most once (the repo guards
/// idempotency). Returns `true` if this call ended the round.
///
/// # Errors
/// [`RepoError::Backend`] on storage failure.
pub async fn process_due_wonder_victory<R>(repo: &R, now: Timestamp) -> Result<bool, RepoError>
where
    R: WonderRepository,
{
    if repo.world_ended().await?.is_some() {
        return Ok(false); // already won — frozen
    }
    let standings = repo.top_wonders().await?;
    let Some(leader) = standings.first() else {
        return Ok(false);
    };
    if !wonder_complete(leader.level) {
        return Ok(false); // no complete Wonder yet
    }
    repo.record_victory(leader.alliance, now).await
}
