//! The per-world rule bundle (048, ADR 0035) — every **sim** rule set a world runs under, grouped into one
//! value so a world can carry a single rule profile (a "preset"). Loaded from the balance data; the only
//! caller-visible change vs. the individual loaders is the grouping.
//!
//! `fair_play_rules` (rate limiting / detection — a process/account-level anti-cheat concern) is **not**
//! here; nor are the hashers/hubs/live `WorldMap`. Wrap in one `Arc<WorldRules>` (one allocation per preset,
//! shared across that preset's worlds); field reads deref through the outer `Arc`.

use crate::balance::{self, ArtifactCatalogue, BalanceError};
use eperica_domain::{
    AchievementDef, AllianceRules, BuildRules, CombatRules, CultureRules, EconomyRules,
    LifecycleRules, LoyaltyRules, MapRules, MedalRules, MerchantRules, OasisRules, QuestDef,
    RankingRules, ScoutRules, StartingVillage, UnitRules, WonderRules,
};

/// The complete sim rule set a single world plays under (048). One bundle = one preset.
#[derive(Debug, Clone)]
pub struct WorldRules {
    /// Economy balance (production, population, capacity, starting amounts — 002).
    pub economy: EconomyRules,
    /// Construction balance (costs, times, prerequisites — 003).
    pub build: BuildRules,
    /// Unit balance (per-tribe rosters, research, Smithy upgrades — 004).
    pub units: UnitRules,
    /// Combat balance (009/010).
    pub combat: CombatRules,
    /// Culture-point + expansion balance (013).
    pub culture: CultureRules,
    /// Loyalty + conquest balance (014).
    pub loyalty: LoyaltyRules,
    /// Alliance + diplomacy balance (015).
    pub alliance: AllianceRules,
    /// Ranking balance (kill point values, leaderboard windows + page size — 016).
    pub ranking: RankingRules,
    /// The achievement catalogue (milestone predicates + rewards — 017).
    pub achievements: Vec<AchievementDef>,
    /// The onboarding quest chain (ordered conditions + rewards — 018).
    pub quests: Vec<QuestDef>,
    /// Account-lifecycle balance (beginner protection + inactivity/abandonment timings — 019, P7).
    pub lifecycle: LifecycleRules,
    /// Merchant/trade balance (per-tribe capacity + speed, merchants per level — 008).
    pub merchant: MerchantRules,
    /// Wonder-of-the-World balance (construction curve, plan/site counts — 021).
    pub wonder: WonderRules,
    /// Oasis balance (bonuses, animal garrisons — 012).
    pub oasis: OasisRules,
    /// Scouting balance (012).
    pub scout: ScoutRules,
    /// The artifact catalogue (types × small/large/unique, effects — 020).
    pub artifacts: ArtifactCatalogue,
    /// Medal balance (weekly settlement categories + thresholds — 017).
    pub medals: MedalRules,
    /// Map-generation balance (terrain weights, oasis density — 006).
    pub map_rules: MapRules,
    /// The starting-village template (fields + core buildings — 001).
    pub starting_village: StartingVillage,
}

/// Load the `classic` rule bundle (the current balance data — 048). Later slices (049+) load a named preset.
///
/// # Errors
/// [`BalanceError`] if any balance file fails to parse/validate.
pub fn load_world_rules() -> Result<WorldRules, BalanceError> {
    Ok(WorldRules {
        economy: balance::economy_rules()?,
        build: balance::build_rules()?,
        units: balance::unit_rules()?,
        combat: balance::combat_rules()?,
        culture: balance::culture_rules()?,
        loyalty: balance::loyalty_rules()?,
        alliance: balance::alliance_rules()?,
        ranking: balance::ranking_rules()?,
        achievements: balance::achievement_catalogue()?,
        quests: balance::quest_chain()?,
        lifecycle: balance::lifecycle_rules()?,
        merchant: balance::merchant_rules()?,
        wonder: balance::wonder_rules()?,
        oasis: balance::oasis_rules()?,
        scout: balance::scout_rules()?,
        artifacts: balance::artifact_catalogue()?,
        medals: balance::medal_rules()?,
        map_rules: balance::map_rules()?,
        starting_village: balance::starting_village()?,
    })
}
