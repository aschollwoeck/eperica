//! Real-time chat delivery (024): a per-process Postgres `LISTEN` task that fans new messages out to
//! locally-connected SSE subscribers via a broadcast channel.
//!
//! A send persists the row **and** `pg_notify('comms', …)` (see the `CommsRepository` impl); this listener
//! receives every instance's notifications and republishes them to in-process subscribers. The DB is the
//! bus, so delivery is correct across **multiple web instances** (P5) with no Redis and no sticky sessions.
//! Live delivery is best-effort on top of the durable record — a dropped notification never loses a
//! message (it is in the table; the next page load shows it).

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use sqlx::postgres::PgListener;
use std::sync::Arc;
use tokio::sync::broadcast;

/// The Postgres `NOTIFY` channel chat messages are published on.
const COMMS_CHANNEL: &str = "comms";

/// A live message pushed to subscribers. `keys` are the broadcast routing keys this line belongs to: a DM
/// carries the single **pair-canonical** key `dmp:<lo>:<hi>` (only the two parties derive it), a channel
/// line carries the channel key. An SSE stream for key `K` forwards the event iff `K ∈ keys`. The rest
/// renders the line. (Note: these are *not* the viewer-relative `dm:<other>` keys used for URLs/read
/// watermarks — using those here would not be pair-unique and would leak DMs.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveMessage {
    /// Conversation keys this line should reach.
    pub keys: Vec<String>,
    /// Sender display name + body + send time (Unix-ms).
    pub sender_name: String,
    pub body: String,
    pub created_ms: i64,
}

/// In-process fan-out of live chat messages. One per web instance; SSE handlers `subscribe`.
#[derive(Debug)]
pub struct ChatHub {
    tx: broadcast::Sender<LiveMessage>,
}

impl ChatHub {
    /// Create a hub with a bounded buffer (a slow subscriber lags rather than blocking senders).
    pub fn new() -> Arc<Self> {
        let (tx, _) = broadcast::channel(1024);
        Arc::new(Self { tx })
    }

    /// Subscribe to live messages (the SSE stream filters by conversation key).
    pub fn subscribe(&self) -> broadcast::Receiver<LiveMessage> {
        self.tx.subscribe()
    }
}

/// The Postgres `NOTIFY` channel notifications are published on (026).
const NOTIFICATIONS_CHANNEL: &str = "notifications";

/// A live notification nudge (026): the recipient's private routing **key** (`notif:<uuid>`) and the kind.
/// The bell only needs to know *that* a notification arrived (it refetches the count); no private payload
/// crosses the wire. An SSE stream for key `K` forwards the event iff `K == key`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveNotification {
    /// The recipient's private routing key (`notif:<uuid>`).
    pub key: String,
    /// The notification kind token (`incoming_attack` / `battle_report` / `new_message`).
    pub kind: String,
}

/// In-process fan-out of live notification nudges. One per web instance; SSE handlers `subscribe`.
#[derive(Debug)]
pub struct NotificationHub {
    tx: broadcast::Sender<LiveNotification>,
}

impl NotificationHub {
    /// Create a hub with a bounded buffer (a slow subscriber lags rather than blocking senders).
    pub fn new() -> Arc<Self> {
        let (tx, _) = broadcast::channel(1024);
        Arc::new(Self { tx })
    }

    /// Subscribe to live notification nudges (the SSE stream filters by the recipient's key).
    pub fn subscribe(&self) -> broadcast::Receiver<LiveNotification> {
        self.tx.subscribe()
    }
}

/// Run the `LISTEN notifications` loop (026), republishing each nudge to `hub`. Mirrors
/// [`run_chat_listener`]; spawn one per process. Best-effort: a dropped nudge never loses a notification
/// (it is persisted; the next poll/page load shows it).
pub async fn run_notification_listener(pool: PgPool, hub: Arc<NotificationHub>) {
    loop {
        match PgListener::connect_with(&pool).await {
            Ok(mut listener) => {
                if let Err(e) = listener.listen(NOTIFICATIONS_CHANNEL).await {
                    tracing::error!(error = %e, "notifications listener LISTEN failed");
                } else {
                    loop {
                        match listener.recv().await {
                            Ok(notification) => {
                                match serde_json::from_str::<LiveNotification>(
                                    notification.payload(),
                                ) {
                                    // Ignore the send error: no subscribers is normal, not a failure.
                                    Ok(n) => {
                                        let _ = hub.tx.send(n);
                                    }
                                    Err(e) => {
                                        tracing::warn!(error = %e, "bad notifications payload");
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::error!(error = %e, "notifications listener recv failed; reconnecting");
                                break;
                            }
                        }
                    }
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "notifications listener connect failed; retrying")
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}

/// Run the `LISTEN comms` loop, republishing each notification to `hub`. Runs until the pool closes; on a
/// listener error it logs and reconnects after a short delay (best-effort live delivery). Spawn one per
/// process next to the scheduler.
pub async fn run_chat_listener(pool: PgPool, hub: Arc<ChatHub>) {
    loop {
        match PgListener::connect_with(&pool).await {
            Ok(mut listener) => {
                if let Err(e) = listener.listen(COMMS_CHANNEL).await {
                    tracing::error!(error = %e, "comms listener LISTEN failed");
                } else {
                    loop {
                        match listener.recv().await {
                            Ok(notification) => {
                                match serde_json::from_str::<LiveMessage>(notification.payload()) {
                                    // Ignore the send error: no subscribers is normal, not a failure.
                                    Ok(msg) => {
                                        let _ = hub.tx.send(msg);
                                    }
                                    Err(e) => {
                                        tracing::warn!(error = %e, "bad comms notification payload");
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::error!(error = %e, "comms listener recv failed; reconnecting");
                                break;
                            }
                        }
                    }
                }
            }
            Err(e) => tracing::error!(error = %e, "comms listener connect failed; retrying"),
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}
