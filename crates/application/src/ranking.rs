//! Ranking & statistics use-cases (016): public leaderboards and stat pages over a
//! [`RankingRepository`]. Pure orchestration — validate the requested scope/window/page against the
//! [`RankingRules`] (P4/P7), then read. No writes; population is derived on read, points/loot are the
//! persisted battle facts summed by the repository.

use crate::ports::{
    AllianceLeaderboardRow, AllianceStats, BoardScope, ConflictMetric, DefenderReport,
    LeaderboardRow, PlayerStats, RankingRepository, RepoError,
};
use eperica_domain::{AllianceId, EconomyRules, PlayerId, RankingRules, Timestamp};

/// A requested leaderboard time window (016 AC5/AC6): all-time, or one of the configured rolling
/// windows (in seconds). A `Last` window not present in [`RankingRules::windows_secs`] is rejected
/// (the client never picks an arbitrary window — P4/P7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Window {
    /// The whole history.
    AllTime,
    /// A rolling window of this many seconds.
    Last(i64),
}

/// Why a ranking read was rejected.
#[derive(Debug, thiserror::Error)]
pub enum RankingError {
    /// The requested rolling window is not one the world offers (P7).
    #[error("unknown leaderboard window")]
    UnknownWindow,
    /// A storage failure.
    #[error(transparent)]
    Repo(#[from] RepoError),
}

/// The lower bound for a window, or `None` for all-time. Validates the window against config (P7).
fn window_since(
    rules: &RankingRules,
    now: Timestamp,
    window: Window,
) -> Result<Option<Timestamp>, RankingError> {
    match window {
        Window::AllTime => Ok(None),
        Window::Last(secs) if rules.windows_secs.contains(&secs) => {
            Ok(Some(Timestamp(now.0 - secs * 1000)))
        }
        Window::Last(_) => Err(RankingError::UnknownWindow),
    }
}

/// Players ranked by total population (016 AC2), bounded by the configured page size.
pub async fn population_leaderboard(
    repo: &impl RankingRepository,
    econ: &EconomyRules,
    rules: &RankingRules,
    scope: BoardScope,
) -> Result<Vec<LeaderboardRow>, RankingError> {
    Ok(repo
        .population_board(econ, scope, rules.page_size as i64)
        .await?)
}

/// Players ranked by a conflict metric over a window (016 AC5/AC6).
pub async fn conflict_leaderboard(
    repo: &impl RankingRepository,
    rules: &RankingRules,
    metric: ConflictMetric,
    scope: BoardScope,
    window: Window,
    now: Timestamp,
) -> Result<Vec<LeaderboardRow>, RankingError> {
    let since = window_since(rules, now, window)?;
    Ok(repo
        .conflict_board(metric, scope, since, None, rules.page_size as i64)
        .await?)
}

/// Alliances ranked by aggregate member population (016 AC8).
pub async fn alliance_population_leaderboard(
    repo: &impl RankingRepository,
    econ: &EconomyRules,
    rules: &RankingRules,
    scope: BoardScope,
) -> Result<Vec<AllianceLeaderboardRow>, RankingError> {
    Ok(repo
        .alliance_population_board(econ, scope, rules.page_size as i64)
        .await?)
}

/// Alliances ranked by aggregate member attack/defense points over a window (016 AC8).
pub async fn alliance_conflict_leaderboard(
    repo: &impl RankingRepository,
    rules: &RankingRules,
    metric: ConflictMetric,
    scope: BoardScope,
    window: Window,
    now: Timestamp,
) -> Result<Vec<AllianceLeaderboardRow>, RankingError> {
    let since = window_since(rules, now, window)?;
    Ok(repo
        .alliance_conflict_board(metric, scope, since, None, rules.page_size as i64)
        .await?)
}

/// A player's public statistics page (016 AC9), or `None` if the player does not exist.
pub async fn player_statistics(
    repo: &impl RankingRepository,
    econ: &EconomyRules,
    player: PlayerId,
) -> Result<Option<PlayerStats>, RankingError> {
    Ok(repo.player_stats(econ, player).await?)
}

/// An alliance's public statistics page (016 AC10), or `None` if the alliance does not exist.
pub async fn alliance_statistics(
    repo: &impl RankingRepository,
    econ: &EconomyRules,
    alliance: AllianceId,
) -> Result<Option<AllianceStats>, RankingError> {
    Ok(repo.alliance_stats(econ, alliance).await?)
}

/// A player's reinforcer/own battle reports (016 AC3/AC12) — the defender inbox — newest first,
/// bounded by the page size.
pub async fn reinforcement_reports(
    repo: &impl RankingRepository,
    rules: &RankingRules,
    player: PlayerId,
) -> Result<Vec<DefenderReport>, RankingError> {
    Ok(repo
        .defender_reports_for(player, rules.page_size as i64)
        .await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn rules() -> RankingRules {
        RankingRules {
            point_value: HashMap::new(),
            windows_secs: vec![7 * 86_400, 30 * 86_400],
            page_size: 100,
        }
    }

    #[test]
    fn window_since_validates_against_config() {
        let r = rules();
        let now = Timestamp(1_000_000_000);
        // All-time has no lower bound.
        assert_eq!(window_since(&r, now, Window::AllTime).unwrap(), None);
        // A configured window subtracts its seconds (in ms).
        assert_eq!(
            window_since(&r, now, Window::Last(7 * 86_400)).unwrap(),
            Some(Timestamp(now.0 - 7 * 86_400 * 1000))
        );
        // An unconfigured window is rejected (P4/P7 — the client cannot pick an arbitrary window).
        assert!(matches!(
            window_since(&r, now, Window::Last(12_345)),
            Err(RankingError::UnknownWindow)
        ));
    }
}
