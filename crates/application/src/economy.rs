//! The economy read use-case: load a player's village economy, computed on read (P1).

use crate::ports::{AccountRepository, RepoError};
use eperica_domain::{
    Coordinate, Economy, EconomyRules, GameSpeed, PlayerId, Timestamp, compute_economy,
};

/// A village's computed economy plus the bits needed to display it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VillageEconomy {
    /// Where the village sits.
    pub coordinate: Coordinate,
    /// Current amounts, rates, and capacities.
    pub economy: Economy,
}

/// Load the owner's (first) village economy, accruing resources from stored state to `now` (P1/P2).
///
/// Returns `None` if the player has no village (or no stored resources).
///
/// # Errors
/// Propagates [`RepoError`] from the repository.
pub async fn load_economy<R>(
    repo: &R,
    rules: &EconomyRules,
    speed: GameSpeed,
    now: Timestamp,
    owner: PlayerId,
) -> Result<Option<VillageEconomy>, RepoError>
where
    R: AccountRepository,
{
    let Some(village) = repo.villages_of(owner).await?.into_iter().next() else {
        return Ok(None);
    };
    let Some((stored, updated_at)) = repo.stored_resources(village.id).await? else {
        return Ok(None);
    };

    let elapsed_secs = (now.0 - updated_at.0) / 1000;
    let economy = compute_economy(
        stored,
        elapsed_secs,
        &village.fields,
        &village.buildings,
        rules,
        speed,
    );
    Ok(Some(VillageEconomy {
        coordinate: village.coordinate,
        economy,
    }))
}
