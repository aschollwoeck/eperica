//! Trade & merchants (GDD §2.4, §4.2) — pure rules over the merchant balance data and resource
//! bundles. The engine that schedules and applies shipments lives in the application/infrastructure
//! layers; this module computes merchant counts, capped deliveries, and (via 007's travel time) the
//! timing. Resource bundles reuse [`ResourceAmounts`].

use crate::economy::{Capacities, ResourceAmounts};
use crate::error::DomainError;
use crate::village::Tribe;
use std::collections::HashMap;

/// What a trade leg does on arrival.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeKind {
    /// Merchants carry resources to the target village and credit its stores.
    Deliver,
    /// Emptied merchants travel home and become available again.
    Return,
}

/// A tribe's merchant profile: how much one merchant carries and how fast it travels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MerchantProfile {
    /// Total resources one merchant carries (summed across the four resource types).
    pub capacity: u32,
    /// Map speed in fields/hour.
    pub speed: u32,
}

/// All merchant/trade balance data, validated on construction.
#[derive(Debug, Clone)]
pub struct MerchantRules {
    profiles: HashMap<Tribe, MerchantProfile>,
    /// Merchant count by Marketplace level (index = level; level 0 ⇒ index 0 ⇒ no merchants).
    per_level: Vec<u32>,
}

impl MerchantRules {
    /// Build validated merchant rules.
    ///
    /// # Errors
    /// [`DomainError::InvalidMerchantRules`] unless every tribe has a profile with positive capacity
    /// and speed and the per-level table is non-empty.
    pub fn new(
        profiles: HashMap<Tribe, MerchantProfile>,
        per_level: Vec<u32>,
    ) -> Result<Self, DomainError> {
        if per_level.is_empty() {
            return Err(DomainError::InvalidMerchantRules(
                "the merchant per-level table must not be empty",
            ));
        }
        for tribe in [Tribe::Romans, Tribe::Teutons, Tribe::Gauls] {
            let p = profiles
                .get(&tribe)
                .ok_or(DomainError::InvalidMerchantRules(
                    "missing a tribe merchant profile",
                ))?;
            if p.capacity == 0 || p.speed == 0 {
                return Err(DomainError::InvalidMerchantRules(
                    "merchant capacity and speed must be positive",
                ));
            }
        }
        Ok(Self {
            profiles,
            per_level,
        })
    }

    /// The tribe's merchant profile.
    pub fn profile(&self, tribe: Tribe) -> MerchantProfile {
        self.profiles[&tribe]
    }

    /// How many merchants a village has at the given Marketplace `level` (0 with no Marketplace).
    /// The table is clamped to its last entry for levels beyond it.
    pub fn merchants_total(&self, level: u8) -> u32 {
        let idx = (level as usize).min(self.per_level.len() - 1);
        self.per_level[idx]
    }
}

/// The total resources in a bundle (summed across types).
pub fn bundle_total(bundle: ResourceAmounts) -> i64 {
    bundle.wood + bundle.clay + bundle.iron + bundle.crop
}

/// Whether a bundle carries nothing (no positive amount).
pub fn bundle_is_empty(bundle: ResourceAmounts) -> bool {
    bundle_total(bundle) <= 0
}

/// Merchants needed to carry `total` resources at per-merchant `capacity` (ceil division; 0 for an
/// empty load).
pub fn merchants_required(total: i64, capacity: u32) -> u32 {
    if total <= 0 {
        return 0;
    }
    let cap = i64::from(capacity.max(1));
    u32::try_from((total + cap - 1) / cap).unwrap_or(u32::MAX)
}

/// Add `add` to `current`, clamping wood/clay/iron to the Warehouse cap and crop to the Granary cap
/// — any overflow is **lost** (AC4). Each resource is clamped independently and never reduced below
/// `current` (a stack already at/over cap simply keeps its amount).
pub fn deposit_capped(
    current: ResourceAmounts,
    add: ResourceAmounts,
    caps: Capacities,
) -> ResourceAmounts {
    let cap_to = |c: i64, a: i64, limit: i64| (c + a.max(0)).min(limit).max(c);
    ResourceAmounts {
        wood: cap_to(current.wood, add.wood, caps.warehouse),
        clay: cap_to(current.clay, add.clay, caps.warehouse),
        iron: cap_to(current.iron, add.iron, caps.warehouse),
        crop: cap_to(current.crop, add.crop, caps.granary),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn amounts(wood: i64, clay: i64, iron: i64, crop: i64) -> ResourceAmounts {
        ResourceAmounts {
            wood,
            clay,
            iron,
            crop,
        }
    }

    fn rules() -> MerchantRules {
        MerchantRules::new(
            HashMap::from([
                (
                    Tribe::Romans,
                    MerchantProfile {
                        capacity: 500,
                        speed: 16,
                    },
                ),
                (
                    Tribe::Teutons,
                    MerchantProfile {
                        capacity: 1000,
                        speed: 12,
                    },
                ),
                (
                    Tribe::Gauls,
                    MerchantProfile {
                        capacity: 750,
                        speed: 24,
                    },
                ),
            ]),
            vec![0, 1, 2, 3, 4],
        )
        .unwrap()
    }

    // AC3: a load needs ceil(total ÷ capacity) merchants; a smaller capacity needs more.
    #[test]
    fn merchants_required_rounds_up_and_scales_with_capacity() {
        assert_eq!(merchants_required(0, 500), 0);
        assert_eq!(merchants_required(1, 500), 1);
        assert_eq!(merchants_required(500, 500), 1);
        assert_eq!(merchants_required(501, 500), 2);
        assert_eq!(merchants_required(1500, 500), 3);
        // Same load, larger-capacity tribe needs fewer; smaller needs more.
        assert_eq!(merchants_required(1500, 1000), 2);
        assert_eq!(merchants_required(1500, 750), 2);
    }

    // AC3: merchant count comes from the Marketplace level, clamped past the table.
    #[test]
    fn merchants_total_reads_the_level_table() {
        let r = rules();
        assert_eq!(r.merchants_total(0), 0);
        assert_eq!(r.merchants_total(1), 1);
        assert_eq!(r.merchants_total(4), 4);
        assert_eq!(r.merchants_total(20), 4); // clamped to the last entry
        assert_eq!(r.profile(Tribe::Teutons).capacity, 1000);
        assert_eq!(r.profile(Tribe::Gauls).speed, 24);
    }

    #[test]
    fn bundle_helpers() {
        assert_eq!(bundle_total(amounts(10, 20, 30, 40)), 100);
        assert!(bundle_is_empty(amounts(0, 0, 0, 0)));
        assert!(!bundle_is_empty(amounts(0, 0, 0, 1)));
    }

    // AC4: a delivery is added up to capacity; overflow is lost; crop uses the granary cap.
    #[test]
    fn deposit_clamps_to_capacity() {
        let caps = Capacities {
            warehouse: 1000,
            granary: 800,
        };
        // Under cap: added in full.
        assert_eq!(
            deposit_capped(
                amounts(100, 100, 100, 100),
                amounts(200, 200, 200, 200),
                caps
            ),
            amounts(300, 300, 300, 300)
        );
        // Over cap: clamped, the excess lost (wood 900+200→1000, crop 700+200→800).
        assert_eq!(
            deposit_capped(amounts(900, 0, 0, 700), amounts(200, 0, 0, 200), caps),
            amounts(1000, 0, 0, 800)
        );
        // Already at/over cap: unchanged (never reduced).
        assert_eq!(
            deposit_capped(amounts(1000, 0, 0, 0), amounts(500, 0, 0, 0), caps),
            amounts(1000, 0, 0, 0)
        );
    }

    #[test]
    fn rejects_invalid_balance() {
        assert!(MerchantRules::new(HashMap::new(), vec![1]).is_err());
        assert!(MerchantRules::new(HashMap::new(), vec![]).is_err());
    }
}
