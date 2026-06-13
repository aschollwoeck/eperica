//! Culture-point use-cases (013): re-anchor the per-player accumulator when its rate changes.
//!
//! Culture points are **lazy** (002 model): the stored `(value, updated_at)` is settled on read at the
//! **live** rate derived from the player's villages' Town Hall levels. That read is only correct if the
//! rate was constant over `[updated_at, now]` — so the accumulator must be **re-anchored** (settled to
//! `now`, at the rate in effect *up to* that instant) at every rate-changing event: before a Town Hall
//! completes (013 T4), and when a village is founded/lost (013 T5 / 014).

use crate::ports::{AccountRepository, CultureRepository, RepoError};
use eperica_domain::{
    BuildingKind, CultureRules, PlayerId, Timestamp, Village, allowed_villages, culture_rate,
    settle_value,
};

/// A player's culture-point standing for the village page (013 AC11): the pooled CP settled to now,
/// its live rate, and the expansion-slot gate (used vs allowed, plus the CP needed for the next
/// village if one remains in the threshold table).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CultureView {
    /// Culture points settled to `now` at the live rate.
    pub cp: i64,
    /// The live CP/hour the player's villages produce.
    pub rate: i64,
    /// How many villages the player may hold now (`min(cpAllows, Residence/Palace capacity)`).
    pub allowed_villages: u32,
    /// How many villages the player currently holds.
    pub used_slots: u32,
    /// The cumulative CP the **next** village requires, or `None` when the threshold table is
    /// exhausted (no further village is gated on CP).
    pub next_threshold: Option<i64>,
}

/// One Residence/Palace level per village that has one (>0) — the input to the expansion-slot count.
fn residence_levels(villages: &[Village]) -> Vec<u8> {
    villages
        .iter()
        .filter_map(|v| {
            let level = building_level(v, BuildingKind::Residence)
                .max(building_level(v, BuildingKind::Palace));
            (level > 0).then_some(level)
        })
        .collect()
}

fn building_level(village: &Village, kind: BuildingKind) -> u8 {
    village
        .buildings
        .iter()
        .find(|b| b.kind == kind)
        .map_or(0, |b| b.level)
}

/// Read the player's culture standing on the village page (013 AC11), settling CP on read (P1): the
/// pooled CP at the live rate, the rate, and the slot gate (used/allowed + the next CP threshold).
///
/// # Errors
/// Propagates [`RepoError`] from the repositories.
pub async fn load_culture<A, C>(
    accounts: &A,
    culture: &C,
    rules: &CultureRules,
    now: Timestamp,
    player: PlayerId,
) -> Result<CultureView, RepoError>
where
    A: AccountRepository,
    C: CultureRepository,
{
    let villages = accounts.villages_of(player).await?;
    let (value, updated_at) = culture.player_culture(player).await?;
    let levels = culture.village_town_hall_levels(player).await?;
    let rate = culture_rate(&levels, rules);
    let cp = settle_value(value, rate, (now.0 - updated_at.0) / 1000);

    let used_slots = villages.len() as u32;
    let allowed_villages = allowed_villages(cp, &residence_levels(&villages), rules);
    // The next village's CP gate (village number = used + 1); `None` once the table is exhausted.
    let next_threshold = rules.cp_thresholds.get(used_slots as usize + 1).copied();

    Ok(CultureView {
        cp,
        rate,
        allowed_villages,
        used_slots,
        next_threshold,
    })
}

/// Re-anchor the player's culture accumulator to `now`: settle the value forward at the rate **in
/// effect up to now** (the live Town Hall levels), then stamp `now` as the new anchor. Idempotent for
/// an unchanged rate. Call this **before** applying a change that alters the rate (e.g. a Town Hall
/// level-up), so the elapsed period is credited at the old rate (P2).
///
/// # Errors
/// Propagates [`RepoError`] from the repository.
pub async fn reanchor_culture<C>(
    culture: &C,
    rules: &CultureRules,
    now: Timestamp,
    player: PlayerId,
) -> Result<(), RepoError>
where
    C: CultureRepository,
{
    let (value, updated_at) = culture.player_culture(player).await?;
    let levels = culture.village_town_hall_levels(player).await?;
    let rate = culture_rate(&levels, rules);
    let elapsed_secs = (now.0 - updated_at.0) / 1000;
    let settled = settle_value(value, rate, elapsed_secs);
    culture.settle_culture(player, settled, now).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Mutex;

    struct Fake {
        value: i64,
        updated_at: Timestamp,
        levels: Vec<u8>,
        settled: Mutex<Option<(i64, Timestamp)>>,
    }

    #[async_trait]
    impl CultureRepository for Fake {
        async fn player_culture(&self, _p: PlayerId) -> Result<(i64, Timestamp), RepoError> {
            Ok((self.value, self.updated_at))
        }
        async fn settle_culture(
            &self,
            _p: PlayerId,
            value: i64,
            at: Timestamp,
        ) -> Result<(), RepoError> {
            *self.settled.lock().unwrap() = Some((value, at));
            Ok(())
        }
        async fn village_town_hall_levels(&self, _p: PlayerId) -> Result<Vec<u8>, RepoError> {
            Ok(self.levels.clone())
        }
    }

    fn rules() -> CultureRules {
        CultureRules {
            base_cp_per_village: 2,
            town_hall_cp_per_level: vec![0, 5, 8],
            cp_thresholds: vec![0, 0, 200],
            expansion_slots_per_level: vec![0, 1],
            settlers_per_village: 3,
            settler_id: "settler".to_owned(),
        }
    }

    // AC1/AC2: re-anchoring settles the stored value forward at the live rate (base per village + each
    // Town Hall's output) and stamps the new anchor.
    #[tokio::test]
    async fn reanchor_settles_at_the_live_rate() {
        // Two villages — one with no Town Hall, one at level 2 — give rate (2+0)+(2+8) = 12/h.
        let f = Fake {
            value: 100,
            updated_at: Timestamp(0),
            levels: vec![0, 2],
            settled: Mutex::new(None),
        };
        reanchor_culture(&f, &rules(), Timestamp(3_600_000), PlayerId(1))
            .await
            .unwrap();
        let (value, at) = f.settled.lock().unwrap().unwrap();
        assert_eq!(value, 100 + 12, "one hour at 12 CP/h");
        assert_eq!(at, Timestamp(3_600_000));
    }
}
