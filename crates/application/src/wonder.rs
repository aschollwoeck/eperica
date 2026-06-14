//! Wonder use-cases (021): the one-time release of Wonder plans + conquerable sites at the world's
//! Wonder-release date, the gated construction order, and the round-victory check (first alliance to a
//! complete Wonder).

use crate::build::{BuildError, place_build_order};
use crate::ports::{
    AccountRepository, AllianceRepository, BuildRepository, RepoError, StarvationRepository,
    WonderRepository,
};
use eperica_domain::{
    BuildRules, BuildTarget, BuildingKind, EconomyRules, GameSpeed, PlayerId, Timestamp, UnitRules,
    VillageId, wonder_complete,
};

/// The release parameters (mirrors [`eperica_domain::WonderRules`]; passed in so the use-case stays
/// I/O-free).
pub struct WonderReleaseSpec<'a> {
    /// How many capturable plans to materialize.
    pub plan_count: u32,
    /// How many conquerable Wonder sites to materialize.
    pub site_count: u32,
    /// The Natar garrison unit + strength.
    pub garrison_unit: &'a str,
    pub garrison_base_count: i64,
    pub garrison_per_index: i64,
}

/// Release the Wonder plans + sites if the world has reached its Wonder-release date (021 AC1).
/// Idempotent — a no-op before the date or once released. Returns the number materialized.
///
/// # Errors
/// [`RepoError::Backend`] on storage failure.
pub async fn process_due_wonder_release<R>(
    repo: &R,
    release_at: Option<Timestamp>,
    now: Timestamp,
    spec: &WonderReleaseSpec<'_>,
) -> Result<usize, RepoError>
where
    R: WonderRepository,
{
    let Some(release_at) = release_at else {
        return Ok(0); // no release scheduled
    };
    if now.0 < release_at.0 {
        return Ok(0); // not yet due
    }
    repo.release_wonder(
        release_at,
        now,
        spec.plan_count,
        spec.site_count,
        spec.garrison_unit,
        spec.garrison_base_count,
        spec.garrison_per_index,
    )
    .await
}

/// The slot the Wonder of the World occupies in a site village's centre (matches the web layout).
const WONDER_SLOT: u8 = 18;

/// Why ordering a Wonder build failed (021 AC4/AC5).
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum WonderError {
    /// The selected village is not a Wonder construction site.
    #[error("not a Wonder site")]
    NotASite,
    /// The orderer is in no alliance (so no alliance can hold a plan for them).
    #[error("not in an alliance")]
    NoAlliance,
    /// The orderer's alliance holds no Wonder plan.
    #[error("alliance holds no Wonder plan")]
    NoPlan,
    /// The Wonder is already complete (level 100).
    #[error("the Wonder is already complete")]
    AlreadyComplete,
    /// The village was not found / not owned by the orderer.
    #[error("not found")]
    NotFound,
    /// An underlying build-queue error (cost, an in-progress build, a conflicting settle).
    #[error(transparent)]
    Build(#[from] BuildError),
}

impl From<RepoError> for WonderError {
    fn from(e: RepoError) -> Self {
        WonderError::Build(BuildError::from(e))
    }
}

/// Order one level of Wonder construction on a controlled site (021 AC4/AC5). Accepted only when the
/// selected village is a Wonder site the orderer controls, the orderer's alliance holds ≥ 1 plan, and
/// the Wonder is below [`MAX_WONDER_LEVEL`]; the build then settles + enqueues through the ordinary
/// construction queue (003) and resolves via the scheduler.
///
/// # Errors
/// See [`WonderError`].
#[allow(clippy::too_many_arguments)]
pub async fn order_wonder_build<A, B, S, L, W>(
    accounts: &A,
    builds: &B,
    starvation: &S,
    alliances: &L,
    wonders: &W,
    economy_rules: &EconomyRules,
    build_rules: &BuildRules,
    unit_rules: &UnitRules,
    speed: GameSpeed,
    now: Timestamp,
    owner: PlayerId,
    selected: Option<VillageId>,
) -> Result<(), WonderError>
where
    A: AccountRepository,
    B: BuildRepository,
    S: StarvationRepository,
    L: AllianceRepository,
    W: WonderRepository,
{
    let Some(village) = crate::economy::select_village(accounts, owner, selected).await? else {
        return Err(WonderError::NotFound);
    };
    // Site control (AC4): the orderer must own a Wonder site (select_village only returns their own).
    if !village.is_wonder_site {
        return Err(WonderError::NotASite);
    }
    // The orderer's alliance must hold ≥ 1 plan (AC4).
    let Some(membership) = alliances.alliance_of(owner).await? else {
        return Err(WonderError::NoAlliance);
    };
    if !wonders.alliance_holds_plan(membership.alliance).await? {
        return Err(WonderError::NoPlan);
    }
    // Below 100 (AC5) — checked here for a clear error before settling.
    if wonder_complete(wonders.wonder_level(village.id).await?) {
        return Err(WonderError::AlreadyComplete);
    }

    place_build_order(
        accounts,
        builds,
        starvation,
        economy_rules,
        build_rules,
        unit_rules,
        speed,
        now,
        owner,
        Some(village.id),
        BuildTarget::Building {
            slot: WONDER_SLOT,
            kind: BuildingKind::Wonder,
        },
    )
    .await
    .map_err(WonderError::from)
}

/// Record the round's victory if any alliance has a complete Wonder (021 AC6). The standings are
/// highest-first, so the leader is the candidate; victory is recorded at most once (the repo guards
/// idempotency). Returns `true` if this call ended the round.
///
/// # Errors
/// [`RepoError::Backend`] on storage failure.
pub async fn process_due_wonder_victory<R>(repo: &R, now: Timestamp) -> Result<bool, RepoError>
where
    R: WonderRepository,
{
    if repo.world_ended().await?.is_some() {
        return Ok(false); // already won — frozen
    }
    let standings = repo.top_wonders().await?;
    let Some(leader) = standings.first() else {
        return Ok(false);
    };
    if !wonder_complete(leader.level) {
        return Ok(false); // no complete Wonder yet
    }
    repo.record_victory(leader.alliance, now).await
}
