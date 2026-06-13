//! Loyalty & conquest (GDD §3.4, §6.1, §9.4 step 5) — the pure rules behind the aggressive
//! expansion path.
//!
//! Every village carries a **loyalty** in `[0, MAX_LOYALTY]` (100 = fully loyal). Like resources (002)
//! and culture (013), loyalty is **lazy**: stored as `value + lastUpdated` and computed on read (P1) —
//! there is no global tick. Loyalty **regenerates** toward the maximum over time; it is reduced only by
//! a surviving **administrator** that wins a battle (009), and at zero — with a free expansion slot and
//! a non-capital target — the village is **conquered** (ownership transfers, 014). Everything here is
//! pure over numbers + injected [`LoyaltyRules`] (P3).

use crate::units::UnitId;
use crate::world::GameSpeed;

/// The maximum (and starting) loyalty a village can hold.
pub const MAX_LOYALTY: i64 = 100;

/// Balance for loyalty + conquest (P7).
#[derive(Debug, Clone)]
pub struct LoyaltyRules {
    /// Loyalty a fresh (founded/registered) village starts at — normally [`MAX_LOYALTY`].
    pub starting_loyalty: i64,
    /// Loyalty a **just-conquered** village resets to (so it isn't instantly re-taken).
    pub post_conquest_loyalty: i64,
    /// Loyalty points regenerated per hour at world speed 1× (scaled by speed, P7).
    pub regen_per_hour: i64,
    /// Minimum loyalty a single surviving administrator removes (seeded draw, P6).
    pub drop_min: i64,
    /// Maximum loyalty a single surviving administrator removes (seeded draw, P6).
    pub drop_max: i64,
    /// The unit ids that **conquer** — the tribes' administrators (Senator/Chief/Chieftain). They are
    /// ordinary `Expansion`-role combatants (they fight, unlike settlers); this list is what marks a
    /// surviving unit as an administrator for the loyalty step, without overloading the unit role.
    pub administrator_ids: Vec<String>,
}

impl LoyaltyRules {
    /// Whether `id` is an administrator (a conqueror) per the balance list.
    #[must_use]
    pub fn is_administrator(&self, id: &UnitId) -> bool {
        self.administrator_ids.iter().any(|a| a == id.as_str())
    }
}

/// How many administrators a composition holds — the input to the loyalty drop (013/014). Counts every
/// unit whose id is an administrator id; `0` for an attack carrying none.
#[must_use]
pub fn administrator_count(troops: &[(UnitId, u32)], rules: &LoyaltyRules) -> u32 {
    troops
        .iter()
        .filter(|(id, _)| rules.is_administrator(id))
        .map(|(_, n)| *n)
        .sum()
}

/// Regenerate loyalty toward [`MAX_LOYALTY`] over `elapsed_secs` at the **speed-scaled** rate, clamped
/// to `[0, MAX_LOYALTY]` (the 002 accrue shape with a ceiling). A non-positive elapsed leaves it
/// unchanged; loyalty never exceeds the maximum (AC1/AC9, P1).
#[must_use]
pub fn regenerate_loyalty(
    value: i64,
    elapsed_secs: i64,
    rules: &LoyaltyRules,
    speed: GameSpeed,
) -> i64 {
    let rate = (rules.regen_per_hour as f64 * speed.multiplier()).round() as i64;
    let delta = rate.saturating_mul(elapsed_secs.max(0)) / 3600;
    value.saturating_add(delta).clamp(0, MAX_LOYALTY)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> LoyaltyRules {
        LoyaltyRules {
            starting_loyalty: 100,
            post_conquest_loyalty: 25,
            regen_per_hour: 5,
            drop_min: 20,
            drop_max: 30,
            administrator_ids: vec!["senator".to_owned(), "chieftain".to_owned()],
        }
    }

    // AC2/AC3: administrators are identified by the balance list (not the role), and counted in a
    // composition; settlers and ordinary troops are not administrators.
    #[test]
    fn administrators_are_identified_and_counted() {
        let r = rules();
        assert!(r.is_administrator(&UnitId("senator".to_owned())));
        assert!(r.is_administrator(&UnitId("chieftain".to_owned())));
        assert!(!r.is_administrator(&UnitId("settler".to_owned())));
        assert!(!r.is_administrator(&UnitId("legionnaire".to_owned())));
        let troops = vec![
            (UnitId("legionnaire".to_owned()), 50),
            (UnitId("senator".to_owned()), 2),
        ];
        assert_eq!(administrator_count(&troops, &r), 2);
        assert_eq!(administrator_count(&[], &r), 0);
    }

    // AC1/AC9: loyalty regenerates toward the maximum at the speed-scaled rate and clamps at 100.
    #[test]
    fn loyalty_regenerates_and_clamps() {
        let r = rules();
        let speed = GameSpeed::new(1.0).unwrap();
        // From 50, two hours at 5/h ⇒ 60.
        assert_eq!(regenerate_loyalty(50, 7200, &r, speed), 60);
        // A non-positive elapsed leaves it unchanged.
        assert_eq!(regenerate_loyalty(50, 0, &r, speed), 50);
        assert_eq!(regenerate_loyalty(50, -10, &r, speed), 50);
        // It never exceeds the maximum, however long the elapsed.
        assert_eq!(regenerate_loyalty(98, 36_000, &r, speed), MAX_LOYALTY);
        // A zero value still regenerates up from the floor.
        assert_eq!(regenerate_loyalty(0, 3600, &r, speed), 5);
    }

    // AC1 (P7): world speed scales the regen rate.
    #[test]
    fn speed_scales_the_regen_rate() {
        let r = rules();
        // 5/h × 3 ⇒ 15/h; one hour from 50 ⇒ 65.
        assert_eq!(
            regenerate_loyalty(50, 3600, &r, GameSpeed::new(3.0).unwrap()),
            65
        );
    }
}
