//! Communication rules (024): chat-channel access + message validation. Pure (P3) — no I/O.
//!
//! Channel access is a pure function of the player's alliance membership (P4 — the server decides, never
//! the client). Validation bounds message/chat content so the persistence + UI layers can trust lengths.

use crate::alliance::AllianceId;

/// Max lengths (characters) for mail subject/body and a chat line.
pub const MAX_SUBJECT: usize = 120;
pub const MAX_BODY: usize = 4000;
pub const MAX_CHAT: usize = 500;

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

/// Whether a piece of text is non-empty after trimming and within `max` characters.
fn in_bounds(text: &str, max: usize) -> bool {
    let n = text.trim().chars().count();
    n > 0 && n <= max
}

/// Whether a mail `subject` + `body` are valid to send (024 AC1) — both non-empty + within caps.
pub fn valid_message(subject: &str, body: &str) -> bool {
    in_bounds(subject, MAX_SUBJECT) && in_bounds(body, MAX_BODY)
}

/// Whether a chat line is valid to post (024 AC6) — non-empty + within the chat cap.
pub fn valid_chat(body: &str) -> bool {
    in_bounds(body, MAX_CHAT)
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
        assert!(valid_message("hi", "there"));
        assert!(!valid_message("   ", "body")); // empty subject
        assert!(!valid_message("subj", "  ")); // empty body
        assert!(!valid_message(&"x".repeat(MAX_SUBJECT + 1), "body"));
        assert!(valid_chat("gg"));
        assert!(!valid_chat(""));
        assert!(!valid_chat(&"x".repeat(MAX_CHAT + 1)));
    }
}
