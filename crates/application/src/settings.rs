//! Settings use-cases (029): a player's own preferences. This slice covers per-kind **notification
//! preferences** — which notification kinds are enabled. Owner-scoped (P4): the caller passes the session
//! player as the subject, so a player only ever reads/changes their own.

use crate::ports::{NotificationRepository, RepoError};
use eperica_domain::{NotificationKind, PlayerId};

/// Why a settings action failed (029) — only a backend error.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SettingsError {
    /// A backend/storage failure.
    #[error("storage error: {0}")]
    Backend(String),
}

impl From<RepoError> for SettingsError {
    fn from(e: RepoError) -> Self {
        SettingsError::Backend(e.to_string())
    }
}

/// The player's notification preferences (029 AC1): every kind with whether it is **enabled** (i.e. not
/// muted). Default-on — a kind with no stored mute is enabled.
///
/// # Errors
/// [`SettingsError::Backend`] on storage failure.
pub async fn notification_settings<N>(
    notifs: &N,
    player: PlayerId,
) -> Result<Vec<(NotificationKind, bool)>, SettingsError>
where
    N: NotificationRepository,
{
    let muted = notifs.muted_kinds(player).await?;
    Ok(NotificationKind::ALL
        .into_iter()
        .map(|k| (k, !muted.contains(&k)))
        .collect())
}

/// Enable or disable a notification kind for `player` (029 AC2/AC4) — owner-scoped, idempotent. `enabled`
/// maps to "not muted".
///
/// # Errors
/// [`SettingsError::Backend`] on storage failure.
pub async fn set_notification_pref<N>(
    notifs: &N,
    player: PlayerId,
    kind: NotificationKind,
    enabled: bool,
) -> Result<(), SettingsError>
where
    N: NotificationRepository,
{
    notifs.set_mute(player, kind, !enabled).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::NotificationRepository;
    use async_trait::async_trait;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeNotifs {
        muted: Mutex<Vec<NotificationKind>>,
    }

    #[async_trait]
    impl NotificationRepository for FakeNotifs {
        async fn muted_kinds(&self, _p: PlayerId) -> Result<Vec<NotificationKind>, RepoError> {
            Ok(self.muted.lock().unwrap().clone())
        }
        async fn set_mute(
            &self,
            _p: PlayerId,
            kind: NotificationKind,
            muted: bool,
        ) -> Result<(), RepoError> {
            let mut m = self.muted.lock().unwrap();
            m.retain(|k| *k != kind);
            if muted {
                m.push(kind);
            }
            Ok(())
        }
    }

    #[tokio::test]
    async fn settings_report_enabled_and_toggle_persists() {
        let n = FakeNotifs::default();
        let p = PlayerId(1);

        // All enabled by default.
        let s = notification_settings(&n, p).await.unwrap();
        assert_eq!(s.len(), NotificationKind::ALL.len());
        assert!(s.iter().all(|(_, enabled)| *enabled));

        // Disable NewMessage → reported disabled, others still enabled.
        set_notification_pref(&n, p, NotificationKind::NewMessage, false)
            .await
            .unwrap();
        let s = notification_settings(&n, p).await.unwrap();
        let nm = s
            .iter()
            .find(|(k, _)| *k == NotificationKind::NewMessage)
            .unwrap();
        assert!(!nm.1, "NewMessage is now disabled");
        assert!(
            s.iter()
                .filter(|(k, _)| *k != NotificationKind::NewMessage)
                .all(|(_, e)| *e)
        );

        // Re-enable → all on again.
        set_notification_pref(&n, p, NotificationKind::NewMessage, true)
            .await
            .unwrap();
        assert!(
            notification_settings(&n, p)
                .await
                .unwrap()
                .iter()
                .all(|(_, e)| *e)
        );
    }
}
