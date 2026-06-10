//! Shared application state injected into handlers (stateless tier — P5: all game state is in the DB).

use axum::extract::FromRef;
use axum_extra::extract::cookie::Key;
use eperica_domain::{EconomyRules, StartingVillage, WorldConfig};
use eperica_infrastructure::{Argon2Hasher, PgAccountRepository};
use std::sync::Arc;

/// Cloneable handler state. Repositories are shared via `Arc`; no per-request state lives here.
#[derive(Clone)]
pub struct AppState {
    /// Account + village persistence.
    pub accounts: Arc<PgAccountRepository>,
    /// Password hasher.
    pub hasher: Arc<Argon2Hasher>,
    /// The starting-village template (from balance data).
    pub template: Arc<StartingVillage>,
    /// Economy balance rules (production, population, capacity, starting amounts).
    pub rules: Arc<EconomyRules>,
    /// World configuration (speed, radius — P7).
    pub world: WorldConfig,
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
