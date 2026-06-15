//! Notification kinds (026). Pure (P3) — no I/O.
//!
//! A notification is a persisted, per-player record that an event already happened (an incoming attack, a
//! resolved battle report, a new message). The domain owns only the **kind vocabulary** — its stable
//! string codec for the DB and a human label — so "what a kind means" lives in the pure crate; *who* gets
//! *which* notification is decided at the event-commit sites (application/infrastructure).

/// The kind of a notification (026, this slice). Stable `as_str`/`parse` codec for persistence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationKind {
    /// An attack or raid is inbound to one of the recipient's villages.
    IncomingAttack,
    /// A battle the recipient was part of has resolved; its report is ready.
    BattleReport,
    /// The recipient received a new direct message.
    NewMessage,
}

impl NotificationKind {
    /// Every kind, for iteration (e.g. the settings page lists each one).
    pub const ALL: [NotificationKind; 3] = [
        NotificationKind::IncomingAttack,
        NotificationKind::BattleReport,
        NotificationKind::NewMessage,
    ];

    /// The stable storage token (the DB `kind` column).
    pub fn as_str(self) -> &'static str {
        match self {
            NotificationKind::IncomingAttack => "incoming_attack",
            NotificationKind::BattleReport => "battle_report",
            NotificationKind::NewMessage => "new_message",
        }
    }

    /// Parse a stored token back to a kind (`None` if unrecognised — e.g. a legacy/foreign value).
    pub fn parse(s: &str) -> Option<NotificationKind> {
        match s {
            "incoming_attack" => Some(NotificationKind::IncomingAttack),
            "battle_report" => Some(NotificationKind::BattleReport),
            "new_message" => Some(NotificationKind::NewMessage),
            _ => None,
        }
    }

    /// A short, human label for the feed.
    pub fn label(self) -> &'static str {
        match self {
            NotificationKind::IncomingAttack => "Incoming attack",
            NotificationKind::BattleReport => "Battle report",
            NotificationKind::NewMessage => "New message",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codec_round_trips_every_variant() {
        for k in NotificationKind::ALL {
            assert_eq!(NotificationKind::parse(k.as_str()), Some(k));
        }
    }

    #[test]
    fn unknown_token_is_none() {
        assert_eq!(NotificationKind::parse("trade_delivered"), None);
        assert_eq!(NotificationKind::parse(""), None);
    }

    #[test]
    fn labels_are_distinct_and_present() {
        for k in NotificationKind::ALL {
            assert!(!k.label().is_empty());
        }
        assert_eq!(NotificationKind::IncomingAttack.label(), "Incoming attack");
    }
}
