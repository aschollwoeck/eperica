//! Quest evaluation (018): gather a player's persisted progress, then complete any quests whose
//! stage-gate is now satisfied (applying their rewards) idempotently. Invoked lazily when a player
//! views their own village/quests, so it stays current without ticking. Cheap and idempotent (the
//! `(player, quest)` PK guards completion), so over-invocation is harmless.

use crate::ports::{QuestRepository, RepoError};
use eperica_domain::{EconomyRules, PlayerId, QuestDef, QuestId, newly_completed};

/// Evaluate `player`'s onboarding chain and complete any quests now satisfied (applying their
/// rewards). Returns the ids completed on this call (empty if none). Idempotent.
///
/// The chain is **stage-gated**: only the player's current (first-uncompleted) quest is eligible,
/// and completion cascades only through consecutively-satisfied quests — see
/// [`eperica_domain::newly_completed`].
pub async fn evaluate_quests<R>(
    repo: &R,
    econ: &EconomyRules,
    chain: &[QuestDef],
    player: PlayerId,
) -> Result<Vec<QuestId>, RepoError>
where
    R: QuestRepository,
{
    // Cheap early-out on the steady state (every village load hits this, P11): the chain is finite
    // and tapers off — once the player has completed it all, skip the progress gather entirely.
    let completed = repo.completed_quests(player).await?;
    if completed.len() >= chain.len() {
        return Ok(Vec::new());
    }
    let progress = repo.quest_progress(econ, player).await?;
    let mut newly = Vec::new();
    for def in newly_completed(chain, &completed, &progress) {
        if repo.complete_quest(econ, player, def).await? {
            newly.push(def.id.clone());
        }
    }
    Ok(newly)
}
