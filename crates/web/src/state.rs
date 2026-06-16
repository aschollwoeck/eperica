//! Shared application state injected into handlers (stateless tier — P5: all game state is in the DB).

use axum::extract::FromRef;
use axum_extra::extract::cookie::Key;
use eperica_domain::{FairPlayRules, WorldConfig, WorldId, WorldMap};
use eperica_infrastructure::{
    Argon2Hasher, ChatHub, NotificationHub, PgAccountRepository, WorldRules,
};
use std::sync::Arc;

/// Cloneable handler state. Repositories are shared via `Arc`; no per-request state lives here.
#[derive(Clone)]
pub struct AppState {
    /// Account + village persistence.
    pub accounts: Arc<PgAccountRepository>,
    /// Password hasher.
    pub hasher: Arc<Argon2Hasher>,
    /// The per-world sim rule bundle (048) — economy/build/units/combat/culture/loyalty/alliance/ranking/
    /// achievements/quests/lifecycle/merchant/wonder/oasis/scout/artifacts/medals/map-rules/starting-village.
    /// One `classic` preset today; 050 serves a per-world preset via the request context.
    pub world_rules: Arc<WorldRules>,
    /// Fair-play balance rules (rate limits, suspension default, detection thresholds — 022).
    pub fair_play_rules: Arc<FairPlayRules>,
    /// Whether to trust the `X-Forwarded-For`/`X-Real-IP` headers for the client IP (022) — only when
    /// behind a known proxy. When `false` the spoofable headers are ignored and the peer address is used.
    pub trust_proxy: bool,
    /// Live chat fan-out hub (024) — SSE handlers subscribe; a background listener publishes.
    pub chat_hub: Arc<ChatHub>,
    /// Live notification fan-out hub (026) — the per-player bell stream subscribes; a background listener
    /// publishes `notif:<uuid>` nudges.
    pub notification_hub: Arc<NotificationHub>,
    /// The world's seeded map for the map view and placement (006).
    pub map: Arc<WorldMap>,
    /// World configuration (speed, radius — P7).
    pub world: WorldConfig,
    /// Operator-configured default end-game release offsets (seconds, from env — 047): the form defaults +
    /// the fallback when a create-world request omits a per-world schedule.
    pub artifact_release_offset_secs: i64,
    pub wonder_release_offset_secs: i64,
    /// The active world's id (038) — the seam the per-world scheduler/registry (039) keys on.
    pub world_id: WorldId,
    /// The world registry (041) — starts a per-world scheduler live (admin world creation).
    pub world_registry: Arc<crate::registry::WorldRegistry>,
    /// Whether new accounts must confirm their email before login (AC1 / Decisions).
    pub require_email_confirmation: bool,
    /// Key used to encrypt the auth cookie.
    pub cookie_key: Key,
}

// Lets `PrivateCookieJar` extract the encryption key from the app state.
impl FromRef<AppState> for Key {
    fn from_ref(state: &AppState) -> Self {
        state.cookie_key.clone()
    }
}
