//! The economy read use-case: load a player's village economy, computed on read (P1).

use crate::ports::{AccountRepository, RepoError};
use eperica_domain::{
    Economy, EconomyRules, GameSpeed, PlayerId, ResourceAmounts, Timestamp, UnitCounts, UnitRules,
    Village, compute_economy, garrison_upkeep,
};

/// Settle a village's stored resources forward to `now` (compute-on-read, P1), net of the garrison's
/// crop upkeep — the amounts a caller debits against. Pure given the stored snapshot.
#[allow(clippy::too_many_arguments)]
pub fn settle_amounts(
    stored: ResourceAmounts,
    updated_at: Timestamp,
    now: Timestamp,
    village: &Village,
    garrison: &UnitCounts,
    rules: &EconomyRules,
    unit_rules: &UnitRules,
    speed: GameSpeed,
) -> ResourceAmounts {
    let upkeep = village
        .tribe
        .map_or(0, |t| garrison_upkeep(garrison, unit_rules.roster(t)));
    compute_economy(
        stored,
        (now.0 - updated_at.0) / 1000,
        &village.fields,
        &village.buildings,
        upkeep,
        rules,
        speed,
        village.oasis_bonus,
    )
    .amounts
}

/// A village (its fields/buildings/levels) plus its garrison and computed economy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VillageEconomy {
    /// The village, including field/building levels and coordinate.
    pub village: Village,
    /// The standing garrison (005); its upkeep is already reflected in the rates.
    pub garrison: UnitCounts,
    /// Current amounts, rates, and capacities.
    pub economy: Economy,
}

/// Load the owner's (first) village economy, accruing resources from stored state to `now` (P1/P2).
/// The garrison's crop upkeep feeds the net rate (005 AC6).
///
/// Returns `None` if the player has no village (or no stored resources).
///
/// # Errors
/// Propagates [`RepoError`] from the repository.
pub async fn load_economy<R>(
    repo: &R,
    rules: &EconomyRules,
    unit_rules: &UnitRules,
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
    let garrison = repo.garrison(village.id).await?;
    let upkeep = village
        .tribe
        .map_or(0, |t| garrison_upkeep(&garrison, unit_rules.roster(t)));

    let elapsed_secs = (now.0 - updated_at.0) / 1000;
    let economy = compute_economy(
        stored,
        elapsed_secs,
        &village.fields,
        &village.buildings,
        upkeep,
        rules,
        speed,
        village.oasis_bonus,
    );
    Ok(Some(VillageEconomy {
        village,
        garrison,
        economy,
    }))
}
