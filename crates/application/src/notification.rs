//! Notification use-cases (026): reading a player's feed + unread count, marking them read, and the
//! generation helpers called at the event-commit sites. Server-authoritative (P4): every read/clear is
//! keyed by the session player, so a player only ever touches their own notifications.

use crate::ports::{NewNotification, NotificationRepository, NotificationView, RepoError};
use eperica_domain::{Coordinate, NotificationKind, PlayerId, Timestamp, VillageId};
use uuid::Uuid;

/// Page size for the notifications feed (P11 — a bounded read).
pub const FEED_LIMIT: i64 = 50;

/// Why a notification action failed (026).
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum NotificationError {
    /// A storage/backend failure.
    #[error("storage error: {0}")]
    Backend(String),
}

impl From<RepoError> for NotificationError {
    fn from(e: RepoError) -> Self {
        NotificationError::Backend(e.to_string())
    }
}

/// A player's notification feed, most-recent first, bounded by [`FEED_LIMIT`] (026 AC4).
///
/// # Errors
/// [`NotificationError::Backend`] on storage failure.
pub async fn list_notifications<N>(
    notifs: &N,
    player: PlayerId,
) -> Result<Vec<NotificationView>, NotificationError>
where
    N: NotificationRepository,
{
    Ok(notifs.list(player, FEED_LIMIT).await?)
}

/// A player's unread notification count — the nav bell (026 AC4).
///
/// # Errors
/// [`NotificationError::Backend`] on storage failure.
pub async fn notification_unread<N>(notifs: &N, player: PlayerId) -> Result<i64, NotificationError>
where
    N: NotificationRepository,
{
    Ok(notifs.unread_count(player).await?)
}

/// Mark all of `player`'s unread notifications read (026 AC5). Owner-scoped — the caller passes the
/// session player.
///
/// # Errors
/// [`NotificationError::Backend`] on storage failure.
pub async fn mark_notifications_read<N>(
    notifs: &N,
    player: PlayerId,
    now: Timestamp,
) -> Result<(), NotificationError>
where
    N: NotificationRepository,
{
    notifs.mark_read(player, now).await?;
    Ok(())
}

/// Record an **incoming-attack** notification for the defending owner (026 AC1) — unless they are the
/// attacker (a player moving troops between their own villages raises no alarm). Best-effort: a failure is
/// surfaced to the caller, which logs and continues (the attack itself must not fail).
///
/// # Errors
/// [`NotificationError::Backend`] on storage failure.
pub async fn notify_incoming_attack<N>(
    notifs: &N,
    attacker: PlayerId,
    defender: PlayerId,
    target: VillageId,
    target_coord: Coordinate,
    arrive: Timestamp,
    now: Timestamp,
) -> Result<(), NotificationError>
where
    N: NotificationRepository,
{
    if defender == attacker {
        return Ok(());
    }
    let note = NewNotification {
        player: defender,
        kind: NotificationKind::IncomingAttack,
        ref_kind: Some("village".to_owned()),
        ref_id: Some(format!("{}|{}", target_coord.x, target_coord.y)),
        body: format!(
            "Troops are inbound to ({}|{}) — arriving in ~{}s",
            target_coord.x,
            target_coord.y,
            ((arrive.0 - now.0).max(0)) / 1000
        ),
    };
    let _ = target; // the coord carries the human reference; the id is retained for future deep-links.
    notifs.record(&[note], now).await?;
    Ok(())
}

/// Record a **new-message** notification for the recipient (026 AC3) — unless it's a self-DM (already
/// rejected upstream, guarded here too).
///
/// # Errors
/// [`NotificationError::Backend`] on storage failure.
pub async fn notify_new_message<N>(
    notifs: &N,
    sender: PlayerId,
    recipient: PlayerId,
    now: Timestamp,
) -> Result<(), NotificationError>
where
    N: NotificationRepository,
{
    if recipient == sender {
        return Ok(());
    }
    let note = NewNotification {
        player: recipient,
        kind: NotificationKind::NewMessage,
        ref_kind: Some("dm".to_owned()),
        // The conversation deep-link is viewer-relative `dm:<other>`; for the recipient the other party
        // is the sender.
        ref_id: Some(Uuid::from_u128(sender.0).to_string()),
        body: String::new(),
    };
    notifs.record(&[note], now).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Mutex;

    /// An in-memory notification sink (records what `record` is asked to persist).
    #[derive(Default)]
    struct FakeNotifs {
        recorded: Mutex<Vec<NewNotification>>,
    }

    #[async_trait]
    impl NotificationRepository for FakeNotifs {
        async fn record(
            &self,
            notes: &[NewNotification],
            _now: Timestamp,
        ) -> Result<(), RepoError> {
            self.recorded.lock().unwrap().extend_from_slice(notes);
            Ok(())
        }
    }

    fn pid(n: u128) -> PlayerId {
        PlayerId(n)
    }

    #[tokio::test]
    async fn incoming_attack_skips_self_and_records_others() {
        let notifs = FakeNotifs::default();
        let coord = Coordinate::new(3, 4);
        // Attacking your own village → no notification.
        notify_incoming_attack(
            &notifs,
            pid(1),
            pid(1),
            VillageId(9),
            coord,
            Timestamp(5000),
            Timestamp(0),
        )
        .await
        .unwrap();
        assert!(notifs.recorded.lock().unwrap().is_empty());

        // Attacking another player → one IncomingAttack for the defender.
        notify_incoming_attack(
            &notifs,
            pid(1),
            pid(2),
            VillageId(9),
            coord,
            Timestamp(5000),
            Timestamp(0),
        )
        .await
        .unwrap();
        let rec = notifs.recorded.lock().unwrap();
        assert_eq!(rec.len(), 1);
        assert_eq!(rec[0].player, pid(2));
        assert_eq!(rec[0].kind, NotificationKind::IncomingAttack);
        assert_eq!(rec[0].ref_kind.as_deref(), Some("village"));
        assert_eq!(rec[0].ref_id.as_deref(), Some("3|4"));
    }

    #[tokio::test]
    async fn new_message_skips_self_and_records_recipient() {
        let notifs = FakeNotifs::default();
        notify_new_message(&notifs, pid(1), pid(1), Timestamp(0))
            .await
            .unwrap();
        assert!(notifs.recorded.lock().unwrap().is_empty());

        notify_new_message(&notifs, pid(1), pid(2), Timestamp(0))
            .await
            .unwrap();
        let rec = notifs.recorded.lock().unwrap();
        assert_eq!(rec.len(), 1);
        assert_eq!(rec[0].player, pid(2));
        assert_eq!(rec[0].kind, NotificationKind::NewMessage);
        assert_eq!(rec[0].ref_kind.as_deref(), Some("dm"));
    }

    #[tokio::test]
    async fn read_use_cases_tolerate_default_noop_repo() {
        // A repo that uses every default no-op method.
        struct NoopNotifs;
        #[async_trait]
        impl NotificationRepository for NoopNotifs {}

        let n = NoopNotifs;
        assert!(list_notifications(&n, pid(1)).await.unwrap().is_empty());
        assert_eq!(notification_unread(&n, pid(1)).await.unwrap(), 0);
        mark_notifications_read(&n, pid(1), Timestamp(0))
            .await
            .unwrap();
    }
}
