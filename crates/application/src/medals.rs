//! The weekly medal settlement (017): a recurring, **state-driven** boundary settlement. Each tick
//! the scheduler calls [`process_due_medal_settlement`], which settles any complete-but-unsettled
//! period — snapshotting population and awarding the period's medals — then naturally advances (the
//! latest settled period is derived from the snapshots). Idempotent per period (P1/P2): re-running a
//! settled period writes no duplicate snapshot or medal.

use crate::ports::{
    BoardScope, ConflictMetric, MedalAward, MedalRepository, MedalSubjectKind, RankingRepository,
    RepoError,
};
use eperica_domain::{
    EconomyRules, MedalCategory, MedalRules, Timestamp, period_index, period_start,
};

/// Settle every complete, unsettled period up to `now`, returning the periods settled (usually 0 or
/// 1; more only when catching up after downtime). The settlement is bound to one world via `repo`.
pub async fn process_due_medal_settlement<R>(
    repo: &R,
    econ: &EconomyRules,
    rules: &MedalRules,
    world_start: Timestamp,
    now: Timestamp,
) -> Result<Vec<i64>, RepoError>
where
    R: RankingRepository + MedalRepository,
{
    let current = period_index(now, world_start, rules.period_secs);
    let mut next = repo
        .latest_settled_period()
        .await?
        .map_or(0, |latest| latest + 1);
    let mut settled = Vec::new();
    // Period P is complete once `now` is in a later period (its boundary has passed).
    while next < current {
        settle_period(repo, econ, rules, world_start, next).await?;
        settled.push(next);
        next += 1;
    }
    Ok(settled)
}

/// Settle one period: gather each non-climber category's top-N from the period-windowed boards, then
/// hand them — plus the climber limit — to the repository's **atomic** `settle_period`, which writes
/// the snapshot, the climber medals (from that snapshot), and these medals in one transaction (AC6).
/// The board reads happen before the transaction; they don't depend on this period's snapshot.
async fn settle_period<R>(
    repo: &R,
    econ: &EconomyRules,
    rules: &MedalRules,
    world_start: Timestamp,
    period: i64,
) -> Result<(), RepoError>
where
    R: RankingRepository + MedalRepository,
{
    let since = period_start(period, world_start, rules.period_secs);
    let until = period_start(period + 1, world_start, rules.period_secs);
    let n = rules.per_category as i64;
    let mut awards = Vec::new();
    let mut climber_limit = None;

    for &category in &rules.categories {
        match category {
            MedalCategory::Attacker | MedalCategory::Defender | MedalCategory::Raider => {
                let metric = match category {
                    MedalCategory::Attacker => ConflictMetric::Attack,
                    MedalCategory::Defender => ConflictMetric::Defense,
                    _ => ConflictMetric::Raided,
                };
                let rows = repo
                    .conflict_board(metric, BoardScope::World, Some(since), Some(until), n)
                    .await?;
                for (i, row) in rows.into_iter().enumerate() {
                    awards.push(MedalAward {
                        category,
                        rank: i + 1,
                        subject_kind: MedalSubjectKind::Player,
                        subject_id: row.player.0,
                    });
                }
            }
            // The climber medals are computed inside the atomic settle (from the snapshot it writes).
            // No prior snapshot in period 0 ⇒ no climber medal (AC4).
            MedalCategory::Climber => {
                if period > 0 {
                    climber_limit = Some(n);
                }
            }
            MedalCategory::AlliancePopulation => {
                let rows = repo
                    .alliance_population_board(econ, BoardScope::World, n)
                    .await?;
                push_alliance(&mut awards, category, rows);
            }
            MedalCategory::AllianceAttacker | MedalCategory::AllianceDefender => {
                let metric = if category == MedalCategory::AllianceAttacker {
                    ConflictMetric::Attack
                } else {
                    ConflictMetric::Defense
                };
                let rows = repo
                    .alliance_conflict_board(metric, BoardScope::World, Some(since), Some(until), n)
                    .await?;
                push_alliance(&mut awards, category, rows);
            }
        }
    }

    repo.settle_period(econ, period, climber_limit, &awards)
        .await
}

fn push_alliance(
    awards: &mut Vec<MedalAward>,
    category: MedalCategory,
    rows: Vec<crate::ports::AllianceLeaderboardRow>,
) {
    for (i, row) in rows.into_iter().enumerate() {
        awards.push(MedalAward {
            category,
            rank: i + 1,
            subject_kind: MedalSubjectKind::Alliance,
            subject_id: row.alliance.0,
        });
    }
}
