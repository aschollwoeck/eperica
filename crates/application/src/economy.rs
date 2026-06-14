//! The economy read use-case: load a player's village economy, computed on read (P1).

use crate::ports::{AccountRepository, RepoError};
use eperica_domain::{
    Economy, EconomyRules, GameSpeed, PlayerId, ResourceAmounts, Timestamp, UnitCounts, UnitRules,
    Village, VillageId, compute_economy, garrison_upkeep,
};

/// Pick the village a multi-village request is acting on (013 AC11): the `selected` village when the
/// player owns it, otherwise their **capital** (or, failing that, their first village) — the
/// single-village default. Single-village play passes `None` and is unchanged. The lookup is one
/// `villages_of` (already on the page's hot path), so no extra query.
///
/// # Errors
/// Propagates [`RepoError`] from the repository.
pub async fn select_village<R>(
    repo: &R,
    owner: PlayerId,
    selected: Option<VillageId>,
) -> Result<Option<Village>, RepoError>
where
    R: AccountRepository,
{
    let villages = repo.villages_of(owner).await?;
    Ok(pick_village(villages, selected))
}

/// The selection rule over an already-fetched village list (kept pure so callers that hold the list
/// reuse it without a second read): the `selected` one if present, else the capital, else the first.
#[must_use]
pub fn pick_village(villages: Vec<Village>, selected: Option<VillageId>) -> Option<Village> {
    if let Some(id) = selected
        && let Some(v) = villages.iter().find(|v| v.id == id)
    {
        return Some(v.clone());
    }
    villages
        .iter()
        .find(|v| v.is_capital)
        .cloned()
        .or_else(|| villages.into_iter().next())
}

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
    let base_upkeep = village
        .tribe
        .map_or(0, |t| garrison_upkeep(garrison, unit_rules.roster(t)));
    // 020 AC6: artifact Sustenance/Storage ride the village read on every settle (consistent, P2).
    let upkeep = (base_upkeep as f64 * village.artifact_effects.upkeep).round() as i64;
    compute_economy(
        stored,
        (now.0 - updated_at.0) / 1000,
        &village.fields,
        &village.buildings,
        upkeep,
        rules,
        speed,
        village.oasis_bonus,
        village.artifact_effects.storage,
    )
    .amounts
}

/// A village (its fields/buildings/levels) plus its garrison and computed economy.
#[derive(Debug, Clone, PartialEq)]
pub struct VillageEconomy {
    /// The village, including field/building levels and coordinate.
    pub village: Village,
    /// The standing garrison (005); its upkeep is already reflected in the rates.
    pub garrison: UnitCounts,
    /// Current amounts, rates, and capacities.
    pub economy: Economy,
}

/// Load a village's economy, accruing resources from stored state to `now` (P1/P2). The garrison's
/// crop upkeep feeds the net rate (005 AC6). `selected` chooses which of the owner's villages to load
/// (013 AC11); `None` (or an unowned id) falls back to the capital / first village.
///
/// Returns `None` if the player has no village (or no stored resources).
///
/// # Errors
/// Propagates [`RepoError`] from the repository.
#[allow(clippy::too_many_arguments)]
pub async fn load_economy<R>(
    repo: &R,
    rules: &EconomyRules,
    unit_rules: &UnitRules,
    speed: GameSpeed,
    now: Timestamp,
    owner: PlayerId,
    selected: Option<VillageId>,
) -> Result<Option<VillageEconomy>, RepoError>
where
    R: AccountRepository,
{
    let Some(village) = select_village(repo, owner, selected).await? else {
        return Ok(None);
    };
    let Some((stored, updated_at)) = repo.stored_resources(village.id).await? else {
        return Ok(None);
    };
    let garrison = repo.garrison(village.id).await?;
    // 020 AC6: Sustenance reduces the garrison's crop upkeep (the village carries its artifact effects).
    let base_upkeep = village
        .tribe
        .map_or(0, |t| garrison_upkeep(&garrison, unit_rules.roster(t)));
    let upkeep = (base_upkeep as f64 * village.artifact_effects.upkeep).round() as i64;
    let economy = compute_economy(
        stored,
        (now.0 - updated_at.0) / 1000,
        &village.fields,
        &village.buildings,
        upkeep,
        rules,
        speed,
        village.oasis_bonus,
        village.artifact_effects.storage, // Storage enlarges warehouse/granary.
    );
    Ok(Some(VillageEconomy {
        village,
        garrison,
        economy,
    }))
}
