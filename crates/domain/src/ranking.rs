//! Ranking & statistics rules (016): valuing battle kills and splitting defense points.
//!
//! Pure (P3) — no I/O. The per-unit **point value** and the leaderboard windows/page bound come from
//! balance (P7); the leaderboards that *sum* these facts are infrastructure read queries. A battle's
//! point yield is computed here at resolution and persisted as a fact (like loot), so leaderboards
//! sum persisted facts and a later balance change never rewrites awarded points (P2/P6).

use crate::units::{UnitCounts, UnitId};
use std::collections::HashMap;

/// Tunable ranking balance (016): the per-unit kill **point value**, the leaderboard time windows,
/// and the page bound. Loaded fail-fast from balance (P7).
#[derive(Debug, Clone)]
pub struct RankingRules {
    /// Point value of destroying one unit of each type (faithful default ≈ its crop upkeep — the
    /// troop's population value, GDD §11.2).
    pub point_value: HashMap<UnitId, i64>,
    /// Rolling leaderboard windows in seconds (e.g. 7d, 30d). "All-time" is the absence of a bound.
    pub windows_secs: Vec<i64>,
    /// Maximum rows any leaderboard returns (P11 — bounds every ranking query).
    pub page_size: usize,
}

impl RankingRules {
    /// The point value of a single unit type — `0` for an unknown id (e.g. wild animals carry no
    /// value), keeping the valuation total and reproducible (P6).
    pub fn unit_value(&self, id: &UnitId) -> i64 {
        self.point_value.get(id).copied().unwrap_or(0)
    }

    /// The total point value of a set of killed units: `Σ count × unit_value` (016 AC4). Applied to
    /// the **defender's** losses this is a battle's **attack points**; applied to the **attacker's**
    /// losses it is the battle's **defense-point total**, before the per-defender split.
    pub fn battle_value(&self, killed: &UnitCounts) -> i64 {
        killed
            .iter()
            .map(|(id, count)| self.unit_value(id) * i64::from(*count))
            .sum()
    }
}

/// Split `total` defense points across defenders weighted by `weights` (each defender's contributed
/// defensive value), so the integer shares **sum exactly** to `total` (016 AC4).
///
/// Uses **largest-remainder** apportionment: floor each exact share, then hand the leftover units to
/// the largest fractional remainders (ties broken by lower index) — deterministic (P2/P6). A
/// non-positive `total` yields all zeros; a zero total weight (no defensive value recorded) splits
/// the points as evenly as possible from index 0 so none are lost.
pub fn apportion(total: i64, weights: &[i64]) -> Vec<i64> {
    let n = weights.len();
    if n == 0 || total <= 0 {
        return vec![0; n];
    }
    let count = n as i64;
    let sum: i64 = weights.iter().map(|w| w.max(&0)).sum();
    if sum <= 0 {
        // No defensive value recorded: even split, the remainder to the first slots.
        let base = total / count;
        let rem = (total % count) as usize;
        return (0..n).map(|i| base + i64::from(i < rem)).collect();
    }
    // floor(total × wᵢ / sum), tracking the remainder numerator for the leftover pass. A single
    // battle's `total × w` stays well within i64 (army-sized counts × small point values).
    let mut shares = Vec::with_capacity(n);
    let mut remainders: Vec<(i64, usize)> = Vec::with_capacity(n);
    let mut assigned = 0i64;
    for (i, &w) in weights.iter().enumerate() {
        let numer = total * w.max(0);
        let share = numer / sum;
        shares.push(share);
        remainders.push((numer % sum, i));
        assigned += share;
    }
    // Largest remainder first; ties → lower index (stable, deterministic).
    remainders.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let mut leftover = total - assigned;
    for &(_, i) in &remainders {
        if leftover == 0 {
            break;
        }
        shares[i] += 1;
        leftover -= 1;
    }
    shares
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> RankingRules {
        let point_value = [
            (UnitId("legionnaire".into()), 1),
            (UnitId("paladin".into()), 2),
            (UnitId("catapult".into()), 6),
        ]
        .into_iter()
        .collect();
        RankingRules {
            point_value,
            windows_secs: vec![7 * 86_400, 30 * 86_400],
            page_size: 100,
        }
    }

    #[test]
    fn battle_value_sums_valued_kills_and_ignores_unknowns() {
        let r = rules();
        let killed: UnitCounts = vec![
            (UnitId("legionnaire".into()), 10), // 10 × 1
            (UnitId("paladin".into()), 3),      // 3 × 2
            (UnitId("catapult".into()), 2),     // 2 × 6
            (UnitId("rat".into()), 99),         // unknown → 0
        ];
        assert_eq!(r.battle_value(&killed), 10 + 6 + 12);
        assert_eq!(r.battle_value(&Vec::new()), 0);
    }

    #[test]
    fn apportion_splits_three_to_one_as_seventy_five_twenty_five() {
        // The spec's worked example (AC4): defenders A:B contribute 3:1 of a 100-point kill.
        assert_eq!(apportion(100, &[3, 1]), vec![75, 25]);
    }

    #[test]
    fn apportion_is_sum_preserving_with_rounding() {
        // 10 points across equal thirds: floors are 3/3/3, the leftover unit goes to the first.
        let shares = apportion(10, &[1, 1, 1]);
        assert_eq!(shares, vec![4, 3, 3]);
        assert_eq!(shares.iter().sum::<i64>(), 10);
        // Largest remainder wins: exact 3.5 / 1.75 / 1.75 → floors 3/1/1, the two 0.75 remainders
        // (indices 1,2) each take a leftover unit.
        let shares = apportion(7, &[2, 1, 1]);
        assert_eq!(shares.iter().sum::<i64>(), 7);
        assert_eq!(shares, vec![3, 2, 2]);
    }

    #[test]
    fn apportion_handles_zero_total_and_zero_weights() {
        assert_eq!(apportion(0, &[5, 3]), vec![0, 0]);
        assert_eq!(apportion(-4, &[5, 3]), vec![0, 0]);
        // No defensive value recorded → even split, remainder to the front; still sum-preserving.
        let shares = apportion(5, &[0, 0]);
        assert_eq!(shares, vec![3, 2]);
        assert_eq!(shares.iter().sum::<i64>(), 5);
        assert!(apportion(10, &[]).is_empty());
    }
}
