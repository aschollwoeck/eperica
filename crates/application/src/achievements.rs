//! Achievement evaluation (017): gather a player's persisted progress, then grant any newly-earned
//! catalogue entries (with rewards) idempotently. Invoked at the natural event hooks (a battle
//! resolves, a village is founded, an oasis is occupied, research completes) and lazily when a player
//! views their own stats. Cheap and idempotent (the `(player, achievement)` PK guards grants), so
//! over-invocation is harmless.

use crate::ports::{AccountRepository, AchievementRepository, RepoError};
use eperica_domain::{
    AchievementDef, AchievementId, EconomyRules, PlayerId, UnitRules, newly_earned,
};

/// Evaluate `player`'s achievements against the catalogue and grant any newly-earned ones (applying
/// their rewards). Returns the ids granted on this call (empty if none). Idempotent.
pub async fn evaluate_achievements<R>(
    repo: &R,
    econ: &EconomyRules,
    unit_rules: &UnitRules,
    catalogue: &[AchievementDef],
    player: PlayerId,
) -> Result<Vec<AchievementId>, RepoError>
where
    R: AchievementRepository + AccountRepository,
{
    // Cheap early-out on the steady state (every village load hits this, P11): if the player already
    // holds the whole catalogue, skip the progress gather entirely.
    let held = repo.held_achievements(player).await?;
    if held.len() >= catalogue.len() {
        return Ok(Vec::new());
    }
    let mut progress = repo.player_progress(econ, player).await?;
    // The "research every unit" target is the count of the tribe's **researchable** units — the tier-1
    // unit is researched by default (no `village_research` row), so it must not count toward the target.
    let villages = repo.villages_of(player).await?;
    if let Some(tribe) = villages.first().and_then(|v| v.tribe) {
        progress.tribe_unit_count = unit_rules
            .roster(tribe)
            .iter()
            .filter(|u| !u.researched_by_default())
            .count() as i64;
    }
    let mut granted = Vec::new();
    for def in newly_earned(&progress, catalogue, &held) {
        if repo.grant_achievement(econ, player, def).await? {
            granted.push(def.id.clone());
        }
    }
    Ok(granted)
}
