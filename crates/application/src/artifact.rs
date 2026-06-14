//! Artifact use-cases (020): the one-time release at the world's artifact-release date, and the
//! aggregation of a player's holdings into the read-time [`ArtifactEffects`] (small = the holding
//! village; large/unique = account-wide).

use crate::ports::{ArtifactRepository, RepoError};
use eperica_domain::{
    ArtifactEffects, ArtifactScope, PlayerId, Timestamp, VillageId, aggregate_effects,
};

/// The released catalogue + garrison spec needed to materialize the artifacts (mirrors the infra
/// catalogue; passed in so the use-case stays I/O-free).
pub struct ReleaseSpec<'a> {
    /// The artifacts to release.
    pub catalogue: &'a [eperica_domain::ArtifactDef],
    /// The Natar garrison unit + strength.
    pub garrison_unit: &'a str,
    pub garrison_base_count: i64,
    pub garrison_per_index: i64,
}

/// Release the artifacts if the world has reached its release date (020 AC1). Idempotent — a no-op
/// before the date or once released. Returns the number materialized on this call.
pub async fn process_due_artifact_release<R>(
    repo: &R,
    release_at: Option<Timestamp>,
    now: Timestamp,
    spec: &ReleaseSpec<'_>,
) -> Result<usize, RepoError>
where
    R: ArtifactRepository,
{
    let Some(release_at) = release_at else {
        return Ok(0); // no release scheduled
    };
    if now.0 < release_at.0 {
        return Ok(0); // not yet due
    }
    repo.release_artifacts(
        release_at,
        now,
        spec.catalogue,
        spec.garrison_unit,
        spec.garrison_base_count,
        spec.garrison_per_index,
    )
    .await
}

/// The artifact effects in force for `village`, owned by `player`: the village's own **small**
/// holdings plus the account's **large/unique** holdings (020 AC6). `ArtifactEffects::NONE` if none.
pub async fn village_effects<R>(
    repo: &R,
    player: PlayerId,
    village: VillageId,
) -> Result<ArtifactEffects, RepoError>
where
    R: ArtifactRepository,
{
    let held = repo.held_by_player(player).await?;
    if held.is_empty() {
        return Ok(ArtifactEffects::NONE);
    }
    let village_small: Vec<_> = held
        .iter()
        .filter(|h| h.holder == village && h.def.scope == ArtifactScope::Small)
        .map(|h| h.def.clone())
        .collect();
    let account_wide: Vec<_> = held
        .iter()
        .filter(|h| matches!(h.def.scope, ArtifactScope::Large | ArtifactScope::Unique))
        .map(|h| h.def.clone())
        .collect();
    Ok(aggregate_effects(&village_small, &account_wide))
}

/// The account-wide artifact effects for `player` (large/unique only) — for paths not tied to a single
/// village (e.g. training/scouting from any of the player's villages).
pub async fn account_effects<R>(repo: &R, player: PlayerId) -> Result<ArtifactEffects, RepoError>
where
    R: ArtifactRepository,
{
    let held = repo.held_by_player(player).await?;
    let account_wide: Vec<_> = held
        .into_iter()
        .filter(|h| matches!(h.def.scope, ArtifactScope::Large | ArtifactScope::Unique))
        .map(|h| h.def)
        .collect();
    Ok(aggregate_effects(&[], &account_wide))
}
