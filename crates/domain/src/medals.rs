//! Medal & weekly-settlement rules (017): the medal categories, the real-time period arithmetic, and
//! the deterministic top-N ranker. Pure (P3) — no I/O. The period is **real time**, not speed-scaled
//! (the decided faithful exception; world speed scales what medals are *awarded from*, not the cadence).

use crate::event::Timestamp;

/// A weekly medal category (017, GDD §11.2). Player categories rank players; alliance categories rank
/// alliances by an aggregate of their members.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MedalCategory {
    /// Most attack points in the period.
    Attacker,
    /// Most defense points in the period.
    Defender,
    /// Most resources looted in the period.
    Raider,
    /// Most population gained over the period (snapshot delta).
    Climber,
    /// Alliance with the most aggregate member population.
    AlliancePopulation,
    /// Alliance with the most aggregate member attack points in the period.
    AllianceAttacker,
    /// Alliance with the most aggregate member defense points in the period.
    AllianceDefender,
}

impl MedalCategory {
    /// The persisted/config string key.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Attacker => "attacker",
            Self::Defender => "defender",
            Self::Raider => "raider",
            Self::Climber => "climber",
            Self::AlliancePopulation => "alliance_population",
            Self::AllianceAttacker => "alliance_attacker",
            Self::AllianceDefender => "alliance_defender",
        }
    }

    /// Parse a config/persisted key.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "attacker" => Self::Attacker,
            "defender" => Self::Defender,
            "raider" => Self::Raider,
            "climber" => Self::Climber,
            "alliance_population" => Self::AlliancePopulation,
            "alliance_attacker" => Self::AllianceAttacker,
            "alliance_defender" => Self::AllianceDefender,
            _ => return None,
        })
    }

    /// Whether this category ranks **alliances** (vs players).
    pub fn is_alliance(self) -> bool {
        matches!(
            self,
            Self::AlliancePopulation | Self::AllianceAttacker | Self::AllianceDefender
        )
    }
}

/// Weekly-settlement balance (017, P7): the real-time period, how many medals per category, and the
/// active category set.
#[derive(Debug, Clone)]
pub struct MedalRules {
    /// Period length in **real-time** seconds (not speed-scaled).
    pub period_secs: i64,
    /// How many medals each category awards per period (rank 1..=per_category).
    pub per_category: usize,
    /// The categories awarded each settlement.
    pub categories: Vec<MedalCategory>,
}

/// The zero-based settlement period a moment falls in, in real time. Anything at or before the world
/// start is period 0.
pub fn period_index(now: Timestamp, world_start: Timestamp, period_secs: i64) -> i64 {
    if period_secs <= 0 {
        return 0;
    }
    let elapsed_ms = now.0 - world_start.0;
    if elapsed_ms <= 0 {
        return 0;
    }
    elapsed_ms / (period_secs * 1000)
}

/// The instant period `period` **starts** (its boundary; `period_start(P+1)` is when period `P` ends
/// and the settlement for `P` is due).
pub fn period_start(period: i64, world_start: Timestamp, period_secs: i64) -> Timestamp {
    Timestamp(world_start.0 + period * period_secs * 1000)
}

/// Assign ranks `1..=n` to already-ordered rows (value desc, id asc upstream), truncating to `n`.
pub fn rank_top<T: Clone>(ordered: &[T], n: usize) -> Vec<(usize, T)> {
    ordered
        .iter()
        .take(n)
        .cloned()
        .enumerate()
        .map(|(i, v)| (i + 1, v))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn period_arithmetic_is_real_time() {
        let start = Timestamp(1_000_000);
        let wk = 604_800; // 7 days in seconds
        let wk_ms = wk * 1000;
        assert_eq!(period_index(start, start, wk), 0);
        assert_eq!(period_index(Timestamp(start.0 + wk_ms - 1), start, wk), 0);
        assert_eq!(period_index(Timestamp(start.0 + wk_ms), start, wk), 1);
        assert_eq!(
            period_index(Timestamp(start.0 + 3 * wk_ms + 5), start, wk),
            3
        );
        // Before/at the start clamps to 0.
        assert_eq!(period_index(Timestamp(start.0 - 50), start, wk), 0);
        // period_start(P+1) is when period P ends.
        assert_eq!(period_start(1, start, wk), Timestamp(start.0 + wk_ms));
        assert_eq!(period_start(3, start, wk), Timestamp(start.0 + 3 * wk_ms));
    }

    #[test]
    fn category_roundtrips_and_alliance_flag() {
        for c in [
            MedalCategory::Attacker,
            MedalCategory::Defender,
            MedalCategory::Raider,
            MedalCategory::Climber,
            MedalCategory::AlliancePopulation,
            MedalCategory::AllianceAttacker,
            MedalCategory::AllianceDefender,
        ] {
            assert_eq!(MedalCategory::parse(c.as_str()), Some(c));
        }
        assert!(MedalCategory::AlliancePopulation.is_alliance());
        assert!(!MedalCategory::Attacker.is_alliance());
        assert_eq!(MedalCategory::parse("nope"), None);
    }

    #[test]
    fn rank_top_numbers_and_truncates() {
        assert_eq!(
            rank_top(&["a", "b", "c", "d"], 3),
            vec![(1, "a"), (2, "b"), (3, "c")]
        );
        assert_eq!(rank_top::<&str>(&[], 3), vec![]);
    }
}
