//! Shared application state injected into handlers (stateless tier — P5: all game state is in the DB).

use axum::extract::FromRef;
use axum_extra::extract::cookie::Key;
use eperica_domain::{
    AllianceRules, BuildRules, CultureRules, EconomyRules, LoyaltyRules, MerchantRules,
    StartingVillage, UnitRules, WorldConfig, WorldMap,
};
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
    /// Construction balance rules (costs, times, prerequisites).
    pub build_rules: Arc<BuildRules>,
    /// Unit balance rules (per-tribe rosters, research, Smithy upgrades — 004).
    pub unit_rules: Arc<UnitRules>,
    /// Culture-point + expansion balance rules (CP rate, thresholds, slots, settlers — 013).
    pub culture_rules: Arc<CultureRules>,
    /// Loyalty + conquest balance rules (regen, drop, post-conquest loyalty — 014).
    pub loyalty_rules: Arc<LoyaltyRules>,
    /// Alliance + diplomacy balance rules (membership cap, Embassy gates — 015).
    pub alliance_rules: Arc<AllianceRules>,
    /// Merchant/trade balance rules (per-tribe capacity + speed, merchants per level — 008).
    pub merchant_rules: Arc<MerchantRules>,
    /// The world's seeded map for the map view and placement (006).
    pub map: Arc<WorldMap>,
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
