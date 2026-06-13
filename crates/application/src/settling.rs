//! Settling use-cases (013): dispatch settlers to a free valley and resolve the founding. Mirrors the
//! movement/oasis engines — `order_settle` validates and debits, `process_due_settles` re-validates at
//! arrival and either **founds** a new village (re-anchoring the player's culture) or **bounces** the
//! settlers home. The slot gate (`villageCount < allowedVillages`) and the free-valley check are
//! re-evaluated at arrival (P4), so an in-flight loss of the tile or the slot is handled.

use crate::ports::{
    AccountRepository, CultureRepository, DueSettle, RepoError, SettleApply, SettleOutcome,
    SettleRepository, StarvationRepository,
};
use eperica_domain::{
    BuildingKind, Coordinate, CultureRules, EconomyRules, GameSpeed, PlayerId, StartingVillage,
    TileKind, Timestamp, UnitId, UnitRules, Village, VillageId, WorldMap, allowed_villages,
    culture_rate, settle_value, slowest_speed, travel_time_secs_floored,
};

/// Why launching a settle failed (013 AC4/AC6).
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SettleError {
    /// The garrison does not hold the settler group.
    #[error("not enough settlers")]
    Insufficient,
    /// The composition is not exactly the settler group (`settlersPerVillage` settlers).
    #[error("send exactly the settler group")]
    NotSettlerGroup,
    /// The player has no free expansion slot (culture points / Residence capacity).
    #[error("no free expansion slot")]
    NoSlot,
    /// The target tile is not a free valley to settle on.
    #[error("target is not a free valley")]
    NotFreeValley,
    /// The owning village/account was not found.
    #[error("not found")]
    NotFound,
    /// A storage/backend failure.
    #[error("storage error: {0}")]
    Backend(String),
}

impl From<RepoError> for SettleError {
    fn from(e: RepoError) -> Self {
        SettleError::Backend(e.to_string())
    }
}

fn building_level(village: &Village, kind: BuildingKind) -> u8 {
    village
        .buildings
        .iter()
        .find(|b| b.kind == kind)
        .map_or(0, |b| b.level)
}

/// One Residence/Palace level per village that has one (>0); the input to the expansion-slot count.
fn residence_levels(villages: &[Village]) -> Vec<u8> {
    villages
        .iter()
        .filter_map(|v| {
            let level = building_level(v, BuildingKind::Residence)
                .max(building_level(v, BuildingKind::Palace));
            (level > 0).then_some(level)
        })
        .collect()
}

/// The player's culture points settled to `now` at the live rate (the lazy read, 013 AC1).
async fn settled_cp<C>(
    culture: &C,
    rules: &CultureRules,
    now: Timestamp,
    player: PlayerId,
) -> Result<i64, RepoError>
where
    C: CultureRepository,
{
    let (value, updated_at) = culture.player_culture(player).await?;
    let levels = culture.village_town_hall_levels(player).await?;
    let rate = culture_rate(&levels, rules);
    Ok(settle_value(value, rate, (now.0 - updated_at.0) / 1000))
}

/// Whether the player may found another village now: `villageCount < allowedVillages(cp, residences)`.
async fn has_free_slot<A, C>(
    accounts: &A,
    culture: &C,
    rules: &CultureRules,
    now: Timestamp,
    player: PlayerId,
    villages: &[Village],
) -> Result<bool, RepoError>
where
    A: AccountRepository,
    C: CultureRepository,
{
    let _ = accounts; // villages already fetched by the caller
    let cp = settled_cp(culture, rules, now, player).await?;
    let allowed = allowed_villages(cp, &residence_levels(villages), rules);
    Ok((villages.len() as u32) < allowed)
}

/// Launch a settle: send the settler group from `owner`'s first village to the free valley at `target`
/// to found a new village (013 AC4/AC6).
///
/// # Errors
/// See [`SettleError`].
#[allow(clippy::too_many_arguments)]
pub async fn order_settle<A, T, C, S>(
    accounts: &A,
    settle: &T,
    culture: &C,
    starvation: &S,
    economy_rules: &EconomyRules,
    unit_rules: &UnitRules,
    culture_rules: &CultureRules,
    map: &WorldMap,
    speed: GameSpeed,
    now: Timestamp,
    owner: PlayerId,
    selected: Option<eperica_domain::VillageId>,
    target: Coordinate,
) -> Result<(), SettleError>
where
    A: AccountRepository,
    T: SettleRepository,
    C: CultureRepository,
    S: StarvationRepository,
{
    let villages = accounts.villages_of(owner).await?;
    let Some(home) = crate::economy::pick_village(villages.clone(), selected) else {
        return Err(SettleError::NotFound);
    };
    let Some(tribe) = home.tribe else {
        return Err(SettleError::NotFound);
    };
    let roster = unit_rules.roster(tribe);

    // The composition is fixed: exactly `settlersPerVillage` of the settler unit.
    let settler = UnitId(culture_rules.settler_id.clone());
    let group = vec![(settler.clone(), culture_rules.settlers_per_village)];

    // The target must be a free valley on another tile (P4).
    if !matches!(map.tile_at(target), Some(TileKind::Valley(_)))
        || target == home.coordinate
        || accounts.village_at(target).await?.is_some()
    {
        return Err(SettleError::NotFreeValley);
    }
    // The player must have a free expansion slot.
    if !has_free_slot(accounts, culture, culture_rules, now, owner, &villages).await? {
        return Err(SettleError::NoSlot);
    }
    // The home garrison must hold the settler group.
    let garrison = accounts.garrison(home.id).await?;
    let have = garrison
        .iter()
        .find(|(u, _)| *u == settler)
        .map_or(0, |(_, c)| *c);
    if have < culture_rules.settlers_per_village {
        return Err(SettleError::Insufficient);
    }

    let Some(slowest) = slowest_speed(&group, roster) else {
        return Err(SettleError::NotSettlerGroup);
    };
    let distance = map.distance(home.coordinate, target);
    let arrive = Timestamp(now.0 + travel_time_secs_floored(distance, slowest, speed) * 1000);

    settle
        .start_settle(home.id, owner, home.coordinate, target, now, arrive, &group)
        .await
        .map_err(|e| match e {
            RepoError::Conflict => SettleError::Insufficient,
            other => SettleError::Backend(other.to_string()),
        })?;

    crate::starvation::sync_starvation_check(
        accounts,
        starvation,
        economy_rules,
        unit_rules,
        speed,
        now,
        home.id,
    )
    .await?;
    Ok(())
}

/// Claim and resolve due settles (the System actor, AC6/AC7/AC12): found a village on the target free
/// valley with a free slot, else bounce the settlers home.
///
/// # Errors
/// Propagates [`RepoError`]; a per-settle failure is logged and skipped (recovered by the requeue).
#[allow(clippy::too_many_arguments)]
pub async fn process_due_settles<A, T, C>(
    accounts: &A,
    settle: &T,
    culture: &C,
    culture_rules: &CultureRules,
    unit_rules: &UnitRules,
    template: &StartingVillage,
    map: &WorldMap,
    speed: GameSpeed,
    now: Timestamp,
    limit: i64,
) -> Result<Vec<VillageId>, RepoError>
where
    A: AccountRepository,
    T: SettleRepository,
    C: CultureRepository,
{
    let due = settle.claim_due_settles(now, limit).await?;
    let mut founded = Vec::new();
    for d in due {
        match resolve_one(
            accounts,
            settle,
            culture,
            culture_rules,
            unit_rules,
            template,
            map,
            speed,
            &d,
        )
        .await
        {
            Ok(Some(v)) => founded.push(v),
            Ok(None) => {}
            Err(e) => tracing::error!(error = %e, "failed to resolve due settle"),
        }
    }
    Ok(founded)
}

#[allow(clippy::too_many_arguments)]
async fn resolve_one<A, T, C>(
    accounts: &A,
    settle: &T,
    culture: &C,
    culture_rules: &CultureRules,
    unit_rules: &UnitRules,
    template: &StartingVillage,
    map: &WorldMap,
    speed: GameSpeed,
    d: &DueSettle,
) -> Result<Option<VillageId>, RepoError>
where
    A: AccountRepository,
    T: SettleRepository,
    C: CultureRepository,
{
    let villages = accounts.villages_of(d.owner).await?;
    let Some(home) = villages.iter().find(|v| v.id == d.home_village).cloned() else {
        return Err(RepoError::Backend("settle home village missing".into()));
    };
    let tribe = home.tribe.unwrap_or(eperica_domain::Tribe::Gauls);

    // Re-validate at arrival (P4): the tile must still be a free valley and the player must still have
    // a free slot — either could have been lost in flight.
    let tile_free = matches!(map.tile_at(d.target), Some(TileKind::Valley(_)))
        && accounts.village_at(d.target).await?.is_none();
    let slot_free = has_free_slot(
        accounts,
        culture,
        culture_rules,
        d.arrive_at,
        d.owner,
        &villages,
    )
    .await
    .unwrap_or(false);

    let outcome = if tile_free && slot_free {
        // Settle the player's culture to the founding instant at the OLD rate (the new village joins
        // the live rate from here), written in the same founding transaction.
        let culture_value = settled_cp(culture, culture_rules, d.arrive_at, d.owner).await?;
        SettleOutcome::Found { culture_value }
    } else {
        // Bounce the settlers home, paced by the settler speed.
        let roster = unit_rules.roster(tribe);
        let slow = slowest_speed(&d.troops, roster).unwrap_or(1);
        let distance = map.distance(d.target, d.origin);
        let return_arrive =
            Timestamp(d.arrive_at.0 + travel_time_secs_floored(distance, slow, speed) * 1000);
        SettleOutcome::Bounce { return_arrive }
    };
    let is_found = matches!(outcome, SettleOutcome::Found { .. });

    settle
        .apply_settle(
            SettleApply {
                movement_id: d.id,
                owner: d.owner,
                home_village: d.home_village,
                home_coord: d.origin,
                target: d.target,
                troops: d.troops.clone(),
                tribe,
                battle_at: d.arrive_at,
                outcome,
            },
            template,
        )
        .await?;
    Ok(is_found.then_some(d.home_village))
}
