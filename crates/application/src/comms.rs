//! Communication use-cases (024): WhatsApp-style conversations — direct messages + group channels,
//! persisted and (in the infrastructure layer) delivered live. All server-authoritative (P4): bodies are
//! validated by the pure domain rules, recipient existence + channel access are gated here.

use crate::ports::{
    AccountRepository, AllianceRepository, CommsRepository, ConversationSummary, MessageView,
    RepoError,
};
use eperica_domain::{
    AllianceId, ChatChannel, PlayerId, Timestamp, can_access_channel, valid_body,
};

/// Why a communication action was rejected (024).
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CommsError {
    /// Body empty or too long.
    #[error("invalid message")]
    Invalid,
    /// A player tried to DM themselves.
    #[error("cannot message yourself")]
    SelfSend,
    /// The DM recipient does not exist or has been retired.
    #[error("recipient unavailable")]
    RecipientUnavailable,
    /// The conversation key is malformed, or the player may not access it.
    #[error("forbidden")]
    Forbidden,
    /// A storage/backend failure.
    #[error("storage error: {0}")]
    Backend(String),
}

impl From<RepoError> for CommsError {
    fn from(e: RepoError) -> Self {
        CommsError::Backend(e.to_string())
    }
}

/// The viewer-relative conversation key for a DM with `other` — `dm:<uuid>`, using the player's uuid so it
/// matches the database's user ids (the read watermark + stream subscription key).
pub fn dm_key(other: PlayerId) -> String {
    format!("dm:{}", uuid::Uuid::from_u128(other.0))
}

/// Parse a `dm:<uuid>` conversation key back to the other player, or `None` if not a DM key.
pub fn parse_dm_key(key: &str) -> Option<PlayerId> {
    key.strip_prefix("dm:")
        .and_then(|s| uuid::Uuid::parse_str(s).ok())
        .map(|u| PlayerId(u.as_u128()))
}

/// Send a direct message from `sender` to `recipient` (024 AC1). Rejects an invalid body, a self-DM, and an
/// unknown/abandoned recipient — server-side. Returns the new message id.
///
/// # Errors
/// See [`CommsError`].
pub async fn send_dm<A, C>(
    accounts: &A,
    comms: &C,
    sender: PlayerId,
    recipient: PlayerId,
    body: &str,
    now: Timestamp,
) -> Result<u128, CommsError>
where
    A: AccountRepository,
    C: CommsRepository,
{
    if !valid_body(body) {
        return Err(CommsError::Invalid);
    }
    if sender == recipient {
        return Err(CommsError::SelfSend);
    }
    match accounts.find_user_by_id(recipient).await? {
        Some(u) if !u.abandoned => {}
        _ => return Err(CommsError::RecipientUnavailable),
    }
    Ok(comms.send_dm(sender, recipient, body.trim(), now).await?)
}

/// Resolve the membership a channel-access check needs (the player's alliance, if any).
async fn membership_of<L: AllianceRepository>(
    alliances: &L,
    player: PlayerId,
) -> Result<Option<AllianceId>, CommsError> {
    Ok(alliances.alliance_of(player).await?.map(|m| m.alliance))
}

/// Post to a channel `channel_key` (`global` / `alliance:<id>`) the sender may access (024 AC1/AC5).
///
/// # Errors
/// See [`CommsError`].
pub async fn send_chat<L, C>(
    alliances: &L,
    comms: &C,
    sender: PlayerId,
    channel_key: &str,
    body: &str,
    now: Timestamp,
) -> Result<u128, CommsError>
where
    L: AllianceRepository,
    C: CommsRepository,
{
    if !valid_body(body) {
        return Err(CommsError::Invalid);
    }
    let Some(channel) = ChatChannel::parse(channel_key) else {
        return Err(CommsError::Forbidden);
    };
    if !can_access_channel(channel, membership_of(alliances, sender).await?) {
        return Err(CommsError::Forbidden);
    }
    Ok(comms
        .post_chat(channel_key, sender, body.trim(), now)
        .await?)
}

/// Open a DM thread with `other`: its history (oldest→newest) + advancing the viewer's read mark (AC2/AC4).
///
/// # Errors
/// See [`CommsError`].
pub async fn open_dm<C>(
    comms: &C,
    viewer: PlayerId,
    other: PlayerId,
    limit: i64,
    now: Timestamp,
) -> Result<Vec<MessageView>, CommsError>
where
    C: CommsRepository,
{
    let history = comms.dm_history(viewer, other, limit).await?;
    comms.mark_read(viewer, &dm_key(other), now).await?;
    Ok(history)
}

/// Open a channel the viewer may access: its history + advancing the viewer's read mark (AC2/AC4/AC5).
///
/// # Errors
/// See [`CommsError`].
pub async fn open_chat<L, C>(
    alliances: &L,
    comms: &C,
    viewer: PlayerId,
    channel_key: &str,
    limit: i64,
    now: Timestamp,
) -> Result<Vec<MessageView>, CommsError>
where
    L: AllianceRepository,
    C: CommsRepository,
{
    let Some(channel) = ChatChannel::parse(channel_key) else {
        return Err(CommsError::Forbidden);
    };
    if !can_access_channel(channel, membership_of(alliances, viewer).await?) {
        return Err(CommsError::Forbidden);
    }
    let history = comms.chat_history(channel_key, limit).await?;
    comms.mark_read(viewer, channel_key, now).await?;
    Ok(history)
}

/// The viewer's conversations list (024 AC3): their DM threads + the global channel + their alliance
/// channel, each with the latest line + unread, newest-activity first.
///
/// # Errors
/// See [`CommsError`].
pub async fn conversation_list<L, C>(
    alliances: &L,
    comms: &C,
    viewer: PlayerId,
) -> Result<Vec<ConversationSummary>, CommsError>
where
    L: AllianceRepository,
    C: CommsRepository,
{
    let mut out = comms.dm_threads(viewer).await?;
    out.push(channel_summary(comms, viewer, "global", "Global".to_owned()).await?);
    if let Some(alliance) = membership_of(alliances, viewer).await? {
        let key = ChatChannel::Alliance(alliance).as_key();
        let title = alliances
            .alliance_summary(alliance)
            .await?
            .map_or_else(|| "Alliance".to_owned(), |(name, _)| name);
        out.push(channel_summary(comms, viewer, &key, title).await?);
    }
    out.sort_by_key(|c| std::cmp::Reverse(c.last_ms));
    Ok(out)
}

/// Build a channel's conversations-list row (latest line + unread).
async fn channel_summary<C: CommsRepository>(
    comms: &C,
    viewer: PlayerId,
    key: &str,
    title: String,
) -> Result<ConversationSummary, CommsError> {
    let (last_body, last_ms) = comms.channel_latest(key).await?.unwrap_or_default();
    let unread = comms.channel_unread(viewer, key).await?;
    Ok(ConversationSummary {
        key: key.to_owned(),
        title,
        last_body,
        last_ms,
        unread,
    })
}

/// The viewer's total unread across all conversations (the nav badge, 024 AC4).
///
/// # Errors
/// See [`CommsError`].
pub async fn unread_badge<L, C>(
    alliances: &L,
    comms: &C,
    viewer: PlayerId,
) -> Result<i64, CommsError>
where
    L: AllianceRepository,
    C: CommsRepository,
{
    Ok(conversation_list(alliances, comms, viewer)
        .await?
        .iter()
        .map(|c| c.unread)
        .sum())
}
