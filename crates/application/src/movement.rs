//! Movement use-cases (007): send a reinforcement, send stationed troops back, and deliver due
//! arrivals. The travel timing is the pure domain (`travel_time_secs`); this layer validates,
//! debits/stations through the repository, and re-syncs the home's starvation check.

use crate::ports::{
    AccountRepository, MovementRepository, RepoError, ResourceWrite, StarvationRepository,
};
use eperica_domain::{
    Coordinate, EconomyRules, GameSpeed, MovementKind, PlayerId, Timestamp, UnitId, UnitRules,
    VillageId, WorldMap, compute_economy, deposit_capped, garrison_upkeep, slowest_speed,
    travel_time_secs_floored,
};

/// Why sending or returning troops failed (007 AC2).
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum MovementError {
    /// The garrison does not hold all the requested troops.
    #[error("not enough troops")]
    Insufficient,
    /// The composition is empty (or none of its types are real units).
    #[error("no troops selected")]
    EmptyComposition,
    /// No village occupies the target tile.
    #[error("no village at the target")]
    NoTargetThere,
    /// The target is the sender's own village tile.
    #[error("cannot reinforce your own village")]
    SameTile,
    /// The sender has no village, or no troops stationed at the requested host.
    #[error("nothing stationed there")]
    NothingStationed,
    /// The owning village/account was not found.
    #[error("not found")]
    NotFound,
    /// A storage/backend failure.
    #[error("storage error: {0}")]
    Backend(String),
}

impl From<RepoError> for MovementError {
    fn from(e: RepoError) -> Self {
        MovementError::Backend(e.to_string())
    }
}

/// Send `troops` from `owner`'s village to reinforce the village at `target` (007 AC1).
///
/// Validates ownership, the composition, garrison availability, and the target; computes travel
/// time from the toroidal distance and the slowest unit (P7); atomically debits the garrison and
/// schedules the arrival; then re-syncs the home's starvation check (the garrison shrank).
///
/// # Errors
/// See [`MovementError`].
#[allow(clippy::too_many_arguments)]
pub async fn order_reinforcement<A, M, S>(
    accounts: &A,
    movements: &M,
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
) -> Result<(), MovementError>
where
    A: AccountRepository,
    M: MovementRepository,
    S: StarvationRepository,
{
    let Some(home) = crate::economy::select_village(accounts, owner, selected).await? else {
        return Err(MovementError::NotFound);
    };
    let Some(tribe) = home.tribe else {
        return Err(MovementError::NotFound);
    };
    let roster = unit_rules.roster(tribe);

    let chosen: Vec<(UnitId, u32)> = troops.into_iter().filter(|(_, n)| *n > 0).collect();
    if chosen.is_empty() {
        return Err(MovementError::EmptyComposition);
    }

    // Availability: every requested count must be in the garrison.
    let garrison = accounts.garrison(home.id).await?;
    for (unit, n) in &chosen {
        let have = garrison
            .iter()
            .find(|(u, _)| u == unit)
            .map_or(0, |(_, c)| *c);
        if have < *n {
            return Err(MovementError::Insufficient);
        }
    }

    // Target: a village on another tile.
    let Some(dest) = accounts.village_at(target).await? else {
        return Err(MovementError::NoTargetThere);
    };
    if dest.id == home.id || dest.coordinate == home.coordinate {
        return Err(MovementError::SameTile);
    }

    let Some(slowest) = slowest_speed(&chosen, roster) else {
        return Err(MovementError::EmptyComposition);
    };
    let distance = map.distance(home.coordinate, dest.coordinate);
    // 020 AC6: a Speed artifact (carried on the sending village's read) shortens travel.
    let secs = ((travel_time_secs_floored(distance, slowest, speed) as f64)
        / home.artifact_effects.troop_speed)
        .round() as i64;
    let arrive = Timestamp(now.0 + secs * 1000);

    movements
        .start_reinforcement(
            home.id,
            dest.id,
            owner,
            home.coordinate,
            dest.coordinate,
            now,
            arrive,
            &chosen,
        )
        .await
        .map_err(|e| match e {
            RepoError::Conflict => MovementError::Insufficient,
            other => MovementError::Backend(other.to_string()),
        })?;

    // The garrison shrank — keep the depletion check exact (it can only improve).
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

/// Recall the troops `owner` has stationed at `host`, sending them home (007 AC5).
///
/// # Errors
/// See [`MovementError`].
#[allow(clippy::too_many_arguments)]
pub async fn order_return<A, M>(
    accounts: &A,
    movements: &M,
    unit_rules: &UnitRules,
    map: &WorldMap,
    speed: GameSpeed,
    now: Timestamp,
    owner: PlayerId,
    host: VillageId,
) -> Result<(), MovementError>
where
    A: AccountRepository,
    M: MovementRepository,
{
    let villages = accounts.villages_of(owner).await?;

    // The group the owner has stationed at `host`.
    let Some(group) = movements
        .reinforcements_of(owner)
        .await?
        .into_iter()
        .find(|g| g.host_village == host)
    else {
        return Err(MovementError::NothingStationed);
    };

    // The troops belong to *this* home village — resolve it exactly (correct once a player can hold
    // more than one village, 013), not just the owner's first.
    let Some(home) = villages.iter().find(|v| v.id == group.home_village) else {
        return Err(MovementError::NotFound);
    };
    let Some(tribe) = home.tribe else {
        return Err(MovementError::NotFound);
    };
    let roster = unit_rules.roster(tribe);

    let Some(slowest) = slowest_speed(&group.troops, roster) else {
        return Err(MovementError::NothingStationed);
    };
    let distance = map.distance(group.other_coord, home.coordinate);
    let secs = travel_time_secs_floored(distance, slowest, speed);
    let arrive = Timestamp(now.0 + secs * 1000);

    movements
        .start_return(
            host,
            home.id,
            owner,
            group.other_coord,
            home.coordinate,
            now,
            arrive,
        )
        .await
        .map_err(|e| match e {
            RepoError::Conflict => MovementError::NothingStationed,
            other => MovementError::Backend(other.to_string()),
        })?;
    Ok(())
}

/// Claim and apply movements whose arrival is due (the System actor, AC4/AC5); returns the home
/// villages of **return** arrivals (their garrison grew — callers re-sync starvation).
///
/// # Errors
/// Propagates [`RepoError`] from the repository.
#[allow(clippy::too_many_arguments)]
pub async fn process_due_movements<A, M>(
    accounts: &A,
    movements: &M,
    economy_rules: &EconomyRules,
    unit_rules: &UnitRules,
    speed: GameSpeed,
    now: Timestamp,
    limit: i64,
) -> Result<Vec<VillageId>, RepoError>
where
    A: AccountRepository,
    M: MovementRepository,
{
    let due = movements.claim_due_movements(now, limit).await?;
    let mut returned_homes = Vec::new();
    for movement in due {
        // A return carrying loot (011) credits the home's resources, capped at storage — settle the
        // home forward then `deposit_capped`, written guarded in the same tx as the garrison rejoin.
        let credit =
            match loot_credit(accounts, economy_rules, unit_rules, speed, now, &movement).await {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!(error = %e, "failed to compute loot credit");
                    continue;
                }
            };
        // Log-and-continue: a failed apply must not strand the rest; the movement is recovered by
        // the startup orphan requeue and re-applied (apply is exactly-once).
        match movements.apply_movement(&movement, credit).await {
            Ok(()) if movement.kind == MovementKind::Return => {
                returned_homes.push(movement.deliver_village);
            }
            Ok(()) => {}
            Err(e) => tracing::error!(error = %e, "failed to apply due movement"),
        }
    }
    Ok(returned_homes)
}

/// The snapshot-guarded loot credit for a `return` carrying loot (011): settle the home's resources
/// to `now` and `deposit_capped` the loot (overflow lost). `None` for non-returns or loot-less returns.
async fn loot_credit<A>(
    accounts: &A,
    economy_rules: &EconomyRules,
    unit_rules: &UnitRules,
    speed: GameSpeed,
    now: Timestamp,
    movement: &crate::ports::DueMovement,
) -> Result<Option<ResourceWrite>, RepoError>
where
    A: AccountRepository,
{
    let loot = movement.loot;
    if movement.kind != MovementKind::Return
        || (loot.wood == 0 && loot.clay == 0 && loot.iron == 0 && loot.crop == 0)
    {
        return Ok(None);
    }
    let Some(home) = accounts.village_by_id(movement.deliver_village).await? else {
        return Ok(None);
    };
    let Some((stored, snapshot)) = accounts.stored_resources(home.id).await? else {
        return Ok(None);
    };
    let garrison = accounts.garrison(home.id).await?;
    let base_upkeep = home
        .tribe
        .map_or(0, |t| garrison_upkeep(&garrison, unit_rules.roster(t)));
    let upkeep = (base_upkeep as f64 * home.artifact_effects.upkeep).round() as i64;
    // Never let the clock regress below the snapshot (mirrors the 008 deliver).
    let clock = Timestamp(now.0.max(snapshot.0));
    let econ = compute_economy(
        stored,
        (clock.0 - snapshot.0) / 1000,
        &home.fields,
        &home.buildings,
        upkeep,
        economy_rules,
        speed,
        home.oasis_bonus,
        home.artifact_effects.storage,
    );
    let after = deposit_capped(econ.amounts, loot, econ.capacities);
    Ok(Some(ResourceWrite {
        after,
        settled_from: snapshot,
        clock,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{
        DueMovement, MovementView, NewUser, StationedGroup, UserRecord, VillageMarker,
    };
    use async_trait::async_trait;
    use eperica_domain::{
        BuildingKind, BuildingSlot, FieldDistribution, ResearchSpec, ResourceAmounts, SmithyRules,
        StartingVillage, TrainingRules, Tribe, UnitCounts, UnitRole, UnitSpec, Village,
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

    fn roster() -> Vec<UnitSpec> {
        (0..10)
            .map(|i| UnitSpec {
                id: UnitId(format!("u{i}")),
                name: format!("u{i}"),
                role: UnitRole::Infantry,
                attack: 1,
                defense_infantry: 1,
                defense_cavalry: 1,
                scouting: 0,
                speed: 6 + i as u32, // u0 slowest (6)
                carry_capacity: 0,
                crop_upkeep: 1,
                point_value: 1,
                cost: amounts(1),
                train_secs: 1,
                trained_in: BuildingKind::Barracks,
                siege_kind: None,
                research: (i > 0).then(|| ResearchSpec {
                    cost: amounts(1),
                    time_secs: 1,
                    requirements: vec![],
                }),
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

    fn map() -> WorldMap {
        use eperica_domain::{MapRules, OasisBonus, Weighted};
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
        async fn village_by_id(&self, _v: VillageId) -> Result<Option<Village>, RepoError> {
            Ok(Some(self.home.clone()))
        }
        async fn stored_resources(
            &self,
            _v: VillageId,
        ) -> Result<Option<(ResourceAmounts, Timestamp)>, RepoError> {
            Ok(Some((amounts(0), Timestamp(0))))
        }
        async fn garrison(&self, _v: VillageId) -> Result<UnitCounts, RepoError> {
            Ok(self.garrison.clone())
        }
        async fn villages_at(&self, _c: &[Coordinate]) -> Result<Vec<VillageMarker>, RepoError> {
            Ok(Vec::new())
        }
        async fn village_at(&self, _c: Coordinate) -> Result<Option<Village>, RepoError> {
            Ok(self.target.clone())
        }
    }

    // 020: this fake holds no artifacts (defaults apply); the movement tests assert base travel times.
    impl crate::ports::ArtifactRepository for FakeAccounts {}

    #[derive(Clone)]
    struct Sent {
        home: VillageId,
        deliver: VillageId,
        troops: UnitCounts,
        arrive: Timestamp,
    }

    #[derive(Default)]
    struct FakeMovements {
        sent: Mutex<Option<Sent>>,
        abroad: Vec<StationedGroup>,
    }

    #[async_trait]
    impl MovementRepository for FakeMovements {
        async fn start_reinforcement(
            &self,
            home: VillageId,
            deliver: VillageId,
            _owner: PlayerId,
            _origin: Coordinate,
            _dest: Coordinate,
            _now: Timestamp,
            arrive_at: Timestamp,
            troops: &[(UnitId, u32)],
        ) -> Result<(), RepoError> {
            *self.sent.lock().unwrap() = Some(Sent {
                home,
                deliver,
                troops: troops.to_vec(),
                arrive: arrive_at,
            });
            Ok(())
        }
        async fn start_return(
            &self,
            _host: VillageId,
            _home: VillageId,
            _owner: PlayerId,
            _origin: Coordinate,
            _dest: Coordinate,
            _now: Timestamp,
            _arrive_at: Timestamp,
        ) -> Result<UnitCounts, RepoError> {
            Ok(Vec::new())
        }
        async fn active_movements(&self, _o: PlayerId) -> Result<Vec<MovementView>, RepoError> {
            Ok(Vec::new())
        }
        async fn reinforcements_at(&self, _v: VillageId) -> Result<Vec<StationedGroup>, RepoError> {
            Ok(Vec::new())
        }
        async fn reinforcements_of(&self, _o: PlayerId) -> Result<Vec<StationedGroup>, RepoError> {
            Ok(self.abroad.clone())
        }
        async fn claim_due_movements(
            &self,
            _now: Timestamp,
            _limit: i64,
        ) -> Result<Vec<DueMovement>, RepoError> {
            Ok(Vec::new())
        }
        async fn apply_movement(
            &self,
            _due: &DueMovement,
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

    fn accounts(garrison: UnitCounts, target: Option<Village>) -> FakeAccounts {
        FakeAccounts {
            home: village(1, 1, Coordinate::new(0, 0)),
            garrison,
            target,
        }
    }

    async fn send(
        acc: &FakeAccounts,
        mv: &FakeMovements,
        troops: Vec<(UnitId, u32)>,
    ) -> Result<(), MovementError> {
        order_reinforcement(
            acc,
            mv,
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
        )
        .await
    }

    // --- AC1: send debits and schedules ---
    #[tokio::test]
    async fn sending_a_reinforcement_schedules_the_arrival() {
        let acc = accounts(
            vec![(UnitId("u0".into()), 10)],
            Some(village(2, 2, Coordinate::new(3, 4))),
        );
        let mv = FakeMovements::default();
        send(&acc, &mv, vec![(UnitId("u0".into()), 4)])
            .await
            .unwrap();
        let sent = mv.sent.lock().unwrap().clone().expect("sent");
        assert_eq!(sent.home, VillageId(1));
        assert_eq!(sent.deliver, VillageId(2));
        assert_eq!(sent.troops, vec![(UnitId("u0".into()), 4)]);
        // distance 5, u0 speed 6, world 1 ⇒ 5/6 h = 3000 s.
        assert_eq!(sent.arrive, Timestamp(3_000_000));
    }

    // --- AC3: the slowest unit paces the movement ---
    #[tokio::test]
    async fn the_slowest_unit_sets_the_arrival() {
        let acc = accounts(
            vec![(UnitId("u0".into()), 5), (UnitId("u5".into()), 5)],
            Some(village(2, 2, Coordinate::new(3, 4))),
        );
        let mv = FakeMovements::default();
        // u5 is faster (11), but u0 (6) is present and paces it: same 3000 s as u0 alone.
        send(
            &acc,
            &mv,
            vec![(UnitId("u0".into()), 1), (UnitId("u5".into()), 5)],
        )
        .await
        .unwrap();
        assert_eq!(
            mv.sent.lock().unwrap().clone().unwrap().arrive,
            Timestamp(3_000_000)
        );
    }

    // --- AC2: rejections leave the garrison untouched ---
    #[tokio::test]
    async fn send_rejections() {
        let target = || Some(village(2, 2, Coordinate::new(3, 4)));

        // Over the garrison.
        let acc = accounts(vec![(UnitId("u0".into()), 3)], target());
        let mv = FakeMovements::default();
        assert_eq!(
            send(&acc, &mv, vec![(UnitId("u0".into()), 4)]).await,
            Err(MovementError::Insufficient)
        );
        assert!(mv.sent.lock().unwrap().is_none());

        // Empty composition.
        let acc = accounts(vec![(UnitId("u0".into()), 10)], target());
        assert_eq!(
            send(&acc, &mv, vec![(UnitId("u0".into()), 0)]).await,
            Err(MovementError::EmptyComposition)
        );

        // No village at the target.
        let acc = accounts(vec![(UnitId("u0".into()), 10)], None);
        assert_eq!(
            send(&acc, &mv, vec![(UnitId("u0".into()), 1)]).await,
            Err(MovementError::NoTargetThere)
        );

        // Target resolves to the sender's own village (same id as home).
        let acc = accounts(
            vec![(UnitId("u0".into()), 10)],
            Some(village(1, 1, Coordinate::new(3, 4))),
        );
        assert_eq!(
            send(&acc, &mv, vec![(UnitId("u0".into()), 1)]).await,
            Err(MovementError::SameTile)
        );
        assert!(mv.sent.lock().unwrap().is_none());
    }

    // --- AC5: return uses the stationed group ---
    #[tokio::test]
    async fn returning_recalls_the_stationed_group() {
        let acc = accounts(vec![], None);
        let mv = FakeMovements {
            abroad: vec![StationedGroup {
                host_village: VillageId(2),
                home_village: VillageId(1),
                other_coord: Coordinate::new(3, 4),
                other_owner: "bob".to_owned(),
                home_tribe: Some(Tribe::Gauls),
                troops: vec![(UnitId("u0".into()), 4)],
            }],
            ..FakeMovements::default()
        };
        order_return(
            &acc,
            &mv,
            &unit_rules(),
            &map(),
            GameSpeed::new(1.0).unwrap(),
            Timestamp(0),
            PlayerId(1),
            VillageId(2),
        )
        .await
        .unwrap();

        // Returning from a host where the owner has nothing stationed is rejected.
        assert_eq!(
            order_return(
                &acc,
                &mv,
                &unit_rules(),
                &map(),
                GameSpeed::new(1.0).unwrap(),
                Timestamp(0),
                PlayerId(1),
                VillageId(99),
            )
            .await,
            Err(MovementError::NothingStationed)
        );
    }
}
