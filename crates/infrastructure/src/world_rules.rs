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

/// The rule presets an operator may run a world under (049). `classic` is the shipped balance; a real
/// second preset (with its balance overlay) lands in 052.
pub const KNOWN_PRESETS: &[&str] = &["classic"];

/// The default preset — every world without an explicit choice plays `classic` (matches the
/// `worlds.rule_preset` DB default).
pub const DEFAULT_PRESET: &str = "classic";

/// Whether `name` is a preset an operator may select (049) — the server-authoritative allow-list (P4).
#[must_use]
pub fn known_preset(name: &str) -> bool {
    KNOWN_PRESETS.contains(&name)
}

/// Load the named rule bundle (049). Only `classic` (the shipped balance) is known today; an unknown name
/// is a clear error. 050 resolves a world's `rule_preset` through this; 052 adds the first non-`classic`
/// preset + its balance overlay.
///
/// # Errors
/// [`BalanceError`] if the preset is unknown or any balance file fails to parse/validate.
pub fn load_world_rules(preset: &str) -> Result<WorldRules, BalanceError> {
    // Resolve the preset to its complete balance directory (052); an unknown name is a clear error (P4).
    let d = balance::preset_data(preset)
        .ok_or_else(|| BalanceError::UnknownPreset(preset.to_owned()))?;
    Ok(WorldRules {
        economy: balance::parse_economy_rules(d.economy)?,
        build: balance::parse_build_rules(d.construction)?,
        units: balance::parse_unit_rules(d.units)?,
        combat: balance::parse_combat_rules(d.combat)?,
        culture: balance::parse_culture_rules(d.culture)?,
        loyalty: balance::parse_loyalty_rules(d.conquest)?,
        alliance: balance::parse_alliance_rules(d.alliance)?,
        ranking: balance::parse_ranking_rules(d.ranking)?,
        achievements: balance::parse_achievement_catalogue(d.achievements)?,
        quests: balance::parse_quest_chain(d.quests)?,
        lifecycle: balance::parse_lifecycle_rules(d.lifecycle)?,
        merchant: balance::parse_merchant_rules(d.trade)?,
        wonder: balance::parse_wonder_rules(d.wonder)?,
        oasis: balance::parse_oasis_rules(d.units)?,
        scout: balance::parse_scout_rules(d.combat)?,
        artifacts: balance::parse_artifact_catalogue(d.artifacts)?,
        medals: balance::parse_medal_rules(d.medals)?,
        map_rules: balance::parse_map_rules(d.map)?,
        starting_village: balance::parse_starting_village(d.starting_village)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classic_loads_and_unknown_preset_errors() {
        assert!(known_preset("classic"));
        assert!(!known_preset("speed"));
        assert_eq!(DEFAULT_PRESET, "classic");
        // The classic bundle assembles from the shipped balance.
        load_world_rules("classic").expect("classic preset loads");
        // An unknown preset is a clear, server-authoritative rejection (049).
        assert!(matches!(
            load_world_rules("speed"),
            Err(BalanceError::UnknownPreset(p)) if p == "speed"
        ));
    }
}
