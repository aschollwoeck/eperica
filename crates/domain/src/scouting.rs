//! Scouting / espionage (GDD §6.1, §9.4 step 1) — the pure reconnaissance formula. Unlike the main
//! battle (009), espionage has **no luck, no morale, and no Wall bonus**, so it needs **no seed**: the
//! outcome is fully determined by the persisted scout counts and the per-unit `scouting` strength
//! (P2/P6). Attacking scouts can die to the defender's counter-espionage; **defending scouts never
//! die**. Everything here is pure over numbers + injected [`ScoutRules`] (P3); the application layer
//! assembles the powers from persisted state, applies the losses, and gathers the intel.

use crate::units::{UnitCounts, UnitRole, UnitSpec};

/// What a scout mission spies on (faithful: one mission cannot get both).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScoutTarget {
    /// Reveal the target village's current stored resources.
    Resources,
    /// Reveal the target's stationed troops (garrison + reinforcements) and Wall level.
    Defenses,
}

impl ScoutTarget {
    /// The storage/form slug for this target type.
    pub fn as_str(self) -> &'static str {
        match self {
            ScoutTarget::Resources => "resources",
            ScoutTarget::Defenses => "defenses",
        }
    }

    /// Parse a slug back to a target type (`None` for anything else).
    pub fn from_slug(s: &str) -> Option<Self> {
        match s {
            "resources" => Some(ScoutTarget::Resources),
            "defenses" => Some(ScoutTarget::Defenses),
            _ => None,
        }
    }
}

/// Espionage balance data (P7).
#[derive(Debug, Clone)]
pub struct ScoutRules {
    /// Power-law casualty exponent `k` for attacking scouts.
    pub loss_exponent: f64,
}

/// The resolved espionage outcome: how the attacking scouts fared, and whether the defender noticed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScoutOutcome {
    /// Fraction of the **attacking** scouts lost to counter-espionage (`0..=1`).
    pub attacker_loss_frac: f64,
    /// Whether the defender detected the mission — true iff ≥ 1 attacking scout died (counter-kill).
    pub detected: bool,
}

/// Total scouting strength of a composition: `Σ count·scouting` over **Scout-role** units only.
/// Non-scout units carry `scouting = 0` and never contribute. Not Smithy-scaled (scouting is not a
/// Smithy stat).
pub fn scouting_power(troops: &UnitCounts, roster: &[UnitSpec]) -> f64 {
    troops
        .iter()
        .filter_map(|(id, count)| {
            roster
                .iter()
                .find(|s| &s.id == id)
                .filter(|s| s.role == UnitRole::Scout)
                .map(|s| f64::from(s.scouting) * f64::from(*count))
        })
        .sum()
}

/// Resolve an espionage encounter to the attacking scouts' loss fraction and the detection flag —
/// pure and deterministic, no seed (P2/P6).
///
/// - No counter power (or no attacking power) ⇒ a **clean** scout: zero losses, undetected.
/// - Counter power **≥** attacker power ⇒ **all** attacking scouts lost (detected).
/// - Otherwise a power-law fraction of the `defender/attacker` ratio (detected) — a stronger attacker
///   loses proportionally fewer scouts.
pub fn resolve_scouting(
    attacker_power: f64,
    defender_power: f64,
    rules: &ScoutRules,
) -> ScoutOutcome {
    if attacker_power <= 0.0 || defender_power <= 0.0 {
        return ScoutOutcome {
            attacker_loss_frac: 0.0,
            detected: false,
        };
    }
    let frac = if defender_power >= attacker_power {
        1.0
    } else {
        (defender_power / attacker_power).powf(rules.loss_exponent)
    };
    ScoutOutcome {
        attacker_loss_frac: frac,
        detected: frac > 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::building::BuildingKind;
    use crate::economy::ResourceAmounts;
    use crate::units::{UnitId, UnitSpec};

    fn rules() -> ScoutRules {
        ScoutRules { loss_exponent: 1.5 }
    }

    fn scout(id: &str, scouting: u32) -> UnitSpec {
        UnitSpec {
            id: UnitId(id.to_owned()),
            name: id.to_owned(),
            role: UnitRole::Scout,
            attack: 0,
            defense_infantry: 10,
            defense_cavalry: 5,
            scouting,
            speed: 9,
            carry_capacity: 0,
            crop_upkeep: 1,
            point_value: 1,
            cost: ResourceAmounts {
                wood: 1,
                clay: 1,
                iron: 1,
                crop: 1,
            },
            train_secs: 1,
            trained_in: BuildingKind::Barracks,
            research: None,
            siege_kind: None,
        }
    }

    // AC5: scouting_power sums Scout-role strength only; non-scouts (scouting 0 or other role) add 0.
    #[test]
    fn scouting_power_counts_scouts_only() {
        let mut infantry = scout("inf", 50); // strength set but…
        infantry.role = UnitRole::Infantry; // …not a scout ⇒ excluded.
        let roster = vec![scout("spy", 20), infantry];
        let troops = vec![(UnitId("spy".into()), 3), (UnitId("inf".into()), 10)];
        assert_eq!(scouting_power(&troops, &roster), 60.0);
        // A scout with zero strength still contributes nothing.
        let zero = vec![scout("nil", 0)];
        assert_eq!(scouting_power(&vec![(UnitId("nil".into()), 5)], &zero), 0.0);
    }

    // AC5: no counter power ⇒ a clean scout (no losses, undetected) regardless of attacker size.
    #[test]
    fn no_counter_is_a_clean_scout() {
        let o = resolve_scouting(40.0, 0.0, &rules());
        assert_eq!(o.attacker_loss_frac, 0.0);
        assert!(!o.detected);
    }

    // AC5: counter power meeting or exceeding the attacker wipes the attacking scouts (detected).
    #[test]
    fn overwhelming_counter_wipes_the_attacker() {
        let equal = resolve_scouting(40.0, 40.0, &rules());
        assert_eq!(equal.attacker_loss_frac, 1.0);
        assert!(equal.detected);
        let greater = resolve_scouting(40.0, 100.0, &rules());
        assert_eq!(greater.attacker_loss_frac, 1.0);
        assert!(greater.detected);
    }

    // AC5: a partial counter costs the attacker some scouts (detected) but not all.
    #[test]
    fn partial_counter_is_a_partial_loss() {
        let o = resolve_scouting(100.0, 40.0, &rules());
        assert!(o.attacker_loss_frac > 0.0 && o.attacker_loss_frac < 1.0);
        assert!(o.detected);
    }

    // AC5: a stronger attacker loses monotonically fewer scouts against the same counter.
    #[test]
    fn stronger_attacker_loses_fewer() {
        let weak = resolve_scouting(60.0, 40.0, &rules());
        let strong = resolve_scouting(120.0, 40.0, &rules());
        assert!(strong.attacker_loss_frac < weak.attacker_loss_frac);
    }

    // AC4: two identical resolutions are equal — deterministic, no seed.
    #[test]
    fn resolution_is_deterministic_without_a_seed() {
        let a = resolve_scouting(100.0, 35.0, &rules());
        let b = resolve_scouting(100.0, 35.0, &rules());
        assert_eq!(a, b);
    }

    // ScoutTarget slug round-trips (storage/forms).
    #[test]
    fn scout_target_slug_round_trips() {
        for t in [ScoutTarget::Resources, ScoutTarget::Defenses] {
            assert_eq!(ScoutTarget::from_slug(t.as_str()), Some(t));
        }
        assert_eq!(ScoutTarget::from_slug("nonsense"), None);
    }
}
