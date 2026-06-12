//! Oasis use-cases (012): launch an attack at an oasis tile, and resolve due oasis battles. The
//! battle math is the pure domain (`combat`), reused unchanged — an oasis is just a defender with
//! **no Wall** and **morale 1** (animals/oases have no population). This layer validates and debits
//! the attacker, gathers the oasis's defenders (seeded wild animals or stationed troops), runs
//! `resolve_battle`, decides occupation against the Outpost capacity, and applies the resolution
//! (casualties + occupy/free/clear + survivor return + report) in one transaction.

use crate::ports::{
    AccountRepository, DueOasisAttack, DueOasisReinforce, NewOasisReport, OasisBattleApply,
    OasisOwnership, OasisReinforceOutcome, OasisRepository, RepoError, StarvationRepository,
    UnitRepository,
};
use eperica_domain::{
    AttackMode, BattleInput, BuildingKind, Coordinate, EconomyRules, GameSpeed, OasisRules,
    PlayerId, Timestamp, Tribe, UnitId, UnitRules, Village, VillageId, WorldMap, add_defense,
    apply_losses, attack_power, luck_factor, oasis_garrison, regrow_step, resolve_battle,
    slowest_speed, travel_time_secs_floored,
};

/// When a cleared, unoccupied oasis next regrows: `from + regrowSecs` scaled by world speed (P7 —
/// a faster world regrows faster, mirroring travel/build scaling). Never sooner than one second.
fn regrow_due(rules: &OasisRules, speed: GameSpeed, from: Timestamp) -> Timestamp {
    let secs = (rules.regrow_secs as f64 / speed.multiplier())
        .round()
        .max(1.0) as i64;
    Timestamp(from.0 + secs * 1000)
}

/// Why launching an oasis attack failed (012 AC2).
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum OasisError {
    /// The garrison does not hold all the requested troops.
    #[error("not enough troops")]
    Insufficient,
    /// The composition is empty (or none of its types are real units).
    #[error("no troops selected")]
    EmptyComposition,
    /// The target tile holds no oasis.
    #[error("no oasis at the target")]
    NotAnOasis,
    /// The target is the player's own occupied oasis (use *reinforce* for that).
    #[error("that is your own oasis; reinforce it instead")]
    OwnOasis,
    /// Reinforce/recall targets a tile that is not an oasis the player owns.
    #[error("that is not your oasis")]
    NotYourOasis,
    /// Nothing is stationed at the oasis to recall (a race).
    #[error("nothing stationed there")]
    NothingStationed,
    /// The owning village/account was not found.
    #[error("not found")]
    NotFound,
    /// A storage/backend failure.
    #[error("storage error: {0}")]
    Backend(String),
}

impl From<RepoError> for OasisError {
    fn from(e: RepoError) -> Self {
        OasisError::Backend(e.to_string())
    }
}

fn building_level(village: &Village, kind: BuildingKind) -> u8 {
    village
        .buildings
        .iter()
        .find(|b| b.kind == kind)
        .map_or(0, |b| b.level)
}

/// Launch an attack from `owner`'s village against the oasis on the `target` tile (012 AC2).
///
/// Validates ownership, the composition, garrison availability, and the target (a tile that holds an
/// oasis the player does not already occupy); computes travel time (007, paced by the slowest unit,
/// P7); atomically debits the garrison and schedules the arrival; re-syncs the home's starvation
/// check (the garrison shrank).
///
/// # Errors
/// See [`OasisError`].
#[allow(clippy::too_many_arguments)]
pub async fn order_oasis_attack<A, O, S>(
    accounts: &A,
    oases: &O,
    starvation: &S,
    economy_rules: &EconomyRules,
    unit_rules: &UnitRules,
    map: &WorldMap,
    speed: GameSpeed,
    now: Timestamp,
    owner: PlayerId,
    target: Coordinate,
    troops: Vec<(UnitId, u32)>,
) -> Result<(), OasisError>
where
    A: AccountRepository,
    O: OasisRepository,
    S: StarvationRepository,
{
    let Some(home) = accounts.villages_of(owner).await?.into_iter().next() else {
        return Err(OasisError::NotFound);
    };
    let Some(tribe) = home.tribe else {
        return Err(OasisError::NotFound);
    };
    let roster = unit_rules.roster(tribe);

    let chosen: Vec<(UnitId, u32)> = troops.into_iter().filter(|(_, n)| *n > 0).collect();
    if chosen.is_empty() {
        return Err(OasisError::EmptyComposition);
    }

    // The target tile must hold an oasis (P4).
    if map.oasis_bonus_at(target).is_none() {
        return Err(OasisError::NotAnOasis);
    }
    // Attacking your own occupied oasis is not allowed — reinforce it instead.
    if let Some(state) = oases.oasis_at(target).await?
        && state.owner == Some(home.id)
    {
        return Err(OasisError::OwnOasis);
    }

    let garrison = accounts.garrison(home.id).await?;
    for (unit, n) in &chosen {
        let have = garrison
            .iter()
            .find(|(u, _)| u == unit)
            .map_or(0, |(_, c)| *c);
        if have < *n {
            return Err(OasisError::Insufficient);
        }
    }

    let Some(slowest) = slowest_speed(&chosen, roster) else {
        return Err(OasisError::EmptyComposition);
    };
    let distance = map.distance(home.coordinate, target);
    let secs = travel_time_secs_floored(distance, slowest, speed);
    let arrive = Timestamp(now.0 + secs * 1000);

    oases
        .start_oasis_attack(
            home.id,
            owner,
            home.coordinate,
            target,
            now,
            arrive,
            &chosen,
        )
        .await
        .map_err(|e| match e {
            RepoError::Conflict => OasisError::Insufficient,
            other => OasisError::Backend(other.to_string()),
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

/// Claim and resolve oasis-attack movements whose arrival is due (the System actor, AC3–AC5/AC10).
///
/// # Errors
/// Propagates [`RepoError`]; a per-battle failure is logged and skipped (recovered by the orphan
/// requeue and re-resolved deterministically).
#[allow(clippy::too_many_arguments)]
pub async fn process_due_oasis_combat<A, O, U>(
    accounts: &A,
    oases: &O,
    units: &U,
    economy_rules: &EconomyRules,
    unit_rules: &UnitRules,
    combat_rules: &eperica_domain::CombatRules,
    oasis_rules: &OasisRules,
    map: &WorldMap,
    speed: GameSpeed,
    world_seed: u64,
    now: Timestamp,
    limit: i64,
) -> Result<(), RepoError>
where
    A: AccountRepository,
    O: OasisRepository,
    U: UnitRepository,
{
    let due = oases.claim_due_oasis_attacks(now, limit).await?;
    for attack in due {
        if let Err(e) = resolve_oasis_one(
            accounts,
            oases,
            units,
            economy_rules,
            unit_rules,
            combat_rules,
            oasis_rules,
            map,
            speed,
            world_seed,
            &attack,
        )
        .await
        {
            tracing::error!(error = %e, "failed to resolve due oasis attack");
        }
    }
    Ok(())
}

/// Send `troops` from `owner`'s village to reinforce an oasis **they own** (012 AC7).
///
/// Validates ownership, the composition, garrison availability, and that the target is the player's
/// own occupied oasis; computes travel time (007, P7); debits the garrison and schedules the
/// stationing; re-syncs the home's starvation check.
///
/// # Errors
/// See [`OasisError`].
#[allow(clippy::too_many_arguments)]
pub async fn order_oasis_reinforce<A, O, S>(
    accounts: &A,
    oases: &O,
    starvation: &S,
    economy_rules: &EconomyRules,
    unit_rules: &UnitRules,
    map: &WorldMap,
    speed: GameSpeed,
    now: Timestamp,
    owner: PlayerId,
    target: Coordinate,
    troops: Vec<(UnitId, u32)>,
) -> Result<(), OasisError>
where
    A: AccountRepository,
    O: OasisRepository,
    S: StarvationRepository,
{
    let Some(home) = accounts.villages_of(owner).await?.into_iter().next() else {
        return Err(OasisError::NotFound);
    };
    let Some(tribe) = home.tribe else {
        return Err(OasisError::NotFound);
    };
    let roster = unit_rules.roster(tribe);

    let chosen: Vec<(UnitId, u32)> = troops.into_iter().filter(|(_, n)| *n > 0).collect();
    if chosen.is_empty() {
        return Err(OasisError::EmptyComposition);
    }

    // The target must be an oasis this village owns (P4).
    match oases.oasis_at(target).await? {
        Some(state) if state.owner == Some(home.id) => {}
        _ => return Err(OasisError::NotYourOasis),
    }

    let garrison = accounts.garrison(home.id).await?;
    for (unit, n) in &chosen {
        let have = garrison
            .iter()
            .find(|(u, _)| u == unit)
            .map_or(0, |(_, c)| *c);
        if have < *n {
            return Err(OasisError::Insufficient);
        }
    }

    let Some(slowest) = slowest_speed(&chosen, roster) else {
        return Err(OasisError::EmptyComposition);
    };
    let distance = map.distance(home.coordinate, target);
    let secs = travel_time_secs_floored(distance, slowest, speed);
    let arrive = Timestamp(now.0 + secs * 1000);

    oases
        .start_oasis_reinforce(
            home.id,
            owner,
            home.coordinate,
            target,
            now,
            arrive,
            &chosen,
        )
        .await
        .map_err(|e| match e {
            RepoError::Conflict => OasisError::Insufficient,
            other => OasisError::Backend(other.to_string()),
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

/// Recall the troops `owner` has stationed at an oasis **they own** back to their home garrison
/// (012 AC7). The oasis stays owned but undefended; the troops travel home as a `return`.
///
/// # Errors
/// See [`OasisError`].
#[allow(clippy::too_many_arguments)]
pub async fn order_oasis_recall<A, O>(
    accounts: &A,
    oases: &O,
    unit_rules: &UnitRules,
    map: &WorldMap,
    speed: GameSpeed,
    now: Timestamp,
    owner: PlayerId,
    target: Coordinate,
) -> Result<(), OasisError>
where
    A: AccountRepository,
    O: OasisRepository,
{
    let Some(home) = accounts.villages_of(owner).await?.into_iter().next() else {
        return Err(OasisError::NotFound);
    };
    let Some(tribe) = home.tribe else {
        return Err(OasisError::NotFound);
    };
    match oases.oasis_at(target).await? {
        Some(state) if state.owner == Some(home.id) => {}
        _ => return Err(OasisError::NotYourOasis),
    }

    // The stationed troops pace their own return; read them to find the slowest unit.
    let stationed = oases
        .oasis_defenders(
            target,
            unit_rules.wild_animal_roster(),
            &OasisRules {
                base_count: 0,
                extra_per_step: 0,
                tiles_per_step: 1,
                max_count: 0,
                tiles_per_tier: 1,
                regrow_secs: 1,
                regrow_per_step: 1,
            },
        )
        .await?;
    if stationed.is_empty() {
        return Err(OasisError::NothingStationed);
    }
    let roster = unit_rules.roster(tribe);
    let slowest = slowest_speed(&stationed, roster).unwrap_or(1);
    let distance = map.distance(target, home.coordinate);
    let arrive = Timestamp(now.0 + travel_time_secs_floored(distance, slowest, speed) * 1000);

    oases
        .start_oasis_recall(target, home.id, owner, home.coordinate, now, arrive)
        .await
        .map_err(|e| match e {
            RepoError::Conflict => OasisError::NothingStationed,
            other => OasisError::Backend(other.to_string()),
        })?;
    Ok(())
}

/// Claim and apply oasis reinforcements whose arrival is due (the System actor, AC7). A reinforcement
/// stations if the sender still owns the oasis, else bounces the troops home.
///
/// # Errors
/// Propagates [`RepoError`]; a per-item failure is logged and skipped (recovered by the requeue).
#[allow(clippy::too_many_arguments)]
pub async fn process_due_oasis_reinforce<A, O>(
    accounts: &A,
    oases: &O,
    unit_rules: &UnitRules,
    map: &WorldMap,
    speed: GameSpeed,
    now: Timestamp,
    limit: i64,
) -> Result<(), RepoError>
where
    A: AccountRepository,
    O: OasisRepository,
{
    let due = oases.claim_due_oasis_reinforcements(now, limit).await?;
    for r in due {
        if let Err(e) = apply_one_reinforce(accounts, oases, unit_rules, map, speed, &r).await {
            tracing::error!(error = %e, "failed to apply due oasis reinforcement");
        }
    }
    Ok(())
}

async fn apply_one_reinforce<A, O>(
    accounts: &A,
    oases: &O,
    unit_rules: &UnitRules,
    map: &WorldMap,
    speed: GameSpeed,
    due: &DueOasisReinforce,
) -> Result<(), RepoError>
where
    A: AccountRepository,
    O: OasisRepository,
{
    // Re-check ownership at arrival: station if still owned, else bounce the troops home (P4).
    let still_owned = matches!(
        oases.oasis_at(due.oasis).await?,
        Some(state) if state.owner == Some(due.home_village)
    );
    let outcome = if still_owned {
        OasisReinforceOutcome::Station
    } else {
        let home = accounts.village_by_id(due.home_village).await?;
        let home_coord = home.as_ref().map_or(due.origin, |v| v.coordinate);
        let roster = home
            .as_ref()
            .and_then(|v| v.tribe)
            .map_or(&[][..], |t| unit_rules.roster(t));
        let slowest = slowest_speed(&due.troops, roster).unwrap_or(1);
        let distance = map.distance(due.oasis, home_coord);
        let return_arrive =
            Timestamp(due.arrive_at.0 + travel_time_secs_floored(distance, slowest, speed) * 1000);
        OasisReinforceOutcome::BounceHome {
            home_coord,
            return_arrive,
        }
    };
    oases.apply_oasis_reinforce(due, outcome).await
}

/// Claim and apply due animal regrows (the System actor, AC9). Each cleared, unoccupied oasis tops
/// its animals up toward the seeded strength by one step and reschedules until full; occupying it in
/// flight cancels the regrow (enforced by the repository's guard).
///
/// # Errors
/// Propagates [`RepoError`]; a per-oasis failure is logged and skipped (re-claimed next tick).
#[allow(clippy::too_many_arguments)]
pub async fn process_due_oasis_regrow<O>(
    oases: &O,
    unit_rules: &UnitRules,
    oasis_rules: &OasisRules,
    world_seed: u64,
    speed: GameSpeed,
    now: Timestamp,
    limit: i64,
) -> Result<(), RepoError>
where
    O: OasisRepository,
{
    let due = oases.claim_due_oasis_regrows(now, limit).await?;
    for r in due {
        let seeded = oasis_garrison(
            world_seed,
            r.oasis,
            unit_rules.wild_animal_roster(),
            oasis_rules,
        );
        let (garrison, full) = regrow_step(&r.current, &seeded, oasis_rules.regrow_per_step);
        let next = (!full).then(|| regrow_due(oasis_rules, speed, now));
        if let Err(e) = oases
            .apply_oasis_regrow(r.oasis, &garrison, r.regrow_at, next)
            .await
        {
            tracing::error!(error = %e, "failed to apply due oasis regrow");
        }
    }
    Ok(())
}

/// Resolve a single due oasis attack: gather the oasis's defenders, run the battle, decide
/// occupation, and apply casualties + occupation + survivor return + report in one transaction.
#[allow(clippy::too_many_arguments)]
async fn resolve_oasis_one<A, O, U>(
    accounts: &A,
    oases: &O,
    units: &U,
    economy_rules: &EconomyRules,
    unit_rules: &UnitRules,
    combat_rules: &eperica_domain::CombatRules,
    oasis_rules: &OasisRules,
    map: &WorldMap,
    speed: GameSpeed,
    world_seed: u64,
    attack: &DueOasisAttack,
) -> Result<(), RepoError>
where
    A: AccountRepository,
    O: OasisRepository,
    U: UnitRepository,
{
    let Some(home) = accounts.village_by_id(attack.home_village).await? else {
        return Err(RepoError::Backend("oasis attacker village missing".into()));
    };

    // Attacker pools (Smithy-scaled).
    let atk_roster = home.tribe.map_or(&[][..], |t| unit_rules.roster(t));
    let atk_levels = units.unit_levels(home.id).await?;
    let power = attack_power(&attack.troops, atk_roster, &atk_levels, combat_rules);

    // The oasis's current state + defenders. An unoccupied oasis is defended by wild animals (whose
    // roster carries their defence stats); an occupied one by the owner's stationed troops (T6).
    let state = oases.oasis_at(attack.oasis).await?;
    let owner_village = state.and_then(|s| s.owner);
    let (def_roster, defender_owner): (&[_], Option<(PlayerId, VillageId)>) = match owner_village {
        Some(vid) => match accounts.village_by_id(vid).await? {
            Some(v) => (
                v.tribe.map_or(&[][..], |t| unit_rules.roster(t)),
                Some((v.owner, vid)),
            ),
            None => (unit_rules.wild_animal_roster(), None),
        },
        None => (unit_rules.wild_animal_roster(), None),
    };
    let defenders = oases
        .oasis_defenders(attack.oasis, unit_rules.wild_animal_roster(), oasis_rules)
        .await?;
    let mut totals = (0.0, 0.0);
    add_defense(&mut totals, &defenders, def_roster, &[], combat_rules);

    // Battle: no Wall, morale 1 (equal populations) — animals/oases have no population (spec AC3).
    let input = BattleInput {
        attack: power,
        def_infantry: totals.0,
        def_cavalry: totals.1,
        wall_tribe: Tribe::Gauls,
        wall_level: 0,
        attacker_pop: 1,
        defender_pop: 1,
    };
    let luck = luck_factor(world_seed, attack.id, combat_rules.luck_range);
    let outcome = resolve_battle(AttackMode::Attack, input, combat_rules, luck);

    let (defenders_after, defender_losses) = apply_losses(&defenders, outcome.defender_loss_frac);
    let (survivors, attacker_losses) = apply_losses(&attack.troops, outcome.attacker_loss_frac);

    // Occupation (AC4/AC5/AC6): on a winning attack, occupy if the attacker has free Outpost
    // capacity; otherwise clear an unoccupied oasis (Unchanged) or free a previously-occupied one.
    let ownership = if outcome.attacker_won {
        let capacity = economy_rules.outpost_capacity(building_level(&home, BuildingKind::Outpost));
        let occupied_now = oases.occupied_oases(home.id).await?.len();
        if occupied_now < usize::from(capacity) {
            OasisOwnership::Occupy(home.id)
        } else if owner_village.is_some() {
            OasisOwnership::Free
        } else {
            OasisOwnership::Unchanged
        }
    } else {
        OasisOwnership::Unchanged
    };

    // Survivors travel home (a return movement rejoins the garrison); empty ⇒ none.
    let return_arrive = match slowest_speed(&survivors, atk_roster) {
        Some(slow) => {
            let distance = map.distance(attack.oasis, attack.origin);
            Timestamp(attack.arrive_at.0 + travel_time_secs_floored(distance, slow, speed) * 1000)
        }
        None => attack.arrive_at,
    };

    let (defender_player, defender_village) = match defender_owner {
        Some((p, v)) => (Some(p), Some(v)),
        None => (None, None),
    };

    // Schedule the animal regrow (AC9) when the oasis ends **unoccupied**; an occupied one never
    // regrows (it holds the owner's troops).
    let regrow_at = match ownership {
        OasisOwnership::Occupy(_) => None,
        _ => Some(regrow_due(oasis_rules, speed, attack.arrive_at)),
    };

    oases
        .apply_oasis_battle(OasisBattleApply {
            movement_id: attack.id,
            owner: attack.owner,
            attacker_home: home.id,
            attacker_origin: attack.origin,
            oasis: attack.oasis,
            defenders_after,
            ownership,
            survivors: survivors.clone(),
            battle_at: attack.arrive_at,
            return_arrive,
            regrow_at,
            report: NewOasisReport {
                attacker_player: attack.owner,
                attacker_village: home.id,
                defender_player,
                defender_village,
                oasis: attack.oasis,
                label: oasis_label(attack.oasis),
                attacker_won: outcome.attacker_won,
                luck: outcome.luck,
                morale: outcome.morale,
                attacker_forces: attack.troops.clone(),
                attacker_losses,
                defender_forces: defenders,
                defender_losses,
            },
        })
        .await?;
    Ok(())
}

/// The synthetic defender label for an oasis report (no village name to join).
fn oasis_label(coord: Coordinate) -> String {
    format!("Oasis ({}|{})", coord.x, coord.y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{NewUser, OasisState, UserRecord, VillageMarker};
    use async_trait::async_trait;
    use eperica_domain::{
        BuildingSlot, CombatRules, FieldDistribution, MapRules, OasisBonus, ResourceAmounts,
        StartingVillage, UnitCounts, UnitRole, UnitSpec, Village, WallProfile, Weighted,
    };
    use std::collections::HashMap;
    use std::sync::Mutex;

    // A full 10-unit roster (unit 0 = "phalanx", the research-free tier-1) so `UnitRules` validates.
    fn roster() -> Vec<UnitSpec> {
        (0..10)
            .map(|i| UnitSpec {
                id: UnitId(if i == 0 {
                    "phalanx".into()
                } else {
                    format!("u{i}")
                }),
                name: format!("u{i}"),
                role: UnitRole::Infantry,
                attack: 15,
                defense_infantry: 40,
                defense_cavalry: 50,
                scouting: 0,
                speed: 7,
                carry_capacity: 35,
                crop_upkeep: 1,
                cost: ResourceAmounts {
                    wood: 1,
                    clay: 1,
                    iron: 1,
                    crop: 1,
                },
                train_secs: 1,
                trained_in: BuildingKind::Barracks,
                research: (i > 0).then(|| eperica_domain::ResearchSpec {
                    cost: ResourceAmounts {
                        wood: 1,
                        clay: 1,
                        iron: 1,
                        crop: 1,
                    },
                    time_secs: 1,
                    requirements: vec![],
                }),
                siege_kind: None,
            })
            .collect()
    }

    fn rat() -> UnitSpec {
        UnitSpec {
            id: UnitId("rat".into()),
            name: "Rat".into(),
            role: UnitRole::Wild,
            attack: 0,
            defense_infantry: 25,
            defense_cavalry: 20,
            scouting: 0,
            speed: 0,
            carry_capacity: 0,
            crop_upkeep: 0,
            cost: ResourceAmounts::default(),
            train_secs: 0,
            trained_in: BuildingKind::Barracks,
            research: None,
            siege_kind: None,
        }
    }

    fn unit_rules() -> UnitRules {
        let rosters = HashMap::from([
            (Tribe::Romans, roster()),
            (Tribe::Teutons, roster()),
            (Tribe::Gauls, roster()),
        ]);
        UnitRules::new(rosters, smithy(), training())
            .unwrap()
            .with_wild_animals(vec![rat()])
    }

    fn smithy() -> eperica_domain::SmithyRules {
        eperica_domain::SmithyRules {
            cost_permille_per_level: vec![1000],
            time_secs_per_level: vec![1],
        }
    }

    fn training() -> eperica_domain::TrainingRules {
        eperica_domain::TrainingRules {
            building_factor_per_level: vec![1.0],
        }
    }

    fn combat_rules() -> CombatRules {
        CombatRules {
            loss_exponent: 1.5,
            luck_range: 0.0,
            morale_exponent: 0.2,
            base_defense: 10.0,
            smithy_bonus_per_level: 0.015,
            catapult_durability: 100.0,
            cranny_bypass_teuton: 0.5,
            cranny_protection_per_level: vec![0],
            walls: HashMap::from([(
                Tribe::Gauls,
                WallProfile {
                    bonus_per_level: vec![1.0],
                    ram_durability: 100.0,
                },
            )]),
        }
    }

    fn oasis_rules() -> OasisRules {
        OasisRules {
            base_count: 3,
            extra_per_step: 1,
            tiles_per_step: 5,
            max_count: 50,
            tiles_per_tier: 15,
            regrow_secs: 3600,
            regrow_per_step: 2,
        }
    }

    fn economy_rules(outpost_capacity: Vec<u8>) -> EconomyRules {
        EconomyRules {
            wood_per_level: vec![0],
            clay_per_level: vec![0],
            iron_per_level: vec![0],
            crop_per_level: vec![0],
            field_population_per_level: vec![0],
            building_population_per_level: HashMap::new(),
            warehouse_capacity_per_level: vec![1_000_000],
            granary_capacity_per_level: vec![1_000_000],
            outpost_capacity_per_level: outpost_capacity,
            starting_amounts: ResourceAmounts::default(),
        }
    }

    fn map() -> WorldMap {
        // ~20% of tiles are oases, so an oasis tile is easy to find while non-oasis tiles remain.
        let rules = MapRules::new(
            200,
            0,
            vec![Weighted {
                value: FieldDistribution::new(4, 4, 4, 6).unwrap(),
                weight: 1,
            }],
            vec![Weighted {
                value: OasisBonus {
                    wood: 25,
                    clay: 0,
                    iron: 0,
                    crop: 0,
                },
                weight: 1,
            }],
        )
        .unwrap();
        WorldMap::new(7, 50, rules)
    }

    fn village(id: u128, owner: u128, coord: Coordinate, outpost: u8) -> Village {
        let mut buildings = vec![BuildingSlot {
            kind: BuildingKind::RallyPoint,
            level: 1,
        }];
        if outpost > 0 {
            buildings.push(BuildingSlot {
                kind: BuildingKind::Outpost,
                level: outpost,
            });
        }
        Village {
            id: VillageId(id),
            owner: PlayerId(owner),
            coordinate: coord,
            tribe: Some(Tribe::Gauls),
            fields: Vec::new(),
            buildings,
            oasis_bonus: Default::default(),
        }
    }

    #[derive(Default)]
    struct Fakes {
        garrison: UnitCounts,
        applied: Mutex<Option<OasisBattleApply>>,
        oasis_state: Option<OasisState>,
        occupied: usize,
        started: Mutex<Option<(VillageId, Coordinate, UnitCounts)>>,
        reinforced: Mutex<Option<(VillageId, Coordinate, UnitCounts)>>,
        reinforce_outcome: Mutex<Option<OasisReinforceOutcome>>,
        recalled: Mutex<bool>,
        home: Option<Village>,
    }

    #[async_trait]
    impl AccountRepository for Fakes {
        async fn create_account(
            &self,
            _u: NewUser,
            _t: &StartingVillage,
        ) -> Result<UserRecord, RepoError> {
            unimplemented!()
        }
        async fn find_user_by_username(&self, _u: &str) -> Result<Option<UserRecord>, RepoError> {
            Ok(None)
        }
        async fn find_user_by_id(&self, _id: PlayerId) -> Result<Option<UserRecord>, RepoError> {
            Ok(None)
        }
        async fn villages_of(&self, _owner: PlayerId) -> Result<Vec<Village>, RepoError> {
            Ok(self.home.clone().into_iter().collect())
        }
        async fn village_by_id(&self, _v: VillageId) -> Result<Option<Village>, RepoError> {
            Ok(self.home.clone())
        }
        async fn stored_resources(
            &self,
            _v: VillageId,
        ) -> Result<Option<(ResourceAmounts, Timestamp)>, RepoError> {
            Ok(None)
        }
        async fn garrison(&self, _v: VillageId) -> Result<UnitCounts, RepoError> {
            Ok(self.garrison.clone())
        }
        async fn villages_at(&self, _c: &[Coordinate]) -> Result<Vec<VillageMarker>, RepoError> {
            Ok(Vec::new())
        }
        async fn village_at(&self, _c: Coordinate) -> Result<Option<Village>, RepoError> {
            Ok(None)
        }
    }

    #[async_trait]
    impl OasisRepository for Fakes {
        async fn oasis_at(&self, _c: Coordinate) -> Result<Option<OasisState>, RepoError> {
            Ok(self.oasis_state)
        }
        async fn oasis_defenders(
            &self,
            coord: Coordinate,
            animals: &[UnitSpec],
            rules: &OasisRules,
        ) -> Result<UnitCounts, RepoError> {
            // Mirror the seeded fallback the real repo uses for an un-fought oasis.
            Ok(eperica_domain::oasis_garrison(7, coord, animals, rules))
        }
        async fn occupied_oases(
            &self,
            _v: VillageId,
        ) -> Result<Vec<(Coordinate, OasisBonus)>, RepoError> {
            Ok(vec![
                (
                    Coordinate::new(0, 0),
                    OasisBonus {
                        wood: 25,
                        clay: 0,
                        iron: 0,
                        crop: 0
                    }
                );
                self.occupied
            ])
        }
        async fn village_oasis_bonus(&self, _v: VillageId) -> Result<OasisBonus, RepoError> {
            Ok(OasisBonus {
                wood: 0,
                clay: 0,
                iron: 0,
                crop: 0,
            })
        }
        async fn start_oasis_attack(
            &self,
            home: VillageId,
            _owner: PlayerId,
            _origin: Coordinate,
            oasis: Coordinate,
            _now: Timestamp,
            _arrive: Timestamp,
            troops: &[(UnitId, u32)],
        ) -> Result<(), RepoError> {
            *self.started.lock().unwrap() = Some((home, oasis, troops.to_vec()));
            Ok(())
        }
        async fn claim_due_oasis_attacks(
            &self,
            _now: Timestamp,
            _limit: i64,
        ) -> Result<Vec<DueOasisAttack>, RepoError> {
            Ok(Vec::new())
        }
        async fn apply_oasis_battle(&self, apply: OasisBattleApply) -> Result<(), RepoError> {
            *self.applied.lock().unwrap() = Some(apply);
            Ok(())
        }
        async fn start_oasis_reinforce(
            &self,
            home: VillageId,
            _owner: PlayerId,
            _origin: Coordinate,
            oasis: Coordinate,
            _now: Timestamp,
            _arrive: Timestamp,
            troops: &[(UnitId, u32)],
        ) -> Result<(), RepoError> {
            *self.reinforced.lock().unwrap() = Some((home, oasis, troops.to_vec()));
            Ok(())
        }
        async fn claim_due_oasis_reinforcements(
            &self,
            _now: Timestamp,
            _limit: i64,
        ) -> Result<Vec<DueOasisReinforce>, RepoError> {
            Ok(Vec::new())
        }
        async fn apply_oasis_reinforce(
            &self,
            _due: &DueOasisReinforce,
            outcome: OasisReinforceOutcome,
        ) -> Result<(), RepoError> {
            *self.reinforce_outcome.lock().unwrap() = Some(outcome);
            Ok(())
        }
        async fn start_oasis_recall(
            &self,
            _oasis: Coordinate,
            _home: VillageId,
            _owner: PlayerId,
            _home_coord: Coordinate,
            _now: Timestamp,
            _arrive: Timestamp,
        ) -> Result<UnitCounts, RepoError> {
            *self.recalled.lock().unwrap() = true;
            Ok(vec![(UnitId("phalanx".into()), 5)])
        }
        async fn claim_due_oasis_regrows(
            &self,
            _now: Timestamp,
            _limit: i64,
        ) -> Result<Vec<crate::ports::DueOasisRegrow>, RepoError> {
            Ok(Vec::new())
        }
        async fn apply_oasis_regrow(
            &self,
            _oasis: Coordinate,
            _garrison: &UnitCounts,
            _prev: Timestamp,
            _next: Option<Timestamp>,
        ) -> Result<(), RepoError> {
            Ok(())
        }
    }

    #[async_trait]
    impl UnitRepository for Fakes {
        async fn start_unit_order(
            &self,
            _v: VillageId,
            _s: ResourceAmounts,
            _f: Timestamp,
            _n: Timestamp,
            _o: crate::ports::NewUnitOrder,
        ) -> Result<(), RepoError> {
            Ok(())
        }
        async fn active_unit_orders(
            &self,
            _v: VillageId,
        ) -> Result<Vec<crate::ports::ActiveUnitOrder>, RepoError> {
            Ok(Vec::new())
        }
        async fn researched_units(&self, _v: VillageId) -> Result<Vec<UnitId>, RepoError> {
            Ok(Vec::new())
        }
        async fn unit_levels(&self, _v: VillageId) -> Result<Vec<(UnitId, u8)>, RepoError> {
            Ok(Vec::new())
        }
        async fn claim_due_unit_orders(
            &self,
            _n: Timestamp,
            _l: i64,
        ) -> Result<Vec<crate::ports::DueUnitOrder>, RepoError> {
            Ok(Vec::new())
        }
        async fn apply_unit_order(&self, _d: crate::ports::DueUnitOrder) -> Result<(), RepoError> {
            Ok(())
        }
    }

    #[async_trait]
    impl StarvationRepository for Fakes {
        async fn schedule_starvation_check(
            &self,
            _v: VillageId,
            _d: Timestamp,
        ) -> Result<(), RepoError> {
            Ok(())
        }
        async fn cancel_starvation_check(&self, _v: VillageId) -> Result<(), RepoError> {
            Ok(())
        }
        async fn claim_due_starvation(
            &self,
            _n: Timestamp,
            _l: i64,
        ) -> Result<Vec<VillageId>, RepoError> {
            Ok(Vec::new())
        }
        async fn apply_starvation(
            &self,
            _v: VillageId,
            _s: ResourceAmounts,
            _f: Timestamp,
            _n: Timestamp,
            _su: &UnitCounts,
        ) -> Result<(), RepoError> {
            Ok(())
        }
        async fn resolve_starvation_check(
            &self,
            _v: VillageId,
            _r: Option<Timestamp>,
        ) -> Result<(), RepoError> {
            Ok(())
        }
    }

    fn oasis_tile(map: &WorldMap) -> Coordinate {
        eperica_domain::coordinates_within(map.radius())
            .find(|c| map.oasis_bonus_at(*c).is_some())
            .expect("seeded map has an oasis")
    }

    // AC2: sending an attack at an oasis debits + schedules; a non-oasis target is rejected.
    #[tokio::test]
    async fn order_validates_and_schedules() {
        let map = map();
        let oasis = oasis_tile(&map);
        let home = village(1, 100, Coordinate::new(0, 1), 0);
        let fakes = Fakes {
            garrison: vec![(UnitId("phalanx".into()), 50)],
            home: Some(home.clone()),
            ..Default::default()
        };
        let speed = GameSpeed::new(1.0).unwrap();
        let now = Timestamp(1_000);
        // A non-oasis tile is rejected with nothing scheduled.
        let non_oasis = eperica_domain::coordinates_within(map.radius())
            .find(|c| map.oasis_bonus_at(*c).is_none() && *c != home.coordinate)
            .unwrap();
        let err = order_oasis_attack(
            &fakes,
            &fakes,
            &fakes,
            &economy_rules(vec![0]),
            &unit_rules(),
            &map,
            speed,
            now,
            PlayerId(100),
            non_oasis,
            vec![(UnitId("phalanx".into()), 10)],
        )
        .await
        .unwrap_err();
        assert_eq!(err, OasisError::NotAnOasis);
        assert!(fakes.started.lock().unwrap().is_none());

        // A real oasis target schedules the attack.
        order_oasis_attack(
            &fakes,
            &fakes,
            &fakes,
            &economy_rules(vec![0]),
            &unit_rules(),
            &map,
            speed,
            now,
            PlayerId(100),
            oasis,
            vec![(UnitId("phalanx".into()), 10)],
        )
        .await
        .expect("schedule");
        let started = fakes.started.lock().unwrap().clone().expect("scheduled");
        assert_eq!(started.0, home.id);
        assert_eq!(started.1, oasis);
        assert_eq!(started.2, vec![(UnitId("phalanx".into()), 10)]);
    }

    // AC2: over-garrison and own-occupied-oasis are rejected.
    #[tokio::test]
    async fn order_rejects_over_garrison_and_own_oasis() {
        let map = map();
        let oasis = oasis_tile(&map);
        let home = village(1, 100, Coordinate::new(0, 1), 0);
        let speed = GameSpeed::new(1.0).unwrap();
        let now = Timestamp(0);

        let thin = Fakes {
            garrison: vec![(UnitId("phalanx".into()), 5)],
            home: Some(home.clone()),
            ..Default::default()
        };
        let err = order_oasis_attack(
            &thin,
            &thin,
            &thin,
            &economy_rules(vec![0]),
            &unit_rules(),
            &map,
            speed,
            now,
            PlayerId(100),
            oasis,
            vec![(UnitId("phalanx".into()), 10)],
        )
        .await
        .unwrap_err();
        assert_eq!(err, OasisError::Insufficient);

        let owned = Fakes {
            garrison: vec![(UnitId("phalanx".into()), 50)],
            home: Some(home.clone()),
            oasis_state: Some(OasisState {
                owner: Some(home.id),
                materialised: true,
            }),
            ..Default::default()
        };
        let err = order_oasis_attack(
            &owned,
            &owned,
            &owned,
            &economy_rules(vec![0]),
            &unit_rules(),
            &map,
            speed,
            now,
            PlayerId(100),
            oasis,
            vec![(UnitId("phalanx".into()), 10)],
        )
        .await
        .unwrap_err();
        assert_eq!(err, OasisError::OwnOasis);
    }

    // AC3/AC4: a strong attacker clears the animals and occupies (free Outpost capacity); a weak one
    // loses and changes nothing; a winner without capacity clears but stays unoccupied.
    #[tokio::test]
    async fn resolve_clears_occupies_and_respects_capacity() {
        let map = map();
        let oasis = oasis_tile(&map);
        let home = village(1, 100, Coordinate::new(0, 1), 5); // Outpost level 5
        let er = economy_rules(vec![0, 1, 1, 2, 2, 3]); // capacity(5) = 3

        // Strong attack with free capacity (0 occupied < 3) → occupy + clear.
        let strong = Fakes {
            home: Some(home.clone()),
            occupied: 0,
            ..Default::default()
        };
        let attack = DueOasisAttack {
            id: 1,
            owner: PlayerId(100),
            home_village: home.id,
            origin: home.coordinate,
            oasis,
            arrive_at: Timestamp(10_000),
            troops: vec![(UnitId("phalanx".into()), 500)],
        };
        process_due_oasis_combat_one(&strong, &er, &attack).await;
        let applied = strong.applied.lock().unwrap().clone().expect("applied");
        assert!(matches!(applied.ownership, OasisOwnership::Occupy(v) if v == home.id));
        assert!(applied.defenders_after.is_empty(), "animals cleared");
        assert!(
            !applied.survivors.is_empty(),
            "the strong attacker survives"
        );
        assert!(applied.report.attacker_won);
        assert_eq!(applied.report.defender_player, None);

        // Same battle but already at capacity → cleared, but unoccupied.
        let capped = Fakes {
            home: Some(home.clone()),
            occupied: 3,
            ..Default::default()
        };
        process_due_oasis_combat_one(&capped, &er, &attack).await;
        let applied = capped.applied.lock().unwrap().clone().expect("applied");
        assert!(matches!(applied.ownership, OasisOwnership::Unchanged));

        // A weak attack loses and changes nothing.
        let weak_home = village(1, 100, Coordinate::new(0, 1), 0);
        let weak = Fakes {
            home: Some(weak_home),
            occupied: 0,
            ..Default::default()
        };
        let weak_attack = DueOasisAttack {
            troops: vec![(UnitId("phalanx".into()), 1)],
            ..attack.clone()
        };
        process_due_oasis_combat_one(&weak, &er, &weak_attack).await;
        let applied = weak.applied.lock().unwrap().clone().expect("applied");
        assert!(matches!(applied.ownership, OasisOwnership::Unchanged));
        assert!(!applied.report.attacker_won);
        assert!(applied.survivors.is_empty(), "the weak attacker is wiped");
    }

    // AC7: reinforcing your own oasis schedules the stationing; a non-owned oasis is rejected.
    #[tokio::test]
    async fn reinforce_validates_ownership() {
        let map = map();
        let oasis = oasis_tile(&map);
        let home = village(1, 100, Coordinate::new(0, 1), 1);
        let speed = GameSpeed::new(1.0).unwrap();
        let now = Timestamp(0);

        let mine = Fakes {
            garrison: vec![(UnitId("phalanx".into()), 50)],
            home: Some(home.clone()),
            oasis_state: Some(OasisState {
                owner: Some(home.id),
                materialised: true,
            }),
            ..Default::default()
        };
        order_oasis_reinforce(
            &mine,
            &mine,
            &mine,
            &economy_rules(vec![0, 1]),
            &unit_rules(),
            &map,
            speed,
            now,
            PlayerId(100),
            oasis,
            vec![(UnitId("phalanx".into()), 10)],
        )
        .await
        .expect("reinforce");
        let r = mine.reinforced.lock().unwrap().clone().expect("scheduled");
        assert_eq!(r.0, home.id);
        assert_eq!(r.1, oasis);
        assert_eq!(r.2, vec![(UnitId("phalanx".into()), 10)]);

        // An oasis owned by someone else is rejected.
        let theirs = Fakes {
            garrison: vec![(UnitId("phalanx".into()), 50)],
            home: Some(home.clone()),
            oasis_state: Some(OasisState {
                owner: Some(VillageId(999)),
                materialised: true,
            }),
            ..Default::default()
        };
        let err = order_oasis_reinforce(
            &theirs,
            &theirs,
            &theirs,
            &economy_rules(vec![0, 1]),
            &unit_rules(),
            &map,
            speed,
            now,
            PlayerId(100),
            oasis,
            vec![(UnitId("phalanx".into()), 10)],
        )
        .await
        .unwrap_err();
        assert_eq!(err, OasisError::NotYourOasis);
    }

    // AC7: a due reinforcement stations when the sender still owns the oasis, else bounces home.
    #[tokio::test]
    async fn reinforce_stations_or_bounces() {
        let map = map();
        let oasis = oasis_tile(&map);
        let home = village(1, 100, Coordinate::new(0, 1), 1);
        let due = DueOasisReinforce {
            id: 1,
            owner: PlayerId(100),
            home_village: home.id,
            origin: home.coordinate,
            oasis,
            arrive_at: Timestamp(10_000),
            troops: vec![(UnitId("phalanx".into()), 10)],
        };

        let owned = Fakes {
            home: Some(home.clone()),
            oasis_state: Some(OasisState {
                owner: Some(home.id),
                materialised: true,
            }),
            ..Default::default()
        };
        apply_one_reinforce(
            &owned,
            &owned,
            &unit_rules(),
            &map,
            GameSpeed::new(1.0).unwrap(),
            &due,
        )
        .await
        .expect("apply");
        assert!(matches!(
            owned.reinforce_outcome.lock().unwrap().unwrap(),
            OasisReinforceOutcome::Station
        ));

        // Lost in flight → bounce the troops home.
        let lost = Fakes {
            home: Some(home.clone()),
            oasis_state: Some(OasisState {
                owner: Some(VillageId(999)),
                materialised: true,
            }),
            ..Default::default()
        };
        apply_one_reinforce(
            &lost,
            &lost,
            &unit_rules(),
            &map,
            GameSpeed::new(1.0).unwrap(),
            &due,
        )
        .await
        .expect("apply");
        assert!(matches!(
            lost.reinforce_outcome.lock().unwrap().unwrap(),
            OasisReinforceOutcome::BounceHome { .. }
        ));
    }

    /// Resolve one attack against the fakes (the loop body of `process_due_oasis_combat`).
    async fn process_due_oasis_combat_one(
        fakes: &Fakes,
        er: &EconomyRules,
        attack: &DueOasisAttack,
    ) {
        resolve_oasis_one(
            fakes,
            fakes,
            fakes,
            er,
            &unit_rules(),
            &combat_rules(),
            &oasis_rules(),
            &map(),
            GameSpeed::new(1.0).unwrap(),
            7,
            attack,
        )
        .await
        .expect("resolve");
    }
}
