//! Who-is search use-case (028): one query → matching players, alliances, and (if it parses as one) a map
//! coordinate. Read-only over public data (P4) — the hits carry only public identity. Bounded (P11).

use crate::ports::{AccountRepository, AllianceHit, AllianceRepository, PlayerHit, RepoError};
use eperica_domain::{Coordinate, parse_coordinate};

/// Caps on each result kind (P11 — bounded reads).
pub const PLAYER_LIMIT: i64 = 20;
pub const ALLIANCE_LIMIT: i64 = 20;

/// The assembled search results (028).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SearchResults {
    pub players: Vec<PlayerHit>,
    pub alliances: Vec<AllianceHit>,
    /// A map tile to jump to, when the query parsed as a coordinate.
    pub coordinate: Option<Coordinate>,
}

/// Why a search failed (028) — only a backend error; an empty/blank query is not an error (returns empty).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SearchError {
    /// A backend/storage failure.
    #[error("storage error: {0}")]
    Backend(String),
}

impl From<RepoError> for SearchError {
    fn from(e: RepoError) -> Self {
        SearchError::Backend(e.to_string())
    }
}

/// Run a who-is search (028 AC1–AC4). A blank query yields empty results (the caller shows the prompt).
/// Players + alliances are bounded prefix matches; a coordinate query also yields a map jump.
///
/// # Errors
/// [`SearchError::Backend`] on storage failure.
pub async fn search<A, L>(
    accounts: &A,
    alliances: &L,
    query: &str,
) -> Result<SearchResults, SearchError>
where
    A: AccountRepository,
    L: AllianceRepository,
{
    let q = query.trim();
    if q.is_empty() {
        return Ok(SearchResults::default());
    }
    Ok(SearchResults {
        players: accounts.search_players(q, PLAYER_LIMIT).await?,
        alliances: alliances.search_alliances(q, ALLIANCE_LIMIT).await?,
        coordinate: parse_coordinate(q),
    })
}

// Orchestration only: the blank-query short-circuit, prefix matching, and coordinate detection are
// exercised end-to-end by the web integration tests (against the real repository) and the domain
// `parse_coordinate` + repository prefix-search unit tests.
