//! Combat use-cases (009): launch an attack/raid, and resolve due battles. The battle math is the
//! pure domain (`combat`); this layer validates and debits the attacker, gathers the defender's
//! state, runs `resolve_battle`, applies the casualties + survivor return, and writes the report.

use crate::culture::load_culture;
use crate::economy::settle_amounts;
use crate::ports::{
    AccountRepository, ArtifactCapture, ArtifactRepository, BattleApply, CombatRepository,
    ConquestRepository, ConquestTransfer, CultureRepository, DefenderContribution, LoyaltyApply,
    MovementRepository, NewBattleReport, NewScoutReport, RazedBuilding, ReinforcementReturn,
    RepoError, ResourceWrite, UnitRepository,
};
use crate::scouting::gather_intel;
use eperica_domain::{
    AttackMode, BattleInput, BuildingKind, CombatRules, Coordinate, CultureRules, EconomyRules,
    GameSpeed, LoyaltyRules, MovementKind, PlayerId, RankingRules, ResourceAmounts, ScoutRules,
    ScoutTarget, SiegeKind, Timestamp, Tribe, UnitCounts, UnitId, UnitRole, UnitRules, UnitSpec,
    Village, VillageId, WorldMap, add_defense, administrator_count, administrator_drop,
    apply_losses, apportion, attack_power, can_capture, carry_capacity_total, catapult_power,
    conquest_outcome, cranny_protection, is_protected, loot_split, luck_factor, population,
    razed_levels, regenerate_loyalty, required_treasury_level, resolve_battle, resolve_scouting,
    scouting_power, slowest_speed, travel_time_secs_floored,
};

/// Why launching an attack/raid failed (009 AC2).
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CombatError {
    /// The garrison does not hold all the requested troops.
    #[error("not enough troops")]
    Insufficient,
    /// The composition is empty (or none of its types are real units).
    #[error("no troops selected")]
    EmptyComposition,
    /// No village occupies the target tile.
    #[error("no village at the target")]
    NoTargetThere,
    /// The target village belongs to the attacker — you cannot attack, raid, or (014) conquer your
    /// own village, whether it is the selected home tile or another of your villages (013).
    #[error("cannot attack your own village")]
    SameTile,
    /// The target's owner is under beginner's protection (019 AC2) — they cannot be attacked yet.
    #[error("that player is under beginner's protection")]
    TargetProtected,
    /// The chosen catapult target is the Wall or Rally Point (not catapultable, 011).
    #[error("that building cannot be targeted by catapults")]
    InvalidCatapultTarget,
    /// The owning village/account was not found.
    #[error("not found")]
    NotFound,
    /// A storage/backend failure.
    #[error("storage error: {0}")]
    Backend(String),
}

impl From<RepoError> for CombatError {
    fn from(e: RepoError) -> Self {
        CombatError::Backend(e.to_string())
    }
}

fn building_level(village: &Village, kind: BuildingKind) -> u8 {
    village
        .buildings
        .iter()
        .find(|b| b.kind == kind)
        .map_or(0, |b| b.level)
}

/// Choose the building catapults raze (011 AC2): the attacker's choice if the target has it at level
/// ≥ 1, else a **seeded-random** eligible building (deterministic from the world seed + movement id,
/// P6). The Wall and Rally Point are never eligible. `None` if no eligible building or nothing is razed.
fn pick_razed_target(
    target: &Village,
    chosen: Option<BuildingKind>,
    world_seed: u64,
    movement_id: u128,
    catapult_power: f64,
    rules: &CombatRules,
) -> Option<RazedBuilding> {
    let eligible: Vec<(BuildingKind, u8)> = target
        .buildings
        .iter()
        .filter(|b| {
            b.level >= 1 && !matches!(b.kind, BuildingKind::Wall | BuildingKind::RallyPoint)
        })
        .map(|b| (b.kind, b.level))
        .collect();
    if catapult_power <= 0.0 || eligible.is_empty() {
        return None;
    }
    let (kind, level) = chosen
        .and_then(|c| eligible.iter().find(|(k, _)| *k == c).copied())
        .unwrap_or_else(|| {
            let r = (luck_factor(world_seed, movement_id, 0.5) - 0.5).clamp(0.0, 0.999_999);
            let idx = ((r * eligible.len() as f64) as usize).min(eligible.len() - 1);
            eligible[idx]
        });
    let razed = razed_levels(catapult_power, rules.catapult_durability, level);
    (razed > 0).then_some(RazedBuilding {
        kind,
        before: level,
        after: level - razed,
    })
}

/// Compute the loot the surviving attackers carry off (011 AC3–AC6) and the target's looted-down,
/// snapshot-guarded resource write — `(loot, debit)`. Settles the target's resources to the arrival,
/// subtracts the Cranny protection (Teuton-adjusted), and bounds by the survivors' carry capacity.
#[allow(clippy::too_many_arguments)]
async fn compute_loot<A>(
    accounts: &A,
    target: &Village,
    target_garrison: &UnitCounts,
    survivors: &UnitCounts,
    attacker_roster: &[eperica_domain::UnitSpec],
    attacker_tribe: Option<Tribe>,
    arrive_at: Timestamp,
    combat_rules: &CombatRules,
    economy_rules: &EconomyRules,
    unit_rules: &UnitRules,
    speed: GameSpeed,
) -> Result<(ResourceAmounts, Option<ResourceWrite>), RepoError>
where
    A: AccountRepository,
{
    let Some((stored, snapshot)) = accounts.stored_resources(target.id).await? else {
        return Ok((ResourceAmounts::default(), None));
    };
    // Settle to the resolution instant (never regressing the clock below the snapshot).
    let clock = Timestamp(arrive_at.0.max(snapshot.0));
    let settled = settle_amounts(
        stored,
        snapshot,
        clock,
        target,
        target_garrison,
        economy_rules,
        unit_rules,
        speed,
    );
    let is_teuton = attacker_tribe == Some(Tribe::Teutons);
    let cap = combat_rules.cranny_capacity(building_level(target, BuildingKind::Cranny));
    let protected = cranny_protection(cap, is_teuton, combat_rules.cranny_bypass_teuton);
    let protection = ResourceAmounts {
        wood: protected,
        clay: protected,
        iron: protected,
        crop: protected,
    };
    let capacity = carry_capacity_total(survivors, attacker_roster);
    let loot = loot_split(settled, protection, capacity);
    if loot == ResourceAmounts::default() {
        return Ok((ResourceAmounts::default(), None));
    }
    let after = ResourceAmounts {
        wood: settled.wood - loot.wood,
        clay: settled.clay - loot.clay,
        iron: settled.iron - loot.iron,
        crop: settled.crop - loot.crop,
    };
    Ok((
        loot,
        Some(ResourceWrite {
            after,
            settled_from: snapshot,
            clock,
        }),
    ))
}

/// Launch an attack or raid from `owner`'s village against the village at `target` (009 AC1).
///
/// Validates ownership, the composition, garrison availability, and the target (another village on a
/// different tile); computes travel time (007, paced by the slowest unit, P7); atomically debits the
/// garrison and schedules the arrival; re-syncs the home's starvation check (the garrison shrank).
///
/// # Errors
/// See [`CombatError`].
#[allow(clippy::too_many_arguments)]
pub async fn order_attack<A, C, S>(
    accounts: &A,
    combat: &C,
    starvation: &S,
    economy_rules: &EconomyRules,
    unit_rules: &UnitRules,
    map: &WorldMap,
    speed: GameSpeed,
    now: Timestamp,
    owner: PlayerId,
    selected: Option<eperica_domain::VillageId>,
    target: Coordinate,
    troops: Vec<(UnitId, u32)>,
    mode: AttackMode,
    scout_target: Option<ScoutTarget>,
    catapult_target: Option<BuildingKind>,
) -> Result<(), CombatError>
where
    A: AccountRepository,
    C: CombatRepository,
    S: crate::ports::StarvationRepository,
{
    // The Wall (rams' job) and the ever-present Rally Point cannot be catapulted (011, P4).
    if matches!(
        catapult_target,
        Some(BuildingKind::Wall | BuildingKind::RallyPoint)
    ) {
        return Err(CombatError::InvalidCatapultTarget);
    }
    let Some(home) = crate::economy::select_village(accounts, owner, selected).await? else {
        return Err(CombatError::NotFound);
    };
    let Some(tribe) = home.tribe else {
        return Err(CombatError::NotFound);
    };
    let roster = unit_rules.roster(tribe);

    let chosen: Vec<(UnitId, u32)> = troops.into_iter().filter(|(_, n)| *n > 0).collect();
    if chosen.is_empty() {
        return Err(CombatError::EmptyComposition);
    }

    let garrison = accounts.garrison(home.id).await?;
    for (unit, n) in &chosen {
        let have = garrison
            .iter()
            .find(|(u, _)| u == unit)
            .map_or(0, |(_, c)| *c);
        if have < *n {
            return Err(CombatError::Insufficient);
        }
    }

    let Some(dest) = accounts.village_at(target).await? else {
        return Err(CombatError::NoTargetThere);
    };
    // P4 / roles: you cannot attack, raid, or conquer a village you own — not the selected home tile,
    // and (013 multi-village) not any other of your villages either. Guarding on ownership here keeps a
    // self-attack from ever becoming a movement, so the 014 conquest step can never transfer a village
    // to its own owner.
    if dest.owner == owner {
        return Err(CombatError::SameTile);
    }
    // 019 AC2: a player under beginner's protection cannot be attacked — reject before any movement is
    // created (server-authoritative, P4). One indexed lookup; the predicate is pure.
    if is_protected(accounts.protection_of(dest.owner).await?, now) {
        return Err(CombatError::TargetProtected);
    }

    let Some(slowest) = slowest_speed(&chosen, roster) else {
        return Err(CombatError::EmptyComposition);
    };
    let distance = map.distance(home.coordinate, dest.coordinate);
    // 020 AC6: a Speed artifact (carried on the sending village's read) shortens travel time.
    let base_secs = travel_time_secs_floored(distance, slowest, speed);
    let secs = ((base_secs as f64) / home.artifact_effects.troop_speed).round() as i64;
    let arrive = Timestamp(now.0 + secs * 1000);
    let kind = match mode {
        AttackMode::Attack => MovementKind::Attack,
        AttackMode::Raid => MovementKind::Raid,
    };
    // Scouts riding along scout the village too (010); pick a target, defaulting to Defenses. With no
    // scouts in the composition the attack carries no scouting intent.
    let has_scouts = chosen.iter().any(|(id, _)| {
        roster
            .iter()
            .find(|s| &s.id == id)
            .is_some_and(|s| s.role == UnitRole::Scout)
    });
    let effective_scout_target = has_scouts.then(|| scout_target.unwrap_or(ScoutTarget::Defenses));
    // A catapult target only rides an attack that actually carries catapults (011).
    let has_catapults = chosen.iter().any(|(id, _)| {
        roster
            .iter()
            .find(|s| &s.id == id)
            .is_some_and(|s| s.siege_kind == Some(SiegeKind::Catapult))
    });
    let effective_catapult_target = if has_catapults { catapult_target } else { None };

    combat
        .start_attack(
            home.id,
            dest.id,
            owner,
            home.coordinate,
            dest.coordinate,
            now,
            arrive,
            kind,
            &chosen,
            effective_scout_target,
            effective_catapult_target,
        )
        .await
        .map_err(|e| match e {
            RepoError::Conflict => CombatError::Insufficient,
            other => CombatError::Backend(other.to_string()),
        })?;

    // 019 AC3: launching an attack/raid ends the attacker's own beginner's protection — you cannot
    // shelter behind protection while on the offensive. Idempotent (no-op if already ended).
    accounts.end_protection(owner, now).await?;

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

/// Sum several compositions into one `unit → count` map (for the report's total forces/losses).
fn merge_counts(groups: &[&UnitCounts]) -> UnitCounts {
    let mut out: UnitCounts = Vec::new();
    for group in groups {
        for (id, count) in *group {
            if let Some(entry) = out.iter_mut().find(|(u, _)| u == id) {
                entry.1 += count;
            } else {
                out.push((id.clone(), *count));
            }
        }
    }
    out
}

/// Claim and resolve attack/raid movements whose arrival is due (the System actor, AC3–AC7). Returns
/// the **target** villages whose garrison shrank (callers re-sync their starvation check).
///
/// # Errors
/// Propagates [`RepoError`]; a per-battle failure is logged and skipped (recovered by the orphan
/// requeue and re-resolved deterministically).
#[allow(clippy::too_many_arguments)]
pub async fn process_due_combat<A, M, U, C>(
    accounts: &A,
    movements: &M,
    units: &U,
    combat: &C,
    economy_rules: &EconomyRules,
    unit_rules: &UnitRules,
    combat_rules: &CombatRules,
    scout_rules: &ScoutRules,
    culture_rules: &CultureRules,
    loyalty_rules: &LoyaltyRules,
    ranking_rules: &RankingRules,
    map: &WorldMap,
    speed: GameSpeed,
    world_seed: u64,
    now: Timestamp,
    limit: i64,
    treasury_levels: (u8, u8, u8),
) -> Result<Vec<VillageId>, RepoError>
where
    A: AccountRepository + CultureRepository + ConquestRepository + ArtifactRepository,
    M: MovementRepository,
    U: UnitRepository,
    C: CombatRepository,
{
    let due = combat.claim_due_attacks(now, limit).await?;
    let mut affected = Vec::new();
    for attack in due {
        match resolve_one(
            accounts,
            movements,
            units,
            combat,
            economy_rules,
            unit_rules,
            combat_rules,
            scout_rules,
            culture_rules,
            loyalty_rules,
            ranking_rules,
            map,
            speed,
            world_seed,
            treasury_levels,
            &attack,
        )
        .await
        {
            Ok(Some(target)) => affected.push(target),
            Ok(None) => {}
            Err(e) => tracing::error!(error = %e, "failed to resolve due attack"),
        }
    }
    Ok(affected)
}

/// Resolve a single due attack/raid; returns the target village id (its garrison shrank) or `None`.
#[allow(clippy::too_many_arguments)]
async fn resolve_one<A, M, U, C>(
    accounts: &A,
    movements: &M,
    units: &U,
    combat: &C,
    economy_rules: &EconomyRules,
    unit_rules: &UnitRules,
    combat_rules: &CombatRules,
    scout_rules: &ScoutRules,
    culture_rules: &CultureRules,
    loyalty_rules: &LoyaltyRules,
    ranking_rules: &RankingRules,
    map: &WorldMap,
    speed: GameSpeed,
    world_seed: u64,
    treasury_levels: (u8, u8, u8),
    attack: &crate::ports::DueAttack,
) -> Result<Option<VillageId>, RepoError>
where
    A: AccountRepository + CultureRepository + ConquestRepository + ArtifactRepository,
    M: MovementRepository,
    U: UnitRepository,
    C: CombatRepository,
{
    let (Some(target), Some(home)) = (
        accounts.village_by_id(attack.target_village).await?,
        accounts.village_by_id(attack.home_village).await?,
    ) else {
        return Err(RepoError::Backend("combat village missing".into()));
    };
    let mode = match attack.kind {
        MovementKind::Attack => AttackMode::Attack,
        MovementKind::Raid => AttackMode::Raid,
        _ => {
            return Err(RepoError::Backend(
                "non-combat kind in combat processor".into(),
            ));
        }
    };
    // 020 AC6: the artifact effects each side brings (carried on the village reads) — attacker's Eyes
    // (scout power) + the defender's Architect (building durability) and Confuser (scout defence).
    let attacker_effects = home.artifact_effects;
    let defender_effects = target.artifact_effects;

    // Attacker pools (Smithy-scaled).
    let atk_roster = home.tribe.map_or(&[][..], |t| unit_rules.roster(t));
    let atk_levels = units.unit_levels(home.id).await?;
    let power = attack_power(&attack.troops, atk_roster, &atk_levels, combat_rules);

    // Defender totals: the garrison (Smithy-scaled) plus every reinforcement group (base levels).
    let def_roster = target.tribe.map_or(&[][..], |t| unit_rules.roster(t));
    let garrison = accounts.garrison(target.id).await?;
    let def_levels = units.unit_levels(target.id).await?;
    let mut totals = (0.0, 0.0);
    add_defense(
        &mut totals,
        &garrison,
        def_roster,
        &def_levels,
        combat_rules,
    );
    let reinforcements = movements.reinforcements_at(target.id).await?;
    for group in &reinforcements {
        // The group's home tribe (carried on the row) selects its roster — no per-group lookup (P11).
        let group_roster = group.home_tribe.map_or(&[][..], |t| unit_rules.roster(t));
        add_defense(&mut totals, &group.troops, group_roster, &[], combat_rules);
    }

    let wall_tribe = target.tribe.unwrap_or(Tribe::Gauls);
    let input = BattleInput {
        attack: power,
        def_infantry: totals.0,
        def_cavalry: totals.1,
        wall_tribe,
        wall_level: building_level(&target, BuildingKind::Wall),
        attacker_pop: population(&home.fields, &home.buildings, economy_rules),
        defender_pop: population(&target.fields, &target.buildings, economy_rules),
    };
    let luck = luck_factor(world_seed, attack.id, combat_rules.luck_range);
    let outcome = resolve_battle(mode, input, combat_rules, luck);

    // Apply casualties to each party.
    let (_, defender_losses) = apply_losses(&garrison, outcome.defender_loss_frac);
    let reinforcement_losses: Vec<(VillageId, UnitCounts)> = reinforcements
        .iter()
        .map(|g| {
            (
                g.home_village,
                apply_losses(&g.troops, outcome.defender_loss_frac).1,
            )
        })
        .filter(|(_, losses)| !losses.is_empty())
        .collect();
    // Espionage sub-step (GDD §9.4 step 1) for scouts riding along — resolved **before** the main
    // battle's effect on the attacker. Scouts add no combat power (009), so they never alter
    // `outcome`. A non-scouting attack keeps the plain one-step casualty application.
    let is_scout = |id: &UnitId| {
        atk_roster
            .iter()
            .any(|s| &s.id == id && s.role == UnitRole::Scout)
    };
    let scouting = attack.scout_target.and_then(|target_type| {
        let scouts: UnitCounts = attack
            .troops
            .iter()
            .filter(|(id, _)| is_scout(id))
            .cloned()
            .collect();
        (!scouts.is_empty()).then_some((target_type, scouts))
    });

    let (survivors, attacker_losses, scout_info) = if let Some((target_type, scouts)) = &scouting {
        let non_scouts: UnitCounts = attack
            .troops
            .iter()
            .filter(|(id, _)| !is_scout(id))
            .cloned()
            .collect();
        // Eyes sharpens the attacker's scouts; Confuser hardens the defender against scouting (020).
        let attacker_power = scouting_power(scouts, atk_roster) * attacker_effects.scout_power;
        let mut defender_power = scouting_power(&garrison, def_roster);
        for group in &reinforcements {
            let group_roster = group.home_tribe.map_or(&[][..], |t| unit_rules.roster(t));
            defender_power += scouting_power(&group.troops, group_roster);
        }
        defender_power *= defender_effects.scout_defense;
        let espionage = resolve_scouting(attacker_power, defender_power, scout_rules);
        let (scouts_after, esp_loss) = apply_losses(scouts, espionage.attacker_loss_frac);
        // Then the main battle's attacker fraction hits the non-scouts and the espionage survivors.
        let (ns_surv, ns_loss) = apply_losses(&non_scouts, outcome.attacker_loss_frac);
        let (sc_surv, sc_loss) = apply_losses(&scouts_after, outcome.attacker_loss_frac);
        let survivors = merge_counts(&[&ns_surv, &sc_surv]);
        let attacker_losses = merge_counts(&[&ns_loss, &esp_loss, &sc_loss]);
        let scouts_lost = merge_counts(&[&esp_loss, &sc_loss]);
        (
            survivors,
            attacker_losses,
            Some((
                *target_type,
                scouts.clone(),
                scouts_lost,
                sc_surv,
                espionage.detected,
            )),
        )
    } else {
        let (survivors, attacker_losses) = apply_losses(&attack.troops, outcome.attacker_loss_frac);
        (survivors, attacker_losses, None)
    };

    // Catapults (011 AC2): surviving catapults raze a building when the attacker prevails.
    let razed = if outcome.attacker_won {
        // Architect (defender) toughens buildings against catapults — less effective siege power (020).
        let cat_power = catapult_power(&survivors, atk_roster, &atk_levels, combat_rules)
            * defender_effects.durability;
        pick_razed_target(
            &target,
            attack.catapult_target,
            world_seed,
            attack.id,
            cat_power,
            combat_rules,
        )
    } else {
        None
    };

    // Loot (011 AC3–AC6): surviving attackers plunder, bounded by carry capacity minus the Cranny.
    let (loot, target_debit) = if survivors.is_empty() {
        (ResourceAmounts::default(), None)
    } else {
        compute_loot(
            accounts,
            &target,
            &garrison,
            &survivors,
            atk_roster,
            home.tribe,
            attack.arrive_at,
            combat_rules,
            economy_rules,
            unit_rules,
            speed,
        )
        .await?
    };

    // The survivors travel home (a return movement); empty ⇒ none.
    let return_arrive = match slowest_speed(&survivors, atk_roster) {
        Some(slow) => {
            let distance = map.distance(attack.dest, attack.origin);
            Timestamp(attack.arrive_at.0 + travel_time_secs_floored(distance, slow, speed) * 1000)
        }
        None => attack.arrive_at,
    };

    let reinforcement_force: Vec<&UnitCounts> = reinforcements.iter().map(|g| &g.troops).collect();
    let mut defender_forces_parts = vec![&garrison];
    defender_forces_parts.extend(reinforcement_force);
    let defender_forces = merge_counts(&defender_forces_parts);
    let reinf_loss_force: Vec<&UnitCounts> = reinforcement_losses.iter().map(|(_, l)| l).collect();
    let mut defender_loss_parts = vec![&defender_losses];
    defender_loss_parts.extend(reinf_loss_force);
    let defender_losses_total = merge_counts(&defender_loss_parts);

    // 016 AC3/AC4: per-defender contributions — the target's garrison owner and each reinforcing
    // player — each tagged with the defensive value it brought (reusing `add_defense`, the same power
    // the battle math used) and, below, its share of the battle's defense points. The battle's
    // attack points value the defender troops the attacker destroyed. This records who held/killed
    // what; it changes **no** outcome.
    let defense_value =
        |troops: &UnitCounts, roster: &[UnitSpec], levels: &[(UnitId, u8)]| -> i64 {
            let mut t = (0.0, 0.0);
            add_defense(&mut t, troops, roster, levels, combat_rules);
            (t.0 + t.1).round() as i64
        };
    let mut defender_contributions = vec![DefenderContribution {
        player: target.owner,
        village: target.id,
        is_owner: true,
        forces: garrison.clone(),
        losses: defender_losses.clone(),
        defense_value: defense_value(&garrison, def_roster, &def_levels),
        defense_points: 0,
    }];
    for group in &reinforcements {
        let group_roster = group.home_tribe.map_or(&[][..], |t| unit_rules.roster(t));
        let Some(group_home) = accounts.village_by_id(group.home_village).await? else {
            continue;
        };
        defender_contributions.push(DefenderContribution {
            player: group_home.owner,
            village: group.home_village,
            is_owner: false,
            forces: group.troops.clone(),
            losses: apply_losses(&group.troops, outcome.defender_loss_frac).1,
            defense_value: defense_value(&group.troops, group_roster, &[]),
            defense_points: 0,
        });
    }
    // Attack points = the value the attacker destroyed; defense points = the value the defenders
    // destroyed, split by each defender's contributed defensive value (sum-preserving, AC4).
    let attack_points = ranking_rules.battle_value(&defender_losses_total);
    let defense_weights: Vec<i64> = defender_contributions
        .iter()
        .map(|c| c.defense_value)
        .collect();
    for (c, share) in defender_contributions.iter_mut().zip(apportion(
        ranking_rules.battle_value(&attacker_losses),
        &defense_weights,
    )) {
        c.defense_points = share;
    }

    // Scouts that rode along: gather intel (only if one survives to return) and a scouter-facing
    // report; the defender's battle report flags scouting only when detected (AC8).
    let (scouted, scout_target, scout_report) = match scout_info {
        Some((target_type, scouts_sent, scouts_lost, scout_survivors, detected)) => {
            let intel = if scout_survivors.is_empty() {
                None
            } else {
                Some(
                    gather_intel(
                        accounts,
                        &target,
                        &garrison,
                        &reinforcements,
                        target_type,
                        economy_rules,
                        unit_rules,
                        speed,
                        attack.arrive_at,
                    )
                    .await?,
                )
            };
            let report = NewScoutReport {
                scouter_player: attack.owner,
                scouter_village: home.id,
                target_player: target.owner,
                target_village: target.id,
                target_coord: attack.dest,
                target_type,
                scouts_sent,
                scouts_lost,
                detected,
                standalone: false,
                intel,
            };
            (detected, Some(target_type), Some(report))
        }
        None => (false, None, None),
    };

    // 014 (GDD §9.4 step 5): the conquest step. A WON attack that keeps a surviving administrator
    // lowers the target's loyalty; at zero — a non-capital target with the attacker holding a free
    // expansion slot (013) — the village is conquered (ownership transfers in `apply_battle`).
    let surviving_admins = administrator_count(&survivors, loyalty_rules);
    let (loyalty_before, loyalty_after, conquered, loyalty_apply) =
        if outcome.attacker_won && surviving_admins > 0 {
            let (stored, anchored) = accounts
                .village_loyalty(target.id)
                .await?
                .unwrap_or((loyalty_rules.starting_loyalty, attack.arrive_at));
            let loyalty_now = regenerate_loyalty(
                stored,
                (attack.arrive_at.0 - anchored.0) / 1000,
                loyalty_rules,
                speed,
            );
            let drop = administrator_drop(surviving_admins, world_seed, attack.id, loyalty_rules);
            // The attacker's free-slot gate + both players' culture re-anchor values, at the battle
            // instant (013): `load_culture` gives `used/allowed` and the settled `cp`.
            let attacker_view = load_culture(
                accounts,
                accounts,
                culture_rules,
                attack.arrive_at,
                attack.owner,
            )
            .await?;
            let has_slot = attacker_view.used_slots < attacker_view.allowed_villages;
            // 020 AC2 / 021 AC3: a capital and an artifact vault are never ownable, but a Natar **Wonder
            // site** is conquerable (so an alliance can take it and build the Wonder).
            let unconquerable = !target.is_conquerable();
            let oc = conquest_outcome(loyalty_now, drop, unconquerable, has_slot);
            let apply: Option<LoyaltyApply> = if oc.transferred {
                let loser_view = load_culture(
                    accounts,
                    accounts,
                    culture_rules,
                    attack.arrive_at,
                    target.owner,
                )
                .await?;
                // Surviving third-party reinforcements (rare — a winning Attack wipes them) return home.
                let mut reinforcement_returns = Vec::new();
                for g in &reinforcements {
                    let (group_survivors, _) = apply_losses(&g.troops, outcome.defender_loss_frac);
                    if group_survivors.is_empty() {
                        continue;
                    }
                    let Some(group_home) = accounts.village_by_id(g.home_village).await? else {
                        continue;
                    };
                    let group_roster = group_home.tribe.map_or(&[][..], |t| unit_rules.roster(t));
                    let arrive_at = match slowest_speed(&group_survivors, group_roster) {
                        Some(slow) => {
                            let distance = map.distance(attack.dest, group_home.coordinate);
                            Timestamp(
                                attack.arrive_at.0
                                    + travel_time_secs_floored(distance, slow, speed) * 1000,
                            )
                        }
                        None => attack.arrive_at,
                    };
                    reinforcement_returns.push(ReinforcementReturn {
                        home_village: g.home_village,
                        owner: group_home.owner,
                        home_coord: group_home.coordinate,
                        troops: group_survivors,
                        arrive_at,
                    });
                }
                Some(LoyaltyApply::Conquered(ConquestTransfer {
                    new_owner: attack.owner,
                    loser: target.owner,
                    post_conquest_loyalty: loyalty_rules.post_conquest_loyalty,
                    loser_culture_value: loser_view.cp,
                    gainer_culture_value: attacker_view.cp,
                    reinforcement_returns,
                }))
            } else if unconquerable {
                // AC5: a capital's (or Natar vault's) loyalty is pinned — the strike "reduces
                // nothing". `conquest_outcome`
                // already left `new_loyalty == loyalty_now`, so there is nothing to persist: skip the
                // write entirely (no re-anchor). The report below still records before == after so the
                // attacker learns the capital is untouchable.
                None
            } else {
                Some(LoyaltyApply::Reduced {
                    new_loyalty: oc.new_loyalty,
                })
            };
            (
                Some(loyalty_now),
                Some(oc.new_loyalty),
                oc.transferred,
                apply,
            )
        } else {
            (None, None, false, None)
        };

    // 020 AC4/AC5: a winning attack from a qualifying Treasury village claims the target's artifact
    // (from a Natar vault or a beaten player holder). The transfer rides the battle transaction.
    let artifact_capture = if outcome.attacker_won {
        match accounts.artifact_at_village(target.id).await? {
            Some(art) => {
                let (small, large, unique) = treasury_levels;
                let required = required_treasury_level(art.scope, small, large, unique);
                let treasury = building_level(&home, BuildingKind::Treasury);
                let home_holds = accounts.artifact_at_village(home.id).await?.is_some();
                if can_capture(treasury, required, home_holds) {
                    Some(ArtifactCapture {
                        artifact_id: art.id.0,
                        from_village: target.id,
                        to_village: home.id,
                    })
                } else {
                    None
                }
            }
            None => None,
        }
    } else {
        None
    };

    combat
        .apply_battle(BattleApply {
            movement_id: attack.id,
            owner: attack.owner,
            attacker_home: home.id,
            attacker_origin: attack.origin,
            target: target.id,
            target_coord: attack.dest,
            defender_losses,
            reinforcement_losses,
            survivors,
            battle_at: attack.arrive_at,
            return_arrive,
            report: NewBattleReport {
                kind: attack.kind,
                attacker_player: attack.owner,
                attacker_village: home.id,
                defender_player: target.owner,
                defender_village: target.id,
                attacker_won: outcome.attacker_won,
                luck: outcome.luck,
                morale: outcome.morale,
                wall_before: outcome.wall_before,
                wall_after: outcome.wall_after,
                attacker_forces: attack.troops.clone(),
                attacker_losses,
                defender_forces,
                defender_losses: defender_losses_total,
                loot,
                razed,
                loyalty_before,
                loyalty_after,
                conquered,
            },
            scouted,
            scout_target,
            scout_report,
            loot,
            target_debit,
            razed,
            loyalty: loyalty_apply,
            attack_points,
            defender_contributions,
            artifact_capture,
        })
        .await?;
    Ok(Some(target.id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{
        BattleReportView, DueAttack, NewUser, StarvationRepository, UserRecord, VillageMarker,
    };
    use async_trait::async_trait;
    use eperica_domain::{
        BuildingSlot, FieldDistribution, MapRules, OasisBonus, ResearchSpec, ResourceAmounts,
        SmithyRules, StartingVillage, TrainingRules, UnitCounts, UnitRole, UnitSpec, Village,
        Weighted,
    };
    use std::collections::HashMap;
    use std::sync::Mutex;

    fn amounts(n: i64) -> ResourceAmounts {
        ResourceAmounts {
            wood: n,
            clay: n,
            iron: n,
            crop: n,
        }
    }

    fn village(id: u128, owner: u128, coord: Coordinate) -> Village {
        Village {
            id: VillageId(id),
            owner: PlayerId(owner),
            coordinate: coord,
            tribe: Some(Tribe::Gauls),
            fields: FieldDistribution::new(4, 4, 4, 6).unwrap().fields(),
            buildings: vec![BuildingSlot {
                kind: BuildingKind::RallyPoint,
                level: 1,
            }],
            oasis_bonus: Default::default(),
            is_capital: false,
            is_natar: false,
            is_wonder_site: false,
            artifact_effects: eperica_domain::ArtifactEffects::NONE,
        }
    }

    struct FakeAccounts {
        home: Village,
        garrison: UnitCounts,
        target: Option<Village>,
        /// The target village's garrison (defenders), keyed separately from the home garrison.
        target_garrison: UnitCounts,
    }

    #[async_trait]
    impl AccountRepository for FakeAccounts {
        async fn create_account(
            &self,
            _u: NewUser,
            _t: &StartingVillage,
        ) -> Result<UserRecord, RepoError> {
            unreachable!()
        }
        async fn find_user_by_username(&self, _u: &str) -> Result<Option<UserRecord>, RepoError> {
            Ok(None)
        }
        async fn find_user_by_id(&self, _id: PlayerId) -> Result<Option<UserRecord>, RepoError> {
            Ok(None)
        }
        async fn villages_of(&self, _owner: PlayerId) -> Result<Vec<Village>, RepoError> {
            Ok(vec![self.home.clone()])
        }
        async fn village_by_id(&self, v: VillageId) -> Result<Option<Village>, RepoError> {
            if Some(v) == self.target.as_ref().map(|t| t.id) {
                Ok(self.target.clone())
            } else {
                Ok(Some(self.home.clone()))
            }
        }
        async fn stored_resources(
            &self,
            _v: VillageId,
        ) -> Result<Option<(ResourceAmounts, Timestamp)>, RepoError> {
            // The target holds 500 of each resource at snapshot 0 (for the loot path, 011).
            Ok(Some((amounts(500), Timestamp(0))))
        }
        async fn garrison(&self, v: VillageId) -> Result<UnitCounts, RepoError> {
            if v == self.home.id {
                Ok(self.garrison.clone())
            } else {
                Ok(self.target_garrison.clone())
            }
        }
        async fn villages_at(&self, _c: &[Coordinate]) -> Result<Vec<VillageMarker>, RepoError> {
            Ok(Vec::new())
        }
        async fn village_at(&self, _c: Coordinate) -> Result<Option<Village>, RepoError> {
            Ok(self.target.clone())
        }
    }

    // The conquest step (014) only fires when the attack carries a surviving administrator; the 009
    // tests carry none, so these are never exercised — trivial impls satisfy the bounds.
    #[async_trait]
    impl CultureRepository for FakeAccounts {
        async fn player_culture(&self, _p: PlayerId) -> Result<(i64, Timestamp), RepoError> {
            Ok((0, Timestamp(0)))
        }
        async fn settle_culture(
            &self,
            _p: PlayerId,
            _value: i64,
            _at: Timestamp,
        ) -> Result<(), RepoError> {
            Ok(())
        }
        async fn village_town_hall_levels(&self, _p: PlayerId) -> Result<Vec<u8>, RepoError> {
            Ok(Vec::new())
        }
    }

    #[async_trait]
    impl ConquestRepository for FakeAccounts {
        async fn village_loyalty(
            &self,
            _v: VillageId,
        ) -> Result<Option<(i64, Timestamp)>, RepoError> {
            Ok(Some((100, Timestamp(0))))
        }
        async fn set_loyalty(
            &self,
            _v: VillageId,
            _value: i64,
            _at: Timestamp,
        ) -> Result<(), RepoError> {
            Ok(())
        }
    }

    // 020: the combat fake holds no artifacts (defaults: never captures) — exercises the no-transfer path.
    impl ArtifactRepository for FakeAccounts {}

    #[derive(Clone)]
    struct Sent {
        home: VillageId,
        deliver: VillageId,
        kind: MovementKind,
        troops: UnitCounts,
        arrive: Timestamp,
        scout_target: Option<ScoutTarget>,
        catapult_target: Option<BuildingKind>,
    }

    #[derive(Default)]
    struct FakeCombat {
        sent: Mutex<Option<Sent>>,
        /// Attacks the combat processor will claim (drained once).
        due: Mutex<Vec<DueAttack>>,
        /// The resolved battle the processor handed back (captured for assertions).
        applied: Mutex<Option<BattleApply>>,
    }

    #[async_trait]
    impl CombatRepository for FakeCombat {
        async fn start_attack(
            &self,
            home: VillageId,
            deliver: VillageId,
            _owner: PlayerId,
            _origin: Coordinate,
            _dest: Coordinate,
            _now: Timestamp,
            arrive_at: Timestamp,
            kind: MovementKind,
            troops: &[(UnitId, u32)],
            scout_target: Option<ScoutTarget>,
            catapult_target: Option<BuildingKind>,
        ) -> Result<(), RepoError> {
            *self.sent.lock().unwrap() = Some(Sent {
                home,
                deliver,
                kind,
                troops: troops.to_vec(),
                arrive: arrive_at,
                scout_target,
                catapult_target,
            });
            Ok(())
        }
        async fn claim_due_attacks(
            &self,
            _now: Timestamp,
            _limit: i64,
        ) -> Result<Vec<DueAttack>, RepoError> {
            Ok(std::mem::take(&mut self.due.lock().unwrap()))
        }
        async fn apply_battle(&self, a: BattleApply) -> Result<(), RepoError> {
            *self.applied.lock().unwrap() = Some(a);
            Ok(())
        }
        async fn reports_for(
            &self,
            _p: PlayerId,
            _l: i64,
        ) -> Result<Vec<BattleReportView>, RepoError> {
            Ok(Vec::new())
        }
        async fn report(
            &self,
            _id: u128,
            _p: PlayerId,
        ) -> Result<Option<BattleReportView>, RepoError> {
            Ok(None)
        }
    }

    struct NoopStarvation;
    #[async_trait]
    impl StarvationRepository for NoopStarvation {
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

    fn roster() -> Vec<UnitSpec> {
        // u0..u7 are infantry (carry 50); u8 is a Catapult (siege, carry 0); u9 is a Scout — so the
        // loot (011), catapult-damage (011), and scout (010) paths can all be exercised.
        (0..10)
            .map(|i| UnitSpec {
                id: UnitId(format!("u{i}")),
                name: format!("u{i}"),
                role: match i {
                    9 => UnitRole::Scout,
                    8 => UnitRole::Siege,
                    _ => UnitRole::Infantry,
                },
                attack: if i == 9 { 0 } else { 10 },
                defense_infantry: 10,
                defense_cavalry: 10,
                scouting: if i == 9 { 20 } else { 0 },
                speed: 6 + i as u32,
                carry_capacity: if i < 8 { 50 } else { 0 },
                crop_upkeep: 0,
                point_value: 0,
                cost: amounts(1),
                train_secs: 1,
                trained_in: BuildingKind::Barracks,
                research: (i > 0).then(|| ResearchSpec {
                    cost: amounts(1),
                    time_secs: 1,
                    requirements: vec![],
                }),
                siege_kind: (i == 8).then_some(SiegeKind::Catapult),
            })
            .collect()
    }

    fn unit_rules() -> UnitRules {
        UnitRules::new(
            HashMap::from([
                (Tribe::Romans, roster()),
                (Tribe::Teutons, roster()),
                (Tribe::Gauls, roster()),
            ]),
            SmithyRules {
                cost_permille_per_level: vec![1500],
                time_secs_per_level: vec![3600],
            },
            TrainingRules {
                building_factor_per_level: vec![1.0],
            },
        )
        .unwrap()
    }

    fn map() -> WorldMap {
        let rules = MapRules::new(
            0,
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
        WorldMap::new(1, 50, rules)
    }

    fn economy_rules() -> EconomyRules {
        EconomyRules {
            wood_per_level: vec![0],
            clay_per_level: vec![0],
            iron_per_level: vec![0],
            crop_per_level: vec![0],
            field_population_per_level: vec![0],
            building_population_per_level: HashMap::new(),
            warehouse_capacity_per_level: vec![1_000_000],
            granary_capacity_per_level: vec![1_000_000],
            outpost_capacity_per_level: vec![0, 1, 2, 3],
            starting_amounts: amounts(0),
        }
    }

    fn accounts(garrison: UnitCounts, target: Option<Village>) -> FakeAccounts {
        FakeAccounts {
            home: village(1, 1, Coordinate::new(0, 0)),
            garrison,
            target,
            target_garrison: Vec::new(),
        }
    }

    fn combat_rules() -> CombatRules {
        use eperica_domain::WallProfile;
        let wall = || WallProfile {
            bonus_per_level: vec![0.0, 0.03],
            ram_durability: 100.0,
        };
        CombatRules {
            loss_exponent: 1.5,
            luck_range: 0.25,
            morale_exponent: 0.2,
            base_defense: 10.0,
            smithy_bonus_per_level: 0.015,
            catapult_durability: 100.0,
            cranny_bypass_teuton: 0.5,
            cranny_protection_per_level: vec![0, 1000, 2000],
            walls: HashMap::from([
                (Tribe::Romans, wall()),
                (Tribe::Teutons, wall()),
                (Tribe::Gauls, wall()),
            ]),
        }
    }

    // Culture/loyalty balance for the combat resolver's conquest step (unexercised by the 009 tests,
    // which carry no administrators).
    fn culture_rules() -> CultureRules {
        CultureRules {
            base_cp_per_village: 2,
            town_hall_cp_per_level: vec![0, 5],
            cp_thresholds: vec![0, 0, 200],
            expansion_slots_per_level: vec![0, 1],
            settlers_per_village: 3,
            settler_id: "settler".to_owned(),
        }
    }

    fn loyalty_rules() -> LoyaltyRules {
        LoyaltyRules {
            starting_loyalty: 100,
            post_conquest_loyalty: 25,
            regen_per_hour: 2,
            drop_min: 20,
            drop_max: 30,
            administrator_ids: vec!["senator".to_owned()],
        }
    }

    fn ranking_rules() -> RankingRules {
        // Points are asserted in infra DB tests; these resolve tests only need a valid value.
        RankingRules {
            point_value: std::collections::HashMap::new(),
            windows_secs: Vec::new(),
            page_size: 100,
        }
    }

    struct FakeUnits;
    #[async_trait]
    impl UnitRepository for FakeUnits {
        async fn start_unit_order(
            &self,
            _v: VillageId,
            _s: ResourceAmounts,
            _sf: Timestamp,
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

    /// A movements repo whose target stations a configurable counter-espionage / defence group.
    struct FakeMovements {
        reinforcements: Vec<crate::ports::StationedGroup>,
    }
    #[async_trait]
    impl MovementRepository for FakeMovements {
        async fn start_reinforcement(
            &self,
            _h: VillageId,
            _d: VillageId,
            _o: PlayerId,
            _og: Coordinate,
            _ds: Coordinate,
            _n: Timestamp,
            _a: Timestamp,
            _t: &[(UnitId, u32)],
        ) -> Result<(), RepoError> {
            Ok(())
        }
        async fn start_return(
            &self,
            _h: VillageId,
            _d: VillageId,
            _o: PlayerId,
            _og: Coordinate,
            _ds: Coordinate,
            _n: Timestamp,
            _a: Timestamp,
        ) -> Result<UnitCounts, RepoError> {
            Ok(Vec::new())
        }
        async fn active_movements(
            &self,
            _o: PlayerId,
        ) -> Result<Vec<crate::ports::MovementView>, RepoError> {
            Ok(Vec::new())
        }
        async fn reinforcements_at(
            &self,
            _v: VillageId,
        ) -> Result<Vec<crate::ports::StationedGroup>, RepoError> {
            Ok(self.reinforcements.clone())
        }
        async fn reinforcements_of(
            &self,
            _o: PlayerId,
        ) -> Result<Vec<crate::ports::StationedGroup>, RepoError> {
            Ok(Vec::new())
        }
        async fn claim_due_movements(
            &self,
            _n: Timestamp,
            _l: i64,
        ) -> Result<Vec<crate::ports::DueMovement>, RepoError> {
            Ok(Vec::new())
        }
        async fn apply_movement(
            &self,
            _d: &crate::ports::DueMovement,
            _credit: Option<crate::ports::ResourceWrite>,
        ) -> Result<(), RepoError> {
            Ok(())
        }
    }

    async fn send(
        acc: &FakeAccounts,
        cb: &FakeCombat,
        troops: Vec<(UnitId, u32)>,
        mode: AttackMode,
    ) -> Result<(), CombatError> {
        order_attack(
            acc,
            cb,
            &NoopStarvation,
            &economy_rules(),
            &unit_rules(),
            &map(),
            GameSpeed::new(1.0).unwrap(),
            Timestamp(0),
            PlayerId(1),
            None,
            Coordinate::new(3, 4), // distance 5 from home
            troops,
            mode,
            None,
            None,
        )
        .await
    }

    async fn send_target(
        acc: &FakeAccounts,
        cb: &FakeCombat,
        troops: Vec<(UnitId, u32)>,
        scout_target: Option<ScoutTarget>,
    ) -> Result<(), CombatError> {
        order_attack(
            acc,
            cb,
            &NoopStarvation,
            &economy_rules(),
            &unit_rules(),
            &map(),
            GameSpeed::new(1.0).unwrap(),
            Timestamp(0),
            PlayerId(1),
            None,
            Coordinate::new(3, 4),
            troops,
            AttackMode::Attack,
            scout_target,
            None,
        )
        .await
    }

    // AC2: an attack carrying scouts records a scout target (defaulting to Defenses); one without
    // scouts records none.
    #[tokio::test]
    async fn combined_send_sets_scout_target() {
        let target = || Some(village(2, 2, Coordinate::new(3, 4)));
        // Scouts present, no explicit choice ⇒ defaults to Defenses.
        let acc = accounts(
            vec![(UnitId("u0".into()), 5), (UnitId("u9".into()), 5)],
            target(),
        );
        let cb = FakeCombat::default();
        send_target(
            &acc,
            &cb,
            vec![(UnitId("u0".into()), 2), (UnitId("u9".into()), 3)],
            None,
        )
        .await
        .unwrap();
        assert_eq!(
            cb.sent.lock().unwrap().clone().unwrap().scout_target,
            Some(ScoutTarget::Defenses)
        );
        // No catapults in these sends ⇒ no catapult target carried (011; T4 wires the real value).
        assert_eq!(
            cb.sent.lock().unwrap().clone().unwrap().catapult_target,
            None
        );

        // Scouts present, explicit Resources ⇒ carried through.
        send_target(
            &acc,
            &cb,
            vec![(UnitId("u9".into()), 1)],
            Some(ScoutTarget::Resources),
        )
        .await
        .unwrap();
        assert_eq!(
            cb.sent.lock().unwrap().clone().unwrap().scout_target,
            Some(ScoutTarget::Resources)
        );

        // No scouts ⇒ no scouting intent even if a target is offered.
        send_target(
            &acc,
            &cb,
            vec![(UnitId("u0".into()), 1)],
            Some(ScoutTarget::Resources),
        )
        .await
        .unwrap();
        assert_eq!(cb.sent.lock().unwrap().clone().unwrap().scout_target, None);
    }

    // AC7: scouts riding an attack that is annihilated bring **no** intel home, though espionage saw
    // the target — the espionage survivors die in the main battle, so no scout returns to carry it.
    #[tokio::test]
    async fn combined_attack_wiped_returns_no_intel() {
        let mut acc = accounts(Vec::new(), Some(village(2, 2, Coordinate::new(3, 4))));
        // A crushing defence wipes the weak attacker; one defending scout provides counter-espionage.
        acc.target_garrison = vec![(UnitId("u0".into()), 1000), (UnitId("u9".into()), 1)];
        let cb = FakeCombat::default();
        *cb.due.lock().unwrap() = vec![DueAttack {
            id: 1,
            kind: MovementKind::Attack,
            owner: PlayerId(1),
            home_village: VillageId(1),
            target_village: VillageId(2),
            origin: Coordinate::new(0, 0),
            dest: Coordinate::new(3, 4),
            arrive_at: Timestamp(1_000_000),
            troops: vec![(UnitId("u0".into()), 1), (UnitId("u9".into()), 3)],
            scout_target: Some(ScoutTarget::Defenses),
            catapult_target: None,
        }];
        let mv = FakeMovements {
            reinforcements: Vec::new(),
        };
        process_due_combat(
            &acc,
            &mv,
            &FakeUnits,
            &cb,
            &economy_rules(),
            &unit_rules(),
            &combat_rules(),
            &ScoutRules { loss_exponent: 1.5 },
            &culture_rules(),
            &loyalty_rules(),
            &ranking_rules(),
            &map(),
            GameSpeed::new(1.0).unwrap(),
            42,
            Timestamp(1_000_000),
            100,
            (3, 6, 10),
        )
        .await
        .unwrap();

        let applied = cb.applied.lock().unwrap().clone().expect("applied");
        // The attacker is wiped — no survivors return at all.
        assert!(applied.survivors.is_empty());
        // Espionage detected the scouting (a defending scout killed ≥1), but no scout came home, so
        // the intel is lost even though espionage gathered it.
        assert!(applied.scouted);
        let report = applied.scout_report.expect("scout report");
        assert!(report.intel.is_none());
        assert!(!report.scouts_lost.is_empty());
        // AC7 (011): a wiped attacker loots nothing and razes nothing.
        assert_eq!(applied.loot, ResourceAmounts::default());
        assert!(applied.target_debit.is_none());
        assert!(applied.razed.is_none());
    }

    // 014 AC3/AC4/AC10: a won attack carrying a surviving administrator lowers the target's loyalty;
    // with loyalty staying above zero the village is **not** taken, and the report records the change.
    #[tokio::test]
    async fn admin_attack_lowers_loyalty_without_conquest() {
        // An empty-defended, non-capital target; the attacker overwhelms it and the administrator
        // (here `u1`) survives to strike loyalty.
        let acc = accounts(Vec::new(), Some(village(2, 2, Coordinate::new(3, 4))));
        let cb = FakeCombat::default();
        *cb.due.lock().unwrap() = vec![DueAttack {
            id: 7,
            kind: MovementKind::Attack,
            owner: PlayerId(1),
            home_village: VillageId(1),
            target_village: VillageId(2),
            origin: Coordinate::new(0, 0),
            dest: Coordinate::new(3, 4),
            arrive_at: Timestamp(1_000_000),
            troops: vec![(UnitId("u0".into()), 50), (UnitId("u1".into()), 2)],
            scout_target: None,
            catapult_target: None,
        }];
        let mv = FakeMovements {
            reinforcements: Vec::new(),
        };
        let loyalty = LoyaltyRules {
            starting_loyalty: 100,
            post_conquest_loyalty: 25,
            regen_per_hour: 2,
            drop_min: 20,
            drop_max: 30,
            administrator_ids: vec!["u1".to_owned()],
        };
        process_due_combat(
            &acc,
            &mv,
            &FakeUnits,
            &cb,
            &economy_rules(),
            &unit_rules(),
            &combat_rules(),
            &ScoutRules { loss_exponent: 1.5 },
            &culture_rules(),
            &loyalty,
            &ranking_rules(),
            &map(),
            GameSpeed::new(1.0).unwrap(),
            42,
            Timestamp(1_000_000),
            100,
            (3, 6, 10),
        )
        .await
        .unwrap();

        let applied = cb.applied.lock().unwrap().clone().expect("applied");
        match applied.loyalty {
            Some(LoyaltyApply::Reduced { new_loyalty }) => {
                // Two surviving administrators each drop 20–30 from a regenerated-to-max 100.
                assert!(
                    (40..=60).contains(&new_loyalty),
                    "loyalty after two admin strikes: {new_loyalty}"
                );
            }
            other => panic!("expected a loyalty drop without conquest, got {other:?}"),
        }
        assert!(!applied.report.conquered);
        assert_eq!(applied.report.loyalty_before, Some(100));
    }

    // 014 AC5: a capital is unconquerable — even a crushing win with surviving administrators and a
    // free slot leaves its loyalty untouched, never transfers, and (being a no-op) writes nothing,
    // while the report still records before == after so the attacker learns it is immune.
    #[tokio::test]
    async fn admin_attack_on_a_capital_changes_nothing() {
        let mut capital = village(2, 2, Coordinate::new(3, 4));
        capital.is_capital = true;
        let acc = accounts(Vec::new(), Some(capital));
        let cb = FakeCombat::default();
        *cb.due.lock().unwrap() = vec![DueAttack {
            id: 7,
            kind: MovementKind::Attack,
            owner: PlayerId(1),
            home_village: VillageId(1),
            target_village: VillageId(2),
            origin: Coordinate::new(0, 0),
            dest: Coordinate::new(3, 4),
            arrive_at: Timestamp(1_000_000),
            troops: vec![(UnitId("u0".into()), 50), (UnitId("u1".into()), 2)],
            scout_target: None,
            catapult_target: None,
        }];
        let mv = FakeMovements {
            reinforcements: Vec::new(),
        };
        let loyalty = LoyaltyRules {
            starting_loyalty: 100,
            post_conquest_loyalty: 25,
            regen_per_hour: 2,
            drop_min: 20,
            drop_max: 30,
            administrator_ids: vec!["u1".to_owned()],
        };
        process_due_combat(
            &acc,
            &mv,
            &FakeUnits,
            &cb,
            &economy_rules(),
            &unit_rules(),
            &combat_rules(),
            &ScoutRules { loss_exponent: 1.5 },
            &culture_rules(),
            &loyalty,
            &ranking_rules(),
            &map(),
            GameSpeed::new(1.0).unwrap(),
            42,
            Timestamp(1_000_000),
            100,
            (3, 6, 10),
        )
        .await
        .unwrap();

        let applied = cb.applied.lock().unwrap().clone().expect("applied");
        assert!(
            applied.loyalty.is_none(),
            "a capital strike persists nothing, got {:?}",
            applied.loyalty
        );
        assert!(!applied.report.conquered, "a capital never transfers");
        assert_eq!(
            (applied.report.loyalty_before, applied.report.loyalty_after),
            (Some(100), Some(100)),
            "the capital's loyalty is unchanged before == after"
        );
    }

    // 020 AC2 / 021 AC3: an artifact vault (Natar, not a site) is immune to conquest — like a capital,
    // an administrator strike leaves its loyalty untouched — while a Natar **Wonder site** is treated as
    // conquerable, so the same strike drops its loyalty (the first step toward taking it).
    #[tokio::test]
    async fn wonder_site_is_conquerable_but_artifact_vault_is_not() {
        let loyalty = LoyaltyRules {
            starting_loyalty: 100,
            post_conquest_loyalty: 25,
            regen_per_hour: 2,
            drop_min: 20,
            drop_max: 30,
            administrator_ids: vec!["u1".to_owned()],
        };
        let strike = async |is_wonder_site: bool| {
            let mut target = village(2, 2, Coordinate::new(3, 4));
            target.is_natar = true;
            target.is_wonder_site = is_wonder_site;
            let acc = accounts(Vec::new(), Some(target));
            let cb = FakeCombat::default();
            *cb.due.lock().unwrap() = vec![DueAttack {
                id: 7,
                kind: MovementKind::Attack,
                owner: PlayerId(1),
                home_village: VillageId(1),
                target_village: VillageId(2),
                origin: Coordinate::new(0, 0),
                dest: Coordinate::new(3, 4),
                arrive_at: Timestamp(1_000_000),
                troops: vec![(UnitId("u0".into()), 50), (UnitId("u1".into()), 2)],
                scout_target: None,
                catapult_target: None,
            }];
            let mv = FakeMovements {
                reinforcements: Vec::new(),
            };
            process_due_combat(
                &acc,
                &mv,
                &FakeUnits,
                &cb,
                &economy_rules(),
                &unit_rules(),
                &combat_rules(),
                &ScoutRules { loss_exponent: 1.5 },
                &culture_rules(),
                &loyalty,
                &ranking_rules(),
                &map(),
                GameSpeed::new(1.0).unwrap(),
                42,
                Timestamp(1_000_000),
                100,
                (3, 6, 10),
            )
            .await
            .unwrap();
            cb.applied.lock().unwrap().clone().expect("applied")
        };

        // The artifact vault is immune: loyalty unchanged, never conquered.
        let vault = strike(false).await;
        assert!(
            vault.loyalty.is_none(),
            "an artifact vault is unconquerable"
        );
        assert!(!vault.report.conquered);
        assert_eq!(
            (vault.report.loyalty_before, vault.report.loyalty_after),
            (Some(100), Some(100)),
            "a vault's loyalty is untouched"
        );

        // The Wonder site is conquerable: the admin strike drops its loyalty.
        let site = strike(true).await;
        match site.loyalty {
            Some(LoyaltyApply::Reduced { new_loyalty }) => {
                assert!(
                    new_loyalty < 100,
                    "a Wonder site's loyalty falls: {new_loyalty}"
                );
            }
            other => panic!("expected a Wonder site's loyalty to drop, got {other:?}"),
        }
    }

    /// 011 AC1: an attack carrying catapults persists a valid target; the Wall/Rally Point is
    /// rejected; an attack without catapults carries no catapult target.
    #[tokio::test]
    async fn order_attack_catapult_target() {
        let target = || Some(village(2, 2, Coordinate::new(3, 4)));
        let acc = accounts(
            vec![(UnitId("u0".into()), 5), (UnitId("u8".into()), 5)],
            target(),
        );
        let cb = FakeCombat::default();
        let send_cat = async |t: Option<BuildingKind>, troops: Vec<(UnitId, u32)>| {
            order_attack(
                &acc,
                &cb,
                &NoopStarvation,
                &economy_rules(),
                &unit_rules(),
                &map(),
                GameSpeed::new(1.0).unwrap(),
                Timestamp(0),
                PlayerId(1),
                None,
                Coordinate::new(3, 4),
                troops,
                AttackMode::Attack,
                None,
                t,
            )
            .await
        };
        // Catapults + a valid target → carried.
        send_cat(
            Some(BuildingKind::Warehouse),
            vec![(UnitId("u0".into()), 2), (UnitId("u8".into()), 2)],
        )
        .await
        .unwrap();
        assert_eq!(
            cb.sent.lock().unwrap().clone().unwrap().catapult_target,
            Some(BuildingKind::Warehouse)
        );
        // The Wall and Rally Point are rejected (P4).
        assert_eq!(
            send_cat(Some(BuildingKind::Wall), vec![(UnitId("u8".into()), 1)]).await,
            Err(CombatError::InvalidCatapultTarget)
        );
        assert_eq!(
            send_cat(
                Some(BuildingKind::RallyPoint),
                vec![(UnitId("u8".into()), 1)]
            )
            .await,
            Err(CombatError::InvalidCatapultTarget)
        );
        // No catapults in the composition → no catapult target carried.
        send_cat(
            Some(BuildingKind::Warehouse),
            vec![(UnitId("u0".into()), 1)],
        )
        .await
        .unwrap();
        assert_eq!(
            cb.sent.lock().unwrap().clone().unwrap().catapult_target,
            None
        );
    }

    /// 011 AC2/AC3: a won attack with surviving catapults razes the target building and the survivors
    /// loot the target (bounded by carry capacity, debited from the target).
    #[tokio::test]
    async fn resolve_loots_and_razes() {
        // A target with a Warehouse (level 3) and a weak garrison, against an overwhelming attacker
        // carrying 20 catapults (power 200 ⇒ razes 2 levels at durability 100).
        let mut target = village(2, 2, Coordinate::new(3, 4));
        target.buildings.push(BuildingSlot {
            kind: BuildingKind::Warehouse,
            level: 3,
        });
        let acc = FakeAccounts {
            home: village(1, 1, Coordinate::new(0, 0)),
            garrison: Vec::new(),
            target: Some(target.clone()),
            target_garrison: vec![(UnitId("u0".into()), 1)],
        };
        let cb = FakeCombat::default();
        *cb.due.lock().unwrap() = vec![DueAttack {
            id: 5,
            kind: MovementKind::Raid,
            owner: PlayerId(1),
            home_village: VillageId(1),
            target_village: VillageId(2),
            origin: Coordinate::new(0, 0),
            dest: Coordinate::new(3, 4),
            arrive_at: Timestamp(1_000_000),
            troops: vec![(UnitId("u0".into()), 50), (UnitId("u8".into()), 20)],
            scout_target: None,
            catapult_target: Some(BuildingKind::Warehouse),
        }];
        let mv = FakeMovements {
            reinforcements: Vec::new(),
        };
        process_due_combat(
            &acc,
            &mv,
            &FakeUnits,
            &cb,
            &economy_rules(),
            &unit_rules(),
            &combat_rules(),
            &ScoutRules { loss_exponent: 1.5 },
            &culture_rules(),
            &loyalty_rules(),
            &ranking_rules(),
            &map(),
            GameSpeed::new(1.0).unwrap(),
            42,
            Timestamp(1_000_000),
            100,
            (3, 6, 10),
        )
        .await
        .unwrap();

        let applied = cb.applied.lock().unwrap().clone().expect("applied");
        // AC2: the Warehouse is razed from 3 toward 1 (200 power / 100 durability = 2 levels).
        let razed = applied.razed.expect("razed");
        assert_eq!(razed.kind, BuildingKind::Warehouse);
        assert_eq!(razed.before, 3);
        assert_eq!(razed.after, 1);
        // AC3: loot was taken (target holds 500 each, no Cranny) and debited from the target.
        assert!(applied.loot.wood > 0);
        assert!(applied.target_debit.is_some());
        assert_eq!(applied.report.loot, applied.loot);
    }

    /// 011 AC2/AC8: catapult target selection — no power / no eligible building → nothing; an unset
    /// target picks a seeded-random **eligible** building deterministically; a chosen-but-absent
    /// building falls back to the random pick.
    #[test]
    fn pick_razed_target_branches() {
        let rules = combat_rules(); // catapult_durability 100
        let mut v = village(2, 2, Coordinate::new(3, 4)); // has only a Rally Point (ineligible)
        // No catapult power ⇒ nothing razed; and no eligible building ⇒ nothing even with power.
        assert!(pick_razed_target(&v, Some(BuildingKind::Warehouse), 1, 1, 0.0, &rules).is_none());
        assert!(pick_razed_target(&v, None, 1, 1, 500.0, &rules).is_none());
        // Add eligible buildings; an unset target picks a random eligible one, never Wall/Rally Point.
        v.buildings.push(BuildingSlot {
            kind: BuildingKind::Warehouse,
            level: 3,
        });
        v.buildings.push(BuildingSlot {
            kind: BuildingKind::Smithy,
            level: 2,
        });
        v.buildings.push(BuildingSlot {
            kind: BuildingKind::Wall,
            level: 5,
        });
        let r1 = pick_razed_target(&v, None, 7, 42, 500.0, &rules).expect("random");
        let r2 = pick_razed_target(&v, None, 7, 42, 500.0, &rules).expect("random");
        assert_eq!(r1, r2); // deterministic from the seed + movement id (AC8)
        assert!(matches!(
            r1.kind,
            BuildingKind::Warehouse | BuildingKind::Smithy
        ));
        // A chosen building the target lacks falls back to the only remaining eligible one.
        v.buildings.retain(|b| b.kind != BuildingKind::Smithy);
        let fallback = pick_razed_target(&v, Some(BuildingKind::Smithy), 7, 42, 500.0, &rules)
            .expect("fallback");
        assert_eq!(fallback.kind, BuildingKind::Warehouse);
    }

    // AC1: a raid debits + schedules an attack/raid movement arriving at now + travelTime.
    #[tokio::test]
    async fn launching_a_raid_schedules_the_arrival() {
        let acc = accounts(
            vec![(UnitId("u0".into()), 10)],
            Some(village(2, 2, Coordinate::new(3, 4))),
        );
        let cb = FakeCombat::default();
        send(&acc, &cb, vec![(UnitId("u0".into()), 6)], AttackMode::Raid)
            .await
            .unwrap();
        let sent = cb.sent.lock().unwrap().clone().expect("sent");
        assert_eq!(sent.home, VillageId(1));
        assert_eq!(sent.deliver, VillageId(2));
        assert_eq!(sent.kind, MovementKind::Raid);
        assert_eq!(sent.troops, vec![(UnitId("u0".into()), 6)]);
        // distance 5, u0 speed 6, world 1 ⇒ 3000 s.
        assert_eq!(sent.arrive, Timestamp(3_000_000));
    }

    // AC1: an attack uses the Attack kind.
    #[tokio::test]
    async fn attack_uses_the_attack_kind() {
        let acc = accounts(
            vec![(UnitId("u0".into()), 10)],
            Some(village(2, 2, Coordinate::new(3, 4))),
        );
        let cb = FakeCombat::default();
        send(
            &acc,
            &cb,
            vec![(UnitId("u0".into()), 1)],
            AttackMode::Attack,
        )
        .await
        .unwrap();
        assert_eq!(
            cb.sent.lock().unwrap().clone().unwrap().kind,
            MovementKind::Attack
        );
    }

    // AC2: rejections leave the garrison untouched (no movement created).
    #[tokio::test]
    async fn send_rejections() {
        let target = || Some(village(2, 2, Coordinate::new(3, 4)));

        // Over the garrison.
        let acc = accounts(vec![(UnitId("u0".into()), 3)], target());
        let cb = FakeCombat::default();
        assert_eq!(
            send(&acc, &cb, vec![(UnitId("u0".into()), 4)], AttackMode::Raid).await,
            Err(CombatError::Insufficient)
        );
        assert!(cb.sent.lock().unwrap().is_none());

        // Empty composition.
        let acc = accounts(vec![(UnitId("u0".into()), 10)], target());
        assert_eq!(
            send(&acc, &cb, vec![(UnitId("u0".into()), 0)], AttackMode::Raid).await,
            Err(CombatError::EmptyComposition)
        );

        // No village at the target.
        let acc = accounts(vec![(UnitId("u0".into()), 10)], None);
        assert_eq!(
            send(
                &acc,
                &cb,
                vec![(UnitId("u0".into()), 1)],
                AttackMode::Attack
            )
            .await,
            Err(CombatError::NoTargetThere)
        );

        // Target is the attacker's own tile (same id as home).
        let acc = accounts(
            vec![(UnitId("u0".into()), 10)],
            Some(village(1, 1, Coordinate::new(3, 4))),
        );
        assert_eq!(
            send(
                &acc,
                &cb,
                vec![(UnitId("u0".into()), 1)],
                AttackMode::Attack
            )
            .await,
            Err(CombatError::SameTile)
        );
        assert!(cb.sent.lock().unwrap().is_none());

        // Roles (P4): you cannot attack — and so cannot conquer — *another* village you own (013
        // multi-village). The target is a distinct village id at a distinct tile but the same owner as
        // the attacker; ownership alone must reject it, never reaching a movement.
        let own_second = village(7, 1, Coordinate::new(3, 4)); // id 7, owner 1 (== attacker), other tile
        let acc = accounts(vec![(UnitId("u0".into()), 10)], Some(own_second));
        assert_eq!(
            send(
                &acc,
                &cb,
                vec![(UnitId("u0".into()), 1)],
                AttackMode::Attack
            )
            .await,
            Err(CombatError::SameTile)
        );
        assert!(cb.sent.lock().unwrap().is_none());
    }
}
