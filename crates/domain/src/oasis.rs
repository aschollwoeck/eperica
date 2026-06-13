//! Oases (GDD §7.4) — the seeded wild-animal garrison that guards an oasis tile until it is cleared.
//!
//! The garrison is a **pure function of the world seed and the tile coordinate** (P6) — the same seed
//! and tile always yield the same animals, so an un-fought oasis needs no stored state. Strength rises
//! with distance from the origin (oases near spawn are easy; the frontier is dangerous), and the seed
//! picks the animal *kind* for variety. Everything here is pure over numbers + injected [`OasisRules`]
//! (P3); the application materialises and mutates the garrison once an oasis is fought.

use crate::map::mix;
use crate::units::{UnitCounts, UnitSpec};
use crate::world::Coordinate;

/// Balance for the seeded oasis garrison (P7).
#[derive(Debug, Clone, Copy)]
pub struct OasisRules {
    /// Animals an oasis at the origin holds.
    pub base_count: u32,
    /// Extra animals added per `tiles_per_step` tiles of distance from the origin.
    pub extra_per_step: u32,
    /// Tiles of distance between each `extra_per_step` increment.
    pub tiles_per_step: u32,
    /// Hard cap on the garrison size.
    pub max_count: u32,
    /// Tiles of distance between each rise in animal **strength tier** (the roster index).
    pub tiles_per_tier: u32,
    /// Seconds between regrow ticks of a cleared, unoccupied oasis (012 AC9; pre-speed-scaling).
    pub regrow_secs: i64,
    /// Animals a single regrow tick adds back toward the seeded strength.
    pub regrow_per_step: u32,
}

/// One regrow tick (012 AC9): top the oasis's (single-kind) animal garrison up toward its `seeded`
/// strength by `per_step`, capped at the seeded count. Returns the new garrison and whether it has
/// reached full strength (so the caller can stop rescheduling). Pure and deterministic.
pub fn regrow_step(current: &UnitCounts, seeded: &UnitCounts, per_step: u32) -> (UnitCounts, bool) {
    let Some((id, target)) = seeded.first() else {
        return (Vec::new(), true);
    };
    let cur = current.iter().find(|(i, _)| i == id).map_or(0, |(_, n)| *n);
    let next = cur.saturating_add(per_step.max(1)).min(*target);
    let garrison = if next == 0 {
        Vec::new()
    } else {
        vec![(id.clone(), next)]
    };
    (garrison, next >= *target)
}

/// The wild-animal garrison guarding the oasis at `coord`, seeded from the world `seed` (P6).
///
/// Returns a single animal kind (a roster index that rises with distance, perturbed ±1 tier by the
/// seed) and a count that grows with distance up to the cap. Empty when there are no animal specs.
/// Pure and deterministic: same `(seed, coord)` ⇒ same garrison.
pub fn oasis_garrison(
    seed: u64,
    coord: Coordinate,
    animals: &[UnitSpec],
    rules: &OasisRules,
) -> UnitCounts {
    if animals.is_empty() {
        return Vec::new();
    }
    // Chebyshev ring from the origin (matches the square map's "how far out" feel).
    let ring = coord.x.unsigned_abs().max(coord.y.unsigned_abs());
    let steps = rules.tiles_per_step.max(1);
    let count = (rules.base_count + (ring / steps) * rules.extra_per_step).min(rules.max_count);
    if count == 0 {
        return Vec::new();
    }
    let h = mix(seed, coord.x, coord.y);
    let tier_w = rules.tiles_per_tier.max(1);
    // Base strength tier from distance, nudged 0/+1 by the seed, clamped to the roster.
    let tier = (ring / tier_w) as usize + (h as usize % 2);
    let tier = tier.min(animals.len() - 1);
    vec![(animals[tier].id.clone(), count)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::building::BuildingKind;
    use crate::economy::ResourceAmounts;
    use crate::units::{UnitId, UnitRole};

    fn rules() -> OasisRules {
        OasisRules {
            base_count: 5,
            extra_per_step: 3,
            tiles_per_step: 5,
            max_count: 60,
            tiles_per_tier: 15,
            regrow_secs: 3600,
            regrow_per_step: 2,
        }
    }

    fn animal(id: &str, di: u32, dc: u32) -> UnitSpec {
        UnitSpec {
            id: UnitId(id.to_owned()),
            name: id.to_owned(),
            role: UnitRole::Wild,
            attack: 0,
            defense_infantry: di,
            defense_cavalry: dc,
            scouting: 0,
            speed: 0,
            carry_capacity: 0,
            crop_upkeep: 0,
            point_value: 0,
            cost: ResourceAmounts::default(),
            train_secs: 0,
            trained_in: BuildingKind::Barracks,
            research: None,
            siege_kind: None,
        }
    }

    fn animals() -> Vec<UnitSpec> {
        vec![
            animal("rat", 25, 20),
            animal("wolf", 80, 70),
            animal("bear", 250, 140),
            animal("elephant", 600, 440),
        ]
    }

    // AC1: deterministic from seed + coordinate; empty roster ⇒ empty garrison.
    #[test]
    fn garrison_is_deterministic() {
        let a = animals();
        let c = Coordinate::new(7, -3);
        let g1 = oasis_garrison(42, c, &a, &rules());
        let g2 = oasis_garrison(42, c, &a, &rules());
        assert_eq!(g1, g2);
        assert!(!g1.is_empty());
        // A different seed (usually) changes the animal tier or count somewhere nearby.
        let mut differ = 0;
        for x in 0..40 {
            let c = Coordinate::new(x, 0);
            if oasis_garrison(1, c, &a, &rules()) != oasis_garrison(2, c, &a, &rules()) {
                differ += 1;
            }
        }
        assert!(differ > 0, "seed had no effect");
        assert!(oasis_garrison(42, c, &[], &rules()).is_empty());
    }

    // AC1: the garrison grows with distance and is capped; far oases hold stronger animals.
    #[test]
    fn garrison_scales_with_distance() {
        let a = animals();
        let near = oasis_garrison(42, Coordinate::new(1, 0), &a, &rules());
        let far = oasis_garrison(42, Coordinate::new(40, 0), &a, &rules());
        let count = |g: &UnitCounts| g.iter().map(|(_, n)| *n).sum::<u32>();
        assert!(
            count(&far) >= count(&near),
            "far should hold at least as many"
        );
        // The cap holds at the edge.
        let edge = oasis_garrison(42, Coordinate::new(200, 0), &a, &rules());
        assert!(count(&edge) <= rules().max_count);
        // The far oasis's animal is a higher (stronger) roster tier than the near one.
        let tier_of = |g: &UnitCounts| {
            a.iter()
                .position(|s| g.iter().any(|(id, _)| *id == s.id))
                .unwrap()
        };
        assert!(tier_of(&far) >= tier_of(&near));
    }

    // AC9: a cleared oasis regrows toward the seeded strength, one step at a time, then stops.
    #[test]
    fn regrow_tops_up_toward_seeded() {
        let seeded = vec![(UnitId("wolf".into()), 5)];
        // From empty (cleared): each step adds `per_step`, capped at the target.
        let (g1, full1) = regrow_step(&Vec::new(), &seeded, 2);
        assert_eq!(g1, vec![(UnitId("wolf".into()), 2)]);
        assert!(!full1);
        let (g2, full2) = regrow_step(&g1, &seeded, 2);
        assert_eq!(g2, vec![(UnitId("wolf".into()), 4)]);
        assert!(!full2);
        let (g3, full3) = regrow_step(&g2, &seeded, 2);
        assert_eq!(
            g3,
            vec![(UnitId("wolf".into()), 5)],
            "caps at the seeded count"
        );
        assert!(full3, "reaching the seeded strength stops the regrow");
        // An empty seeded roster never regrows.
        assert_eq!(regrow_step(&Vec::new(), &Vec::new(), 2), (Vec::new(), true));
    }
}
