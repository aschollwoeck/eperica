//! Combat resolution (GDD §9) — the pure battle formula. Power is split into infantry/cavalry pools,
//! the defender's blended defence is multiplied by the (ram-reduced) Wall bonus, morale dampens a
//! much larger attacker, and seeded luck perturbs the comparison; **power-law** casualties then fall
//! on both sides. Everything here is pure over numbers + injected [`CombatRules`] (P3); the
//! application layer assembles the inputs from persisted state and applies the results.

use crate::economy::ResourceAmounts;
use crate::units::{SiegeKind, UnitCounts, UnitId, UnitRole, UnitSpec};
use crate::village::Tribe;
use std::collections::HashMap;

/// How a battle settles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttackMode {
    /// Fight to destroy — the loser loses **all** participating troops.
    Attack,
    /// Fight to plunder — **both** sides take proportional losses; survivors remain.
    Raid,
}

/// Per-tribe Wall balance.
#[derive(Debug, Clone)]
pub struct WallProfile {
    /// Defence-multiplier **bonus** by Wall level (index = level; e.g. `0.03` ⇒ +3 %). Clamped to
    /// the last entry beyond the table.
    pub bonus_per_level: Vec<f64>,
    /// Ram attack power needed to raze **one** Wall level (durability; tribe-flavoured).
    pub ram_durability: f64,
}

impl WallProfile {
    fn bonus(&self, level: u8) -> f64 {
        if self.bonus_per_level.is_empty() {
            return 0.0;
        }
        let idx = (level as usize).min(self.bonus_per_level.len() - 1);
        self.bonus_per_level[idx]
    }
}

/// All combat balance data (P7).
#[derive(Debug, Clone)]
pub struct CombatRules {
    /// Power-law casualty exponent `k`.
    pub loss_exponent: f64,
    /// Luck spread `L`: the factor lies in `[1−L, 1+L]`.
    pub luck_range: f64,
    /// Morale exponent `e`: `morale = min(1, (defPop/atkPop)^e)`.
    pub morale_exponent: f64,
    /// A village's small innate defence (before the Wall multiplier).
    pub base_defense: f64,
    /// Combat-strength bonus per Smithy level (e.g. `0.015` ⇒ +1.5 %/level).
    pub smithy_bonus_per_level: f64,
    /// Catapult attack power needed to raze **one** level of a targeted building (011).
    pub catapult_durability: f64,
    /// Fraction of a Cranny's protection a **Teuton** attacker ignores when looting (011, `0..=1`).
    pub cranny_bypass_teuton: f64,
    /// The quantity of **each** resource a Cranny hides from looting, by level (index = level; 011).
    pub cranny_protection_per_level: Vec<i64>,
    /// Per-tribe Wall profiles.
    pub walls: HashMap<Tribe, WallProfile>,
}

impl CombatRules {
    fn wall(&self, tribe: Tribe) -> Option<&WallProfile> {
        self.walls.get(&tribe)
    }

    /// The per-resource quantity a Cranny of `level` hides from looting (clamped to the table; an
    /// empty table or level 0 protects nothing).
    pub fn cranny_capacity(&self, level: u8) -> i64 {
        if self.cranny_protection_per_level.is_empty() {
            return 0;
        }
        let idx = (level as usize).min(self.cranny_protection_per_level.len() - 1);
        self.cranny_protection_per_level[idx]
    }

    /// The combat-strength multiplier for a unit upgraded to Smithy `level`.
    pub fn smithy_factor(&self, level: u8) -> f64 {
        1.0 + f64::from(level) * self.smithy_bonus_per_level
    }
}

/// A side's split attack power (Smithy-scaled), with ram force tracked separately for the Wall.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AttackPower {
    /// Infantry-class attack pool (infantry + catapults).
    pub infantry: f64,
    /// Cavalry-class attack pool.
    pub cavalry: f64,
    /// Ram attack force (razes the Wall; does not fight the main battle).
    pub ram: f64,
}

/// The assembled inputs to one battle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BattleInput {
    /// Attacker power pools.
    pub attack: AttackPower,
    /// Defender total `Σ count·defInfantry` (Smithy-scaled).
    pub def_infantry: f64,
    /// Defender total `Σ count·defCavalry` (Smithy-scaled).
    pub def_cavalry: f64,
    /// The defender village's tribe (selects the Wall profile).
    pub wall_tribe: Tribe,
    /// The defender's Wall level.
    pub wall_level: u8,
    /// Attacker population (for morale).
    pub attacker_pop: i64,
    /// Defender population (for morale).
    pub defender_pop: i64,
}

/// The resolved outcome: loss fractions per side, Wall damage, and the modifiers that applied.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BattleOutcome {
    /// Whether the attacker prevailed (power ≥ defence).
    pub attacker_won: bool,
    /// Fraction of the attacker's troops lost (`0..=1`).
    pub attacker_loss_frac: f64,
    /// Fraction of every defender's troops lost (`0..=1`).
    pub defender_loss_frac: f64,
    /// The Wall level before the battle.
    pub wall_before: u8,
    /// The Wall level after rams razed it.
    pub wall_after: u8,
    /// The luck factor applied (`[1−L, 1+L]`).
    pub luck: f64,
    /// The morale factor applied (`≤ 1`).
    pub morale: f64,
}

fn splitmix64(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// A bounded luck factor in `[1−range, 1+range]`, deterministic from the world seed and the movement
/// id (P6) — never wall-clock or online state.
pub fn luck_factor(world_seed: u64, movement_id: u128, range: f64) -> f64 {
    let lo = movement_id as u64;
    let hi = (movement_id >> 64) as u64;
    let mix = splitmix64(world_seed ^ splitmix64(lo) ^ splitmix64(hi.rotate_left(32)));
    let unit = (mix >> 11) as f64 / (1u64 << 53) as f64; // [0, 1)
    1.0 - range + unit * 2.0 * range
}

/// Split an attacking composition into infantry/cavalry/ram power, Smithy-scaled by `levels`.
/// **Scout**-role units are excluded from the main battle (010); **rams** feed only the ram pool,
/// **catapults** fight as infantry (their building damage lands in 011).
pub fn attack_power(
    troops: &UnitCounts,
    roster: &[UnitSpec],
    levels: &[(UnitId, u8)],
    rules: &CombatRules,
) -> AttackPower {
    let mut power = AttackPower {
        infantry: 0.0,
        cavalry: 0.0,
        ram: 0.0,
    };
    for (id, count) in troops {
        let Some(spec) = roster.iter().find(|s| &s.id == id) else {
            continue;
        };
        let level = levels.iter().find(|(u, _)| u == id).map_or(0, |(_, l)| *l);
        let p = f64::from(spec.attack) * f64::from(*count) * rules.smithy_factor(level);
        match spec.role {
            UnitRole::Scout => {} // reconnaissance, resolved separately (010)
            UnitRole::Wild => {}  // oasis animals defend only, never attack (012)
            UnitRole::Cavalry => power.cavalry += p,
            UnitRole::Siege if spec.siege_kind == Some(SiegeKind::Ram) => power.ram += p,
            _ => power.infantry += p,
        }
    }
    power
}

/// Accumulate a defender group's `(Σ count·defInf, Σ count·defCav)` (Smithy-scaled) onto `totals`.
pub fn add_defense(
    totals: &mut (f64, f64),
    defenders: &UnitCounts,
    roster: &[UnitSpec],
    levels: &[(UnitId, u8)],
    rules: &CombatRules,
) {
    for (id, count) in defenders {
        let Some(spec) = roster.iter().find(|s| &s.id == id) else {
            continue;
        };
        if spec.role == UnitRole::Scout {
            continue; // scouts do not defend the main battle (010)
        }
        let level = levels.iter().find(|(u, _)| u == id).map_or(0, |(_, l)| *l);
        let f = rules.smithy_factor(level) * f64::from(*count);
        totals.0 += f64::from(spec.defense_infantry) * f;
        totals.1 += f64::from(spec.defense_cavalry) * f;
    }
}

fn morale_factor(attacker_pop: i64, defender_pop: i64, exponent: f64) -> f64 {
    if attacker_pop <= defender_pop || attacker_pop <= 0 {
        return 1.0; // no dampening unless the attacker is the larger player
    }
    let ratio = defender_pop.max(0) as f64 / attacker_pop as f64;
    ratio.powf(exponent).clamp(0.0, 1.0)
}

/// `(attacker_loss_frac, defender_loss_frac, attacker_won)` for the given powers and mode.
fn casualties(mode: AttackMode, attack: f64, defense: f64, k: f64) -> (f64, f64, bool) {
    if attack <= 0.0 {
        return (1.0, 0.0, false); // no offensive power: attacker wiped, defender untouched
    }
    if defense <= 0.0 {
        return (0.0, 1.0, true); // undefended: attacker unscathed, defender wiped
    }
    let won = attack >= defense;
    match mode {
        AttackMode::Attack => {
            if won {
                ((defense / attack).powf(k), 1.0, true)
            } else {
                (1.0, (attack / defense).powf(k), false)
            }
        }
        AttackMode::Raid => {
            let a = attack.powf(k);
            let d = defense.powf(k);
            (d / (a + d), a / (a + d), won) // each side loses the other's power share
        }
    }
}

/// Resolve a battle to its casualty fractions and Wall damage — pure and deterministic (P2/P6).
pub fn resolve_battle(
    mode: AttackMode,
    input: BattleInput,
    rules: &CombatRules,
    luck: f64,
) -> BattleOutcome {
    let wall = rules.wall(input.wall_tribe);
    let durability = wall.map_or(f64::INFINITY, |w| w.ram_durability.max(1.0));
    let razed = razed_levels(input.attack.ram, durability, input.wall_level);
    let wall_after = input.wall_level - razed;
    let wall_bonus = wall.map_or(0.0, |w| w.bonus(wall_after));

    let total_attack = input.attack.infantry + input.attack.cavalry;
    let (inf_share, cav_share) = if total_attack > 0.0 {
        (
            input.attack.infantry / total_attack,
            input.attack.cavalry / total_attack,
        )
    } else {
        (1.0, 0.0)
    };
    let blended_def = inf_share * input.def_infantry + cav_share * input.def_cavalry;
    let defense = (blended_def + rules.base_defense) * (1.0 + wall_bonus);

    let morale = morale_factor(
        input.attacker_pop,
        input.defender_pop,
        rules.morale_exponent,
    );
    let attack = total_attack * morale * luck;

    let (attacker_loss_frac, defender_loss_frac, attacker_won) =
        casualties(mode, attack, defense, rules.loss_exponent);

    BattleOutcome {
        attacker_won,
        attacker_loss_frac,
        defender_loss_frac,
        wall_before: input.wall_level,
        wall_after,
        luck,
        morale,
    }
}

/// Apply a loss fraction to a composition, returning `(survivors, losses)` with deterministic
/// rounding — **round half to even** (banker's rounding, spec Decision), never more than the stack.
pub fn apply_losses(counts: &UnitCounts, frac: f64) -> (UnitCounts, UnitCounts) {
    let frac = frac.clamp(0.0, 1.0);
    let mut survivors = Vec::new();
    let mut losses = Vec::new();
    for (id, count) in counts {
        let lost = ((f64::from(*count) * frac).round_ties_even() as u32).min(*count);
        let surviving = count - lost;
        if surviving > 0 {
            survivors.push((id.clone(), surviving));
        }
        if lost > 0 {
            losses.push((id.clone(), lost));
        }
    }
    (survivors, losses)
}

// ---------------------------------------------------------------- siege & loot (011)

/// Whole levels razed by `power` against a structure of `durability` per level, capped at `level`.
/// Shared by rams→Wall (009) and catapults→building (011); a non-positive/infinite case razes 0.
pub fn razed_levels(power: f64, durability: f64, level: u8) -> u8 {
    if power <= 0.0 || !durability.is_finite() || durability <= 0.0 {
        return 0;
    }
    let n = (power / durability).floor();
    if n >= f64::from(level) {
        level
    } else {
        n as u8
    }
}

/// Total catapult attack power of a composition (Smithy-scaled) — `Σ count·attack·smithy` over
/// **`Catapult`** siege units only. Used on the **surviving** attackers for building damage (011).
pub fn catapult_power(
    troops: &UnitCounts,
    roster: &[UnitSpec],
    levels: &[(UnitId, u8)],
    rules: &CombatRules,
) -> f64 {
    troops
        .iter()
        .filter_map(|(id, count)| {
            roster
                .iter()
                .find(|s| &s.id == id)
                .filter(|s| s.siege_kind == Some(SiegeKind::Catapult))
                .map(|s| {
                    let level = levels.iter().find(|(u, _)| u == id).map_or(0, |(_, l)| *l);
                    f64::from(s.attack) * f64::from(*count) * rules.smithy_factor(level)
                })
        })
        .sum()
}

/// Total carry capacity of a composition — `Σ count·carryCapacity` (011 loot bound).
pub fn carry_capacity_total(troops: &UnitCounts, roster: &[UnitSpec]) -> u64 {
    troops
        .iter()
        .filter_map(|(id, count)| {
            roster
                .iter()
                .find(|s| &s.id == id)
                .map(|s| u64::from(s.carry_capacity) * u64::from(*count))
        })
        .sum()
}

/// The per-resource amount a Cranny of `level_capacity` shields from loot — reduced by the configured
/// **bypass** fraction when the attacker is a **Teuton** (011, GDD §5.2).
pub fn cranny_protection(level_capacity: i64, is_teuton: bool, bypass: f64) -> i64 {
    if level_capacity <= 0 {
        return 0;
    }
    if is_teuton {
        let kept = (1.0 - bypass.clamp(0.0, 1.0)).max(0.0);
        (level_capacity as f64 * kept).floor() as i64
    } else {
        level_capacity
    }
}

/// Split the loot an army carries off: per resource `lootable = max(0, stored − protection)`; the
/// **total** taken is `min(Σ lootable, capacity)`, distributed across the four resources **in
/// proportion** to each one's lootable share (round-half-to-even; the rounding remainder is placed on
/// the largest-lootable types so `Σ loot == total` and `0 ≤ loot ≤ lootable`). Pure (011 AC3).
pub fn loot_split(
    stored: ResourceAmounts,
    protection: ResourceAmounts,
    capacity: u64,
) -> ResourceAmounts {
    let lootable = [
        (stored.wood - protection.wood).max(0),
        (stored.clay - protection.clay).max(0),
        (stored.iron - protection.iron).max(0),
        (stored.crop - protection.crop).max(0),
    ];
    let total_lootable: i64 = lootable.iter().sum();
    let cap = i64::try_from(capacity).unwrap_or(i64::MAX);
    let total = total_lootable.min(cap);
    let bundle = |a: [i64; 4]| ResourceAmounts {
        wood: a[0],
        clay: a[1],
        iron: a[2],
        crop: a[3],
    };
    if total <= 0 {
        return bundle([0; 4]);
    }
    if total == total_lootable {
        return bundle(lootable); // capacity covers everything outside the Cranny
    }
    // Proportional shares of `total`, rounded; then fix the remainder deterministically.
    let mut loot = [0i64; 4];
    for i in 0..4 {
        let exact = lootable[i] as f64 * total as f64 / total_lootable as f64;
        loot[i] = exact.round_ties_even() as i64;
    }
    let mut diff = total - loot.iter().sum::<i64>();
    // Adjust one unit at a time, largest-lootable first (ties by index), respecting `[0, lootable]`.
    let mut order: Vec<usize> = (0..4).collect();
    order.sort_by(|&a, &b| lootable[b].cmp(&lootable[a]).then(a.cmp(&b)));
    let step = if diff > 0 { 1 } else { -1 };
    let mut guard = 0;
    let mut oi = 0;
    while diff != 0 && guard < 256 {
        let i = order[oi % 4];
        let next = loot[i] + step;
        if (0..=lootable[i]).contains(&next) {
            loot[i] = next;
            diff -= step;
        }
        oi += 1;
        guard += 1;
    }
    bundle(loot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::building::BuildingKind;
    use crate::economy::ResourceAmounts;
    use crate::units::UnitRole;

    fn amounts(n: i64) -> ResourceAmounts {
        ResourceAmounts {
            wood: n,
            clay: n,
            iron: n,
            crop: n,
        }
    }

    fn unit(
        id: &str,
        role: UnitRole,
        attack: u32,
        di: u32,
        dc: u32,
        siege: Option<SiegeKind>,
    ) -> UnitSpec {
        UnitSpec {
            id: UnitId(id.to_owned()),
            name: id.to_owned(),
            role,
            attack,
            defense_infantry: di,
            defense_cavalry: dc,
            scouting: 0,
            speed: 6,
            carry_capacity: 0,
            crop_upkeep: 1,
            cost: amounts(1),
            train_secs: 1,
            trained_in: BuildingKind::Barracks,
            research: None,
            siege_kind: siege,
        }
    }

    fn rules() -> CombatRules {
        CombatRules {
            loss_exponent: 1.5,
            luck_range: 0.25,
            morale_exponent: 0.2,
            base_defense: 10.0,
            smithy_bonus_per_level: 0.015,
            catapult_durability: 100.0,
            cranny_bypass_teuton: 0.5,
            cranny_protection_per_level: vec![0, 1000, 2000],
            walls: HashMap::from([
                (
                    Tribe::Gauls,
                    WallProfile {
                        bonus_per_level: vec![0.0, 0.03, 0.06, 0.09, 0.12, 0.15],
                        ram_durability: 100.0,
                    },
                ),
                (
                    Tribe::Romans,
                    WallProfile {
                        bonus_per_level: vec![0.0, 0.03, 0.06, 0.09, 0.12, 0.15],
                        ram_durability: 100.0,
                    },
                ),
                (
                    Tribe::Teutons,
                    WallProfile {
                        bonus_per_level: vec![0.0, 0.02, 0.04, 0.06, 0.08, 0.10],
                        ram_durability: 200.0,
                    },
                ),
            ]),
        }
    }

    fn input(attack: AttackPower, def_i: f64, def_c: f64, wall: u8) -> BattleInput {
        BattleInput {
            attack,
            def_infantry: def_i,
            def_cavalry: def_c,
            wall_tribe: Tribe::Gauls,
            wall_level: wall,
            attacker_pop: 100,
            defender_pop: 100,
        }
    }

    fn pwr(inf: f64, cav: f64, ram: f64) -> AttackPower {
        AttackPower {
            infantry: inf,
            cavalry: cav,
            ram,
        }
    }

    // AC4: a much stronger attack wipes the defender and bleeds little (attack mode).
    #[test]
    fn strong_attack_wipes_defender() {
        let o = resolve_battle(
            AttackMode::Attack,
            input(pwr(1000.0, 0.0, 0.0), 100.0, 0.0, 0),
            &rules(),
            1.0,
        );
        assert!(o.attacker_won);
        assert_eq!(o.defender_loss_frac, 1.0);
        assert!(
            o.attacker_loss_frac < 0.1,
            "attacker lost {}",
            o.attacker_loss_frac
        );
    }

    // AC4: a weaker attack is annihilated; the defender bleeds a fraction.
    #[test]
    fn weak_attack_is_annihilated() {
        let o = resolve_battle(
            AttackMode::Attack,
            input(pwr(100.0, 0.0, 0.0), 1000.0, 0.0, 0),
            &rules(),
            1.0,
        );
        assert!(!o.attacker_won);
        assert_eq!(o.attacker_loss_frac, 1.0);
        assert!(o.defender_loss_frac > 0.0 && o.defender_loss_frac < 1.0);
    }

    // AC4: a raid bleeds both sides; the stronger side loses less.
    #[test]
    fn raid_bleeds_both_sides() {
        let o = resolve_battle(
            AttackMode::Raid,
            input(pwr(1000.0, 0.0, 0.0), 250.0, 0.0, 0),
            &rules(),
            1.0,
        );
        assert!(o.attacker_won);
        assert!(o.attacker_loss_frac > 0.0 && o.attacker_loss_frac < o.defender_loss_frac);
        assert!(o.defender_loss_frac < 1.0); // survivors remain
    }

    // AC4: the inf/cav split picks the matching defence — cavalry attack faces def-cavalry.
    #[test]
    fn split_uses_matching_defense() {
        // Defender is strong vs infantry, weak vs cavalry. A pure-cavalry attack faces the weak side.
        let vs_cav = resolve_battle(
            AttackMode::Attack,
            input(pwr(0.0, 400.0, 0.0), 5000.0, 50.0, 0),
            &rules(),
            1.0,
        );
        let vs_inf = resolve_battle(
            AttackMode::Attack,
            input(pwr(400.0, 0.0, 0.0), 5000.0, 50.0, 0),
            &rules(),
            1.0,
        );
        assert!(vs_cav.attacker_won && !vs_inf.attacker_won);
    }

    // AC5: a Wall raises defender power (more attacker losses, fewer defender losses).
    #[test]
    fn wall_strengthens_the_defender() {
        let no_wall = resolve_battle(
            AttackMode::Raid,
            input(pwr(500.0, 0.0, 0.0), 400.0, 0.0, 0),
            &rules(),
            1.0,
        );
        let walled = resolve_battle(
            AttackMode::Raid,
            input(pwr(500.0, 0.0, 0.0), 400.0, 0.0, 5),
            &rules(),
            1.0,
        );
        assert!(walled.attacker_loss_frac > no_wall.attacker_loss_frac);
        assert!(walled.defender_loss_frac < no_wall.defender_loss_frac);
    }

    // AC5: rams reduce the effective Wall level; enough force razes it to 0.
    #[test]
    fn rams_raze_the_wall() {
        // durability 100 ⇒ 550 ram power razes 5 levels (the whole level-5 wall).
        let o = resolve_battle(
            AttackMode::Raid,
            input(pwr(500.0, 0.0, 550.0), 400.0, 0.0, 5),
            &rules(),
            1.0,
        );
        assert_eq!(o.wall_before, 5);
        assert_eq!(o.wall_after, 0);
        // partial: 250 ram power razes 2 levels (5 → 3).
        let p = resolve_battle(
            AttackMode::Raid,
            input(pwr(500.0, 0.0, 250.0), 400.0, 0.0, 5),
            &rules(),
            1.0,
        );
        assert_eq!(p.wall_after, 3);
    }

    // AC4: morale dampens a much larger attacker.
    #[test]
    fn morale_dampens_a_large_attacker() {
        let mut big = input(pwr(1000.0, 0.0, 0.0), 800.0, 0.0, 0);
        big.attacker_pop = 10_000;
        big.defender_pop = 100;
        let damp = resolve_battle(AttackMode::Attack, big, &rules(), 1.0);
        let even = resolve_battle(
            AttackMode::Attack,
            input(pwr(1000.0, 0.0, 0.0), 800.0, 0.0, 0),
            &rules(),
            1.0,
        );
        assert!(damp.morale < 1.0);
        // The dampened attacker fares worse than the equal-population one.
        assert!(damp.attacker_loss_frac >= even.attacker_loss_frac);
    }

    // AC3: same inputs + luck ⇒ identical outcome; luck stays within range.
    #[test]
    fn resolution_is_deterministic_and_luck_bounded() {
        let i = input(pwr(500.0, 0.0, 0.0), 400.0, 0.0, 2);
        let a = resolve_battle(AttackMode::Attack, i, &rules(), 1.1);
        let b = resolve_battle(AttackMode::Attack, i, &rules(), 1.1);
        assert_eq!(a, b);
        for id in 0..200u128 {
            let l = luck_factor(42, id.wrapping_mul(0x9E37_79B9), 0.25);
            assert!((0.75..=1.25).contains(&l), "luck {l} out of range");
        }
        // Same seed + id is stable.
        assert_eq!(luck_factor(7, 99, 0.25), luck_factor(7, 99, 0.25));
    }

    #[test]
    fn attack_power_splits_roles_and_ram() {
        let roster = vec![
            unit("sword", UnitRole::Infantry, 50, 35, 20, None),
            unit("knight", UnitRole::Cavalry, 120, 30, 40, None),
            unit("ram", UnitRole::Siege, 60, 30, 75, Some(SiegeKind::Ram)),
            unit(
                "cat",
                UnitRole::Siege,
                75,
                60,
                10,
                Some(SiegeKind::Catapult),
            ),
            unit("scout", UnitRole::Scout, 35, 10, 5, None),
        ];
        let troops = vec![
            (UnitId("sword".into()), 10),
            (UnitId("knight".into()), 5),
            (UnitId("ram".into()), 2),
            (UnitId("cat".into()), 4),
            (UnitId("scout".into()), 7), // excluded
        ];
        let p = attack_power(&troops, &roster, &[], &rules());
        assert_eq!(p.infantry, 50.0 * 10.0 + 75.0 * 4.0); // swords + catapults
        assert_eq!(p.cavalry, 120.0 * 5.0);
        assert_eq!(p.ram, 60.0 * 2.0);
    }

    // 014 AC2: an administrator (the Expansion-role conqueror) **fights** — it contributes attack and
    // defence in the main battle, unlike a Scout (reconnaissance, resolved separately).
    #[test]
    fn administrators_are_combatants() {
        let roster = vec![unit("senator", UnitRole::Expansion, 50, 40, 30, None)];
        let troops = vec![(UnitId("senator".into()), 3)];
        // Attack: the administrator adds to infantry power.
        let p = attack_power(&troops, &roster, &[], &rules());
        assert_eq!(p.infantry, 50.0 * 3.0);
        // Defence: the administrator defends (both infantry and cavalry defence accumulate).
        let mut totals = (0.0, 0.0);
        add_defense(&mut totals, &troops, &roster, &[], &rules());
        assert_eq!(totals, (40.0 * 3.0, 30.0 * 3.0));
    }

    #[test]
    fn apply_losses_rounds_half_to_even_and_conserves() {
        // Round half to even (banker's). The discriminating ties: 5×0.5 = 2.5 → 2 (down to even;
        // away-from-zero would give 3) and 9×0.5 = 4.5 → 4 (down to even; away-from-zero gives 5).
        let counts = vec![
            (UnitId("a".into()), 5),
            (UnitId("b".into()), 9),
            (UnitId("c".into()), 3),
        ];
        let (surv, lost) = apply_losses(&counts, 0.5);
        assert_eq!(
            lost,
            vec![
                (UnitId("a".into()), 2),
                (UnitId("b".into()), 4),
                (UnitId("c".into()), 2),
            ]
        );
        // Survivors + losses reconstruct each stack.
        for (id, total) in &counts {
            let s = surv.iter().find(|(u, _)| u == id).map_or(0, |(_, n)| *n);
            let l = lost.iter().find(|(u, _)| u == id).map_or(0, |(_, n)| *n);
            assert_eq!(s + l, *total);
        }
        // Total wipe and zero loss.
        assert_eq!(apply_losses(&counts, 1.0).0, Vec::new());
        assert_eq!(apply_losses(&counts, 0.0).1, Vec::new());
    }

    fn res(w: i64, c: i64, i: i64, cr: i64) -> ResourceAmounts {
        ResourceAmounts {
            wood: w,
            clay: c,
            iron: i,
            crop: cr,
        }
    }

    // AC2: razed_levels floors and caps; degenerate inputs raze nothing.
    #[test]
    fn razed_levels_floors_and_caps() {
        assert_eq!(razed_levels(550.0, 100.0, 5), 5); // 5.5 floored → 5, capped at 5
        assert_eq!(razed_levels(250.0, 100.0, 5), 2); // 2.5 floored → 2
        assert_eq!(razed_levels(99.0, 100.0, 5), 0);
        assert_eq!(razed_levels(10_000.0, 100.0, 3), 3); // capped at the level
        assert_eq!(razed_levels(0.0, 100.0, 5), 0);
        assert_eq!(razed_levels(500.0, f64::INFINITY, 5), 0); // no profile ⇒ no razing
    }

    // AC2: catapult_power sums only catapults (Smithy-scaled); rams/other roles excluded.
    #[test]
    fn catapult_power_counts_catapults_only() {
        let roster = vec![
            unit(
                "cat",
                UnitRole::Siege,
                75,
                60,
                10,
                Some(SiegeKind::Catapult),
            ),
            unit("ram", UnitRole::Siege, 60, 30, 75, Some(SiegeKind::Ram)),
            unit("sword", UnitRole::Infantry, 50, 35, 20, None),
        ];
        let troops = vec![
            (UnitId("cat".into()), 4),
            (UnitId("ram".into()), 3),
            (UnitId("sword".into()), 10),
        ];
        assert_eq!(catapult_power(&troops, &roster, &[], &rules()), 75.0 * 4.0);
        // Smithy level scales it.
        let lv = [(UnitId("cat".into()), 10u8)];
        let scaled = catapult_power(&troops, &roster, &lv, &rules());
        assert!(scaled > 75.0 * 4.0);
    }

    // AC3: carry_capacity_total sums count·carryCapacity.
    #[test]
    fn carry_capacity_sums() {
        let mut a = unit("a", UnitRole::Infantry, 1, 1, 1, None);
        a.carry_capacity = 50;
        let mut b = unit("b", UnitRole::Cavalry, 1, 1, 1, None);
        b.carry_capacity = 100;
        let roster = vec![a, b];
        let troops = vec![(UnitId("a".into()), 10), (UnitId("b".into()), 3)];
        assert_eq!(carry_capacity_total(&troops, &roster), 50 * 10 + 100 * 3);
    }

    // AC5: a Teuton attacker faces less Cranny protection than others; level 0 protects nothing.
    #[test]
    fn cranny_protection_teuton_bypass() {
        assert_eq!(cranny_protection(2000, false, 0.5), 2000);
        assert_eq!(cranny_protection(2000, true, 0.5), 1000); // half bypassed
        assert_eq!(cranny_protection(0, false, 0.5), 0);
        assert_eq!(cranny_protection(2000, true, 1.0), 0); // full bypass
    }

    // AC3/AC4: loot is bounded by capacity, floored by the Cranny, proportional, and conserved.
    #[test]
    fn loot_split_is_bounded_proportional_and_conserved() {
        // No Cranny, capacity covers everything → take it all.
        let all = loot_split(res(1000, 800, 600, 400), res(0, 0, 0, 0), 10_000);
        assert_eq!(all, res(1000, 800, 600, 400));

        // Cranny shields 500 of each → lootable (500,300,100,0)=900; capacity 10000 → take all surplus.
        let shielded = loot_split(res(1000, 800, 600, 400), res(500, 500, 500, 500), 10_000);
        assert_eq!(shielded, res(500, 300, 100, 0));

        // Capacity-bound: lootable 900 but capacity 300 → exactly 300 taken, proportional, conserved.
        let capped = loot_split(res(1000, 800, 600, 400), res(500, 500, 500, 500), 300);
        let total: i64 = capped.wood + capped.clay + capped.iron + capped.crop;
        assert_eq!(total, 300);
        assert!(capped.wood >= capped.clay && capped.clay >= capped.iron); // largest share leads
        // Never exceeds the lootable surplus per type.
        assert!(capped.wood <= 500 && capped.clay <= 300 && capped.iron <= 100 && capped.crop == 0);

        // Nothing lootable (everything shielded) or zero capacity → no loot.
        assert_eq!(
            loot_split(res(100, 100, 100, 100), res(100, 100, 100, 100), 9999),
            res(0, 0, 0, 0)
        );
        assert_eq!(
            loot_split(res(1000, 1000, 1000, 1000), res(0, 0, 0, 0), 0),
            res(0, 0, 0, 0)
        );

        // AC8: deterministic — same inputs, same split.
        assert_eq!(
            loot_split(res(1000, 800, 600, 400), res(500, 500, 500, 500), 300),
            capped
        );
    }
}
