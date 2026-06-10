//! The scheduler use-case: process due events exactly once (P1).

use crate::ports::{EventStore, RepoError};
use eperica_domain::{EventKind, Timestamp};

/// Claim and process all events due at `now` (up to `limit`), returning how many were processed.
///
/// Claiming transitions each event `pending` → `processing` atomically, so an event is processed
/// exactly once even across workers (AC6). Processing dispatches on the event kind; for slice 001 the
/// only kind is a no-op heartbeat.
///
/// # Errors
/// Propagates [`RepoError`] from the store.
pub async fn process_due<S>(store: &S, now: Timestamp, limit: i64) -> Result<usize, RepoError>
where
    S: EventStore,
{
    let due = store.claim_due(now, limit).await?;
    let count = due.len();
    for event in due {
        match event.kind {
            EventKind::Heartbeat => { /* no-op: exists to prove the engine */ }
        }
        store.mark_done(event.id).await?;
    }
    Ok(count)
}
