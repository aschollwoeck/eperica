//! Communication rules (024): chat-channel access + message validation. Pure (P3) — no I/O.
//!
//! Channel access is a pure function of the player's alliance membership (P4 — the server decides, never
//! the client). Validation bounds message/chat content so the persistence + UI layers can trust lengths.

use crate::alliance::AllianceId;

/// Max length (characters) of a single message body (DM line or chat line). No subjects — these are
/// conversations, not mail.
pub const MAX_MESSAGE: usize = 2000;

/// A chat channel a player may read/post: the open `Global` channel, or a per-alliance channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatChannel {
    /// Open to every player.
    Global,
    /// Restricted to members of the given alliance (015).
    Alliance(AllianceId),
}

impl ChatChannel {
    /// The persisted / URL form: `"global"` or `"alliance:<u128>"`.
    pub fn as_key(self) -> String {
        match self {
            ChatChannel::Global => "global".to_owned(),
            ChatChannel::Alliance(a) => format!("alliance:{}", a.0),
        }
    }

    /// Parse a channel key, or `None` if malformed.
    pub fn parse(key: &str) -> Option<Self> {
        if key == "global" {
            return Some(ChatChannel::Global);
        }
        key.strip_prefix("alliance:")
            .and_then(|id| id.parse::<u128>().ok())
            .map(|id| ChatChannel::Alliance(AllianceId(id)))
    }
}

/// Whether a player with the given alliance membership may access `channel` (024 AC5). Global is open to
/// all; an alliance channel only to a member of that alliance.
pub fn can_access_channel(channel: ChatChannel, membership: Option<AllianceId>) -> bool {
    match channel {
        ChatChannel::Global => true,
        ChatChannel::Alliance(a) => membership == Some(a),
    }
}

/// Whether a message `body` is valid to send (024 AC1) — non-empty after trimming and within
/// [`MAX_MESSAGE`] characters. Applies to both DM lines and channel lines.
pub fn valid_body(body: &str) -> bool {
    let n = body.trim().chars().count();
    n > 0 && n <= MAX_MESSAGE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_access_follows_membership() {
        let a = AllianceId(7);
        let b = AllianceId(8);
        assert!(can_access_channel(ChatChannel::Global, None));
        assert!(can_access_channel(ChatChannel::Global, Some(a)));
        assert!(can_access_channel(ChatChannel::Alliance(a), Some(a)));
        assert!(!can_access_channel(ChatChannel::Alliance(a), Some(b)));
        assert!(!can_access_channel(ChatChannel::Alliance(a), None));
    }

    #[test]
    fn channel_key_round_trips() {
        for c in [ChatChannel::Global, ChatChannel::Alliance(AllianceId(42))] {
            assert_eq!(ChatChannel::parse(&c.as_key()), Some(c));
        }
        assert_eq!(ChatChannel::parse("nope"), None);
        assert_eq!(ChatChannel::parse("alliance:notanumber"), None);
    }

    #[test]
    fn validation_bounds() {
        assert!(valid_body("hi there"));
        assert!(!valid_body("   ")); // empty after trim
        assert!(!valid_body(""));
        assert!(!valid_body(&"x".repeat(MAX_MESSAGE + 1)));
        assert!(valid_body(&"x".repeat(MAX_MESSAGE)));
    }
}
