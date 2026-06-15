//! Shared application state injected into handlers (stateless tier — P5: all game state is in the DB).

use axum::extract::FromRef;
use axum_extra::extract::cookie::Key;
use eperica_domain::{
    AchievementDef, AllianceRules, BuildRules, CombatRules, CultureRules, EconomyRules,
    FairPlayRules, LifecycleRules, LoyaltyRules, MerchantRules, QuestDef, RankingRules,
    StartingVillage, UnitRules, WonderRules, WorldConfig, WorldId, WorldMap,
};
use eperica_infrastructure::{Argon2Hasher, ChatHub, NotificationHub, PgAccountRepository};
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
    /// Combat balance (031 — used to show Smithy upgrade stat gains; combat itself runs in the scheduler).
    pub combat_rules: Arc<CombatRules>,
    /// Culture-point + expansion balance rules (CP rate, thresholds, slots, settlers — 013).
    pub culture_rules: Arc<CultureRules>,
    /// Loyalty + conquest balance rules (regen, drop, post-conquest loyalty — 014).
    pub loyalty_rules: Arc<LoyaltyRules>,
    /// Alliance + diplomacy balance rules (membership cap, Embassy gates — 015).
    pub alliance_rules: Arc<AllianceRules>,
    /// Ranking balance rules (per-unit kill point values, leaderboard windows + page size — 016).
    pub ranking_rules: Arc<RankingRules>,
    /// The achievement catalogue (milestone predicates + rewards — 017), evaluated lazily on view.
    pub achievement_catalogue: Arc<Vec<AchievementDef>>,
    /// The onboarding quest chain (ordered conditions + rewards — 018), evaluated lazily on view.
    pub quest_chain: Arc<Vec<QuestDef>>,
    /// Account-lifecycle rules (beginner's protection + inactivity/abandonment timings — 019, P7).
    pub lifecycle_rules: Arc<LifecycleRules>,
    /// Merchant/trade balance rules (per-tribe capacity + speed, merchants per level — 008).
    pub merchant_rules: Arc<MerchantRules>,
    /// Wonder-of-the-World balance rules (construction curve, plan/site counts — 021).
    pub wonder_rules: Arc<WonderRules>,
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
    /// The active world's id (038) — the seam the per-world scheduler/registry (039) keys on.
    pub world_id: WorldId,
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
