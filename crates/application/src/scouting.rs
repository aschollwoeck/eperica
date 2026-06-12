//! Scouting use-cases (010): launch a standalone scout mission, and resolve due missions. The
//! espionage math is the pure domain (`scouting`); this layer validates and debits the scouter,
//! gathers the defender's counter-espionage, runs `resolve_scouting`, applies the scout losses,
//! reads the intel from persisted state, schedules the survivor return, and writes the report.
//!
//! Scouts riding an **attack/raid** are handled by the combat processor (`process_due_combat`), which
//! reuses [`gather_intel`] and the same domain formula before the 009 main battle.

use crate::economy::settle_amounts;
use crate::ports::{
    AccountRepository, MovementRepository, NewScoutReport, RepoError, ScoutApply, ScoutIntel,
    ScoutRepository, StationedGroup,
};
use eperica_domain::{
    BuildingKind, Coordinate, EconomyRules, GameSpeed, PlayerId, ResourceAmounts, ScoutRules,
    ScoutTarget, Timestamp, UnitCounts, UnitId, UnitRole, UnitRules, Village, WorldMap,
    resolve_scouting, scouting_power, slowest_speed, travel_time_secs_floored,
};

/// Why launching a scout mission failed (010 AC3).
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ScoutError {
    /// The garrison does not hold all the requested scouts.
    #[error("not enough scouts")]
    Insufficient,
    /// The composition is empty (or none of its types are real units).
    #[error("no scouts selected")]
    EmptyComposition,
    /// A standalone scout mission was given a non-Scout-role unit (scouts only).
    #[error("only scouts may be sent on a scout mission")]
    NotAllScouts,
    /// No village occupies the target tile.
    #[error("no village at the target")]
    NoTargetThere,
    /// The target is the scouter's own village tile.
    #[error("cannot scout your own village")]
    SameTile,
    /// The owning village/account was not found.
    #[error("not found")]
    NotFound,
    /// A storage/backend failure.
    #[error("storage error: {0}")]
    Backend(String),
}

impl From<RepoError> for ScoutError {
    fn from(e: RepoError) -> Self {
        ScoutError::Backend(e.to_string())
    }
}

fn building_level(village: &Village, kind: BuildingKind) -> u8 {
    village
        .buildings
        .iter()
        .find(|b| b.kind == kind)
        .map_or(0, |b| b.level)
}

/// Merge several `unit → count` compositions into one (the defenders' troops for `Defenses` intel).
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

/// Read the intel a successful scout brings home (010 AC9), computed from persisted state at `now`:
/// **Resources** accrues the target's stored resources (002 model, P1); **Defenses** reveals the
/// stationed troops (garrison + reinforcements, merged) and the Wall level. Shared with the combat
/// processor for scouts riding an attack.
#[allow(clippy::too_many_arguments)]
pub async fn gather_intel<A>(
    accounts: &A,
    target: &Village,
    garrison: &UnitCounts,
    reinforcements: &[StationedGroup],
    target_type: ScoutTarget,
    economy_rules: &EconomyRules,
    unit_rules: &UnitRules,
    speed: GameSpeed,
    now: Timestamp,
) -> Result<ScoutIntel, RepoError>
where
    A: AccountRepository,
{
    match target_type {
        ScoutTarget::Resources => {
            let amounts = match accounts.stored_resources(target.id).await? {
                Some((stored, updated_at)) => settle_amounts(
                    stored,
                    updated_at,
                    now,
                    target,
                    garrison,
                    economy_rules,
                    unit_rules,
                    speed,
                ),
                None => ResourceAmounts {
                    wood: 0,
                    clay: 0,
                    iron: 0,
                    crop: 0,
                },
            };
            Ok(ScoutIntel::Resources(amounts))
        }
        ScoutTarget::Defenses => {
            let reinforcement_force: Vec<&UnitCounts> =
                reinforcements.iter().map(|g| &g.troops).collect();
            let mut parts = vec![garrison];
            parts.extend(reinforcement_force);
            Ok(ScoutIntel::Defenses {
                troops: merge_counts(&parts),
                wall_level: building_level(target, BuildingKind::Wall),
            })
        }
    }
}

/// Launch a standalone scout mission from `owner`'s village against the village at `target` (010 AC1).
///
/// Validates ownership, the composition, **that every unit is a Scout-role unit**, garrison
/// availability, and the target (another village on a different tile); computes travel time (007,
/// paced by the slowest scout, P7); atomically debits the garrison and schedules the arrival;
/// re-syncs the home's starvation check (the garrison shrank).
///
/// # Errors
/// See [`ScoutError`].
#[allow(clippy::too_many_arguments)]
pub async fn order_scout<A, C, S>(
    accounts: &A,
    scout: &C,
    starvation: &S,
    economy_rules: &EconomyRules,
    unit_rules: &UnitRules,
    map: &WorldMap,
    speed: GameSpeed,
    now: Timestamp,
    owner: PlayerId,
    target: Coordinate,
    troops: Vec<(UnitId, u32)>,
    target_type: ScoutTarget,
) -> Result<(), ScoutError>
where
    A: AccountRepository,
    C: ScoutRepository,
    S: crate::ports::StarvationRepository,
{
    let Some(home) = accounts.villages_of(owner).await?.into_iter().next() else {
        return Err(ScoutError::NotFound);
    };
    let Some(tribe) = home.tribe else {
        return Err(ScoutError::NotFound);
    };
    let roster = unit_rules.roster(tribe);

    let chosen: Vec<(UnitId, u32)> = troops.into_iter().filter(|(_, n)| *n > 0).collect();
    if chosen.is_empty() {
        return Err(ScoutError::EmptyComposition);
    }
    // Scouts-only: every requested unit must be a known Scout-role unit (P4).
    for (unit, _) in &chosen {
        let is_scout = roster
            .iter()
            .find(|s| &s.id == unit)
            .is_some_and(|s| s.role == UnitRole::Scout);
        if !is_scout {
            return Err(ScoutError::NotAllScouts);
        }
    }

    let garrison = accounts.garrison(home.id).await?;
    for (unit, n) in &chosen {
        let have = garrison
            .iter()
            .find(|(u, _)| u == unit)
            .map_or(0, |(_, c)| *c);
        if have < *n {
            return Err(ScoutError::Insufficient);
        }
    }

    let Some(dest) = accounts.village_at(target).await? else {
        return Err(ScoutError::NoTargetThere);
    };
    if dest.id == home.id || dest.coordinate == home.coordinate {
        return Err(ScoutError::SameTile);
    }

    let Some(slowest) = slowest_speed(&chosen, roster) else {
        return Err(ScoutError::EmptyComposition);
    };
    let distance = map.distance(home.coordinate, dest.coordinate);
    let secs = travel_time_secs_floored(distance, slowest, speed);
    let arrive = Timestamp(now.0 + secs * 1000);

    scout
        .start_scout(
            home.id,
            dest.id,
            owner,
            home.coordinate,
            dest.coordinate,
            now,
            arrive,
            &chosen,
            target_type,
        )
        .await
        .map_err(|e| match e {
            RepoError::Conflict => ScoutError::Insufficient,
            other => ScoutError::Backend(other.to_string()),
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

/// Claim and resolve standalone scout missions whose arrival is due (the System actor, AC4–AC11).
///
/// # Errors
/// Propagates [`RepoError`]; a per-mission failure is logged and skipped (recovered by the orphan
/// requeue and re-resolved deterministically).
#[allow(clippy::too_many_arguments)]
pub async fn process_due_scouts<A, M, C>(
    accounts: &A,
    movements: &M,
    scout: &C,
    economy_rules: &EconomyRules,
    unit_rules: &UnitRules,
    scout_rules: &ScoutRules,
    map: &WorldMap,
    speed: GameSpeed,
    now: Timestamp,
    limit: i64,
) -> Result<(), RepoError>
where
    A: AccountRepository,
    M: MovementRepository,
    C: ScoutRepository,
{
    let due = scout.claim_due_scouts(now, limit).await?;
    for mission in due {
        if let Err(e) = resolve_one_scout(
            accounts,
            movements,
            scout,
            economy_rules,
            unit_rules,
            scout_rules,
            map,
            speed,
            now,
            &mission,
        )
        .await
        {
            tracing::error!(error = %e, "failed to resolve due scout");
        }
    }
    Ok(())
}

/// Resolve a single due standalone scout mission.
#[allow(clippy::too_many_arguments)]
async fn resolve_one_scout<A, M, C>(
    accounts: &A,
    movements: &M,
    scout: &C,
    economy_rules: &EconomyRules,
    unit_rules: &UnitRules,
    scout_rules: &ScoutRules,
    map: &WorldMap,
    speed: GameSpeed,
    _now: Timestamp,
    mission: &crate::ports::DueScout,
) -> Result<(), RepoError>
where
    A: AccountRepository,
    M: MovementRepository,
    C: ScoutRepository,
{
    let (Some(target), Some(home)) = (
        accounts.village_by_id(mission.target_village).await?,
        accounts.village_by_id(mission.home_village).await?,
    ) else {
        return Err(RepoError::Backend("scout village missing".into()));
    };

    // Attacker espionage power: the scouts, by the scouter's roster.
    let atk_roster = home.tribe.map_or(&[][..], |t| unit_rules.roster(t));
    let attacker_power = scouting_power(&mission.troops, atk_roster);

    // Defender counter-espionage: the target's garrison + every reinforcement group's scouts.
    let def_roster = target.tribe.map_or(&[][..], |t| unit_rules.roster(t));
    let garrison = accounts.garrison(target.id).await?;
    let reinforcements = movements.reinforcements_at(target.id).await?;
    let mut defender_power = scouting_power(&garrison, def_roster);
    for group in &reinforcements {
        let group_roster = group.home_tribe.map_or(&[][..], |t| unit_rules.roster(t));
        defender_power += scouting_power(&group.troops, group_roster);
    }

    let outcome = resolve_scouting(attacker_power, defender_power, scout_rules);
    let (survivors, losses) =
        eperica_domain::apply_losses(&mission.troops, outcome.attacker_loss_frac);

    // Intel comes home only if a scout survived to carry it (AC7).
    let intel = if survivors.is_empty() {
        None
    } else {
        Some(
            gather_intel(
                accounts,
                &target,
                &garrison,
                &reinforcements,
                mission.target_type,
                economy_rules,
                unit_rules,
                speed,
                mission.arrive_at,
            )
            .await?,
        )
    };

    // Survivors travel home (a return movement); empty ⇒ none.
    let return_arrive = match slowest_speed(&survivors, atk_roster) {
        Some(slow) => {
            let distance = map.distance(mission.dest, mission.origin);
            Timestamp(mission.arrive_at.0 + travel_time_secs_floored(distance, slow, speed) * 1000)
        }
        None => mission.arrive_at,
    };

    scout
        .apply_scout(ScoutApply {
            movement_id: mission.id,
            owner: mission.owner,
            scouter_home: home.id,
            scouter_origin: mission.origin,
            target_coord: mission.dest,
            survivors,
            scouted_at: mission.arrive_at,
            return_arrive,
            report: NewScoutReport {
                scouter_player: mission.owner,
                scouter_village: home.id,
                target_player: target.owner,
                target_village: target.id,
                target_coord: mission.dest,
                target_type: mission.target_type,
                scouts_sent: mission.troops.clone(),
                scouts_lost: losses,
                detected: outcome.detected,
                standalone: true,
                intel,
            },
        })
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{
        DueScout, NewUser, ScoutReportView, StarvationRepository, UserRecord, VillageMarker,
    };
    use async_trait::async_trait;
    use eperica_domain::{
        BuildingKind, BuildingSlot, FieldDistribution, MapRules, OasisBonus, ResearchSpec,
        ResourceAmounts, SmithyRules, StartingVillage, TrainingRules, Tribe, UnitRole, UnitSpec,
        Village, VillageId, Weighted,
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
        }
    }

    struct FakeAccounts {
        home: Village,
        garrison: UnitCounts,
        target: Option<Village>,
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
            Ok(Some((amounts(500), Timestamp(0))))
        }
        async fn garrison(&self, v: VillageId) -> Result<UnitCounts, RepoError> {
            // The scouter's home holds the garrison; any other village (the target) has none here.
            if v == self.home.id {
                Ok(self.garrison.clone())
            } else {
                Ok(Vec::new())
            }
        }
        async fn villages_at(&self, _c: &[Coordinate]) -> Result<Vec<VillageMarker>, RepoError> {
            Ok(Vec::new())
        }
        async fn village_at(&self, _c: Coordinate) -> Result<Option<Village>, RepoError> {
            Ok(self.target.clone())
        }
    }

    #[derive(Clone)]
    struct Sent {
        home: VillageId,
        deliver: VillageId,
        troops: UnitCounts,
        arrive: Timestamp,
        target: ScoutTarget,
    }

    #[derive(Default)]
    struct FakeScout {
        sent: Mutex<Option<Sent>>,
        applied: Mutex<Option<ScoutApply>>,
    }

    #[async_trait]
    impl ScoutRepository for FakeScout {
        async fn start_scout(
            &self,
            home: VillageId,
            deliver: VillageId,
            _owner: PlayerId,
            _origin: Coordinate,
            _dest: Coordinate,
            _now: Timestamp,
            arrive_at: Timestamp,
            troops: &[(UnitId, u32)],
            target: ScoutTarget,
        ) -> Result<(), RepoError> {
            *self.sent.lock().unwrap() = Some(Sent {
                home,
                deliver,
                troops: troops.to_vec(),
                arrive: arrive_at,
                target,
            });
            Ok(())
        }
        async fn claim_due_scouts(
            &self,
            _now: Timestamp,
            _limit: i64,
        ) -> Result<Vec<DueScout>, RepoError> {
            Ok(Vec::new())
        }
        async fn apply_scout(&self, apply: ScoutApply) -> Result<(), RepoError> {
            *self.applied.lock().unwrap() = Some(apply);
            Ok(())
        }
        async fn scout_reports_for(
            &self,
            _p: PlayerId,
            _l: i64,
        ) -> Result<Vec<ScoutReportView>, RepoError> {
            Ok(Vec::new())
        }
        async fn scout_report(
            &self,
            _id: u128,
            _p: PlayerId,
        ) -> Result<Option<ScoutReportView>, RepoError> {
            Ok(None)
        }
    }

    /// A movements repo whose target stations a configurable counter-espionage group.
    struct FakeMovements {
        counter: Vec<StationedGroup>,
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
        async fn reinforcements_at(&self, _v: VillageId) -> Result<Vec<StationedGroup>, RepoError> {
            Ok(self.counter.clone())
        }
        async fn reinforcements_of(&self, _o: PlayerId) -> Result<Vec<StationedGroup>, RepoError> {
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

    /// A roster of 10 units: u0 = a Scout (scouting 20, speed 9); u1.. = infantry.
    fn roster() -> Vec<UnitSpec> {
        (0..10)
            .map(|i| UnitSpec {
                id: UnitId(format!("u{i}")),
                name: format!("u{i}"),
                role: if i == 0 {
                    UnitRole::Scout
                } else {
                    UnitRole::Infantry
                },
                attack: if i == 0 { 0 } else { 10 },
                defense_infantry: 10,
                defense_cavalry: 10,
                scouting: if i == 0 { 20 } else { 0 },
                speed: if i == 0 { 9 } else { 6 },
                carry_capacity: 0,
                crop_upkeep: 0,
                cost: amounts(1),
                train_secs: 1,
                trained_in: BuildingKind::Barracks,
                research: (i > 0).then(|| ResearchSpec {
                    cost: amounts(1),
                    time_secs: 1,
                    requirements: vec![],
                }),
                siege_kind: None,
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
            starting_amounts: amounts(0),
        }
    }

    fn accounts(garrison: UnitCounts, target: Option<Village>) -> FakeAccounts {
        FakeAccounts {
            home: village(1, 1, Coordinate::new(0, 0)),
            garrison,
            target,
        }
    }

    async fn send(
        acc: &FakeAccounts,
        sc: &FakeScout,
        troops: Vec<(UnitId, u32)>,
        target_type: ScoutTarget,
    ) -> Result<(), ScoutError> {
        order_scout(
            acc,
            sc,
            &NoopStarvation,
            &economy_rules(),
            &unit_rules(),
            &map(),
            GameSpeed::new(1.0).unwrap(),
            Timestamp(0),
            PlayerId(1),
            Coordinate::new(3, 4), // distance 5 from home
            troops,
            target_type,
        )
        .await
    }

    // AC1: a standalone scout debits the scouts and schedules a Scout movement carrying its target.
    #[tokio::test]
    async fn launching_a_scout_schedules_the_arrival() {
        let acc = accounts(
            vec![(UnitId("u0".into()), 5)],
            Some(village(2, 2, Coordinate::new(3, 4))),
        );
        let sc = FakeScout::default();
        send(
            &acc,
            &sc,
            vec![(UnitId("u0".into()), 3)],
            ScoutTarget::Defenses,
        )
        .await
        .unwrap();
        let sent = sc.sent.lock().unwrap().clone().expect("sent");
        assert_eq!(sent.home, VillageId(1));
        assert_eq!(sent.deliver, VillageId(2));
        assert_eq!(sent.troops, vec![(UnitId("u0".into()), 3)]);
        assert_eq!(sent.target, ScoutTarget::Defenses);
        // distance 5, scout speed 9, world 1 ⇒ 2000 s.
        assert_eq!(sent.arrive, Timestamp(2_000_000));
    }

    // AC3: rejections leave the garrison untouched (no movement created).
    #[tokio::test]
    async fn send_rejections() {
        let target = || Some(village(2, 2, Coordinate::new(3, 4)));

        // A non-scout unit in the composition is rejected (scouts-only).
        let acc = accounts(
            vec![(UnitId("u0".into()), 5), (UnitId("u1".into()), 5)],
            target(),
        );
        let sc = FakeScout::default();
        assert_eq!(
            send(
                &acc,
                &sc,
                vec![(UnitId("u1".into()), 1)],
                ScoutTarget::Resources
            )
            .await,
            Err(ScoutError::NotAllScouts)
        );
        assert!(sc.sent.lock().unwrap().is_none());

        // Over the garrison.
        let acc = accounts(vec![(UnitId("u0".into()), 2)], target());
        assert_eq!(
            send(
                &acc,
                &sc,
                vec![(UnitId("u0".into()), 4)],
                ScoutTarget::Resources
            )
            .await,
            Err(ScoutError::Insufficient)
        );

        // Empty composition.
        let acc = accounts(vec![(UnitId("u0".into()), 5)], target());
        assert_eq!(
            send(
                &acc,
                &sc,
                vec![(UnitId("u0".into()), 0)],
                ScoutTarget::Resources
            )
            .await,
            Err(ScoutError::EmptyComposition)
        );

        // No village at the target.
        let acc = accounts(vec![(UnitId("u0".into()), 5)], None);
        assert_eq!(
            send(
                &acc,
                &sc,
                vec![(UnitId("u0".into()), 1)],
                ScoutTarget::Resources
            )
            .await,
            Err(ScoutError::NoTargetThere)
        );

        // Target is the scouter's own tile (same id as home).
        let acc = accounts(
            vec![(UnitId("u0".into()), 5)],
            Some(village(1, 1, Coordinate::new(3, 4))),
        );
        assert_eq!(
            send(
                &acc,
                &sc,
                vec![(UnitId("u0".into()), 1)],
                ScoutTarget::Resources
            )
            .await,
            Err(ScoutError::SameTile)
        );
        assert!(sc.sent.lock().unwrap().is_none());
    }

    /// AC4–AC7: resolving a mission wires the domain outcome + intel into the repo apply. With no
    /// counter-espionage the scouts survive (clean, undetected) and bring Defenses intel home.
    #[tokio::test]
    async fn resolve_clean_scout_returns_intel_undetected() {
        let acc = accounts(Vec::new(), Some(village(2, 2, Coordinate::new(3, 4))));
        let sc = FakeScout::default();
        let mv = FakeMovements {
            counter: Vec::new(),
        };
        let mission = DueScout {
            id: 7,
            owner: PlayerId(1),
            home_village: VillageId(1),
            target_village: VillageId(2),
            origin: Coordinate::new(0, 0),
            dest: Coordinate::new(3, 4),
            arrive_at: Timestamp(1_000_000),
            troops: vec![(UnitId("u0".into()), 3)],
            target_type: ScoutTarget::Defenses,
        };
        resolve_one_scout(
            &acc,
            &mv,
            &sc,
            &economy_rules(),
            &unit_rules(),
            &ScoutRules { loss_exponent: 1.5 },
            &map(),
            GameSpeed::new(1.0).unwrap(),
            Timestamp(1_000_000),
            &mission,
        )
        .await
        .unwrap();

        let applied = sc.applied.lock().unwrap().clone().expect("applied");
        // No counter ⇒ no losses, undetected, all 3 survive and a return is scheduled.
        assert!(!applied.report.detected);
        assert!(applied.report.scouts_lost.is_empty());
        assert_eq!(applied.survivors, vec![(UnitId("u0".into()), 3)]);
        assert!(matches!(
            applied.report.intel,
            Some(ScoutIntel::Defenses { .. })
        ));
    }

    /// AC5/AC7: overwhelming counter-espionage wipes the scouts — no survivors, detected, no intel.
    #[tokio::test]
    async fn resolve_overwhelmed_scout_loses_all_and_returns_no_intel() {
        let acc = accounts(Vec::new(), Some(village(2, 2, Coordinate::new(3, 4))));
        let sc = FakeScout::default();
        // The target stations a strong counter group (10 scouts × scouting 20 = 200 vs 60).
        let mv = FakeMovements {
            counter: vec![StationedGroup {
                host_village: VillageId(2),
                home_village: VillageId(9),
                other_coord: Coordinate::new(3, 4),
                other_owner: "ally".into(),
                home_tribe: Some(Tribe::Gauls),
                troops: vec![(UnitId("u0".into()), 10)],
            }],
        };
        let mission = DueScout {
            id: 8,
            owner: PlayerId(1),
            home_village: VillageId(1),
            target_village: VillageId(2),
            origin: Coordinate::new(0, 0),
            dest: Coordinate::new(3, 4),
            arrive_at: Timestamp(1_000_000),
            troops: vec![(UnitId("u0".into()), 3)],
            target_type: ScoutTarget::Resources,
        };
        resolve_one_scout(
            &acc,
            &mv,
            &sc,
            &economy_rules(),
            &unit_rules(),
            &ScoutRules { loss_exponent: 1.5 },
            &map(),
            GameSpeed::new(1.0).unwrap(),
            Timestamp(1_000_000),
            &mission,
        )
        .await
        .unwrap();

        let applied = sc.applied.lock().unwrap().clone().expect("applied");
        assert!(applied.report.detected);
        assert_eq!(applied.report.scouts_lost, vec![(UnitId("u0".into()), 3)]);
        assert!(applied.survivors.is_empty());
        assert!(applied.report.intel.is_none()); // no scout came home
    }
}
