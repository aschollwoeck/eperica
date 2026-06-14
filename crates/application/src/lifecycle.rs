//! Lifecycle use-cases (019): the lazy beginner's-protection threshold end (evaluated on the player's
//! authenticated view) and — added in this slice's persistence task — the periodic abandonment sweep.

use crate::ports::{AccountRepository, RepoError};
use eperica_domain::{
    EconomyRules, LifecycleRules, PlayerId, Timestamp, is_protected, population,
    protection_ended_by_population,
};

/// End `player`'s beginner's protection early if they are now **established** — total population at or
/// past the threshold (019 AC4). Evaluated server-side on their authenticated view; returns `true` if
/// it ended protection on this call. Idempotent: a no-op once protection has ended or if not protected.
pub async fn end_protection_if_established<R>(
    accounts: &R,
    econ: &EconomyRules,
    rules: &LifecycleRules,
    player: PlayerId,
    now: Timestamp,
) -> Result<bool, RepoError>
where
    R: AccountRepository,
{
    if !is_protected(accounts.protection_of(player).await?, now) {
        return Ok(false); // not protected (or already ended) — nothing to do
    }
    let villages = accounts.villages_of(player).await?;
    let pop: i64 = villages
        .iter()
        .map(|v| population(&v.fields, &v.buildings, econ))
        .sum();
    if protection_ended_by_population(pop, rules.protection_population_threshold) {
        accounts.end_protection(player, now).await?;
        return Ok(true);
    }
    Ok(false)
}
