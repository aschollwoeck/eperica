//! The spectator read-aggregation use-cases (125 T2): a bounded, capped snapshot of a world's
//! ongoing activity assembled on read from existing due-stamped state (P1) — never a new event
//! store, never a whole-world scan (P11). Village detail reuses the **exact** owner-view read-model
//! ([`crate::economy::load_economy`]) with the village's true owner substituted for the caller, so
//! an omniscient read can never drift from what the owner's own page shows (AC3; plan.md "Owner-view
//! reuse for village detail").

use crate::economy::{VillageEconomy, load_economy};
use crate::ports::{
    AccountRepository, ActiveBuild, ActiveTraining, BuildRepository, ConquestRepository,
    MovementRepository, RepoError, SpectateReadRepository, SpectatorPlayerRow, StationedGroup,
    TrainingRepository, UnitRepository, WorldBuildOrder, WorldMovement, WorldReportRow,
    WorldShipment, WorldTrainingOrder,
};
use eperica_domain::{EconomyRules, GameSpeed, Timestamp, UnitId, UnitRules, VillageId};

/// The maximum rows any one feed category returns (125 AC5 — "N ≤ 50").
pub const FEED_CAP: i64 = 50;

/// Players per page of the spectator index (125 AC5).
pub const PLAYERS_PER_PAGE: i64 = 50;

/// The capped activity snapshot for a world (125 AC4/AC5): movements, shipments, active build
/// orders, active training batches, and the world's most recent reports — each a fixed, capped,
/// world-scoped read (AC5 — no whole-world scan per refresh).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldFeed {
    /// In-flight troop movements (attacks, raids, reinforcements, returns, scouts, settlers, oasis
    /// movements — both directions), soonest-arrival first.
    pub movements: Vec<WorldMovement>,
    /// In-flight merchant shipments (both legs), soonest-arrival first.
    pub shipments: Vec<WorldShipment>,
    /// Active build/upgrade orders, soonest-completing first.
    pub builds: Vec<WorldBuildOrder>,
    /// Active training batches, soonest-next-unit first.
    pub trainings: Vec<WorldTrainingOrder>,
    /// The world's most recent battle/scout reports, newest first.
    pub reports: Vec<WorldReportRow>,
}

/// Assemble the world feed: five capped, world-scoped reads (AC4/AC5). Each category is capped
/// independently at [`FEED_CAP`], so the endpoint cost is a fixed set of indexed queries regardless
/// of how much is happening in the world (P11).
///
/// # Errors
/// Propagates [`RepoError`] from any of the underlying reads.
pub async fn world_feed<R: SpectateReadRepository>(repo: &R) -> Result<WorldFeed, RepoError> {
    Ok(WorldFeed {
        movements: repo.movements_in_world(FEED_CAP).await?,
        shipments: repo.shipments_in_world(FEED_CAP).await?,
        builds: repo.active_build_orders_in_world(FEED_CAP).await?,
        trainings: repo.active_training_in_world(FEED_CAP).await?,
        reports: repo.recent_reports_in_world(FEED_CAP).await?,
    })
}

/// The paged, population-descending player index for a world (125 AC5). `page` is 1-based; a page
/// below 1 is clamped up (P4 — the client cannot request a negative offset). `econ` feeds the same
/// population formula as the 016 boards.
///
/// # Errors
/// Propagates [`RepoError`] from the underlying read.
pub async fn players<R: SpectateReadRepository>(
    repo: &R,
    econ: &EconomyRules,
    page: i64,
) -> Result<Vec<SpectatorPlayerRow>, RepoError> {
    repo.spectate_player_index(econ, page.max(1), PLAYERS_PER_PAGE)
        .await
}

/// Full omniscient detail for one village (125 AC3): the owner-view economy (resources computed on
/// read, fields/buildings, garrison) plus the build queue, training batches, stationed
/// reinforcements, loyalty, and research — every value equal to what the owner's own `/village` page
/// shows, because [`load_economy`] is called with the village's **true owner**, not the spectator.
#[derive(Debug, Clone)]
pub struct SpectatorVillageDetail {
    /// Village + garrison + computed economy — identical to the owner's `/village` read.
    pub economy: VillageEconomy,
    /// The active build queue (003/004).
    pub builds: Vec<ActiveBuild>,
    /// Active training batches across the troop buildings (005).
    pub trainings: Vec<ActiveTraining>,
    /// Reinforcement groups stationed here by other players (007).
    pub reinforcements: Vec<StationedGroup>,
    /// The stored loyalty accumulator `(value, last-settled instant)` (014, lazily regenerated on
    /// read); `None` if the village does not track loyalty.
    pub loyalty: Option<(i64, Timestamp)>,
    /// Unit types researched at this village (004 Academy).
    pub researched: Vec<UnitId>,
}

/// Load the omniscient village detail (125 AC3). Bypasses the ownership check every other village
/// read applies — the caller has already been authorized by the Spectator role (server-side, T4).
/// Returns `None` if the village does not exist.
///
/// # Errors
/// Propagates [`RepoError`] from the underlying reads.
pub async fn village_detail<R>(
    repo: &R,
    rules: &EconomyRules,
    unit_rules: &UnitRules,
    speed: GameSpeed,
    now: Timestamp,
    village: VillageId,
) -> Result<Option<SpectatorVillageDetail>, RepoError>
where
    R: AccountRepository
        + BuildRepository
        + TrainingRepository
        + MovementRepository
        + ConquestRepository
        + UnitRepository,
{
    let Some(target) = repo.village_by_id(village).await? else {
        return Ok(None);
    };
    let Some(economy) = load_economy(
        repo,
        rules,
        unit_rules,
        speed,
        now,
        target.owner,
        Some(village),
    )
    .await?
    else {
        return Ok(None);
    };
    let builds = repo.active_builds(village).await?;
    let trainings = repo.active_training(village).await?;
    let reinforcements = repo.reinforcements_at(village).await?;
    let loyalty = repo.village_loyalty(village).await?;
    let researched = repo.researched_units(village).await?;
    Ok(Some(SpectatorVillageDetail {
        economy,
        builds,
        trainings,
        reinforcements,
        loyalty,
        researched,
    }))
}
