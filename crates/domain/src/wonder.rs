//! Wonder-of-the-World rules (021, GDD §11.3): the end-game victory building. Pure (P3) — no I/O.
//!
//! The Wonder is an ordinary [`crate::building::BuildingKind::Wonder`] raised level by level to
//! [`MAX_WONDER_LEVEL`] through the **003 construction queue**: [`wonder_level_spec`] generates its
//! 100-entry cost/time table from a geometric curve so the build path is reused unchanged. The first
//! alliance to a complete Wonder wins the round.

use crate::construction::LevelSpec;
use crate::economy::ResourceAmounts;

/// The level a Wonder must reach to win the round (GDD §11.3).
pub const MAX_WONDER_LEVEL: u8 = 100;

/// Tunable Wonder balance (P7): the construction curve, how many plans/sites release, and the Natar
/// site garrison. `cost`/`time` grow geometrically with the level.
#[derive(Debug, Clone, PartialEq)]
pub struct WonderRules {
    /// Level-1 construction cost; each further level multiplies by `cost_ratio`.
    pub base_cost: ResourceAmounts,
    /// Per-level cost growth (> 1.0).
    pub cost_ratio: f64,
    /// Level-1 construction time (base seconds, speed-scaled by the build path); ×`time_ratio` per level.
    pub base_time_secs: i64,
    /// Per-level time growth (> 1.0).
    pub time_ratio: f64,
    /// How many capturable Wonder **plans** release into Natar vaults.
    pub plan_count: u32,
    /// How many conquerable Wonder **sites** release.
    pub site_count: u32,
    /// The site's defensive garrison unit + strength (mirrors the artifact-vault garrison, 020).
    pub garrison_unit: String,
    pub garrison_base_count: i64,
    pub garrison_per_index: i64,
}

/// Whether a Wonder at `level` is complete (the round-winning condition).
pub fn wonder_complete(level: u8) -> bool {
    level >= MAX_WONDER_LEVEL
}

/// Build the Wonder's [`LevelSpec`] — `MAX_WONDER_LEVEL` entries whose cost/time grow geometrically from
/// the rule's bases. Merged into the construction rules so `order_build`/`build_time_secs` work unchanged.
pub fn wonder_level_spec(rules: &WonderRules) -> LevelSpec {
    let mut cost_per_level = Vec::with_capacity(MAX_WONDER_LEVEL as usize);
    let mut time_secs_per_level = Vec::with_capacity(MAX_WONDER_LEVEL as usize);
    for i in 0..MAX_WONDER_LEVEL {
        let cost_factor = rules.cost_ratio.powi(i32::from(i));
        let scale = |base: i64| ((base as f64) * cost_factor).round() as i64;
        cost_per_level.push(ResourceAmounts {
            wood: scale(rules.base_cost.wood),
            clay: scale(rules.base_cost.clay),
            iron: scale(rules.base_cost.iron),
            crop: scale(rules.base_cost.crop),
        });
        let secs =
            ((rules.base_time_secs as f64) * rules.time_ratio.powi(i32::from(i))).round() as i64;
        time_secs_per_level.push(secs.max(1));
    }
    LevelSpec {
        cost_per_level,
        time_secs_per_level,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> WonderRules {
        WonderRules {
            base_cost: ResourceAmounts {
                wood: 1000,
                clay: 1000,
                iron: 1000,
                crop: 1000,
            },
            cost_ratio: 1.2,
            base_time_secs: 3600,
            time_ratio: 1.15,
            plan_count: 3,
            site_count: 2,
            garrison_unit: "praetorian".to_owned(),
            garrison_base_count: 1000,
            garrison_per_index: 200,
        }
    }

    #[test]
    fn complete_at_100() {
        assert!(!wonder_complete(99));
        assert!(wonder_complete(100));
        assert!(wonder_complete(101));
    }

    #[test]
    fn spec_has_100_monotonic_levels() {
        let spec = wonder_level_spec(&rules());
        assert_eq!(spec.cost_per_level.len(), MAX_WONDER_LEVEL as usize);
        assert_eq!(spec.time_secs_per_level.len(), MAX_WONDER_LEVEL as usize);
        assert_eq!(spec.max_level(), MAX_WONDER_LEVEL);
        // Geometric growth ⇒ strictly increasing cost + time.
        for w in spec.cost_per_level.windows(2) {
            assert!(w[1].wood > w[0].wood, "cost increases each level");
        }
        for w in spec.time_secs_per_level.windows(2) {
            assert!(w[1] >= w[0], "time non-decreasing each level");
        }
        // Level 1 == the base cost.
        assert_eq!(spec.cost_per_level[0].wood, 1000);
    }
}
