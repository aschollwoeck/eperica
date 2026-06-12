//! Culture points & expansion gating (GDD §11.1, §3.3) — the pure rules behind multi-village growth.
//!
//! Culture points (CP) are a **per-player** accumulator produced over time by the player's buildings
//! (chiefly the **Town Hall**, plus a small base per village). Like resources (002), CP is **lazy**:
//! stored as `value + rate + lastUpdated` and computed on read (P1) — there is no global tick. CP is
//! never *spent*; it is a **threshold gate** on how many villages a player may hold. How many villages
//! are actually allowed is the **minimum** of what CP permits and what the player's **Residence/Palace**
//! buildings grant. Everything here is pure over numbers + injected [`CultureRules`] (P3).

/// Balance for culture points + expansion (P7).
#[derive(Debug, Clone)]
pub struct CultureRules {
    /// Base culture points per hour every village contributes (even with no Town Hall).
    pub base_cp_per_village: i64,
    /// Culture points per hour a **Town Hall** adds, by its level (index = level; clamps to the last).
    pub town_hall_cp_per_level: Vec<i64>,
    /// Cumulative CP needed to be **allowed** the `n`-th village (index = village count; `[0]` unused,
    /// `[1] = 0` so the first village is free). A rising table gating expansion pace.
    pub cp_thresholds: Vec<i64>,
    /// Expansion slots a single **Residence/Palace** grants, by its level (index = level).
    pub expansion_slots_per_level: Vec<u32>,
    /// Settlers consumed to found a new village.
    pub settlers_per_village: u32,
}

/// Clamp-to-last table lookup (level beyond the table reuses the last entry).
fn at_level(table: &[i64], level: u8) -> i64 {
    if table.is_empty() {
        return 0;
    }
    let idx = (level as usize).min(table.len() - 1);
    table[idx]
}

impl CultureRules {
    /// The CP/hour a Town Hall of `level` adds (level 0 ⇒ none).
    #[must_use]
    pub fn town_hall_cp(&self, level: u8) -> i64 {
        at_level(&self.town_hall_cp_per_level, level)
    }

    /// Expansion slots a Residence/Palace of `level` grants (clamped to the table).
    #[must_use]
    pub fn slots_at(&self, level: u8) -> u32 {
        if self.expansion_slots_per_level.is_empty() {
            return 0;
        }
        let idx = (level as usize).min(self.expansion_slots_per_level.len() - 1);
        self.expansion_slots_per_level[idx]
    }
}

/// The player's total CP/hour: each village contributes the base plus its Town Hall's output (012-style
/// per-village sum). `town_hall_levels` holds one entry per owned village (0 where there is no Town Hall).
#[must_use]
pub fn culture_rate(town_hall_levels: &[u8], rules: &CultureRules) -> i64 {
    town_hall_levels
        .iter()
        .map(|&l| rules.base_cp_per_village + rules.town_hall_cp(l))
        .sum()
}

/// Accrue the culture accumulator forward: `value + rate·elapsed/3600`, floored at 0. **Uncapped** —
/// CP only ever grows toward thresholds (P1, the 002 accrue without a storage cap).
#[must_use]
pub fn settle_value(value: i64, rate_per_hour: i64, elapsed_secs: i64) -> i64 {
    let delta = rate_per_hour.saturating_mul(elapsed_secs.max(0)) / 3600;
    value.saturating_add(delta).max(0)
}

/// How many villages the player's **culture points** permit: the largest `n` whose
/// `cp_thresholds[n] ≤ cp` (the first village is free, `cp_thresholds[1] = 0`).
#[must_use]
pub fn cp_allows(cp: i64, rules: &CultureRules) -> u32 {
    let mut allowed = 0;
    for (n, &threshold) in rules.cp_thresholds.iter().enumerate() {
        if n >= 1 && threshold <= cp {
            allowed = n as u32;
        }
    }
    allowed
}

/// The total expansion slots the player's Residence/Palace buildings grant: the per-building slot
/// count summed over `residence_levels` (one entry per Residence/Palace the player holds).
#[must_use]
pub fn expansion_slots(residence_levels: &[u8], rules: &CultureRules) -> u32 {
    residence_levels.iter().map(|&l| rules.slots_at(l)).sum()
}

/// How many villages a player may hold: the **minimum** of what their CP permits and what their
/// Residence/Palace buildings grant — **plus the always-present home village** (which needs no
/// Residence). Founding is allowed only while `villageCount < allowedVillages` (AC4, P4).
#[must_use]
pub fn allowed_villages(cp: i64, residence_levels: &[u8], rules: &CultureRules) -> u32 {
    let by_cp = cp_allows(cp, rules);
    let by_buildings = 1 + expansion_slots(residence_levels, rules);
    by_cp.min(by_buildings)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> CultureRules {
        CultureRules {
            base_cp_per_village: 2,
            town_hall_cp_per_level: vec![0, 5, 8, 12], // level 0 = none
            cp_thresholds: vec![0, 0, 200, 500, 1000], // [1]=0 free; 2nd=200; 3rd=500; 4th=1000
            expansion_slots_per_level: vec![0, 1, 1, 2, 2, 3],
            settlers_per_village: 3,
        }
    }

    // AC1/AC2: the rate sums a per-village base plus each Town Hall's output, clamping to the table.
    #[test]
    fn rate_sums_base_and_town_halls() {
        let r = rules();
        // Two villages: one with no Town Hall (base 2 only), one at level 2 (base 2 + TH 8).
        assert_eq!(culture_rate(&[0, 2], &r), 2 + (2 + 8));
        // No villages ⇒ no rate.
        assert_eq!(culture_rate(&[], &r), 0);
        // A level beyond the table reuses the last entry (12).
        assert_eq!(culture_rate(&[9], &r), 2 + 12);
    }

    // AC1: CP accrues lazily and never goes negative; it is uncapped.
    #[test]
    fn culture_accrues_uncapped() {
        // 100 CP/h for 2 hours from 50 ⇒ 250 (no cap).
        assert_eq!(settle_value(50, 100, 7200), 250);
        // A zero/negative elapsed leaves it unchanged.
        assert_eq!(settle_value(50, 100, 0), 50);
        assert_eq!(settle_value(50, 100, -10), 50);
    }

    // AC4: the CP gate returns the largest village count the thresholds permit.
    #[test]
    fn cp_allows_rises_with_thresholds() {
        let r = rules();
        assert_eq!(cp_allows(0, &r), 1); // first village free
        assert_eq!(cp_allows(199, &r), 1);
        assert_eq!(cp_allows(200, &r), 2);
        assert_eq!(cp_allows(999, &r), 3);
        assert_eq!(cp_allows(100_000, &r), 4); // capped at the table length
    }

    // AC3/AC4: allowed villages is the min of the CP gate and the building (Residence/Palace) capacity,
    // plus the always-present home village.
    #[test]
    fn allowed_villages_is_min_of_cp_and_buildings() {
        let r = rules();
        // Plenty of CP (3rd village allowed) but only a level-1 Residence (+1 slot) ⇒ home + 1 = 2.
        assert_eq!(allowed_villages(500, &[1], &r), 2);
        // Plenty of buildings (two Residences = +3) but CP only allows 2 ⇒ 2.
        assert_eq!(allowed_villages(200, &[1, 3], &r), 2);
        // No Residence at all ⇒ just the home village, whatever the CP.
        assert_eq!(allowed_villages(100_000, &[], &r), 1);
        // CP + buildings both ample ⇒ the CP table cap (4).
        assert_eq!(allowed_villages(100_000, &[5, 5], &r), 4);
    }
}
