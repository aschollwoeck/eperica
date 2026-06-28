//! Trade use-cases (008): send a resource shipment via merchants, and deliver due shipments
//! (crediting the target capped to its storage) then free the merchants on the empty return. The
//! merchant count, capacity, and travel timing are the pure domain (`trade`/`movement`); this layer
//! validates, debits/credits through the repository, and schedules the legs.

use crate::economy::settle_amounts;
use crate::ports::{AccountRepository, DueTrade, RepoError, TradeRepository};
use eperica_domain::{
    BuildingKind, Coordinate, EconomyRules, GameSpeed, MerchantRules, PlayerId, ResourceAmounts,
    Timestamp, TradeKind, UnitRules, WorldMap, bundle_is_empty, bundle_total, debit,
    deposit_capped, merchants_required, travel_time_secs_floored,
};

/// Why sending a shipment failed (008 AC2).
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TradeError {
    /// The sender has no Marketplace.
    #[error("a marketplace is required to trade")]
    NoMarketplace,
    /// The bundle is empty (or carries a negative amount).
    #[error("no resources selected")]
    EmptyBundle,
    /// The sender does not hold all the requested resources.
    #[error("not enough resources")]
    Insufficient,
    /// The shipment needs more merchants than are free.
    #[error("not enough free merchants")]
    NotEnoughMerchants,
    /// No village occupies the target tile.
    #[error("no village at the target")]
    NoTargetThere,
    /// The target is the sender's own village tile.
    #[error("cannot trade with your own village")]
    SameTile,
    /// The owning village/account was not found.
    #[error("not found")]
    NotFound,
    /// A storage/backend failure.
    #[error("storage error: {0}")]
    Backend(String),
}

impl From<RepoError> for TradeError {
    fn from(e: RepoError) -> Self {
        TradeError::Backend(e.to_string())
    }
}

/// The Marketplace level of a village (0 if none built).
fn marketplace_level(village: &eperica_domain::Village) -> u8 {
    village
        .buildings
        .iter()
        .find(|b| b.kind == BuildingKind::Marketplace)
        .map_or(0, |b| b.level)
}

/// Send `bundle` from `owner`'s village to the village at `target` (008 AC1).
///
/// Validates the Marketplace, the bundle, free merchants, and the target; computes travel time from
/// the toroidal distance and the tribe's merchant speed (P7); atomically debits the sender's stores
/// (optimistic settle) and schedules the delivery.
///
/// # Errors
/// See [`TradeError`].
#[allow(clippy::too_many_arguments)]
pub async fn order_trade<A, T>(
    accounts: &A,
    trades: &T,
    economy_rules: &EconomyRules,
    unit_rules: &UnitRules,
    merchant_rules: &MerchantRules,
    map: &WorldMap,
    speed: GameSpeed,
    now: Timestamp,
    owner: PlayerId,
    selected: Option<eperica_domain::VillageId>,
    target: Coordinate,
    bundle: ResourceAmounts,
) -> Result<(), TradeError>
where
    A: AccountRepository,
    T: TradeRepository,
{
    let Some(home) = crate::economy::select_village(accounts, owner, selected).await? else {
        return Err(TradeError::NotFound);
    };
    let Some(tribe) = home.tribe else {
        return Err(TradeError::NotFound);
    };

    let level = marketplace_level(&home);
    if level == 0 {
        return Err(TradeError::NoMarketplace);
    }
    if bundle.wood < 0
        || bundle.clay < 0
        || bundle.iron < 0
        || bundle.crop < 0
        || bundle_is_empty(bundle)
    {
        return Err(TradeError::EmptyBundle);
    }

    // Settle the sender's stores to `now`; every carried amount must be available.
    let Some((stored, updated_at)) = accounts.stored_resources(home.id).await? else {
        return Err(TradeError::NotFound);
    };
    let garrison = accounts.garrison(home.id).await?;
    let settled = settle_amounts(
        stored,
        updated_at,
        now,
        &home,
        &garrison,
        economy_rules,
        unit_rules,
        speed,
    );
    if bundle.wood > settled.wood
        || bundle.clay > settled.clay
        || bundle.iron > settled.iron
        || bundle.crop > settled.crop
    {
        return Err(TradeError::Insufficient);
    }

    // Free merchants must cover the load.
    let profile = merchant_rules.profile(tribe);
    let required = merchants_required(bundle_total(bundle), profile.capacity);
    let available = merchant_rules
        .merchants_total(level)
        .saturating_sub(trades.committed_merchants(home.id).await?);
    if required > available {
        return Err(TradeError::NotEnoughMerchants);
    }

    // Target: a village on another tile.
    let Some(dest) = accounts.village_at(target).await? else {
        return Err(TradeError::NoTargetThere);
    };
    if dest.id == home.id || dest.coordinate == home.coordinate {
        return Err(TradeError::SameTile);
    }

    let distance = map.distance(home.coordinate, dest.coordinate);
    let secs = travel_time_secs_floored(distance, profile.speed, speed);
    let arrive = Timestamp(now.0 + secs * 1000);
    let after = debit(settled, bundle);

    trades
        .start_trade(
            home.id,
            dest.id,
            owner,
            home.coordinate,
            dest.coordinate,
            after,
            updated_at,
            now,
            arrive,
            bundle,
            required,
        )
        .await
        .map_err(|e| match e {
            RepoError::Conflict => TradeError::Insufficient,
            other => TradeError::Backend(other.to_string()),
        })?;
    Ok(())
}

/// Claim and apply trade legs whose arrival is due (the System actor, AC4/AC5): deliver shipments
/// (credit the target capped to capacity, then schedule the empty return) and free merchants when a
/// return arrives. Returns the **credited target** villages (their crop rose — callers re-sync the
/// starvation check).
///
/// # Errors
/// Propagates [`RepoError`] from the repository (per-leg failures are logged and skipped — recovered
/// by the startup orphan requeue).
#[allow(clippy::too_many_arguments)]
pub async fn process_due_trades<A, T>(
    accounts: &A,
    trades: &T,
    economy_rules: &EconomyRules,
    unit_rules: &UnitRules,
    merchant_rules: &MerchantRules,
    map: &WorldMap,
    speed: GameSpeed,
    now: Timestamp,
    limit: i64,
) -> Result<Vec<eperica_domain::VillageId>, RepoError>
where
    A: AccountRepository,
    T: TradeRepository,
{
    let due = trades.claim_due_trades(now, limit).await?;
    let mut credited = Vec::new();
    for leg in due {
        match leg.kind {
            TradeKind::Deliver => {
                match deliver(
                    accounts,
                    trades,
                    economy_rules,
                    unit_rules,
                    merchant_rules,
                    map,
                    speed,
                    &leg,
                )
                .await
                {
                    // Credited — the target's crop rose, so the caller re-syncs its starvation check.
                    Ok(true) => credited.push(leg.target_village),
                    // Released after retry exhaustion (nothing credited) — retried next tick.
                    Ok(false) => {}
                    // Log-and-continue: a failed apply leaves the leg `processing`, recovered by the
                    // startup orphan requeue and re-applied (delivery is exactly-once under the guard).
                    Err(e) => tracing::error!(error = %e, "failed to deliver due trade"),
                }
            }
            TradeKind::Return => {
                if let Err(e) = trades.complete_trade(leg.id).await {
                    tracing::error!(error = %e, "failed to complete due return");
                }
            }
        }
    }
    Ok(credited)
}

/// Deliver one due shipment: credit the target capped to its storage and schedule the empty return.
/// Retries the optimistic credit a few times if the target's stores move underneath it. Returns
/// `true` if the target was credited, `false` if the leg was released back for a later retry.
#[allow(clippy::too_many_arguments)]
async fn deliver<A, T>(
    accounts: &A,
    trades: &T,
    economy_rules: &EconomyRules,
    unit_rules: &UnitRules,
    merchant_rules: &MerchantRules,
    map: &WorldMap,
    speed: GameSpeed,
    leg: &DueTrade,
) -> Result<bool, RepoError>
where
    A: AccountRepository,
    T: TradeRepository,
{
    let Some(target) = accounts.village_by_id(leg.target_village).await? else {
        // The target was deleted (its trade rows cascade with it); nothing to credit.
        trades.complete_trade(leg.id).await?;
        return Ok(false);
    };
    // The return is paced by the sender's merchant speed; it departs at the delivery instant (P2).
    let Some(home) = accounts.village_by_id(leg.home_village).await? else {
        return Err(RepoError::Backend("trade home village missing".into()));
    };
    let speed_fields = home
        .tribe
        .map_or(1, |t| merchant_rules.profile(t).speed.max(1));
    let distance = map.distance(leg.dest, leg.origin);
    let return_secs = travel_time_secs_floored(distance, speed_fields, speed);
    let return_arrive = Timestamp(leg.arrive_at.0 + return_secs * 1000);
    let garrison = accounts.garrison(target.id).await?;

    for _ in 0..5 {
        let Some((stored, snapshot)) = accounts.stored_resources(target.id).await? else {
            return Err(RepoError::Backend("target resources missing".into()));
        };
        // The credit's settle clock never runs backwards: if the target was already settled past the
        // arrival (the normal poll window), settle to that later instant — otherwise the next read
        // would re-accrue production over [arrive, snapshot] already folded into `stored` (double
        // credit). Mirrors the training-delivery clamp (units.rs). The return still departs at the
        // true arrival.
        let credit_clock = Timestamp(leg.arrive_at.0.max(snapshot.0));
        let upkeep = (village_upkeep(&target, &garrison, unit_rules) as f64
            * target.artifact_effects.upkeep)
            .round() as i64;
        let economy = eperica_domain::compute_economy(
            stored,
            (credit_clock.0 - snapshot.0) / 1000,
            &target.fields,
            &target.buildings,
            upkeep,
            economy_rules,
            speed,
            target.oasis_bonus,
            target.artifact_effects.storage,
        );
        let credited = deposit_capped(economy.amounts, leg.bundle, economy.capacities);
        match trades
            .deliver_and_schedule_return(leg, credited, snapshot, credit_clock, return_arrive)
            .await
        {
            Ok(()) => return Ok(true),
            Err(RepoError::Conflict) => continue, // the target settled concurrently — re-read & retry
            Err(e) => return Err(e),
        }
    }
    // Persistent contention: hand the leg back to `in_transit` so the next tick retries it (its
    // merchants stay committed meanwhile), rather than stranding it in `processing` until restart.
    trades.release_trade(leg.id).await?;
    Ok(false)
}

/// A village garrison's crop upkeep (for settling its net crop), 0 for a tribe-less village.
fn village_upkeep(
    village: &eperica_domain::Village,
    garrison: &eperica_domain::UnitCounts,
    unit_rules: &UnitRules,
) -> i64 {
    village.tribe.map_or(0, |t| {
        eperica_domain::garrison_upkeep(garrison, unit_rules.roster(t))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{NewUser, TradeView, UserRecord, VillageMarker};
    use async_trait::async_trait;
    use eperica_domain::{
        BuildingKind, BuildingSlot, FieldDistribution, MerchantProfile, SmithyRules,
        StartingVillage, TrainingRules, Tribe, UnitCounts, UnitRole, UnitRules, UnitSpec, Village,
        VillageId,
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

    fn unit_rules() -> UnitRules {
        let roster = || {
            (0..10)
                .map(|i| UnitSpec {
                    id: eperica_domain::UnitId(format!("u{i}")),
                    name: format!("u{i}"),
                    role: UnitRole::Infantry,
                    attack: 1,
                    defense_infantry: 1,
                    defense_cavalry: 1,
                    scouting: 0,
                    speed: 6,
                    carry_capacity: 0,
                    crop_upkeep: 0,
                    point_value: 0,
                    cost: amounts(1),
                    train_secs: 1,
                    trained_in: BuildingKind::Barracks,
                    siege_kind: None,
                    research: (i > 0).then(|| eperica_domain::ResearchSpec {
                        cost: amounts(1),
                        time_secs: 1,
                        requirements: vec![],
                    }),
                })
                .collect::<Vec<_>>()
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
        .unwrap()
    }

    // Zero production / huge caps: settled == stored, so trade math is exact in the tests.
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

    fn merchant_rules() -> MerchantRules {
        MerchantRules::new(
            HashMap::from([
                (
                    Tribe::Romans,
                    MerchantProfile {
                        capacity: 500,
                        speed: 16,
                    },
                ),
                (
                    Tribe::Teutons,
                    MerchantProfile {
                        capacity: 1000,
                        speed: 12,
                    },
                ),
                (
                    Tribe::Gauls,
                    MerchantProfile {
                        capacity: 750,
                        speed: 24,
                    },
                ),
            ]),
            vec![0, 1, 2, 3, 4, 5],
        )
        .unwrap()
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

    fn village(id: u128, owner: u128, coord: Coordinate, market_level: u8) -> Village {
        let mut buildings = vec![BuildingSlot {
            slot: 0,
            kind: BuildingKind::RallyPoint,
            level: 1,
        }];
        if market_level > 0 {
            buildings.push(BuildingSlot {
                slot: 0,
                kind: BuildingKind::Marketplace,
                level: market_level,
            });
        }
        Village {
            id: VillageId(id),
            owner: PlayerId(owner),
            coordinate: coord,
            tribe: Some(Tribe::Gauls),
            fields: FieldDistribution::new(4, 4, 4, 6).unwrap().fields(),
            buildings,
            oasis_bonus: Default::default(),
            is_capital: false,
            is_natar: false,
            is_wonder_site: false,
            artifact_effects: eperica_domain::ArtifactEffects::NONE,
        }
    }

    struct FakeAccounts {
        home: Village,
        stored: ResourceAmounts,
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
            Ok(Some((self.stored, Timestamp(0))))
        }
        async fn garrison(&self, _v: VillageId) -> Result<UnitCounts, RepoError> {
            Ok(Vec::new())
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
        target: VillageId,
        bundle: ResourceAmounts,
        merchants: u32,
        arrive: Timestamp,
    }

    #[derive(Default)]
    struct FakeTrades {
        committed: u32,
        sent: Mutex<Option<Sent>>,
        /// Legs returned (once) by `claim_due_trades`.
        claim: Mutex<Vec<DueTrade>>,
        /// When set, every `deliver_and_schedule_return` fails with `Conflict` (drives the retry path).
        conflict: bool,
        /// Leg ids handed back via `release_trade`.
        released: Mutex<Vec<u128>>,
    }

    #[async_trait]
    impl TradeRepository for FakeTrades {
        async fn committed_merchants(&self, _home: VillageId) -> Result<u32, RepoError> {
            Ok(self.committed)
        }
        #[allow(clippy::too_many_arguments)]
        async fn start_trade(
            &self,
            home: VillageId,
            target: VillageId,
            _owner: PlayerId,
            _origin: Coordinate,
            _dest: Coordinate,
            _settled: ResourceAmounts,
            _settled_from: Timestamp,
            _now: Timestamp,
            arrive_at: Timestamp,
            bundle: ResourceAmounts,
            merchants: u32,
        ) -> Result<(), RepoError> {
            *self.sent.lock().unwrap() = Some(Sent {
                home,
                target,
                bundle,
                merchants,
                arrive: arrive_at,
            });
            Ok(())
        }
        async fn active_trades(&self, _o: PlayerId) -> Result<Vec<TradeView>, RepoError> {
            Ok(Vec::new())
        }
        async fn claim_due_trades(
            &self,
            _now: Timestamp,
            _limit: i64,
        ) -> Result<Vec<DueTrade>, RepoError> {
            Ok(std::mem::take(&mut self.claim.lock().unwrap()))
        }
        async fn deliver_and_schedule_return(
            &self,
            _due: &DueTrade,
            _target_settled: ResourceAmounts,
            _target_from: Timestamp,
            _credit_clock: Timestamp,
            _return_arrive: Timestamp,
        ) -> Result<(), RepoError> {
            if self.conflict {
                Err(RepoError::Conflict)
            } else {
                Ok(())
            }
        }
        async fn complete_trade(&self, _id: u128) -> Result<(), RepoError> {
            Ok(())
        }
        async fn release_trade(&self, id: u128) -> Result<(), RepoError> {
            self.released.lock().unwrap().push(id);
            Ok(())
        }
    }

    fn accounts(stored: ResourceAmounts, market: u8, target: Option<Village>) -> FakeAccounts {
        FakeAccounts {
            home: village(1, 1, Coordinate::new(0, 0), market),
            stored,
            target,
        }
    }

    async fn send(
        acc: &FakeAccounts,
        tr: &FakeTrades,
        bundle: ResourceAmounts,
    ) -> Result<(), TradeError> {
        order_trade(
            acc,
            tr,
            &economy_rules(),
            &unit_rules(),
            &merchant_rules(),
            &map(),
            GameSpeed::new(1.0).unwrap(),
            Timestamp(0),
            PlayerId(1),
            None,
            Coordinate::new(3, 4), // distance 5 from home
            bundle,
        )
        .await
    }

    fn bundle(wood: i64, clay: i64, iron: i64, crop: i64) -> ResourceAmounts {
        ResourceAmounts {
            wood,
            clay,
            iron,
            crop,
        }
    }

    // AC1: a shipment debits the sender, commits the right merchants, and schedules the arrival.
    #[tokio::test]
    async fn sending_a_shipment_schedules_the_delivery() {
        let acc = accounts(
            amounts(1000),
            5,
            Some(village(2, 2, Coordinate::new(3, 4), 0)),
        );
        let tr = FakeTrades::default();
        send(&acc, &tr, bundle(300, 0, 0, 0)).await.unwrap();
        let sent = tr.sent.lock().unwrap().clone().expect("sent");
        assert_eq!(sent.home, VillageId(1));
        assert_eq!(sent.target, VillageId(2));
        assert_eq!(sent.bundle, bundle(300, 0, 0, 0));
        assert_eq!(sent.merchants, 1); // ceil(300 / 750)
        // distance 5, Gaul merchant speed 24, world 1 ⇒ 5/24 h = 750 s.
        assert_eq!(sent.arrive, Timestamp(750_000));
    }

    // AC3: a bigger load needs more merchants; committed merchants lower availability.
    #[tokio::test]
    async fn merchant_count_scales_and_is_limited() {
        let acc = accounts(
            amounts(1000),
            5,
            Some(village(2, 2, Coordinate::new(3, 4), 0)),
        );
        let tr = FakeTrades::default();
        // total 1500 / Gaul cap 750 ⇒ 2 merchants.
        send(&acc, &tr, bundle(750, 750, 0, 0)).await.unwrap();
        assert_eq!(tr.sent.lock().unwrap().clone().unwrap().merchants, 2);

        // Only 1 merchant free (4 of 5 committed) but the load needs 2 ⇒ rejected, nothing sent.
        let tr = FakeTrades {
            committed: 4,
            ..FakeTrades::default()
        };
        assert_eq!(
            send(&acc, &tr, bundle(750, 750, 0, 0)).await,
            Err(TradeError::NotEnoughMerchants)
        );
        assert!(tr.sent.lock().unwrap().is_none());
    }

    // AC2: rejections leave the sender untouched (no shipment created).
    #[tokio::test]
    async fn send_rejections() {
        let target = || Some(village(2, 2, Coordinate::new(3, 4), 0));

        // No Marketplace.
        let acc = accounts(amounts(1000), 0, target());
        let tr = FakeTrades::default();
        assert_eq!(
            send(&acc, &tr, bundle(100, 0, 0, 0)).await,
            Err(TradeError::NoMarketplace)
        );
        assert!(tr.sent.lock().unwrap().is_none());

        // Empty bundle.
        let acc = accounts(amounts(1000), 5, target());
        assert_eq!(
            send(&acc, &tr, bundle(0, 0, 0, 0)).await,
            Err(TradeError::EmptyBundle)
        );

        // Over the stored amount.
        assert_eq!(
            send(&acc, &tr, bundle(2000, 0, 0, 0)).await,
            Err(TradeError::Insufficient)
        );

        // No village at the target.
        let acc = accounts(amounts(1000), 5, None);
        assert_eq!(
            send(&acc, &tr, bundle(100, 0, 0, 0)).await,
            Err(TradeError::NoTargetThere)
        );

        // Target resolves to the sender's own village (same id as home).
        let acc = accounts(
            amounts(1000),
            5,
            Some(village(1, 1, Coordinate::new(3, 4), 0)),
        );
        assert_eq!(
            send(&acc, &tr, bundle(100, 0, 0, 0)).await,
            Err(TradeError::SameTile)
        );
        assert!(tr.sent.lock().unwrap().is_none());
    }

    // AC4 liveness: a deliver that loses the optimistic credit every retry releases the leg back to
    // in_transit (for the next tick) instead of stranding it, and does not count as credited.
    #[tokio::test]
    async fn deliver_releases_the_leg_when_credit_keeps_conflicting() {
        let acc = accounts(amounts(1000), 5, None);
        let leg = DueTrade {
            id: 42,
            kind: TradeKind::Deliver,
            owner: PlayerId(1),
            home_village: VillageId(1),
            target_village: VillageId(2),
            origin: Coordinate::new(0, 0),
            dest: Coordinate::new(3, 4),
            arrive_at: Timestamp(1000),
            bundle: bundle(300, 0, 0, 0),
            merchants: 1,
        };
        let tr = FakeTrades {
            claim: Mutex::new(vec![leg]),
            conflict: true,
            ..FakeTrades::default()
        };
        let credited = process_due_trades(
            &acc,
            &tr,
            &economy_rules(),
            &unit_rules(),
            &merchant_rules(),
            &map(),
            GameSpeed::new(1.0).unwrap(),
            Timestamp(2000),
            100,
        )
        .await
        .unwrap();
        assert!(credited.is_empty()); // nothing credited
        assert_eq!(*tr.released.lock().unwrap(), vec![42]); // handed back for a retry
    }
}
