//! Unit use-cases: order Academy research and Smithy upgrades, and process their due orders.

use crate::ports::{
    AccountRepository, NewTrainingOrder, NewUnitOrder, RepoError, TrainingRepository,
    UnitOrderKind, UnitRepository,
};
use eperica_domain::{
    EconomyRules, GameSpeed, PlayerId, ResearchDenied, ResourceAmounts, Timestamp, TrainDenied,
    Tribe, UnitId, UnitRules, UpgradeDenied, Village, VillageId, batch_cost, can_afford,
    can_research, can_train, can_upgrade, compute_economy, debit, garrison_upkeep,
    per_unit_time_secs, scaled_time_secs,
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
    selected: Option<eperica_domain::VillageId>,
) -> Result<Option<(Village, Tribe, ResourceAmounts, Timestamp)>, RepoError> {
    let Some(village) = crate::economy::select_village(accounts, owner, selected).await? else {
        return Ok(None);
    };
    let Some(tribe) = village.tribe else {
        return Ok(None);
    };
    let Some((stored, updated_at)) = accounts.stored_resources(village.id).await? else {
        return Ok(None);
    };
    let garrison = accounts.garrison(village.id).await?;
    // 020 AC6: artifact Sustenance/Storage ride this settle too (consistent with the read).
    let base_upkeep = garrison_upkeep(&garrison, unit_rules.roster(tribe));
    let upkeep = (base_upkeep as f64 * village.artifact_effects.upkeep).round() as i64;
    let elapsed = (now.0 - updated_at.0) / 1000;
    let amounts = compute_economy(
        stored,
        elapsed,
        &village.fields,
        &village.buildings,
        upkeep,
        economy_rules,
        speed,
        village.oasis_bonus,
        village.artifact_effects.storage,
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
pub async fn order_research<A, U, S>(
    accounts: &A,
    units: &U,
    starvation: &S,
    economy_rules: &EconomyRules,
    unit_rules: &UnitRules,
    speed: GameSpeed,
    now: Timestamp,
    owner: PlayerId,
    selected: Option<eperica_domain::VillageId>,
    unit: UnitId,
) -> Result<(), ResearchError>
where
    A: AccountRepository,
    U: UnitRepository,
    S: crate::ports::StarvationRepository,
{
    let Some((village, tribe, amounts, settled_from)) = village_and_amounts(
        accounts,
        economy_rules,
        unit_rules,
        speed,
        now,
        owner,
        selected,
    )
    .await?
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
    // The settle changed the store; keep the depletion check exact (005 AC7).
    crate::starvation::sync_starvation_check(
        accounts,
        starvation,
        economy_rules,
        unit_rules,
        speed,
        now,
        village.id,
    )
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
pub async fn order_smithy_upgrade<A, U, S>(
    accounts: &A,
    units: &U,
    starvation: &S,
    economy_rules: &EconomyRules,
    unit_rules: &UnitRules,
    speed: GameSpeed,
    now: Timestamp,
    owner: PlayerId,
    selected: Option<eperica_domain::VillageId>,
    unit: UnitId,
) -> Result<(), UpgradeError>
where
    A: AccountRepository,
    U: UnitRepository,
    S: crate::ports::StarvationRepository,
{
    let Some((village, tribe, amounts, settled_from)) = village_and_amounts(
        accounts,
        economy_rules,
        unit_rules,
        speed,
        now,
        owner,
        selected,
    )
    .await?
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
    // The settle changed the store; keep the depletion check exact (005 AC7).
    crate::starvation::sync_starvation_check(
        accounts,
        starvation,
        economy_rules,
        unit_rules,
        speed,
        now,
        village.id,
    )
    .await?;
    Ok(())
}

/// Why ordering a training batch failed (005 AC2/AC3).
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TrainError {
    /// Not enough resources for `count × cost`.
    #[error("not enough resources")]
    Insufficient,
    /// The unit's training building already runs a batch.
    #[error("a batch is already training at this building")]
    QueueBusy,
    /// The unit is not researched in this village.
    #[error("not researched")]
    NotResearched,
    /// The unit's training building is not built in this village.
    #[error("the training building is missing")]
    BuildingMissing,
    /// The unit trains in a building from a later slice (Residence).
    #[error("that unit cannot be trained yet")]
    BuildingUnavailable,
    /// The batch size is outside the allowed range.
    #[error("batch size out of range")]
    CountOutOfRange,
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

impl From<RepoError> for TrainError {
    fn from(e: RepoError) -> Self {
        match e {
            RepoError::Duplicate => TrainError::QueueBusy,
            RepoError::Conflict => TrainError::Conflict,
            other => TrainError::Backend(other.to_string()),
        }
    }
}

/// Order a batch of `count × unit` in `owner`'s village (005 AC2/AC3, P4).
///
/// Validates tribe membership, research, the training building, and the batch size; debits the
/// full batch cost after settling; enqueues a batch whose `i`-th unit completes at
/// `now + i × perUnitTime` (building- and speed-scaled, AC4). The one-batch-per-building rule is
/// enforced by storage even under races.
///
/// # Errors
/// See [`TrainError`].
#[allow(clippy::too_many_arguments)]
pub async fn order_train<A, U, T, S>(
    accounts: &A,
    units: &U,
    training: &T,
    starvation: &S,
    economy_rules: &EconomyRules,
    unit_rules: &UnitRules,
    speed: GameSpeed,
    now: Timestamp,
    owner: PlayerId,
    selected: Option<eperica_domain::VillageId>,
    unit: UnitId,
    count: u32,
) -> Result<(), TrainError>
where
    A: AccountRepository,
    U: UnitRepository,
    T: TrainingRepository,
    S: crate::ports::StarvationRepository,
{
    let Some((village, tribe, amounts, settled_from)) = village_and_amounts(
        accounts,
        economy_rules,
        unit_rules,
        speed,
        now,
        owner,
        selected,
    )
    .await?
    else {
        return Err(TrainError::NotFound);
    };
    let spec = unit_rules.unit(tribe, &unit).ok_or(TrainError::NotFound)?;
    let researched = units.researched_units(village.id).await?;

    can_train(spec, researched.contains(&unit), count, &village.buildings).map_err(
        |d| match d {
            TrainDenied::NotResearched => TrainError::NotResearched,
            TrainDenied::BuildingMissing => TrainError::BuildingMissing,
            TrainDenied::BuildingUnavailable => TrainError::BuildingUnavailable,
            TrainDenied::CountOutOfRange => TrainError::CountOutOfRange,
        },
    )?;

    let cost = batch_cost(spec, count);
    if !can_afford(amounts, cost) {
        return Err(TrainError::Insufficient);
    }
    let settled = debit(amounts, cost);
    let building_level = village
        .buildings
        .iter()
        .find(|b| b.kind == spec.trained_in)
        .map_or(0, |b| b.level);
    // 020 AC6: a Trainer artifact (carried on the training village's read) speeds training.
    let base_per_unit =
        per_unit_time_secs(spec.train_secs, building_level, &unit_rules.training, speed);
    let per_unit_secs = ((base_per_unit as f64) * village.artifact_effects.training)
        .round()
        .max(1.0) as i64;
    let order = NewTrainingOrder {
        building: spec.trained_in,
        unit,
        count,
        per_unit_secs,
    };
    training
        .start_training(village.id, settled, settled_from, now, order)
        .await?;
    // The settle changed the store; keep the depletion check exact (005 AC7).
    crate::starvation::sync_starvation_check(
        accounts,
        starvation,
        economy_rules,
        unit_rules,
        speed,
        now,
        village.id,
    )
    .await?;
    Ok(())
}

/// Claim batches with due completions and deliver them (the System actor, AC5); returns the
/// villages that received units (their upkeep rose — callers re-sync starvation checks).
///
/// Each delivery settles the village's resources **piecewise**: every completed unit's upkeep
/// starts at that unit's own completion instant, never retroactively (AC6; spec Decision "troops
/// in training do not eat"). A conflicting settle releases the batch for a fresh retry next tick.
///
/// # Errors
/// Propagates [`RepoError`] from the repository.
pub async fn process_due_training<A, T>(
    accounts: &A,
    training: &T,
    economy_rules: &EconomyRules,
    unit_rules: &UnitRules,
    speed: GameSpeed,
    now: Timestamp,
    limit: i64,
) -> Result<Vec<VillageId>, RepoError>
where
    A: AccountRepository,
    T: TrainingRepository,
{
    let due = training.claim_due_training(now, limit).await?;
    let mut villages = Vec::new();
    for batch in due {
        match deliver_batch(
            accounts,
            training,
            economy_rules,
            unit_rules,
            speed,
            now,
            &batch,
        )
        .await
        {
            Ok(true) => villages.push(batch.village),
            Ok(false) => {}
            // Log-and-continue: a failed apply must not strand the rest of the claimed set; the
            // batch itself is recovered by the startup orphan requeue.
            Err(e) => tracing::error!(error = %e, "failed to apply due training"),
        }
    }
    Ok(villages)
}

/// Deliver one claimed batch's due completions; `Ok(true)` if units joined the garrison.
async fn deliver_batch<A, T>(
    accounts: &A,
    training: &T,
    economy_rules: &EconomyRules,
    unit_rules: &UnitRules,
    speed: GameSpeed,
    now: Timestamp,
    batch: &crate::ports::DueTraining,
) -> Result<bool, RepoError>
where
    A: AccountRepository,
    T: TrainingRepository,
{
    let per_unit = batch.per_unit_secs.max(1);
    // `.max(0)` is defensive (the claim predicate implies ≥ per_unit): a corrupted `started_at`
    // must fail safe (deliver nothing) instead of delivering the whole batch early.
    let elapsed_secs = ((now.0 - batch.started_at.0) / 1000).max(0);
    let total_finished = u32::try_from(elapsed_secs / per_unit)
        .unwrap_or(u32::MAX)
        .min(batch.count_total);
    let completed = total_finished.saturating_sub(batch.count_done);
    if completed == 0 {
        // Claimed at the boundary with nothing actually due — put it back unchanged.
        training.release_training(batch).await?;
        return Ok(false);
    }

    let (Some(village), Some((stored, updated_at))) = (
        accounts.village_by_id(batch.village).await?,
        accounts.stored_resources(batch.village).await?,
    ) else {
        // The village is gone (cascade) — nothing to deliver to.
        training.release_training(batch).await?;
        return Ok(false);
    };
    let garrison = accounts.garrison(batch.village).await?; // pre-delivery
    let roster = village
        .tribe
        .map_or(&[][..], |tribe| unit_rules.roster(tribe));
    let unit_upkeep = roster
        .iter()
        .find(|s| s.id == batch.unit)
        .map_or(0, |s| i64::from(s.crop_upkeep));

    // Settle piecewise: segment i runs up to the i-th delivered unit's completion instant with
    // the upkeep of everything delivered before it. A player settle that landed after a
    // completion instant already charged the (slightly lower) old rate for that sliver — the
    // skew is bounded by the scheduler's processing latency, not by elapsed time.
    let mut amounts = stored;
    let mut from = updated_at;
    let mut upkeep = garrison_upkeep(&garrison, roster);
    for i in 1..=i64::from(completed) {
        let t_i = Timestamp(
            batch.started_at.0
                + per_unit
                    .saturating_mul(i64::from(batch.count_done) + i)
                    .saturating_mul(1000),
        );
        let segment_secs = ((t_i.0 - from.0) / 1000).max(0);
        amounts = compute_economy(
            amounts,
            segment_secs,
            &village.fields,
            &village.buildings,
            (upkeep as f64 * village.artifact_effects.upkeep).round() as i64,
            economy_rules,
            speed,
            village.oasis_bonus,
            village.artifact_effects.storage,
        )
        .amounts;
        from = Timestamp(from.0.max(t_i.0));
        upkeep += unit_upkeep;
    }

    match training
        .apply_training(batch, completed, amounts, updated_at, from)
        .await
    {
        Ok(()) => Ok(true),
        Err(RepoError::Conflict) => {
            // Someone settled concurrently — release; the next tick retries from a fresh snapshot.
            training.release_training(batch).await?;
            Ok(false)
        }
        Err(e) => Err(e),
    }
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
            scouting: 0,
            speed: 6,
            carry_capacity: 50,
            crop_upkeep: 1,
            point_value: 1,
            cost: amounts(100),
            train_secs: 1600,
            trained_in: BuildingKind::Barracks,
            siege_kind: None,
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
            outpost_capacity_per_level: vec![0, 1, 2, 3],
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
            oasis_bonus: Default::default(),
            is_capital: false,
            is_natar: false,
            is_wonder_site: false,
            artifact_effects: eperica_domain::ArtifactEffects::NONE,
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
        async fn village_by_id(&self, _v: VillageId) -> Result<Option<Village>, RepoError> {
            Ok(Some(self.village.clone()))
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
        async fn villages_at(
            &self,
            _coords: &[eperica_domain::Coordinate],
        ) -> Result<Vec<crate::ports::VillageMarker>, RepoError> {
            Ok(Vec::new())
        }
        async fn village_at(
            &self,
            _coord: eperica_domain::Coordinate,
        ) -> Result<Option<Village>, RepoError> {
            Ok(None)
        }
    }

    // 020: no artifacts in the training tests (defaults apply); they assert base training times.
    impl crate::ports::ArtifactRepository for FakeAccounts {}

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

    #[derive(Default)]
    struct FakeTraining {
        duplicate: bool,
        due: Vec<crate::ports::DueTraining>,
        last: Mutex<Option<NewTrainingOrder>>,
        last_settled: Mutex<Option<ResourceAmounts>>,
        /// `(completed, settled, settled_from, settle_to)` of the last apply.
        applied: Mutex<Option<(u32, ResourceAmounts, Timestamp, Timestamp)>>,
        released: Mutex<bool>,
    }

    #[async_trait]
    impl TrainingRepository for FakeTraining {
        async fn start_training(
            &self,
            _v: VillageId,
            settled: ResourceAmounts,
            _settled_from: Timestamp,
            _now: Timestamp,
            order: NewTrainingOrder,
        ) -> Result<(), RepoError> {
            if self.duplicate {
                return Err(RepoError::Duplicate);
            }
            *self.last_settled.lock().unwrap() = Some(settled);
            *self.last.lock().unwrap() = Some(order);
            Ok(())
        }
        async fn active_training(
            &self,
            _v: VillageId,
        ) -> Result<Vec<crate::ports::ActiveTraining>, RepoError> {
            Ok(Vec::new())
        }
        async fn claim_due_training(
            &self,
            _now: Timestamp,
            _limit: i64,
        ) -> Result<Vec<crate::ports::DueTraining>, RepoError> {
            Ok(self.due.clone())
        }
        async fn apply_training(
            &self,
            _due: &crate::ports::DueTraining,
            completed: u32,
            settled: ResourceAmounts,
            settled_from: Timestamp,
            settle_to: Timestamp,
        ) -> Result<(), RepoError> {
            *self.applied.lock().unwrap() = Some((completed, settled, settled_from, settle_to));
            Ok(())
        }
        async fn release_training(
            &self,
            _due: &crate::ports::DueTraining,
        ) -> Result<(), RepoError> {
            *self.released.lock().unwrap() = true;
            Ok(())
        }
    }

    struct NoopStarvation;

    #[async_trait]
    impl crate::ports::StarvationRepository for NoopStarvation {
        async fn schedule_starvation_check(
            &self,
            _v: VillageId,
            _due: Timestamp,
        ) -> Result<(), RepoError> {
            Ok(())
        }
        async fn cancel_starvation_check(&self, _v: VillageId) -> Result<(), RepoError> {
            Ok(())
        }
        async fn claim_due_starvation(
            &self,
            _now: Timestamp,
            _limit: i64,
        ) -> Result<Vec<VillageId>, RepoError> {
            Ok(Vec::new())
        }
        async fn apply_starvation(
            &self,
            _v: VillageId,
            _settled: ResourceAmounts,
            _from: Timestamp,
            _now: Timestamp,
            _survivors: &eperica_domain::UnitCounts,
        ) -> Result<(), RepoError> {
            Ok(())
        }
        async fn resolve_starvation_check(
            &self,
            _v: VillageId,
            _reschedule: Option<Timestamp>,
        ) -> Result<(), RepoError> {
            Ok(())
        }
    }

    async fn train(
        accounts: &FakeAccounts,
        units: &FakeUnits,
        training: &FakeTraining,
        unit: &str,
        count: u32,
    ) -> Result<(), TrainError> {
        order_train(
            accounts,
            units,
            training,
            &NoopStarvation,
            &economy_rules(),
            &unit_rules(),
            GameSpeed::new(1.0).unwrap(),
            Timestamp(0),
            PlayerId(1),
            None,
            UnitId(unit.to_owned()),
            count,
        )
        .await
    }

    // --- 005 AC2: training success path ---
    #[tokio::test]
    async fn ordering_a_batch_debits_and_enqueues() {
        let accounts = FakeAccounts {
            village: make_village(&[(BuildingKind::Barracks, 1)]),
            stored: amounts(800),
        };
        let units = FakeUnits::default();
        let training = FakeTraining::default();
        // Tier-1 trains without research (AC9 of 004 carries over).
        train(&accounts, &units, &training, "tier1", 5)
            .await
            .unwrap();
        let order = training.last.lock().unwrap().clone().expect("enqueued");
        assert_eq!(order.building, BuildingKind::Barracks);
        assert_eq!(order.count, 5);
        // train_secs 1600 ÷ (speed 1 × factor(level 1) = 1.0) = 1600 s per unit (AC4).
        assert_eq!(order.per_unit_secs, 1600);
        // The full batch cost (5 × 100) was debited from 800.
        assert_eq!(
            training.last_settled.lock().unwrap().expect("settled"),
            amounts(300)
        );
    }

    // --- 005 AC5/AC6: deliveries settle piecewise — upkeep starts at each completion instant ---
    #[tokio::test]
    async fn delivery_settles_piecewise() {
        // Zero production (test economy rules), tier-1 upkeep 1 crop/h, perUnit = 3600 s, batch
        // of 3 started at t=0, processed at t = 2.5 h ⇒ 2 units deliver. Segment [0, 1 h]: empty
        // garrison, no drain; segment [1 h, 2 h]: one delivered unit eats 1 crop. A retroactive
        // implementation would charge 2 units × 2 h = 4 crop instead of exactly 1.
        let accounts = FakeAccounts {
            village: make_village(&[(BuildingKind::Barracks, 1)]),
            stored: amounts(1000),
        };
        let training = FakeTraining {
            due: vec![crate::ports::DueTraining {
                id: 1,
                village: VillageId(1),
                unit: UnitId("tier1".into()),
                count_total: 3,
                count_done: 0,
                per_unit_secs: 3600,
                started_at: Timestamp(0),
            }],
            ..FakeTraining::default()
        };
        let delivered = process_due_training(
            &accounts,
            &training,
            &economy_rules(),
            &unit_rules(),
            GameSpeed::new(1.0).unwrap(),
            Timestamp(9_000_000), // 2.5 h
            10,
        )
        .await
        .unwrap();
        assert_eq!(delivered, vec![VillageId(1)]);
        let (completed, settled, settled_from, settle_to) =
            training.applied.lock().unwrap().expect("applied");
        assert_eq!(completed, 2);
        assert_eq!(settled_from, Timestamp(0));
        assert_eq!(settle_to, Timestamp(7_200_000)); // t₂, not `now`
        assert_eq!(
            settled,
            ResourceAmounts {
                wood: 1000,
                clay: 1000,
                iron: 1000,
                crop: 999, // exactly one unit-hour of upkeep, never retroactive
            }
        );
        assert!(!*training.released.lock().unwrap());
    }

    // --- 005 AC3: every rejection leaves state untouched ---
    #[tokio::test]
    async fn training_rejections() {
        let barracks = || make_village(&[(BuildingKind::Barracks, 1)]);
        let units = FakeUnits::default();
        let training = FakeTraining::default();

        // Unresearched.
        let accounts = FakeAccounts {
            village: barracks(),
            stored: amounts(800),
        };
        assert_eq!(
            train(&accounts, &units, &training, "spearman", 1).await,
            Err(TrainError::NotResearched)
        );
        assert!(training.last.lock().unwrap().is_none());

        // Building missing.
        let no_barracks = FakeAccounts {
            village: make_village(&[(BuildingKind::Academy, 1)]),
            stored: amounts(800),
        };
        assert_eq!(
            train(&no_barracks, &units, &training, "tier1", 1).await,
            Err(TrainError::BuildingMissing)
        );

        // Count out of range.
        let accounts = FakeAccounts {
            village: barracks(),
            stored: amounts(800),
        };
        assert_eq!(
            train(&accounts, &units, &training, "tier1", 0).await,
            Err(TrainError::CountOutOfRange)
        );

        // Insufficient (5 × 100 > 400).
        let poor = FakeAccounts {
            village: barracks(),
            stored: amounts(400),
        };
        assert_eq!(
            train(&poor, &units, &training, "tier1", 5).await,
            Err(TrainError::Insufficient)
        );

        // Not in this tribe's roster.
        assert_eq!(
            train(&accounts, &units, &training, "legionnaire", 1).await,
            Err(TrainError::NotFound)
        );

        // Queue busy (storage-enforced).
        let busy = FakeTraining {
            duplicate: true,
            ..FakeTraining::default()
        };
        assert_eq!(
            train(&accounts, &units, &busy, "tier1", 1).await,
            Err(TrainError::QueueBusy)
        );
        assert!(training.last.lock().unwrap().is_none());
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
            &NoopStarvation,
            &economy_rules(),
            &unit_rules(),
            GameSpeed::new(speed).unwrap(),
            Timestamp(0),
            PlayerId(1),
            None,
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
            &NoopStarvation,
            &economy_rules(),
            &unit_rules(),
            GameSpeed::new(1.0).unwrap(),
            Timestamp(0),
            PlayerId(1),
            None,
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
