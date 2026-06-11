//! Unit use-cases: order Academy research and Smithy upgrades, and process their due orders.

use crate::ports::{AccountRepository, NewUnitOrder, RepoError, UnitOrderKind, UnitRepository};
use eperica_domain::{
    EconomyRules, GameSpeed, PlayerId, ResearchDenied, ResourceAmounts, Timestamp, Tribe, UnitId,
    UnitRules, UpgradeDenied, Village, can_afford, can_research, can_upgrade, compute_economy,
    debit, garrison_upkeep, scaled_time_secs,
};

/// Why ordering a research failed (AC6/AC7).
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ResearchError {
    /// Not enough resources for the research cost.
    #[error("not enough resources")]
    Insufficient,
    /// A research is already in progress in this village.
    #[error("a research is already in progress")]
    InProgress,
    /// The unit is already researched here (tier-1 counts as researched).
    #[error("already researched")]
    AlreadyResearched,
    /// A building requirement is unmet (including: no Academy).
    #[error("requirements not met")]
    RequirementsUnmet,
    /// The village or unit does not exist (or the unit is not of the owner's tribe).
    #[error("not found")]
    NotFound,
    /// The village's resources changed while ordering (another order settled first); retry.
    #[error("resources changed; try again")]
    Conflict,
    /// A storage/backend failure.
    #[error("storage error: {0}")]
    Backend(String),
}

impl From<RepoError> for ResearchError {
    fn from(e: RepoError) -> Self {
        match e {
            RepoError::Duplicate => ResearchError::InProgress,
            RepoError::Conflict => ResearchError::Conflict,
            other => ResearchError::Backend(other.to_string()),
        }
    }
}

/// Why ordering a Smithy upgrade failed (AC10/AC11).
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum UpgradeError {
    /// Not enough resources for the upgrade cost.
    #[error("not enough resources")]
    Insufficient,
    /// An upgrade is already in progress in this village.
    #[error("an upgrade is already in progress")]
    InProgress,
    /// The unit is not researched in this village.
    #[error("not researched")]
    NotResearched,
    /// The village has no Smithy.
    #[error("no smithy")]
    NoSmithy,
    /// The unit is at the balance max level.
    #[error("already at maximum level")]
    MaxLevel,
    /// The unit's level has caught up with the Smithy's building level.
    #[error("the smithy level is too low")]
    SmithyLevelTooLow,
    /// The village or unit does not exist (or the unit is not of the owner's tribe).
    #[error("not found")]
    NotFound,
    /// The village's resources changed while ordering (another order settled first); retry.
    #[error("resources changed; try again")]
    Conflict,
    /// A storage/backend failure.
    #[error("storage error: {0}")]
    Backend(String),
}

impl From<RepoError> for UpgradeError {
    fn from(e: RepoError) -> Self {
        match e {
            RepoError::Duplicate => UpgradeError::InProgress,
            RepoError::Conflict => UpgradeError::Conflict,
            other => UpgradeError::Backend(other.to_string()),
        }
    }
}

/// The owner's (first) village, its tribe, its settled-to-now resource amounts (net of garrison
/// upkeep, 005 AC6), and the snapshot time the amounts were computed from (for the optimistic
/// settle).
async fn village_and_amounts<A: AccountRepository>(
    accounts: &A,
    economy_rules: &EconomyRules,
    unit_rules: &UnitRules,
    speed: GameSpeed,
    now: Timestamp,
    owner: PlayerId,
) -> Result<Option<(Village, Tribe, ResourceAmounts, Timestamp)>, RepoError> {
    let Some(village) = accounts.villages_of(owner).await?.into_iter().next() else {
        return Ok(None);
    };
    let Some(tribe) = village.tribe else {
        return Ok(None);
    };
    let Some((stored, updated_at)) = accounts.stored_resources(village.id).await? else {
        return Ok(None);
    };
    let garrison = accounts.garrison(village.id).await?;
    let upkeep = garrison_upkeep(&garrison, unit_rules.roster(tribe));
    let elapsed = (now.0 - updated_at.0) / 1000;
    let amounts = compute_economy(
        stored,
        elapsed,
        &village.fields,
        &village.buildings,
        upkeep,
        economy_rules,
        speed,
    )
    .amounts;
    Ok(Some((village, tribe, amounts, updated_at)))
}

/// Order the research of `unit` in `owner`'s village (AC6/AC7, P4).
///
/// Validates tribe membership, the Academy and per-unit building requirements, the not-yet-
/// researched state, and affordability; then settles resources, debits the research cost, and
/// enqueues an order completing after `researchTime ÷ speed` (AC14). The one-active-research rule
/// is enforced by storage even under races.
///
/// # Errors
/// See [`ResearchError`].
#[allow(clippy::too_many_arguments)]
pub async fn order_research<A, U>(
    accounts: &A,
    units: &U,
    economy_rules: &EconomyRules,
    unit_rules: &UnitRules,
    speed: GameSpeed,
    now: Timestamp,
    owner: PlayerId,
    unit: UnitId,
) -> Result<(), ResearchError>
where
    A: AccountRepository,
    U: UnitRepository,
{
    let Some((village, tribe, amounts, settled_from)) =
        village_and_amounts(accounts, economy_rules, unit_rules, speed, now, owner).await?
    else {
        return Err(ResearchError::NotFound);
    };
    let spec = unit_rules
        .unit(tribe, &unit)
        .ok_or(ResearchError::NotFound)?;
    let researched = units.researched_units(village.id).await?;

    can_research(spec, researched.contains(&unit), &village.buildings).map_err(|d| match d {
        ResearchDenied::AlreadyResearched => ResearchError::AlreadyResearched,
        ResearchDenied::NoAcademy | ResearchDenied::RequirementsUnmet => {
            ResearchError::RequirementsUnmet
        }
    })?;
    // can_research guarantees a research spec exists (tier-1 was rejected above).
    let research = spec.research.as_ref().ok_or(ResearchError::NotFound)?;

    if !can_afford(amounts, research.cost) {
        return Err(ResearchError::Insufficient);
    }
    let settled = debit(amounts, research.cost);
    let duration = scaled_time_secs(research.time_secs, speed);
    let order = NewUnitOrder {
        kind: UnitOrderKind::Research,
        unit,
        target_level: None,
        complete_at: Timestamp(now.0 + duration * 1000),
    };
    units
        .start_unit_order(village.id, settled, settled_from, now, order)
        .await?;
    Ok(())
}

/// Order a Smithy upgrade of `unit` by one level in `owner`'s village (AC10/AC11, P4).
///
/// Validates tribe membership, researched state, the Smithy/level caps, and affordability; then
/// settles, debits, and enqueues an order completing after `upgradeTime ÷ speed` (AC14). The
/// one-active-upgrade rule is enforced by storage even under races.
///
/// # Errors
/// See [`UpgradeError`].
#[allow(clippy::too_many_arguments)]
pub async fn order_smithy_upgrade<A, U>(
    accounts: &A,
    units: &U,
    economy_rules: &EconomyRules,
    unit_rules: &UnitRules,
    speed: GameSpeed,
    now: Timestamp,
    owner: PlayerId,
    unit: UnitId,
) -> Result<(), UpgradeError>
where
    A: AccountRepository,
    U: UnitRepository,
{
    let Some((village, tribe, amounts, settled_from)) =
        village_and_amounts(accounts, economy_rules, unit_rules, speed, now, owner).await?
    else {
        return Err(UpgradeError::NotFound);
    };
    let spec = unit_rules
        .unit(tribe, &unit)
        .ok_or(UpgradeError::NotFound)?;
    let researched = units.researched_units(village.id).await?;
    let level = units
        .unit_levels(village.id)
        .await?
        .into_iter()
        .find(|(u, _)| u == &unit)
        .map_or(0, |(_, l)| l);

    can_upgrade(
        spec,
        researched.contains(&unit),
        level,
        &village.buildings,
        &unit_rules.smithy,
    )
    .map_err(|d| match d {
        UpgradeDenied::NotResearched => UpgradeError::NotResearched,
        UpgradeDenied::NoSmithy => UpgradeError::NoSmithy,
        UpgradeDenied::AtMaxLevel => UpgradeError::MaxLevel,
        UpgradeDenied::SmithyLevelTooLow => UpgradeError::SmithyLevelTooLow,
    })?;

    let cost = unit_rules
        .smithy
        .upgrade_cost(spec, level)
        .ok_or(UpgradeError::MaxLevel)?;
    let base_time = unit_rules
        .smithy
        .base_time_secs(level)
        .ok_or(UpgradeError::MaxLevel)?;
    if !can_afford(amounts, cost) {
        return Err(UpgradeError::Insufficient);
    }
    let settled = debit(amounts, cost);
    let duration = scaled_time_secs(base_time, speed);
    let order = NewUnitOrder {
        kind: UnitOrderKind::SmithyUpgrade,
        unit,
        target_level: Some(level + 1),
        complete_at: Timestamp(now.0 + duration * 1000),
    };
    units
        .start_unit_order(village.id, settled, settled_from, now, order)
        .await?;
    Ok(())
}

/// Claim and apply all unit orders due at `now` (up to `limit`); returns how many were applied —
/// the System actor completing research/upgrades (AC8/AC12).
///
/// # Errors
/// Propagates [`RepoError`] from the repository.
pub async fn process_due_unit_orders<U>(
    units: &U,
    now: Timestamp,
    limit: i64,
) -> Result<usize, RepoError>
where
    U: UnitRepository,
{
    let due = units.claim_due_unit_orders(now, limit).await?;
    let mut applied = 0;
    for order in due {
        // Log-and-continue: a failed apply must not strand the rest of the batch. Failed
        // (still-`processing`) orders are requeued at scheduler startup; apply is idempotent.
        match units.apply_unit_order(order).await {
            Ok(()) => applied += 1,
            Err(e) => tracing::error!(error = %e, "failed to apply due unit order"),
        }
    }
    Ok(applied)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{ActiveUnitOrder, DueUnitOrder, NewUser, UserRecord};
    use async_trait::async_trait;
    use eperica_domain::{
        BuildingKind, BuildingSlot, Coordinate, ResearchSpec, ResourceField, ResourceKind,
        SmithyRules, StartingVillage, TrainingRules, UnitRole, UnitSpec, VillageId,
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

    fn unit(id: &str, research: Option<ResearchSpec>) -> UnitSpec {
        UnitSpec {
            id: UnitId(id.to_owned()),
            name: id.to_owned(),
            role: UnitRole::Infantry,
            attack: 40,
            defense_infantry: 35,
            defense_cavalry: 50,
            speed: 6,
            carry_capacity: 50,
            crop_upkeep: 1,
            cost: amounts(100),
            train_secs: 1600,
            trained_in: BuildingKind::Barracks,
            research,
        }
    }

    fn roster() -> Vec<UnitSpec> {
        let mut units = vec![unit("tier1", None)];
        units.push(unit(
            "spearman",
            Some(ResearchSpec {
                cost: amounts(500),
                time_secs: 3600,
                requirements: vec![(BuildingKind::Academy, 1)],
            }),
        ));
        for i in 2..10 {
            units.push(unit(
                &format!("u{i}"),
                Some(ResearchSpec {
                    cost: amounts(500),
                    time_secs: 3600,
                    requirements: vec![(BuildingKind::Academy, 5)],
                }),
            ));
        }
        units
    }

    fn unit_rules() -> UnitRules {
        UnitRules::new(
            HashMap::from([
                (Tribe::Romans, roster()),
                (Tribe::Teutons, roster()),
                (Tribe::Gauls, roster()),
            ]),
            SmithyRules {
                cost_permille_per_level: vec![1500, 1900],
                time_secs_per_level: vec![3600, 4500],
            },
            TrainingRules {
                building_factor_per_level: vec![1.0, 1.0, 1.25],
            },
        )
        .expect("valid rules")
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

    fn make_village(buildings: &[(BuildingKind, u8)]) -> Village {
        Village {
            id: VillageId(1),
            owner: PlayerId(1),
            coordinate: Coordinate::new(0, 0),
            tribe: Some(Tribe::Gauls),
            fields: (0..18)
                .map(|_| ResourceField {
                    kind: ResourceKind::Wood,
                    level: 0,
                })
                .collect(),
            buildings: buildings
                .iter()
                .map(|&(kind, level)| BuildingSlot { kind, level })
                .collect(),
        }
    }

    struct FakeAccounts {
        village: Village,
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
            Ok(vec![self.village.clone()])
        }
        async fn stored_resources(
            &self,
            _v: VillageId,
        ) -> Result<Option<(ResourceAmounts, Timestamp)>, RepoError> {
            Ok(Some((self.stored, Timestamp(0))))
        }
        async fn garrison(&self, _v: VillageId) -> Result<eperica_domain::UnitCounts, RepoError> {
            Ok(Vec::new())
        }
    }

    #[derive(Default)]
    struct FakeUnits {
        duplicate: bool,
        researched: Vec<UnitId>,
        levels: Vec<(UnitId, u8)>,
        last: Mutex<Option<NewUnitOrder>>,
        last_settled: Mutex<Option<ResourceAmounts>>,
    }

    #[async_trait]
    impl UnitRepository for FakeUnits {
        async fn start_unit_order(
            &self,
            _v: VillageId,
            settled: ResourceAmounts,
            _settled_from: Timestamp,
            _now: Timestamp,
            order: NewUnitOrder,
        ) -> Result<(), RepoError> {
            if self.duplicate {
                return Err(RepoError::Duplicate);
            }
            *self.last_settled.lock().unwrap() = Some(settled);
            *self.last.lock().unwrap() = Some(order);
            Ok(())
        }
        async fn active_unit_orders(
            &self,
            _v: VillageId,
        ) -> Result<Vec<ActiveUnitOrder>, RepoError> {
            Ok(Vec::new())
        }
        async fn researched_units(&self, _v: VillageId) -> Result<Vec<UnitId>, RepoError> {
            Ok(self.researched.clone())
        }
        async fn unit_levels(&self, _v: VillageId) -> Result<Vec<(UnitId, u8)>, RepoError> {
            Ok(self.levels.clone())
        }
        async fn claim_due_unit_orders(
            &self,
            _now: Timestamp,
            _limit: i64,
        ) -> Result<Vec<DueUnitOrder>, RepoError> {
            Ok(Vec::new())
        }
        async fn apply_unit_order(&self, _due: DueUnitOrder) -> Result<(), RepoError> {
            Ok(())
        }
    }

    async fn research(
        accounts: &FakeAccounts,
        units: &FakeUnits,
        unit: &str,
        speed: f64,
    ) -> Result<(), ResearchError> {
        order_research(
            accounts,
            units,
            &economy_rules(),
            &unit_rules(),
            GameSpeed::new(speed).unwrap(),
            Timestamp(0),
            PlayerId(1),
            UnitId(unit.to_owned()),
        )
        .await
    }

    async fn upgrade(
        accounts: &FakeAccounts,
        units: &FakeUnits,
        unit: &str,
    ) -> Result<(), UpgradeError> {
        order_smithy_upgrade(
            accounts,
            units,
            &economy_rules(),
            &unit_rules(),
            GameSpeed::new(1.0).unwrap(),
            Timestamp(0),
            PlayerId(1),
            UnitId(unit.to_owned()),
        )
        .await
    }

    // --- AC6: research success path ---
    #[tokio::test]
    async fn ordering_a_research_debits_and_enqueues() {
        let accounts = FakeAccounts {
            village: make_village(&[(BuildingKind::Academy, 1)]),
            stored: amounts(800),
        };
        let units = FakeUnits::default();
        research(&accounts, &units, "spearman", 1.0).await.unwrap();
        let order = units.last.lock().unwrap().clone().expect("enqueued");
        assert_eq!(order.kind, UnitOrderKind::Research);
        assert_eq!(order.unit, UnitId("spearman".into()));
        assert_eq!(order.target_level, None);
        // AC6: completes at now + researchTime (3600s at speed 1).
        assert_eq!(order.complete_at, Timestamp(3_600_000));
        // The cost (500) was debited from the current amount (800).
        assert_eq!(
            units.last_settled.lock().unwrap().expect("settled"),
            amounts(300)
        );
    }

    // --- AC14: research time scales with world speed ---
    #[tokio::test]
    async fn research_time_scales_with_speed() {
        let accounts = FakeAccounts {
            village: make_village(&[(BuildingKind::Academy, 1)]),
            stored: amounts(800),
        };
        let units = FakeUnits::default();
        research(&accounts, &units, "spearman", 2.0).await.unwrap();
        let order = units.last.lock().unwrap().clone().expect("enqueued");
        assert_eq!(order.complete_at, Timestamp(1_800_000)); // 3600s ÷ 2
    }

    // --- AC7: every research rejection leaves state untouched ---
    #[tokio::test]
    async fn research_rejections() {
        // Insufficient resources.
        let accounts = FakeAccounts {
            village: make_village(&[(BuildingKind::Academy, 1)]),
            stored: amounts(100),
        };
        let units = FakeUnits::default();
        assert_eq!(
            research(&accounts, &units, "spearman", 1.0).await,
            Err(ResearchError::Insufficient)
        );
        assert!(units.last.lock().unwrap().is_none());

        // No Academy.
        let accounts = FakeAccounts {
            village: make_village(&[(BuildingKind::MainBuilding, 3)]),
            stored: amounts(800),
        };
        assert_eq!(
            research(&accounts, &units, "spearman", 1.0).await,
            Err(ResearchError::RequirementsUnmet)
        );

        // Per-unit requirement unmet (u2 needs Academy 5).
        let accounts = FakeAccounts {
            village: make_village(&[(BuildingKind::Academy, 1)]),
            stored: amounts(800),
        };
        assert_eq!(
            research(&accounts, &units, "u2", 1.0).await,
            Err(ResearchError::RequirementsUnmet)
        );

        // AC9: tier-1 is already researched.
        assert_eq!(
            research(&accounts, &units, "tier1", 1.0).await,
            Err(ResearchError::AlreadyResearched)
        );

        // Already researched.
        let researched = FakeUnits {
            researched: vec![UnitId("spearman".into())],
            ..FakeUnits::default()
        };
        assert_eq!(
            research(&accounts, &researched, "spearman", 1.0).await,
            Err(ResearchError::AlreadyResearched)
        );

        // Not in this tribe's roster.
        assert_eq!(
            research(&accounts, &units, "legionnaire", 1.0).await,
            Err(ResearchError::NotFound)
        );

        // A research already in progress (storage-enforced).
        let busy = FakeUnits {
            duplicate: true,
            ..FakeUnits::default()
        };
        assert_eq!(
            research(&accounts, &busy, "spearman", 1.0).await,
            Err(ResearchError::InProgress)
        );
        assert!(units.last.lock().unwrap().is_none());
    }

    // --- AC10: upgrade success path ---
    #[tokio::test]
    async fn ordering_an_upgrade_debits_and_enqueues() {
        let accounts = FakeAccounts {
            village: make_village(&[(BuildingKind::Smithy, 3)]),
            stored: amounts(800),
        };
        let units = FakeUnits::default();
        // Tier-1 is upgradable without a research row (AC9).
        upgrade(&accounts, &units, "tier1").await.unwrap();
        let order = units.last.lock().unwrap().clone().expect("enqueued");
        assert_eq!(order.kind, UnitOrderKind::SmithyUpgrade);
        assert_eq!(order.target_level, Some(1));
        // AC10: completes at now + upgradeTime (3600s at speed 1).
        assert_eq!(order.complete_at, Timestamp(3_600_000));
        // Cost = unit cost (100) × 1500‰ = 150, debited from 800.
        assert_eq!(
            units.last_settled.lock().unwrap().expect("settled"),
            amounts(650)
        );
    }

    // --- AC11: every upgrade rejection leaves state untouched ---
    #[tokio::test]
    async fn upgrade_rejections() {
        let smithy3 = || make_village(&[(BuildingKind::Smithy, 3)]);

        // Not researched.
        let accounts = FakeAccounts {
            village: smithy3(),
            stored: amounts(800),
        };
        let units = FakeUnits::default();
        assert_eq!(
            upgrade(&accounts, &units, "spearman").await,
            Err(UpgradeError::NotResearched)
        );
        assert!(units.last.lock().unwrap().is_none());

        // No Smithy.
        let no_smithy = FakeAccounts {
            village: make_village(&[(BuildingKind::Academy, 1)]),
            stored: amounts(800),
        };
        assert_eq!(
            upgrade(&no_smithy, &units, "tier1").await,
            Err(UpgradeError::NoSmithy)
        );

        // Level caught up with the Smithy (level 1 unit, Smithy 1).
        let smithy1 = FakeAccounts {
            village: make_village(&[(BuildingKind::Smithy, 1)]),
            stored: amounts(800),
        };
        let leveled = FakeUnits {
            levels: vec![(UnitId("tier1".into()), 1)],
            ..FakeUnits::default()
        };
        assert_eq!(
            upgrade(&smithy1, &leveled, "tier1").await,
            Err(UpgradeError::SmithyLevelTooLow)
        );

        // Balance max level (2-entry tables).
        let maxed = FakeUnits {
            levels: vec![(UnitId("tier1".into()), 2)],
            ..FakeUnits::default()
        };
        let smithy9 = FakeAccounts {
            village: make_village(&[(BuildingKind::Smithy, 9)]),
            stored: amounts(8000),
        };
        assert_eq!(
            upgrade(&smithy9, &maxed, "tier1").await,
            Err(UpgradeError::MaxLevel)
        );

        // Insufficient resources (cost 150 at level 0).
        let poor = FakeAccounts {
            village: smithy3(),
            stored: amounts(100),
        };
        assert_eq!(
            upgrade(&poor, &units, "tier1").await,
            Err(UpgradeError::Insufficient)
        );

        // An upgrade already in progress (storage-enforced).
        let busy = FakeUnits {
            duplicate: true,
            ..FakeUnits::default()
        };
        let accounts = FakeAccounts {
            village: smithy3(),
            stored: amounts(800),
        };
        assert_eq!(
            upgrade(&accounts, &busy, "tier1").await,
            Err(UpgradeError::InProgress)
        );
    }
}
