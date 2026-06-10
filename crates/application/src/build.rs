//! Construction use-cases: order an upgrade, and process due builds.

use crate::ports::{AccountRepository, BuildRepository, NewBuildOrder, RepoError};
use eperica_domain::{
    BuildRules, BuildTarget, BuildingKind, EconomyRules, GameSpeed, PlayerId, Timestamp, Village,
    build_time_secs, can_afford, compute_economy, debit, prerequisites_met,
};

/// Why ordering a build failed.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BuildError {
    /// Not enough resources for the cost.
    #[error("not enough resources")]
    Insufficient,
    /// The village already has an active build order.
    #[error("a build is already in progress")]
    AlreadyBuilding,
    /// The target is already at max level.
    #[error("already at maximum level")]
    MaxLevel,
    /// A building prerequisite is unmet.
    #[error("prerequisites not met")]
    PrereqUnmet,
    /// The village or target does not exist.
    #[error("not found")]
    NotFound,
    /// A storage/backend failure.
    #[error("storage error: {0}")]
    Backend(String),
}

impl From<RepoError> for BuildError {
    fn from(e: RepoError) -> Self {
        match e {
            RepoError::Duplicate => BuildError::AlreadyBuilding,
            other => BuildError::Backend(other.to_string()),
        }
    }
}

/// Current level of `target` in `village` (buildings identified by kind; absent ⇒ 0).
fn current_level(village: &Village, target: BuildTarget) -> Option<u8> {
    match target {
        BuildTarget::Field { slot } => village.fields.get(slot as usize).map(|f| f.level),
        BuildTarget::Building { kind, .. } => Some(building_level(village, kind)),
    }
}

fn building_level(village: &Village, kind: BuildingKind) -> u8 {
    village
        .buildings
        .iter()
        .find(|b| b.kind == kind)
        .map_or(0, |b| b.level)
}

/// Order an upgrade of `target` for `owner`'s village (AC1–AC4, AC6, AC7).
///
/// Validates max level, prerequisites, and affordability, then settles resources, debits the cost,
/// and enqueues a build order completing after the (speed- and Main-Building-scaled) build time.
///
/// # Errors
/// See [`BuildError`].
#[allow(clippy::too_many_arguments)]
pub async fn order_build<A, B>(
    accounts: &A,
    builds: &B,
    economy_rules: &EconomyRules,
    build_rules: &BuildRules,
    speed: GameSpeed,
    now: Timestamp,
    owner: PlayerId,
    target: BuildTarget,
) -> Result<(), BuildError>
where
    A: AccountRepository,
    B: BuildRepository,
{
    let Some(village) = accounts.villages_of(owner).await?.into_iter().next() else {
        return Err(BuildError::NotFound);
    };
    let current = current_level(&village, target).ok_or(BuildError::NotFound)?;

    if current >= build_rules.max_level(target) {
        return Err(BuildError::MaxLevel);
    }
    if let BuildTarget::Building { kind, .. } = target
        && !prerequisites_met(kind, &village.buildings, build_rules)
    {
        return Err(BuildError::PrereqUnmet);
    }

    let cost = build_rules
        .cost(target, current)
        .ok_or(BuildError::MaxLevel)?;
    let base_time = build_rules
        .base_time_secs(target, current)
        .ok_or(BuildError::MaxLevel)?;

    let Some((stored, updated_at)) = accounts.stored_resources(village.id).await? else {
        return Err(BuildError::NotFound);
    };
    let elapsed = (now.0 - updated_at.0) / 1000;
    let amounts = compute_economy(
        stored,
        elapsed,
        &village.fields,
        &village.buildings,
        economy_rules,
        speed,
    )
    .amounts;
    if !can_afford(amounts, cost) {
        return Err(BuildError::Insufficient);
    }

    let settled = debit(amounts, cost);
    let duration = build_time_secs(
        base_time,
        building_level(&village, BuildingKind::MainBuilding),
        build_rules,
        speed,
    );
    let order = NewBuildOrder {
        target,
        target_level: current + 1,
        complete_at: Timestamp(now.0 + duration * 1000),
    };
    builds.start_build(village.id, settled, now, order).await?;
    Ok(())
}

/// Claim and apply all builds due at `now` (up to `limit`); returns how many were applied (AC5).
///
/// # Errors
/// Propagates [`RepoError`] from the repository.
pub async fn process_due_builds<B>(
    builds: &B,
    now: Timestamp,
    limit: i64,
) -> Result<usize, RepoError>
where
    B: BuildRepository,
{
    let due = builds.claim_due_builds(now, limit).await?;
    let count = due.len();
    for order in due {
        builds.apply_build(order).await?;
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{ActiveBuild, DueBuild, NewUser, UserRecord};
    use async_trait::async_trait;
    use eperica_domain::{
        BuildingSlot, Coordinate, LevelSpec, ResourceAmounts, ResourceField, ResourceKind,
        StartingVillage, VillageId,
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

    fn make_village(field_level: u8, with_main_building: bool) -> Village {
        let fields = (0..18)
            .map(|_| ResourceField {
                kind: ResourceKind::Wood,
                level: field_level,
            })
            .collect();
        let buildings = if with_main_building {
            vec![BuildingSlot {
                kind: BuildingKind::MainBuilding,
                level: 1,
            }]
        } else {
            Vec::new()
        };
        Village {
            id: VillageId(1),
            owner: PlayerId(1),
            coordinate: Coordinate::new(0, 0),
            tribe: None,
            fields,
            buildings,
        }
    }

    fn economy_rules() -> EconomyRules {
        EconomyRules {
            wood_per_level: vec![0],
            clay_per_level: vec![0],
            iron_per_level: vec![0],
            crop_per_level: vec![0],
            field_population_per_level: vec![0],
            main_building_population_per_level: vec![0],
            rally_point_population_per_level: vec![0],
            warehouse_population_per_level: vec![0],
            granary_population_per_level: vec![0],
            warehouse_capacity_per_level: vec![1_000_000],
            granary_capacity_per_level: vec![1_000_000],
            starting_amounts: amounts(0),
        }
    }

    fn build_rules() -> BuildRules {
        let mut buildings = HashMap::new();
        buildings.insert(
            BuildingKind::Warehouse,
            LevelSpec {
                cost_per_level: vec![amounts(50)],
                time_secs_per_level: vec![800],
            },
        );
        let mut prerequisites = HashMap::new();
        prerequisites.insert(
            BuildingKind::Warehouse,
            vec![(BuildingKind::MainBuilding, 1)],
        );
        BuildRules {
            field: LevelSpec {
                cost_per_level: vec![amounts(40)],
                time_secs_per_level: vec![600],
            },
            buildings,
            prerequisites,
            main_building_factor_per_level: vec![1.0, 1.0],
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
    }

    #[derive(Default)]
    struct FakeBuilds {
        duplicate: bool,
        last: Mutex<Option<NewBuildOrder>>,
    }

    #[async_trait]
    impl BuildRepository for FakeBuilds {
        async fn start_build(
            &self,
            _v: VillageId,
            _settled: ResourceAmounts,
            _now: Timestamp,
            order: NewBuildOrder,
        ) -> Result<(), RepoError> {
            if self.duplicate {
                return Err(RepoError::Duplicate);
            }
            *self.last.lock().unwrap() = Some(order);
            Ok(())
        }
        async fn active_build(&self, _v: VillageId) -> Result<Option<ActiveBuild>, RepoError> {
            Ok(None)
        }
        async fn claim_due_builds(
            &self,
            _now: Timestamp,
            _limit: i64,
        ) -> Result<Vec<DueBuild>, RepoError> {
            Ok(Vec::new())
        }
        async fn apply_build(&self, _due: DueBuild) -> Result<(), RepoError> {
            Ok(())
        }
    }

    async fn order(
        accounts: &FakeAccounts,
        builds: &FakeBuilds,
        target: BuildTarget,
    ) -> Result<(), BuildError> {
        order_build(
            accounts,
            builds,
            &economy_rules(),
            &build_rules(),
            GameSpeed::new(1.0).unwrap(),
            Timestamp(0),
            PlayerId(1),
            target,
        )
        .await
    }

    #[tokio::test]
    async fn ordering_a_field_upgrade_enqueues_it() {
        let accounts = FakeAccounts {
            village: make_village(0, true),
            stored: amounts(100),
        };
        let builds = FakeBuilds::default();
        order(&accounts, &builds, BuildTarget::Field { slot: 0 })
            .await
            .unwrap();
        let enqueued = builds.last.lock().unwrap().expect("an order was enqueued");
        assert_eq!(enqueued.target_level, 1);
    }

    #[tokio::test]
    async fn insufficient_resources_rejected() {
        // AC2: cost 40, only 10 stored.
        let accounts = FakeAccounts {
            village: make_village(0, true),
            stored: amounts(10),
        };
        let err = order(
            &accounts,
            &FakeBuilds::default(),
            BuildTarget::Field { slot: 0 },
        )
        .await
        .unwrap_err();
        assert_eq!(err, BuildError::Insufficient);
    }

    #[tokio::test]
    async fn second_order_rejected_when_building() {
        let accounts = FakeAccounts {
            village: make_village(0, true),
            stored: amounts(100),
        };
        let builds = FakeBuilds {
            duplicate: true,
            ..FakeBuilds::default()
        };
        let err = order(&accounts, &builds, BuildTarget::Field { slot: 0 })
            .await
            .unwrap_err();
        assert_eq!(err, BuildError::AlreadyBuilding);
    }

    #[tokio::test]
    async fn prerequisites_enforced() {
        // AC4: Warehouse needs a Main Building, which this village lacks.
        let accounts = FakeAccounts {
            village: make_village(0, false),
            stored: amounts(1000),
        };
        let err = order(
            &accounts,
            &FakeBuilds::default(),
            BuildTarget::Building {
                slot: 5,
                kind: BuildingKind::Warehouse,
            },
        )
        .await
        .unwrap_err();
        assert_eq!(err, BuildError::PrereqUnmet);
    }

    #[tokio::test]
    async fn max_level_rejected() {
        // Field max level is 1 here; a level-1 field cannot be upgraded.
        let accounts = FakeAccounts {
            village: make_village(1, true),
            stored: amounts(1000),
        };
        let err = order(
            &accounts,
            &FakeBuilds::default(),
            BuildTarget::Field { slot: 0 },
        )
        .await
        .unwrap_err();
        assert_eq!(err, BuildError::MaxLevel);
    }
}
