//! Starvation use-cases (005 AC7/AC8): keep each village's crop-depletion check in sync with its
//! live state, and cull the garrison when the store actually runs dry — the natural army cap
//! (GDD §2.2). One due-timestamped check per village (P1); the handler re-validates at fire time.

use crate::ports::{AccountRepository, RepoError, StarvationRepository};
use eperica_domain::{
    EconomyRules, GameSpeed, Timestamp, UnitRules, VillageId, compute_economy, depletion_secs,
    garrison_upkeep, net_crop_base, starve,
};

/// Re-derive the village's depletion check from live state: cancelled when there is no garrison
/// (AC8) or net crop ≥ 0, otherwise (re)scheduled at the exact depletion instant. Called after
/// every mutation that changes the store or the net rate, so the scheduled time stays exact
/// between mutations (the rates are constant in between).
///
/// # Errors
/// Propagates [`RepoError`] from the repositories.
pub async fn sync_starvation_check<A, S>(
    accounts: &A,
    starvation: &S,
    economy_rules: &EconomyRules,
    unit_rules: &UnitRules,
    speed: GameSpeed,
    now: Timestamp,
    village_id: VillageId,
) -> Result<(), RepoError>
where
    A: AccountRepository,
    S: StarvationRepository,
{
    // Garrison first: the common no-army case costs one query + a cancel (AC8, P11).
    let garrison = accounts.garrison(village_id).await?;
    if garrison.is_empty() {
        return starvation.cancel_starvation_check(village_id).await;
    }
    let Some(village) = accounts.village_by_id(village_id).await? else {
        return starvation.cancel_starvation_check(village_id).await;
    };
    let Some(tribe) = village.tribe else {
        return starvation.cancel_starvation_check(village_id).await;
    };
    let Some((stored, updated_at)) = accounts.stored_resources(village_id).await? else {
        return starvation.cancel_starvation_check(village_id).await;
    };

    let upkeep = garrison_upkeep(&garrison, unit_rules.roster(tribe));
    let economy = compute_economy(
        stored,
        (now.0 - updated_at.0) / 1000,
        &village.fields,
        &village.buildings,
        upkeep,
        economy_rules,
        speed,
    );
    match depletion_secs(economy.amounts.crop, economy.rates.crop_net) {
        None => starvation.cancel_starvation_check(village_id).await,
        Some(secs) => {
            starvation
                .schedule_starvation_check(village_id, Timestamp(now.0 + secs * 1000))
                .await
        }
    }
}

/// Claim due depletion checks and act on each (the System actor, AC7): re-validate from live
/// state, then cull (highest-upkeep first, until sustainable), reschedule (the village recovered
/// or the store is not empty yet), or finish (nothing to starve, AC8). Returns how many villages
/// were actually culled.
///
/// # Errors
/// Propagates [`RepoError`] from the repositories.
pub async fn process_due_starvation<A, S>(
    accounts: &A,
    starvation: &S,
    economy_rules: &EconomyRules,
    unit_rules: &UnitRules,
    speed: GameSpeed,
    now: Timestamp,
    limit: i64,
) -> Result<usize, RepoError>
where
    A: AccountRepository,
    S: StarvationRepository,
{
    let claimed = starvation.claim_due_starvation(now, limit).await?;
    let mut culled = 0;
    for village_id in claimed {
        match starve_village(
            accounts,
            starvation,
            economy_rules,
            unit_rules,
            speed,
            now,
            village_id,
        )
        .await
        {
            Ok(true) => culled += 1,
            Ok(false) => {}
            // Log-and-continue: the claimed check is recovered by the startup orphan requeue.
            Err(e) => tracing::error!(error = %e, "starvation check failed"),
        }
    }
    Ok(culled)
}

/// Handle one claimed check; `Ok(true)` if a cull was applied.
async fn starve_village<A, S>(
    accounts: &A,
    starvation: &S,
    economy_rules: &EconomyRules,
    unit_rules: &UnitRules,
    speed: GameSpeed,
    now: Timestamp,
    village_id: VillageId,
) -> Result<bool, RepoError>
where
    A: AccountRepository,
    S: StarvationRepository,
{
    let Some(village) = accounts.village_by_id(village_id).await? else {
        starvation
            .resolve_starvation_check(village_id, None)
            .await?;
        return Ok(false);
    };
    let garrison = accounts.garrison(village_id).await?;
    let (Some(tribe), Some((stored, updated_at))) =
        (village.tribe, accounts.stored_resources(village_id).await?)
    else {
        starvation
            .resolve_starvation_check(village_id, None)
            .await?;
        return Ok(false);
    };
    if garrison.is_empty() {
        // AC8: only troops starve; without a garrison the store just sits at 0.
        starvation
            .resolve_starvation_check(village_id, None)
            .await?;
        return Ok(false);
    }

    let roster = unit_rules.roster(tribe);
    let upkeep = garrison_upkeep(&garrison, roster);
    let economy = compute_economy(
        stored,
        (now.0 - updated_at.0) / 1000,
        &village.fields,
        &village.buildings,
        upkeep,
        economy_rules,
        speed,
    );

    if economy.rates.crop_net >= 0 {
        // Recovered before depletion (an upgrade completed, troops died elsewhere, …): no cull.
        starvation
            .resolve_starvation_check(village_id, None)
            .await?;
        return Ok(false);
    }
    if economy.amounts.crop > 0 {
        // Not empty yet (the check was scheduled from an older, lower net) — fire again on time.
        let secs = depletion_secs(economy.amounts.crop, economy.rates.crop_net).unwrap_or(0);
        starvation
            .resolve_starvation_check(village_id, Some(Timestamp(now.0 + secs * 1000)))
            .await?;
        return Ok(false);
    }

    // The store is dry and the net is negative: cull until sustainable (AC7).
    let net_base = net_crop_base(&village.fields, &village.buildings, economy_rules);
    let (survivors, casualties) = starve(&garrison, roster, net_base);
    match starvation
        .apply_starvation(village_id, economy.amounts, updated_at, now, &survivors)
        .await
    {
        Ok(()) => {
            tracing::info!(
                ?village_id,
                ?casualties,
                "garrison starved to sustainable size"
            );
            Ok(true)
        }
        Err(RepoError::Conflict) => {
            // Someone settled concurrently; the claimed check is requeued at startup or can be
            // retried next tick once re-synced by whatever settled.
            tracing::warn!(?village_id, "starvation settle conflicted; will retry");
            Ok(false)
        }
        Err(e) => Err(e),
    }
}

/// After due builds/training mutated villages, re-sync their checks (population/upkeep changed).
/// Villages without a garrison are cheap no-ops (the sync cancels immediately).
///
/// # Errors
/// Propagates [`RepoError`] from the repositories.
pub async fn sync_starvation_checks<A, S>(
    accounts: &A,
    starvation: &S,
    economy_rules: &EconomyRules,
    unit_rules: &UnitRules,
    speed: GameSpeed,
    now: Timestamp,
    villages: &[VillageId],
) -> Result<(), RepoError>
where
    A: AccountRepository,
    S: StarvationRepository,
{
    for &village in villages {
        sync_starvation_check(
            accounts,
            starvation,
            economy_rules,
            unit_rules,
            speed,
            now,
            village,
        )
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{AccountRepository, NewUser, UserRecord};
    use async_trait::async_trait;
    use eperica_domain::{
        Coordinate, PlayerId, ResearchSpec, ResourceAmounts, ResourceField, ResourceKind,
        SmithyRules, StartingVillage, TrainingRules, Tribe, UnitCounts, UnitId, UnitRole, UnitSpec,
        Village,
    };
    use std::collections::HashMap;
    use std::sync::Mutex;

    fn amounts(crop: i64) -> ResourceAmounts {
        ResourceAmounts {
            wood: 100,
            clay: 100,
            iron: 100,
            crop,
        }
    }

    fn economy_rules() -> EconomyRules {
        EconomyRules {
            wood_per_level: vec![0],
            clay_per_level: vec![0],
            iron_per_level: vec![0],
            crop_per_level: vec![10], // one crop field => 10/h before upkeep
            field_population_per_level: vec![0],
            building_population_per_level: HashMap::new(),
            warehouse_capacity_per_level: vec![1_000_000],
            granary_capacity_per_level: vec![1_000_000],
            starting_amounts: amounts(0),
        }
    }

    fn unit_rules() -> UnitRules {
        let roster = || -> Vec<UnitSpec> {
            (0..10)
                .map(|i| UnitSpec {
                    id: UnitId(format!("u{i}")),
                    name: format!("u{i}"),
                    role: UnitRole::Infantry,
                    attack: 1,
                    defense_infantry: 1,
                    defense_cavalry: 1,
                    speed: 1,
                    carry_capacity: 0,
                    crop_upkeep: 1,
                    cost: amounts(1),
                    train_secs: 1,
                    trained_in: eperica_domain::BuildingKind::Barracks,
                    research: (i > 0).then(|| ResearchSpec {
                        cost: amounts(1),
                        time_secs: 1,
                        requirements: vec![],
                    }),
                })
                .collect()
        };
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
        .expect("valid rules")
    }

    fn village() -> Village {
        Village {
            id: VillageId(1),
            owner: PlayerId(1),
            coordinate: Coordinate::new(0, 0),
            tribe: Some(Tribe::Gauls),
            fields: vec![ResourceField {
                kind: ResourceKind::Crop,
                level: 0,
            }],
            buildings: vec![],
        }
    }

    struct FakeAccounts {
        garrison: UnitCounts,
        stored: ResourceAmounts,
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
            Ok(vec![village()])
        }
        async fn village_by_id(&self, _v: VillageId) -> Result<Option<Village>, RepoError> {
            Ok(Some(village()))
        }
        async fn stored_resources(
            &self,
            _v: VillageId,
        ) -> Result<Option<(ResourceAmounts, Timestamp)>, RepoError> {
            Ok(Some((self.stored, Timestamp(0))))
        }
        async fn garrison(&self, _v: VillageId) -> Result<UnitCounts, RepoError> {
            Ok(self.garrison.clone())
        }
    }

    #[derive(Default)]
    struct RecordingStarvation {
        scheduled: Mutex<Option<Timestamp>>,
        cancelled: Mutex<bool>,
        resolved: Mutex<Option<Option<Timestamp>>>,
        applied_survivors: Mutex<Option<UnitCounts>>,
        claims: Vec<VillageId>,
    }

    #[async_trait]
    impl StarvationRepository for RecordingStarvation {
        async fn schedule_starvation_check(
            &self,
            _v: VillageId,
            due: Timestamp,
        ) -> Result<(), RepoError> {
            *self.scheduled.lock().unwrap() = Some(due);
            Ok(())
        }
        async fn cancel_starvation_check(&self, _v: VillageId) -> Result<(), RepoError> {
            *self.cancelled.lock().unwrap() = true;
            Ok(())
        }
        async fn claim_due_starvation(
            &self,
            _now: Timestamp,
            _limit: i64,
        ) -> Result<Vec<VillageId>, RepoError> {
            Ok(self.claims.clone())
        }
        async fn apply_starvation(
            &self,
            _v: VillageId,
            _settled: ResourceAmounts,
            _from: Timestamp,
            _now: Timestamp,
            survivors: &UnitCounts,
        ) -> Result<(), RepoError> {
            *self.applied_survivors.lock().unwrap() = Some(survivors.clone());
            Ok(())
        }
        async fn resolve_starvation_check(
            &self,
            _v: VillageId,
            reschedule: Option<Timestamp>,
        ) -> Result<(), RepoError> {
            *self.resolved.lock().unwrap() = Some(reschedule);
            Ok(())
        }
    }

    fn speed() -> GameSpeed {
        GameSpeed::new(1.0).unwrap()
    }

    // --- AC8: no garrison => no check ---
    #[tokio::test]
    async fn sync_cancels_without_garrison() {
        let accounts = FakeAccounts {
            garrison: vec![],
            stored: amounts(0),
        };
        let s = RecordingStarvation::default();
        sync_starvation_check(
            &accounts,
            &s,
            &economy_rules(),
            &unit_rules(),
            speed(),
            Timestamp(0),
            VillageId(1),
        )
        .await
        .unwrap();
        assert!(*s.cancelled.lock().unwrap());
        assert!(s.scheduled.lock().unwrap().is_none());
    }

    // --- AC7: a negative net schedules the check at the exact depletion instant ---
    #[tokio::test]
    async fn sync_schedules_at_depletion_instant() {
        // Production 10/h, upkeep 30/h => net -20/h; 100 crop empties in ceil(100·3600/20) = 18000 s.
        let accounts = FakeAccounts {
            garrison: vec![(UnitId("u0".into()), 30)],
            stored: amounts(100),
        };
        let s = RecordingStarvation::default();
        sync_starvation_check(
            &accounts,
            &s,
            &economy_rules(),
            &unit_rules(),
            speed(),
            Timestamp(0),
            VillageId(1),
        )
        .await
        .unwrap();
        assert_eq!(*s.scheduled.lock().unwrap(), Some(Timestamp(18_000_000)));
    }

    #[tokio::test]
    async fn sync_cancels_when_net_nonnegative() {
        // Upkeep 5 <= production 10 => sustainable.
        let accounts = FakeAccounts {
            garrison: vec![(UnitId("u0".into()), 5)],
            stored: amounts(100),
        };
        let s = RecordingStarvation::default();
        sync_starvation_check(
            &accounts,
            &s,
            &economy_rules(),
            &unit_rules(),
            speed(),
            Timestamp(0),
            VillageId(1),
        )
        .await
        .unwrap();
        assert!(*s.cancelled.lock().unwrap());
    }

    // --- AC7: the cull fires only on a dry store and brings the army to sustainable size ---
    #[tokio::test]
    async fn process_culls_to_sustainable_size() {
        let accounts = FakeAccounts {
            garrison: vec![(UnitId("u0".into()), 30)],
            stored: amounts(0),
        };
        let s = RecordingStarvation {
            claims: vec![VillageId(1)],
            ..RecordingStarvation::default()
        };
        let culled = process_due_starvation(
            &accounts,
            &s,
            &economy_rules(),
            &unit_rules(),
            speed(),
            Timestamp(0),
            10,
        )
        .await
        .unwrap();
        assert_eq!(culled, 1);
        // net_base = 10, upkeep 1 each: 10 survive, 20 starve.
        assert_eq!(
            *s.applied_survivors.lock().unwrap(),
            Some(vec![(UnitId("u0".into()), 10)])
        );
    }

    #[tokio::test]
    async fn process_reschedules_when_store_not_empty() {
        let accounts = FakeAccounts {
            garrison: vec![(UnitId("u0".into()), 30)],
            stored: amounts(100),
        };
        let s = RecordingStarvation {
            claims: vec![VillageId(1)],
            ..RecordingStarvation::default()
        };
        let culled = process_due_starvation(
            &accounts,
            &s,
            &economy_rules(),
            &unit_rules(),
            speed(),
            Timestamp(0),
            10,
        )
        .await
        .unwrap();
        assert_eq!(culled, 0);
        assert_eq!(
            *s.resolved.lock().unwrap(),
            Some(Some(Timestamp(18_000_000)))
        );
        assert!(s.applied_survivors.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn process_finishes_when_recovered() {
        let accounts = FakeAccounts {
            garrison: vec![(UnitId("u0".into()), 5)],
            stored: amounts(0),
        };
        let s = RecordingStarvation {
            claims: vec![VillageId(1)],
            ..RecordingStarvation::default()
        };
        let culled = process_due_starvation(
            &accounts,
            &s,
            &economy_rules(),
            &unit_rules(),
            speed(),
            Timestamp(0),
            10,
        )
        .await
        .unwrap();
        assert_eq!(culled, 0);
        assert_eq!(*s.resolved.lock().unwrap(), Some(None));
    }
}
