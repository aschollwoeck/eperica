//! Lifecycle use-cases (019): the lazy beginner's-protection threshold end (evaluated on the player's
//! authenticated view) and — added in this slice's persistence task — the periodic abandonment sweep.

use crate::ports::{AccountRepository, LifecycleRepository, RepoError};
use eperica_domain::{
    EconomyRules, GameSpeed, LifecycleRules, PlayerId, Timestamp, abandon_cutoff, is_protected,
    period_index, population, protection_ended_by_population,
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

/// Settle every complete, unswept abandonment period up to `now` (019 AC7), returning `(period,
/// abandoned_count)` per period swept (usually 0 or 1; more only when catching up after downtime).
/// State-driven and idempotent, mirroring the 017 medal settlement: the latest swept period is the
/// watermark, and each period's deletion **cutoff is anchored to its boundary** so the same persisted
/// activity yields the same abandonments (P2/P6).
pub async fn process_due_lifecycle<R>(
    repo: &R,
    world_start: Timestamp,
    now: Timestamp,
    rules: &LifecycleRules,
    speed: GameSpeed,
) -> Result<Vec<(i64, usize)>, RepoError>
where
    R: LifecycleRepository,
{
    let current = period_index(now, world_start, rules.sweep_interval_secs);
    let mut next = repo
        .latest_swept_period()
        .await?
        .map_or(0, |latest| latest + 1);
    let mut swept = Vec::new();
    // Period P is complete once `now` is in a later period (its boundary has passed).
    while next < current {
        let cutoff = abandon_cutoff(
            next,
            world_start,
            rules.sweep_interval_secs,
            rules.abandon_after_secs,
            speed,
        );
        let count = repo.sweep_abandoned(next, cutoff).await?;
        swept.push((next, count));
        next += 1;
    }
    Ok(swept)
}
