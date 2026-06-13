//! Achievement rules (017): the milestone catalogue (predicates over a player's persisted progress)
//! and the once-only grant logic. Pure (P3) — no I/O. The catalogue is balance data (P7); detecting
//! progress and persisting grants is the application/infrastructure's job.

use crate::economy::ResourceAmounts;
use std::collections::HashSet;

/// A catalogue entry's stable id (also the persisted/granted key).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AchievementId(pub String);

/// The milestone an achievement tracks (017 seed set, GDD §11.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AchievementKind {
    /// Found your 2nd village.
    SecondVillage,
    /// Win `threshold` defensive battles.
    DefensiveWins,
    /// Occupy your first oasis.
    FirstOasis,
    /// Reach `threshold` total population.
    Population,
    /// Research every unit of your tribe.
    ResearchAllUnits,
}

/// A one-time reward an achievement may carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reward {
    /// No reward — the badge only.
    None,
    /// Resources credited to the player's capital (capped by its stores).
    Resources(ResourceAmounts),
    /// Culture points added to the player.
    Culture(i64),
}

/// One catalogue entry: a milestone predicate + an optional reward.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AchievementDef {
    pub id: AchievementId,
    pub kind: AchievementKind,
    /// The threshold for count-based kinds (`DefensiveWins`, `Population`); ignored otherwise.
    pub threshold: i64,
    pub reward: Reward,
}

/// A player's current persisted progress, gathered by the application from authoritative state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PlayerProgress {
    pub village_count: i64,
    pub defensive_wins: i64,
    pub oases_held: i64,
    pub population: i64,
    /// How many of the player's tribe's units they have researched.
    pub units_researched: i64,
    /// How many units the player's tribe has (the target for `ResearchAllUnits`).
    pub tribe_unit_count: i64,
}

/// Whether `progress` satisfies `def`'s milestone (P3 — pure, deterministic).
pub fn met(def: &AchievementDef, progress: &PlayerProgress) -> bool {
    match def.kind {
        AchievementKind::SecondVillage => progress.village_count >= 2,
        AchievementKind::DefensiveWins => progress.defensive_wins >= def.threshold,
        AchievementKind::FirstOasis => progress.oases_held >= 1,
        AchievementKind::Population => progress.population >= def.threshold,
        AchievementKind::ResearchAllUnits => {
            progress.tribe_unit_count > 0 && progress.units_researched >= progress.tribe_unit_count
        }
    }
}

/// The catalogue entries the player has newly earned: predicate met **and** not already held (so the
/// grant is once-only, P2/P6).
pub fn newly_earned<'a>(
    progress: &PlayerProgress,
    catalogue: &'a [AchievementDef],
    held: &HashSet<AchievementId>,
) -> Vec<&'a AchievementDef> {
    catalogue
        .iter()
        .filter(|d| !held.contains(&d.id) && met(d, progress))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(kind: AchievementKind, threshold: i64) -> AchievementDef {
        AchievementDef {
            id: AchievementId(format!("{kind:?}")),
            kind,
            threshold,
            reward: Reward::None,
        }
    }

    #[test]
    fn predicates_fire_at_their_threshold() {
        let p = PlayerProgress {
            village_count: 2,
            defensive_wins: 100,
            oases_held: 1,
            population: 1000,
            units_researched: 10,
            tribe_unit_count: 10,
        };
        assert!(met(&def(AchievementKind::SecondVillage, 0), &p));
        assert!(met(&def(AchievementKind::DefensiveWins, 100), &p));
        assert!(!met(&def(AchievementKind::DefensiveWins, 101), &p));
        assert!(met(&def(AchievementKind::FirstOasis, 0), &p));
        assert!(met(&def(AchievementKind::Population, 1000), &p));
        assert!(met(&def(AchievementKind::ResearchAllUnits, 0), &p));

        // Below thresholds: nothing fires.
        let none = PlayerProgress::default();
        assert!(!met(&def(AchievementKind::SecondVillage, 0), &none));
        assert!(!met(&def(AchievementKind::FirstOasis, 0), &none));
        // Research-all requires a known target and full coverage.
        let partial = PlayerProgress {
            units_researched: 9,
            tribe_unit_count: 10,
            ..PlayerProgress::default()
        };
        assert!(!met(&def(AchievementKind::ResearchAllUnits, 0), &partial));
    }

    #[test]
    fn newly_earned_excludes_held() {
        let cat = vec![
            def(AchievementKind::SecondVillage, 0),
            def(AchievementKind::FirstOasis, 0),
        ];
        let p = PlayerProgress {
            village_count: 2,
            oases_held: 1,
            ..PlayerProgress::default()
        };
        let mut held = HashSet::new();
        held.insert(cat[0].id.clone());
        let earned = newly_earned(&p, &cat, &held);
        assert_eq!(earned.len(), 1);
        assert_eq!(earned[0].kind, AchievementKind::FirstOasis);
    }
}
