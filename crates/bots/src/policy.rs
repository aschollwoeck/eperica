//! Pure reflex-policy: digest + persona → `Vec<Intent>`.
//!
//! `plan_tick` is a pure function (no I/O, no clocks other than the passed-in
//! `now_ms`, no randomness).  It mirrors the server's P3 discipline on the
//! client side.
//!
//! # Doctrine (rules evaluated in priority order)
//!
//! For each village, rules are evaluated in the order below.  Rule 1 is a hard
//! override: if the evacuate condition fires, all other intents for that village
//! are suppressed.  For the build decision (rules 2–4), the first rule that
//! produces a build intent wins (one build per village per tick); training,
//! settling, and raiding (rules 5–7) are applied regardless of the build outcome.
//!
//! 1. **Evacuate / Recall** — imminent attack → reinforce another own village.
//! 2. **Storage** — any resource ≥ 90 % capacity → build/upgrade Warehouse or Granary.
//! 3. **Fields** — upgrade the lowest-level field (crop-biased when crop net < 25/h;
//!    cap at level 10).
//! 4. **Core buildings** — Main Building→3, Barracks→3, Warehouse→3, Granary→3,
//!    Main Building→5, Academy→1, Residence→10; gated on fields ≥ average level 2.
//! 5. **Training** — garrison below floor (10 + 10·aggression) → train up to 5 units.
//! 6. **Settling** — culture allows more villages + Residence ≥ 10 → train settlers
//!    or send them to the nearest free valley.
//! 7. **Raiding** — aggression ≥ 1, garrison ≥ floor (15 + 5·aggression) → raid up to
//!    `aggression` inactive-labeled map cells within radar range.

use std::collections::{BTreeMap, HashSet};

use crate::digest::{Digest, MapWindow, VillageDigest};
use crate::persona::Persona;

// ---------------------------------------------------------------------------
// Intent
// ---------------------------------------------------------------------------

/// One action the executor should attempt on behalf of this bot.
///
/// Field slugs (village name, unit, kind) are the API's own identifier strings
/// — the executor forwards them verbatim to the relevant endpoint.
#[derive(Debug, Clone, PartialEq)]
pub enum Intent {
    /// Upgrade a resource field or place/upgrade a centre building.
    ///
    /// `target` is the static string `"field"` or `"building"`.
    /// `kind` is required for building orders; absent for field orders.
    Build {
        village: String,
        target: &'static str,
        slot: u8,
        kind: Option<String>,
    },
    /// Queue a training batch for `count` units of `unit` in `village`.
    Train {
        village: String,
        unit: String,
        count: u32,
    },
    /// Research `unit` in `village` (requires Academy + prerequisites).
    Research { village: String, unit: String },
    /// Send troops from `village` to reinforce coordinates (x, y).
    Reinforce {
        village: String,
        x: i32,
        y: i32,
        units: BTreeMap<String, u32>,
    },
    /// Recall reinforcements sent to `host` (another own village) back to `village`.
    Recall { village: String, host: String },
    /// Send `units` from `village` on a raid to (x, y).
    Raid {
        village: String,
        x: i32,
        y: i32,
        units: BTreeMap<String, u32>,
    },
    /// Train `count` settler units in `village` (unit slug "settler", all tribes).
    TrainSettlers { village: String, count: u32 },
    /// Send settlers from `village` to found a new village at (x, y).
    Settle { village: String, x: i32, y: i32 },
}

// ---------------------------------------------------------------------------
// Building-slot constants (mirrors crates/domain/src/building.rs)
// ---------------------------------------------------------------------------

/// Total centre-building slot count per village (slots 0..VILLAGE_BUILDING_SLOTS).
const VILLAGE_BUILDING_SLOTS: u8 = 22;

/// Reserved slots that the bot must never place a general building on.
/// - 0  = Main Building  (always present, fixed position)
/// - 1  = Rally Point    (always present, fixed position)
/// - 11 = Wall           (tribe-specific, fixed position)
const RESERVED_SLOTS: [u8; 3] = [0, 1, 11];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Return the lowest general centre-building slot not already occupied.
///
/// "General" means not in `RESERVED_SLOTS` (0, 1, 11).  Returns `None` if all
/// 22 slots are occupied (extremely unlikely in practice).
fn free_building_slot(buildings: &[crate::digest::SlotLevel]) -> Option<u8> {
    let occupied: HashSet<u8> = buildings.iter().map(|b| b.slot).collect();
    (0..VILLAGE_BUILDING_SLOTS).find(|&s| !RESERVED_SLOTS.contains(&s) && !occupied.contains(&s))
}

/// Chebyshev distance between two tile coordinates.
///
/// We use max(|dx|, |dy|) rather than a full toroidal formula.  This is accurate
/// for small distances (raid range ≤ 15 tiles, settle within the map window radius)
/// and is the simplest reading of the spec's "Chebyshev max" directive.
fn chebyshev(ax: i32, ay: i32, bx: i32, by: i32) -> u32 {
    ((ax - bx).unsigned_abs()).max((ay - by).unsigned_abs())
}

/// Total unit count in the garrison (own troops stationed here).
fn garrison_total(v: &VillageDigest) -> u32 {
    v.garrison.iter().map(|g| g.count).sum()
}

/// Count of a specific unit in the garrison.
fn garrison_count_of(v: &VillageDigest, unit: &str) -> u32 {
    v.garrison
        .iter()
        .filter(|g| g.unit == unit)
        .map(|g| g.count)
        .sum()
}

/// Find a building by kind slug in a village's buildings list.
fn find_building<'a>(
    buildings: &'a [crate::digest::SlotLevel],
    kind: &str,
) -> Option<&'a crate::digest::SlotLevel> {
    buildings.iter().find(|b| b.kind == kind)
}

/// Map a tribe slug to its tier-1 infantry unit slug.
///
/// romans   → "legionnaire"
/// teutons  → "clubswinger"
/// gauls    → "phalanx"
///
/// Unknown tribes fall back to "legionnaire" (roman default) — noted for T4.
fn tier1_unit(tribe: &str) -> &'static str {
    match tribe {
        // The wire truth: /api/me carries Tribe::slug — PLURAL ("romans"/"teutons"/"gauls").
        // Singular forms tolerated as aliases. (An e2e run caught the original singular-only
        // match: every bot fell back to legionnaire and non-Roman training 409'd.)
        "romans" | "roman" => "legionnaire",
        "teutons" | "teuton" => "clubswinger",
        "gauls" | "gaul" => "phalanx",
        // Unknown tribe defaults to roman tier-1; the runner logs a warning.
        _ => "legionnaire",
    }
}

/// Settler unit slug — "settler" for all three tribes (verified in
/// specs/balance/presets/classic/units.toml: each tribe has a `[[<tribe>.units]]`
/// entry with `id = "settler"` and `role = "expansion"`).
const SETTLER_UNIT: &str = "settler";

/// Resource field level cap: the bot will not queue upgrades beyond level 10
/// (the non-capital field cap; a level-10 field would 409 anyway).
const FIELD_LEVEL_CAP: u8 = 10;

/// Crop-net floor (per hour, world-real — the digest rates are already scaled):
/// if crop net < this threshold, bias field upgrades toward crop fields.
const CROP_NET_FLOOR: i64 = 25;

/// Training cap per tick: train at most this many units per tick and let the
/// server's 409 "insufficient" response say no if resources are lacking.
/// Chosen over a per-unit cost formula to avoid duplicating balance constants.
const TRAIN_CAP_PER_TICK: u32 = 5;

// ---------------------------------------------------------------------------
// Build-intent helpers
// ---------------------------------------------------------------------------

/// Produce a Build intent for upgrading an existing building, or placing a new
/// one on the lowest free general slot.
fn build_or_upgrade(
    village_id: &str,
    kind: &str,
    buildings: &[crate::digest::SlotLevel],
) -> Option<Intent> {
    // Upgrade the existing instance if present (first match — for single-instance buildings
    // there is at most one; for multi-instance buildings we upgrade whichever appears first
    // in the buildings list, which is consistent with how the server orders them).
    if let Some(b) = find_building(buildings, kind) {
        Some(Intent::Build {
            village: village_id.to_owned(),
            target: "building",
            slot: b.slot,
            kind: Some(kind.to_owned()),
        })
    } else {
        // Place on a new slot.
        let slot = free_building_slot(buildings)?;
        Some(Intent::Build {
            village: village_id.to_owned(),
            target: "building",
            slot,
            kind: Some(kind.to_owned()),
        })
    }
}

// ---------------------------------------------------------------------------
// Raid helper (extracted to avoid clippy::collapsible_if)
// ---------------------------------------------------------------------------

/// Append `Raid` intents for targets reachable from village `v`, respecting
/// the mutable party budget so the garrison floor is never breached.
#[allow(clippy::too_many_arguments)]
fn raid_targets(
    v: &VillageDigest,
    map_win: &MapWindow,
    in_flight: &HashSet<(i32, i32)>,
    p: &Persona,
    garrison_sum: u32,
    raid_floor: u32,
    unit: &str,
    intents: &mut Vec<Intent>,
) {
    // How many units can leave without breaching the floor.
    let mut remaining = garrison_sum.saturating_sub(raid_floor);
    let max_by_aggression = 8 + 4 * (p.aggression as u32);
    let max_by_garrison = garrison_sum / 3;

    // Collect inactive targets in range, sorted nearest-first.
    let mut targets: Vec<_> = map_win
        .rows
        .iter()
        .flatten()
        .filter(|cell| {
            cell.label.contains("(inactive)")
                && !in_flight.contains(&(cell.x, cell.y))
                && chebyshev(v.x, v.y, cell.x, cell.y) <= p.raid_range as u32
        })
        .collect();

    targets.sort_by_key(|cell| chebyshev(v.x, v.y, cell.x, cell.y));

    for cell in targets.iter().take(p.aggression as usize) {
        let party = max_by_aggression.min(max_by_garrison).min(remaining);
        // Minimum viable raiding party: stop if we can't send at least 4.
        if party < 4 {
            break;
        }
        remaining -= party;
        let mut units = BTreeMap::new();
        units.insert(unit.to_owned(), party);
        intents.push(Intent::Raid {
            village: v.id.clone(),
            x: cell.x,
            y: cell.y,
            units,
        });
    }
}

// ---------------------------------------------------------------------------
// plan_tick — the main entry point
// ---------------------------------------------------------------------------

/// Derive a list of intents for the current tick from the player's digest.
///
/// # Parameters
/// - `d`       — full state digest (fetched once per tick).
/// - `map`     — optional cached map window (may be `None` if unavailable).
/// - `p`       — deterministic persona for this bot.
/// - `now_ms`  — server clock at digest assembly time (`d.now_ms`).
/// - `tribe`   — tribe slug from `/api/me` `worlds[].tribe`; T4 passes it.
///   Used to select the tier-1 infantry unit for training and raiding.
///
/// # Determinism
/// Pure function: same arguments → identical `Vec<Intent>` every time.
pub fn plan_tick(
    d: &Digest,
    map: Option<&MapWindow>,
    p: &Persona,
    now_ms: i64,
    // T4 passes tribe from /api/me worlds[].tribe; it is not in the Digest.
    tribe: &str,
) -> Vec<Intent> {
    let mut intents: Vec<Intent> = Vec::new();

    let unit = tier1_unit(tribe);

    // Set of own village IDs (used for evacuate/recall checks).
    let own_ids: HashSet<&str> = d.villages.iter().map(|v| v.id.as_str()).collect();

    // Set of (dest_x, dest_y) already targeted by own in-flight movements.
    // Raiding skips any (x, y) already in this set.
    let in_flight: HashSet<(i32, i32)> = d.movements.iter().map(|m| (m.dest_x, m.dest_y)).collect();

    // Villages under imminent attack: village_id → arrive_at_ms.
    // "Imminent" = arrives within 2 × tick_max_secs × 1 000 ms of now_ms.
    let evacuation_window_ms = 2 * (p.tick_max_secs as i64) * 1_000;
    let imminent_attack_targets: HashSet<&str> = d
        .incoming_attacks
        .iter()
        .filter(|inc| {
            inc.arrive_at_ms > now_ms && inc.arrive_at_ms - now_ms <= evacuation_window_ms
        })
        .map(|inc| inc.village.as_str())
        .collect();

    // Recall: troops stationed at own villages that are not under imminent attack.
    // The issuer village is only strict path addressing — order_return matches the
    // stationed group by (owner, host), so any owned village may issue the recall.
    // We pick the first non-attacked own village that is not the host to avoid
    // issuing from the attacked village itself.
    // Recall is emitted once per host entry (not once per iteration of the village
    // loop) to avoid duplicate intents.
    for abroad in &d.reinforcements_abroad {
        if own_ids.contains(abroad.host_village.as_str()) {
            // Find a suitable source village to issue the recall from.
            if let Some(src) = d.villages.iter().find(|v| {
                v.id != abroad.host_village && !imminent_attack_targets.contains(v.id.as_str())
            }) {
                intents.push(Intent::Recall {
                    village: src.id.clone(),
                    host: abroad.host_village.clone(),
                });
            }
        }
    }

    // -----------------------------------------------------------------------
    // Per-village rules
    // -----------------------------------------------------------------------
    for v in &d.villages {
        let garrison_sum = garrison_total(v);

        // -------------------------------------------------------------------
        // Rule 1: Evacuate
        // Condition: incoming attack arriving within the evacuation window AND
        //   ≥ 2 own villages AND garrison is non-empty.
        // Effect: Reinforce the first other own village; skip all other intents
        //   for this village this tick.
        // -------------------------------------------------------------------
        if imminent_attack_targets.contains(v.id.as_str())
            && d.villages.len() >= 2
            && garrison_sum > 0
        {
            if let Some(other) = d.villages.iter().find(|o| o.id != v.id) {
                let units: BTreeMap<String, u32> = v
                    .garrison
                    .iter()
                    .map(|g| (g.unit.clone(), g.count))
                    .collect();
                intents.push(Intent::Reinforce {
                    village: v.id.clone(),
                    x: other.x,
                    y: other.y,
                    units,
                });
            }
            // Skip economy, training, settling, and raiding for this village.
            continue;
        }

        // -------------------------------------------------------------------
        // Build decision: rules 2–4 (first match wins; skipped when the
        // village's build queue is already occupied).
        // -------------------------------------------------------------------
        let mut build_emitted = false;

        if v.build_queue.is_empty() {
            // -----------------------------------------------------------------
            // Rule 2: Storage
            // If any resource is ≥ 90 % of its capacity, upgrade or place a
            // storage building.  Crop overflows a Granary; others overflow a
            // Warehouse.  We check crop first (crop starvation kills troops).
            // -----------------------------------------------------------------
            let crop_near_cap = v.resources.crop.capacity > 0
                && v.resources.crop.amount >= 9 * v.resources.crop.capacity / 10;
            let noncrop_near_cap = (v.resources.wood.capacity > 0
                && v.resources.wood.amount >= 9 * v.resources.wood.capacity / 10)
                || (v.resources.clay.capacity > 0
                    && v.resources.clay.amount >= 9 * v.resources.clay.capacity / 10)
                || (v.resources.iron.capacity > 0
                    && v.resources.iron.amount >= 9 * v.resources.iron.capacity / 10);

            if crop_near_cap {
                if let Some(intent) = build_or_upgrade(&v.id, "granary", &v.buildings) {
                    intents.push(intent);
                    build_emitted = true;
                }
            } else if noncrop_near_cap
                && let Some(intent) = build_or_upgrade(&v.id, "warehouse", &v.buildings)
            {
                intents.push(intent);
                build_emitted = true;
            }

            // -----------------------------------------------------------------
            // Rules 3/4 (interleaved — plan doctrine as clarified): while the
            // average field level is below 2, fields only; once it reaches 2 the
            // core-building doctrine takes priority until complete; then fields
            // resume to the cap. (The original strict ordering made the avg-2
            // gate unreachable — fields would monopolize until all 18 hit 10.)
            // -----------------------------------------------------------------
            let field_avg = if v.fields.is_empty() {
                0.0_f64
            } else {
                v.fields.iter().map(|f| f.level as f64).sum::<f64>() / v.fields.len() as f64
            };
            // Prereq-consistent against the classic preset (specs/balance/presets/classic/construction.toml):
            //   barracks  prereq main_building≥3  → raise MB to 3 first
            //   academy   prereq barracks≥3        → raise barracks to 3 before academy
            //   residence prereq main_building≥5   → raise MB to 5 before residence
            // Duplicate kinds are intentional: the first entry that is unmet wins.
            const DOCTRINE: &[(&str, u8)] = &[
                ("main_building", 3),
                ("barracks", 3),
                ("warehouse", 3),
                ("granary", 3),
                ("main_building", 5),
                ("academy", 1),
                ("residence", 10),
            ];
            // Rule 4 first when its gate holds and the table is unmet.
            if !build_emitted && field_avg >= 2.0 {
                for &(kind, target_level) in DOCTRINE {
                    let current_level = find_building(&v.buildings, kind)
                        .map(|b| b.level)
                        .unwrap_or(0);
                    if current_level < target_level {
                        if let Some(intent) = build_or_upgrade(&v.id, kind, &v.buildings) {
                            intents.push(intent);
                            build_emitted = true;
                        }
                        break; // One build intent per tick.
                    }
                }
            }

            // Rule 3: Fields — the default whenever no storage/core intent fired.
            // Upgrade the lowest-level resource field (≤ level 9, to keep the
            // result level ≤ 10).  When crop net < CROP_NET_FLOOR (/h), bias
            // toward the lowest-level crop field.  Tie-breaking: wood > clay >
            // iron > crop (Travian convention — metals are scarcer).
            //
            // Interpretation: only fields present in v.fields are considered.
            // Fields not listed are assumed absent/unknown; the server may return
            // all 18 slots including level-0 ones in practice.
            if !build_emitted {
                let crop_net_low = v.resources.crop.rate < CROP_NET_FLOOR;

                // Kind sort order for tie-breaking when crop net is sufficient.
                // Lower value = higher priority.
                let kind_priority = |kind: &str| match kind {
                    "wood" => 0u8,
                    "clay" => 1,
                    "iron" => 2,
                    "crop" => 3,
                    _ => 4,
                };

                let target_field = if crop_net_low {
                    // Prefer lowest-level crop field.
                    v.fields
                        .iter()
                        .filter(|f| f.kind == "crop" && f.level < FIELD_LEVEL_CAP)
                        .min_by_key(|f| f.level)
                        .or_else(|| {
                            // Fallback: any field below the cap.
                            v.fields
                                .iter()
                                .filter(|f| f.level < FIELD_LEVEL_CAP)
                                .min_by_key(|f| (f.level, kind_priority(&f.kind)))
                        })
                } else {
                    // Pick lowest-level field; ties broken by kind priority.
                    v.fields
                        .iter()
                        .filter(|f| f.level < FIELD_LEVEL_CAP)
                        .min_by_key(|f| (f.level, kind_priority(&f.kind)))
                };

                if let Some(field) = target_field {
                    intents.push(Intent::Build {
                        village: v.id.clone(),
                        target: "field",
                        slot: field.slot,
                        kind: None,
                    });
                    build_emitted = true;
                }
            }
        }
        // Suppress unused-variable warning in paths where build_emitted is set but
        // never read again — all paths above check it before writing.
        let _ = build_emitted;

        // -------------------------------------------------------------------
        // Rule 5: Training
        // Train the tribe's tier-1 infantry when the garrison is below the
        // floor (10 + 10 × aggression).  In-training units (training[].remaining
        // for the tier-1 unit) count toward the floor to avoid queuing duplicates
        // when a batch is already in progress.
        // Heuristic: train min(needed, TRAIN_CAP_PER_TICK) and let the server's
        // 409 "insufficient" say no — we do not duplicate balance cost tables.
        // -------------------------------------------------------------------
        {
            let in_training_tier1: u32 = v
                .training
                .iter()
                .filter(|t| t.unit == unit)
                .map(|t| t.remaining)
                .sum();
            let effective_garrison = garrison_sum + in_training_tier1;
            let floor = 10 + 10 * (p.aggression as u32);
            if effective_garrison < floor {
                let needed = floor - effective_garrison;
                let count = needed.min(TRAIN_CAP_PER_TICK);
                intents.push(Intent::Train {
                    village: v.id.clone(),
                    unit: unit.to_owned(),
                    count,
                });
            }
        }

        // -------------------------------------------------------------------
        // Rule 6: Settling
        // Conditions: culture allows another village AND Residence ≥ 10.
        // - If settler count in garrison < 3 → TrainSettlers(3 - current).
        // - If garrison holds ≥ 3 settlers AND map has a free valley →
        //   Settle(nearest free valley by Chebyshev distance).
        // -------------------------------------------------------------------
        {
            let culture_allows = d.culture.villages_used < d.culture.villages_allowed;
            let residence_level = find_building(&v.buildings, "residence")
                .map(|b| b.level)
                .unwrap_or(0);

            if culture_allows && residence_level >= 10 {
                // Count both settlers in the garrison and any currently training, to avoid
                // queuing duplicate TrainSettlers batches when training is in progress.
                let settler_in_training: u32 = v
                    .training
                    .iter()
                    .filter(|t| t.unit == SETTLER_UNIT)
                    .map(|t| t.remaining)
                    .sum();
                let settler_count = garrison_count_of(v, SETTLER_UNIT) + settler_in_training;

                if settler_count < 3 {
                    let needed = 3 - settler_count;
                    intents.push(Intent::TrainSettlers {
                        village: v.id.clone(),
                        count: needed,
                    });
                } else if let Some(map_win) = map {
                    // Find nearest free valley (settle == true) by Chebyshev distance.
                    let target = map_win
                        .rows
                        .iter()
                        .flatten()
                        .filter(|cell| cell.settle)
                        .min_by_key(|cell| chebyshev(v.x, v.y, cell.x, cell.y));

                    if let Some(cell) = target {
                        intents.push(Intent::Settle {
                            village: v.id.clone(),
                            x: cell.x,
                            y: cell.y,
                        });
                    }
                }
            }
        }

        // -------------------------------------------------------------------
        // Rule 7: Raiding
        // Conditions: aggression ≥ 1 AND garrison ≥ floor (15 + 5 × aggression).
        // Raid up to `aggression` nearest inactive-labeled map cells within
        // raid_range tiles (Chebyshev).  Skip (x, y) already targeted by an
        // own in-flight movement.
        //
        // Party budget: start = garrison − floor (what can leave without breaching
        // the floor).  Per target: min(8 + 4×agg, garrison/3, remaining).  Stop
        // early when the remaining budget falls below 4.  This prevents overdraw
        // across multiple simultaneous raids — the floor is always maintained.
        // -------------------------------------------------------------------
        if p.aggression >= 1 {
            let raid_floor = 15 + 5 * (p.aggression as u32);
            if garrison_sum >= raid_floor
                && let Some(map_win) = map
            {
                raid_targets(
                    v,
                    map_win,
                    &in_flight,
                    p,
                    garrison_sum,
                    raid_floor,
                    unit,
                    &mut intents,
                );
            }
        }
    }

    intents
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::digest::{
        Culture, Digest, GarrisonEntry, Incoming, MapCell, MapWindow, MovementEntry, QueueEntry,
        ReinforcementAbroad, ResourceLine, Resources, SlotLevel, VillageDigest,
    };

    // -----------------------------------------------------------------------
    // Fixture builders
    // -----------------------------------------------------------------------

    // The wire slugs (plural — Tribe::slug) must map to the right tier-1 unit; the singular
    // aliases stay tolerated. Regression pin for the e2e-caught fallback bug.
    #[test]
    fn tier1_unit_matches_wire_slugs() {
        assert_eq!(tier1_unit("romans"), "legionnaire");
        assert_eq!(tier1_unit("teutons"), "clubswinger");
        assert_eq!(tier1_unit("gauls"), "phalanx");
        assert_eq!(tier1_unit("teuton"), "clubswinger");
        assert_eq!(tier1_unit("martians"), "legionnaire");
    }

    fn persona(aggression: u8) -> Persona {
        Persona {
            window_start_hour: 0,
            window_len_hours: 24,
            tick_min_secs: 300,
            tick_max_secs: 720,
            aggression,
            raid_range: 10,
        }
    }

    fn resources_at(pct: u8) -> Resources {
        // capacity=1000; amount = pct% of capacity.
        let amt = |pct: u8| (1000_i64 * pct as i64) / 100;
        Resources {
            wood: ResourceLine {
                amount: amt(pct),
                rate: 30,
                capacity: 1000,
            },
            clay: ResourceLine {
                amount: amt(pct),
                rate: 25,
                capacity: 1000,
            },
            iron: ResourceLine {
                amount: amt(pct),
                rate: 20,
                capacity: 1000,
            },
            crop: ResourceLine {
                amount: amt(pct),
                rate: 30,
                capacity: 1000,
            },
        }
    }

    fn full_resources_with_crop_low() -> Resources {
        // crop near capacity but low net; others well below.
        Resources {
            wood: ResourceLine {
                amount: 200,
                rate: 30,
                capacity: 1000,
            },
            clay: ResourceLine {
                amount: 200,
                rate: 25,
                capacity: 1000,
            },
            iron: ResourceLine {
                amount: 200,
                rate: 20,
                capacity: 1000,
            },
            crop: ResourceLine {
                amount: 920,
                rate: 10,
                capacity: 1000,
            },
        }
    }

    fn make_village(id: &str, x: i32, y: i32) -> VillageDigest {
        VillageDigest {
            id: id.to_owned(),
            x,
            y,
            capital: false,
            resources: resources_at(50), // 50% — nothing near capacity
            fields: vec![
                SlotLevel {
                    slot: 0,
                    kind: "wood".into(),
                    level: 2,
                },
                SlotLevel {
                    slot: 1,
                    kind: "clay".into(),
                    level: 2,
                },
                SlotLevel {
                    slot: 2,
                    kind: "iron".into(),
                    level: 2,
                },
                SlotLevel {
                    slot: 3,
                    kind: "crop".into(),
                    level: 2,
                },
            ],
            buildings: vec![
                // Main building at reserved slot 0, Rally Point at slot 1.
                SlotLevel {
                    slot: 0,
                    kind: "main_building".into(),
                    level: 3,
                },
                SlotLevel {
                    slot: 1,
                    kind: "rally_point".into(),
                    level: 1,
                },
            ],
            build_queue: vec![],
            training: vec![],
            garrison: vec![GarrisonEntry {
                unit: "legionnaire".into(),
                count: 20,
            }],
            reinforcements_here: vec![],
            research: Default::default(),
        }
    }

    fn single_village_digest(v: VillageDigest) -> Digest {
        Digest {
            world: "world-0001".into(),
            player: "42".into(),
            now_ms: 1_700_000_000_000,
            villages: vec![v],
            culture: Culture {
                cp: 0,
                rate_per_hour: 5,
                villages_used: 1,
                villages_allowed: 1,
                next_threshold: 200,
            },
            ..Default::default()
        }
    }

    fn two_village_digest(v1: VillageDigest, v2: VillageDigest) -> Digest {
        Digest {
            world: "world-0001".into(),
            player: "42".into(),
            now_ms: 1_700_000_000_000,
            villages: vec![v1, v2],
            culture: Culture {
                cp: 0,
                rate_per_hour: 5,
                villages_used: 2,
                villages_allowed: 2,
                next_threshold: 200,
            },
            ..Default::default()
        }
    }

    // -----------------------------------------------------------------------
    // Rule 1: Evacuate
    // -----------------------------------------------------------------------

    #[test]
    fn rule1_evacuate_fires_when_imminent_attack_two_villages_nonempty_garrison() {
        let v1 = make_village("v1", 0, 0);
        let mut v2 = make_village("v2", 5, 5);
        v2.garrison = vec![GarrisonEntry {
            unit: "legionnaire".into(),
            count: 15,
        }];

        let now_ms = 1_700_000_000_000_i64;
        let p = persona(0);
        // Attack on v2 arrives in 1 000 ms (well within 2×720×1000 = 1 440 000 ms).
        let mut d = two_village_digest(v1.clone(), v2.clone());
        d.incoming_attacks = vec![Incoming {
            village: "v2".into(),
            arrive_at_ms: now_ms + 1_000,
        }];

        let intents = plan_tick(&d, None, &p, now_ms, "roman");
        // Expect exactly one Reinforce for v2 → v1.
        let reinforce = intents
            .iter()
            .find(|i| matches!(i, Intent::Reinforce { village, .. } if village == "v2"));
        assert!(
            reinforce.is_some(),
            "expected Reinforce for v2: {intents:?}"
        );
        if let Some(Intent::Reinforce { x, y, units, .. }) = reinforce {
            assert_eq!((*x, *y), (0, 0), "should reinforce to v1 coords");
            assert_eq!(units["legionnaire"], 15);
        }
    }

    #[test]
    fn rule1_no_evacuate_when_only_one_village() {
        let mut v = make_village("v1", 0, 0);
        v.garrison = vec![GarrisonEntry {
            unit: "legionnaire".into(),
            count: 15,
        }];

        let now_ms = 1_700_000_000_000_i64;
        let p = persona(0);
        let mut d = single_village_digest(v);
        d.incoming_attacks = vec![Incoming {
            village: "v1".into(),
            arrive_at_ms: now_ms + 1_000,
        }];

        let intents = plan_tick(&d, None, &p, now_ms, "roman");
        // Only one village → no evacuation, no reinforce.
        assert!(
            !intents
                .iter()
                .any(|i| matches!(i, Intent::Reinforce { .. })),
            "single village should not evacuate: {intents:?}"
        );
    }

    #[test]
    fn rule1_no_evacuate_when_garrison_empty() {
        let v1 = make_village("v1", 0, 0);
        let mut v2 = make_village("v2", 5, 5);
        v2.garrison = vec![]; // empty garrison

        let now_ms = 1_700_000_000_000_i64;
        let p = persona(0);
        let mut d = two_village_digest(v1.clone(), v2.clone());
        d.incoming_attacks = vec![Incoming {
            village: "v2".into(),
            arrive_at_ms: now_ms + 1_000,
        }];

        let intents = plan_tick(&d, None, &p, now_ms, "roman");
        // Empty garrison → no Reinforce emitted for v2.
        let reinforce_v2 = intents
            .iter()
            .any(|i| matches!(i, Intent::Reinforce { village, .. } if village == "v2"));
        assert!(
            !reinforce_v2,
            "empty garrison should not evacuate: {intents:?}"
        );
        // Clear v1 from the warning about unused; it's used by the digest builder.
        let _ = v1;
    }

    #[test]
    fn rule1_no_evacuate_when_attack_outside_window() {
        let v1 = make_village("v1", 0, 0);
        let mut v2 = make_village("v2", 5, 5);
        v2.garrison = vec![GarrisonEntry {
            unit: "legionnaire".into(),
            count: 15,
        }];

        let now_ms = 1_700_000_000_000_i64;
        let p = persona(0);
        // Attack arrives far in the future (beyond 2×720×1000 ms).
        let mut d = two_village_digest(v1.clone(), v2.clone());
        d.incoming_attacks = vec![Incoming {
            village: "v2".into(),
            arrive_at_ms: now_ms + 10_000_000, // 10 000 s in the future
        }];

        let intents = plan_tick(&d, None, &p, now_ms, "roman");
        let reinforce_v2 = intents
            .iter()
            .any(|i| matches!(i, Intent::Reinforce { village, .. } if village == "v2"));
        assert!(
            !reinforce_v2,
            "distant attack should not trigger evacuate: {intents:?}"
        );
        let _ = v1;
    }

    #[test]
    fn rule1_evacuate_suppresses_build_for_that_village() {
        // The evacuated village must not emit any Build intent.
        let v1 = make_village("v1", 0, 0);
        let mut v2 = make_village("v2", 5, 5);
        // Give v2 a low-level field so rule 3 would normally fire.
        v2.fields = vec![SlotLevel {
            slot: 0,
            kind: "wood".into(),
            level: 1,
        }];
        v2.garrison = vec![GarrisonEntry {
            unit: "legionnaire".into(),
            count: 20,
        }];
        v2.resources = resources_at(50);

        let now_ms = 1_700_000_000_000_i64;
        let p = persona(0);
        let mut d = two_village_digest(v1, v2);
        d.incoming_attacks = vec![Incoming {
            village: "v2".into(),
            arrive_at_ms: now_ms + 500,
        }];

        let intents = plan_tick(&d, None, &p, now_ms, "roman");
        // No Build for v2.
        let build_v2 = intents
            .iter()
            .any(|i| matches!(i, Intent::Build { village, .. } if village == "v2"));
        assert!(
            !build_v2,
            "evacuating village must not get a Build: {intents:?}"
        );
    }

    // -----------------------------------------------------------------------
    // Rule 1: Recall
    // -----------------------------------------------------------------------

    #[test]
    fn rule1_recall_when_troops_at_own_village() {
        let v1 = make_village("v1", 0, 0);
        let v2 = make_village("v2", 5, 5);

        let now_ms = 1_700_000_000_000_i64;
        let p = persona(0);
        let mut d = two_village_digest(v1, v2);
        // Troops from v1 stationed at v2.
        d.reinforcements_abroad = vec![ReinforcementAbroad {
            host_village: "v2".into(),
            x: 5,
            y: 5,
            owner: "bot".into(),
            troops: {
                let mut m = std::collections::HashMap::new();
                m.insert("legionnaire".into(), 5);
                m
            },
        }];

        let intents = plan_tick(&d, None, &p, now_ms, "roman");
        let recall = intents
            .iter()
            .find(|i| matches!(i, Intent::Recall { host, .. } if host == "v2"));
        assert!(recall.is_some(), "expected Recall for host v2: {intents:?}");
    }

    #[test]
    fn rule1_no_recall_when_troops_at_foreign_village() {
        let v1 = make_village("v1", 0, 0);
        let now_ms = 1_700_000_000_000_i64;
        let p = persona(0);
        let mut d = single_village_digest(v1);
        // Troops at a foreign village (not an own village id).
        d.reinforcements_abroad = vec![ReinforcementAbroad {
            host_village: "foreign-uuid".into(),
            x: 99,
            y: 99,
            owner: "enemy".into(),
            troops: {
                let mut m = std::collections::HashMap::new();
                m.insert("legionnaire".into(), 5);
                m
            },
        }];

        let intents = plan_tick(&d, None, &p, now_ms, "roman");
        assert!(
            !intents.iter().any(|i| matches!(i, Intent::Recall { .. })),
            "foreign host should not trigger Recall: {intents:?}"
        );
    }

    // -----------------------------------------------------------------------
    // Rule 2: Storage
    // -----------------------------------------------------------------------

    #[test]
    fn rule2_storage_granary_when_crop_near_capacity() {
        let mut v = make_village("v1", 0, 0);
        v.resources = full_resources_with_crop_low(); // crop at 92%

        let p = persona(0);
        let d = single_village_digest(v);
        let intents = plan_tick(&d, None, &p, d.now_ms, "roman");

        let build = intents
            .iter()
            .find(|i| matches!(i, Intent::Build { kind: Some(k), .. } if k == "granary"));
        assert!(
            build.is_some(),
            "expected Granary build for near-cap crop: {intents:?}"
        );
    }

    #[test]
    fn rule2_storage_warehouse_when_noncrop_near_capacity() {
        let mut v = make_village("v1", 0, 0);
        v.resources = Resources {
            wood: ResourceLine {
                amount: 950,
                rate: 30,
                capacity: 1000,
            }, // 95%
            clay: ResourceLine {
                amount: 200,
                rate: 25,
                capacity: 1000,
            },
            iron: ResourceLine {
                amount: 200,
                rate: 20,
                capacity: 1000,
            },
            crop: ResourceLine {
                amount: 200,
                rate: 30,
                capacity: 1000,
            },
        };

        let p = persona(0);
        let d = single_village_digest(v);
        let intents = plan_tick(&d, None, &p, d.now_ms, "roman");

        let build = intents
            .iter()
            .find(|i| matches!(i, Intent::Build { kind: Some(k), .. } if k == "warehouse"));
        assert!(
            build.is_some(),
            "expected Warehouse build for near-cap wood: {intents:?}"
        );
    }

    #[test]
    fn rule2_upgrades_existing_warehouse_slot() {
        let mut v = make_village("v1", 0, 0);
        v.resources = Resources {
            wood: ResourceLine {
                amount: 910,
                rate: 30,
                capacity: 1000,
            }, // 91%
            clay: ResourceLine {
                amount: 100,
                rate: 25,
                capacity: 1000,
            },
            iron: ResourceLine {
                amount: 100,
                rate: 20,
                capacity: 1000,
            },
            crop: ResourceLine {
                amount: 100,
                rate: 30,
                capacity: 1000,
            },
        };
        // Existing warehouse at slot 5.
        v.buildings.push(SlotLevel {
            slot: 5,
            kind: "warehouse".into(),
            level: 2,
        });

        let p = persona(0);
        let d = single_village_digest(v);
        let intents = plan_tick(&d, None, &p, d.now_ms, "roman");

        let build = intents.iter().find(
            |i| matches!(i, Intent::Build { slot: 5, kind: Some(k), .. } if k == "warehouse"),
        );
        assert!(
            build.is_some(),
            "should upgrade existing warehouse at slot 5: {intents:?}"
        );
    }

    #[test]
    fn rule2_no_storage_when_below_90pct() {
        let v = make_village("v1", 0, 0); // 50% resources
        let p = persona(0);
        let d = single_village_digest(v);
        let intents = plan_tick(&d, None, &p, d.now_ms, "roman");

        let storage_build = intents.iter().any(|i| {
            matches!(i, Intent::Build { kind: Some(k), .. } if k == "warehouse" || k == "granary")
        });
        assert!(!storage_build, "no storage build below 90%: {intents:?}");
    }

    #[test]
    fn rule2_no_build_when_queue_busy() {
        let mut v = make_village("v1", 0, 0);
        // Queue is non-empty.
        v.build_queue = vec![QueueEntry {
            target: "building".into(),
            slot: 5,
            kind: Some("main_building".into()),
            level: 2,
            completes_at_ms: 1_700_000_060_000,
        }];
        v.resources = Resources {
            wood: ResourceLine {
                amount: 950,
                rate: 30,
                capacity: 1000,
            },
            clay: ResourceLine {
                amount: 950,
                rate: 25,
                capacity: 1000,
            },
            iron: ResourceLine {
                amount: 950,
                rate: 20,
                capacity: 1000,
            },
            crop: ResourceLine {
                amount: 950,
                rate: 30,
                capacity: 1000,
            },
        };

        let p = persona(0);
        let d = single_village_digest(v);
        let intents = plan_tick(&d, None, &p, d.now_ms, "roman");

        let any_build = intents.iter().any(|i| matches!(i, Intent::Build { .. }));
        assert!(
            !any_build,
            "busy queue must suppress all build intents: {intents:?}"
        );
    }

    // -----------------------------------------------------------------------
    // Rule 3: Fields
    // -----------------------------------------------------------------------

    #[test]
    fn rule3_fields_upgrades_lowest_level_field() {
        let mut v = make_village("v1", 0, 0);
        v.fields = vec![
            SlotLevel {
                slot: 0,
                kind: "wood".into(),
                level: 2, // avg 1.75 < 2 — below the core-doctrine gate, so fields decide
            },
            SlotLevel {
                slot: 1,
                kind: "clay".into(),
                level: 1,
            }, // lowest
            SlotLevel {
                slot: 2,
                kind: "iron".into(),
                level: 2,
            },
            SlotLevel {
                slot: 3,
                kind: "crop".into(),
                level: 2,
            },
        ];
        v.resources.crop.rate = 50; // crop net fine

        let p = persona(0);
        let d = single_village_digest(v);
        let intents = plan_tick(&d, None, &p, d.now_ms, "roman");

        let build = intents.iter().find(|i| {
            matches!(
                i,
                Intent::Build {
                    target: "field",
                    slot: 1,
                    ..
                }
            )
        });
        assert!(
            build.is_some(),
            "should pick lowest-level field (clay slot 1): {intents:?}"
        );
    }

    #[test]
    fn rule3_fields_tiebreak_prefers_wood() {
        let mut v = make_village("v1", 0, 0);
        v.fields = vec![
            SlotLevel {
                slot: 0,
                kind: "iron".into(),
                level: 1,
            },
            SlotLevel {
                slot: 1,
                kind: "wood".into(),
                level: 1,
            }, // same level, wood preferred
            SlotLevel {
                slot: 2,
                kind: "clay".into(),
                level: 1,
            },
            SlotLevel {
                slot: 3,
                kind: "crop".into(),
                level: 1,
            },
        ];
        v.resources.crop.rate = 50;

        let p = persona(0);
        let d = single_village_digest(v);
        let intents = plan_tick(&d, None, &p, d.now_ms, "roman");

        let build = intents.iter().find(|i| {
            matches!(
                i,
                Intent::Build {
                    target: "field",
                    slot: 1,
                    ..
                }
            ) // wood at slot 1
        });
        assert!(
            build.is_some(),
            "wood should win tie at level 1: {intents:?}"
        );
    }

    #[test]
    fn rule3_fields_crop_bias_when_crop_net_low() {
        let mut v = make_village("v1", 0, 0);
        v.fields = vec![
            SlotLevel {
                slot: 0,
                kind: "wood".into(),
                level: 1,
            },
            SlotLevel {
                slot: 1,
                kind: "crop".into(),
                level: 2,
            }, // higher level but crop
        ];
        v.resources.crop.rate = 10; // below CROP_NET_FLOOR (25)

        let p = persona(0);
        let d = single_village_digest(v);
        let intents = plan_tick(&d, None, &p, d.now_ms, "roman");

        // Should prefer the crop field despite its higher level.
        let build = intents.iter().find(|i| {
            matches!(
                i,
                Intent::Build {
                    target: "field",
                    slot: 1,
                    ..
                }
            ) // crop at slot 1
        });
        assert!(
            build.is_some(),
            "crop bias should pick crop field when net < 25: {intents:?}"
        );
    }

    #[test]
    fn rule3_fields_skip_when_all_at_level_cap() {
        let mut v = make_village("v1", 0, 0);
        v.fields = vec![
            SlotLevel {
                slot: 0,
                kind: "wood".into(),
                level: 10,
            },
            SlotLevel {
                slot: 1,
                kind: "clay".into(),
                level: 10,
            },
        ];
        // Core-building gate not met (field avg = 10 ≥ 2, so core building would fire,
        // but we'll also add some core building targets to verify field rule skip).
        // For simplicity: verify no field Build is emitted.

        let p = persona(0);
        let d = single_village_digest(v);
        let intents = plan_tick(&d, None, &p, d.now_ms, "roman");

        let field_build = intents.iter().any(|i| {
            matches!(
                i,
                Intent::Build {
                    target: "field",
                    ..
                }
            )
        });
        assert!(
            !field_build,
            "all fields at cap — no field upgrade: {intents:?}"
        );
    }

    // -----------------------------------------------------------------------
    // Rule 4: Core buildings
    // -----------------------------------------------------------------------

    #[test]
    fn rule4_core_main_building_when_below_target() {
        let mut v = make_village("v1", 0, 0);
        // main_building at level 1 (< target 3).
        v.buildings = vec![
            SlotLevel {
                slot: 0,
                kind: "main_building".into(),
                level: 1,
            },
            SlotLevel {
                slot: 1,
                kind: "rally_point".into(),
                level: 1,
            },
        ];
        // Fields all at level 10 (cap) so rule 3 does not fire; avg = 10 ≥ 2 (gate met).
        v.fields = vec![
            SlotLevel {
                slot: 0,
                kind: "wood".into(),
                level: 10,
            },
            SlotLevel {
                slot: 1,
                kind: "clay".into(),
                level: 10,
            },
        ];

        let p = persona(0);
        let d = single_village_digest(v);
        let intents = plan_tick(&d, None, &p, d.now_ms, "roman");

        let build = intents.iter().find(
            |i| matches!(i, Intent::Build { slot: 0, kind: Some(k), .. } if k == "main_building"),
        );
        assert!(
            build.is_some(),
            "should upgrade main_building at slot 0: {intents:?}"
        );
    }

    // Interleave (plan 3./4. as clarified): once avg ≥ 2 the core doctrine outranks further field
    // upgrades — a bot mid-fields builds its Barracks instead of the 19th field level.
    #[test]
    fn core_doctrine_outranks_fields_once_avg_reached() {
        let mut v = make_village("v1", 0, 0);
        v.buildings = vec![
            SlotLevel {
                slot: 0,
                kind: "main_building".into(),
                level: 3, // met
            },
            SlotLevel {
                slot: 1,
                kind: "rally_point".into(),
                level: 1,
            },
        ];
        // avg = 2.5 ≥ 2; fields still below cap.
        v.fields = vec![
            SlotLevel {
                slot: 0,
                kind: "wood".into(),
                level: 3,
            },
            SlotLevel {
                slot: 1,
                kind: "clay".into(),
                level: 2,
            },
        ];
        v.resources.crop.rate = 50;
        let p = persona(0);
        let d = single_village_digest(v);
        let intents = plan_tick(&d, None, &p, d.now_ms, "roman");
        assert!(
            intents
                .iter()
                .any(|i| matches!(i, Intent::Build { kind: Some(k), .. } if k == "barracks")),
            "barracks (next unmet doctrine entry) outranks fields at avg ≥ 2: {intents:?}"
        );
        assert!(
            !intents.iter().any(|i| matches!(
                i,
                Intent::Build {
                    target: "field",
                    ..
                }
            )),
            "no field intent while the doctrine is unmet: {intents:?}"
        );
    }

    // Interleave, other side: with the doctrine COMPLETE, fields resume toward the cap.
    #[test]
    fn fields_resume_after_doctrine_complete() {
        let mut v = make_village("v1", 0, 0);
        // All doctrine entries met:
        //   main_building≥3 ✓, barracks≥3 ✓, warehouse≥3 ✓, granary≥3 ✓,
        //   main_building≥5 ✓, academy≥1 ✓, residence≥10 ✓
        v.buildings = vec![
            SlotLevel {
                slot: 0,
                kind: "main_building".into(),
                level: 5,
            },
            SlotLevel {
                slot: 2,
                kind: "barracks".into(),
                level: 3,
            },
            SlotLevel {
                slot: 3,
                kind: "warehouse".into(),
                level: 3,
            },
            SlotLevel {
                slot: 4,
                kind: "granary".into(),
                level: 3,
            },
            SlotLevel {
                slot: 5,
                kind: "academy".into(),
                level: 1,
            },
            SlotLevel {
                slot: 6,
                kind: "residence".into(),
                level: 10,
            },
        ];
        v.fields = vec![
            SlotLevel {
                slot: 0,
                kind: "wood".into(),
                level: 4,
            },
            SlotLevel {
                slot: 1,
                kind: "clay".into(),
                level: 3, // lowest — the expected pick
            },
        ];
        v.resources.crop.rate = 50;
        let p = persona(0);
        let d = single_village_digest(v);
        let intents = plan_tick(&d, None, &p, d.now_ms, "roman");
        assert!(
            intents.iter().any(|i| matches!(
                i,
                Intent::Build {
                    target: "field",
                    slot: 1,
                    ..
                }
            )),
            "fields resume (lowest first) once the doctrine is complete: {intents:?}"
        );
    }

    #[test]
    fn rule4_core_skipped_when_field_avg_below_2() {
        let mut v = make_village("v1", 0, 0);
        v.buildings = vec![
            SlotLevel {
                slot: 0,
                kind: "main_building".into(),
                level: 1,
            }, // upgrade target
            SlotLevel {
                slot: 1,
                kind: "rally_point".into(),
                level: 1,
            },
        ];
        // Field avg = 1 (gate NOT met).
        v.fields = vec![
            SlotLevel {
                slot: 0,
                kind: "wood".into(),
                level: 1,
            },
            SlotLevel {
                slot: 1,
                kind: "clay".into(),
                level: 1,
            },
        ];
        v.resources.crop.rate = 50; // crop fine → fields rule picks slot 0 (wood, level 1)

        let p = persona(0);
        let d = single_village_digest(v);
        let intents = plan_tick(&d, None, &p, d.now_ms, "roman");

        // Core building rule should not fire; field rule fires instead.
        let core_build = intents
            .iter()
            .find(|i| matches!(i, Intent::Build { kind: Some(k), .. } if k == "main_building"));
        assert!(
            core_build.is_none(),
            "core rule must not fire when field avg < 2: {intents:?}"
        );
        // Field rule should have fired.
        let field_build = intents.iter().any(|i| {
            matches!(
                i,
                Intent::Build {
                    target: "field",
                    ..
                }
            )
        });
        assert!(field_build, "field rule should fire instead: {intents:?}");
    }

    #[test]
    fn rule4_core_places_barracks_on_free_slot() {
        let mut v = make_village("v1", 0, 0);
        // main_building already at level 3 — barracks is next in doctrine.
        v.buildings = vec![
            SlotLevel {
                slot: 0,
                kind: "main_building".into(),
                level: 3,
            },
            SlotLevel {
                slot: 1,
                kind: "rally_point".into(),
                level: 1,
            },
        ];
        // Fields all at cap so rule 3 is dormant; avg = 10 ≥ 2 (gate met).
        v.fields = vec![
            SlotLevel {
                slot: 0,
                kind: "wood".into(),
                level: 10,
            },
            SlotLevel {
                slot: 1,
                kind: "clay".into(),
                level: 10,
            },
        ];

        let p = persona(0);
        let d = single_village_digest(v);
        let intents = plan_tick(&d, None, &p, d.now_ms, "roman");

        let build = intents
            .iter()
            .find(|i| matches!(i, Intent::Build { kind: Some(k), .. } if k == "barracks"));
        assert!(
            build.is_some(),
            "should place barracks on free slot: {intents:?}"
        );
        if let Some(Intent::Build { slot, .. }) = build {
            // First free general slot after reserved 0, 1 (slot 2 is next).
            assert!(
                !RESERVED_SLOTS.contains(slot),
                "barracks must not go on reserved slot"
            );
        }
    }

    #[test]
    fn rule4_core_doctrine_order_first_match_wins() {
        let mut v = make_village("v1", 0, 0);
        // main_building at level 1 (< 3) AND barracks absent → main_building fires first.
        v.buildings = vec![
            SlotLevel {
                slot: 0,
                kind: "main_building".into(),
                level: 1,
            },
            SlotLevel {
                slot: 1,
                kind: "rally_point".into(),
                level: 1,
            },
        ];
        // Fields at cap so rule 3 is dormant; avg = 10 ≥ 2 (gate met).
        v.fields = vec![
            SlotLevel {
                slot: 0,
                kind: "wood".into(),
                level: 10,
            },
            SlotLevel {
                slot: 1,
                kind: "clay".into(),
                level: 10,
            },
        ];

        let p = persona(0);
        let d = single_village_digest(v);
        let intents = plan_tick(&d, None, &p, d.now_ms, "roman");

        // Only one build intent; it is for main_building, not barracks.
        let builds: Vec<_> = intents
            .iter()
            .filter(|i| matches!(i, Intent::Build { .. }))
            .collect();
        assert_eq!(builds.len(), 1, "exactly one build intent: {intents:?}");
        assert!(
            matches!(builds[0], Intent::Build { kind: Some(k), .. } if k == "main_building"),
            "first match must be main_building: {intents:?}"
        );
    }

    // -----------------------------------------------------------------------
    // Rule 5: Training
    // -----------------------------------------------------------------------

    #[test]
    fn rule5_training_fires_when_garrison_below_floor() {
        let mut v = make_village("v1", 0, 0);
        let p = persona(0); // floor = 10 + 10*0 = 10
        v.garrison = vec![GarrisonEntry {
            unit: "legionnaire".into(),
            count: 5,
        }]; // below 10

        let d = single_village_digest(v);
        let intents = plan_tick(&d, None, &p, d.now_ms, "roman");

        let train = intents
            .iter()
            .find(|i| matches!(i, Intent::Train { unit, .. } if unit == "legionnaire"));
        assert!(
            train.is_some(),
            "should train when garrison below floor: {intents:?}"
        );
        if let Some(Intent::Train { count, .. }) = train {
            // needed = 5, count = min(5, 5) = 5
            assert_eq!(*count, 5);
        }
    }

    #[test]
    fn rule5_training_capped_at_5_per_tick() {
        let mut v = make_village("v1", 0, 0);
        let p = persona(3); // floor = 10 + 30 = 40
        v.garrison = vec![]; // garrison = 0, needed = 40

        let d = single_village_digest(v);
        let intents = plan_tick(&d, None, &p, d.now_ms, "roman");

        // tribe="roman" passed, so unit is legionnaire (not clubswinger)
        let train = intents.iter().find(|i| matches!(i, Intent::Train { .. }));
        assert!(train.is_some(), "should train: {intents:?}");
        if let Some(Intent::Train { count, .. }) = train {
            assert_eq!(*count, TRAIN_CAP_PER_TICK, "capped at 5 per tick");
        }
    }

    #[test]
    fn rule5_training_respects_tribe_tier1() {
        let mut v = make_village("v1", 0, 0);
        let p = persona(0);
        v.garrison = vec![]; // garrison empty, needs training

        let d = single_village_digest(v.clone());
        let intents_roman = plan_tick(&d, None, &p, d.now_ms, "roman");
        let train_roman = intents_roman
            .iter()
            .find(|i| matches!(i, Intent::Train { unit, .. } if unit == "legionnaire"));
        assert!(
            train_roman.is_some(),
            "roman gets legionnaire: {intents_roman:?}"
        );

        let intents_teuton = plan_tick(&d, None, &p, d.now_ms, "teuton");
        let train_teuton = intents_teuton
            .iter()
            .find(|i| matches!(i, Intent::Train { unit, .. } if unit == "clubswinger"));
        assert!(
            train_teuton.is_some(),
            "teuton gets clubswinger: {intents_teuton:?}"
        );

        let intents_gaul = plan_tick(&d, None, &p, d.now_ms, "gaul");
        let train_gaul = intents_gaul
            .iter()
            .find(|i| matches!(i, Intent::Train { unit, .. } if unit == "phalanx"));
        assert!(train_gaul.is_some(), "gaul gets phalanx: {intents_gaul:?}");
    }

    #[test]
    fn rule5_no_training_when_garrison_at_floor() {
        let mut v = make_village("v1", 0, 0);
        let p = persona(0); // floor = 10
        v.garrison = vec![GarrisonEntry {
            unit: "legionnaire".into(),
            count: 10,
        }]; // exactly at floor

        let d = single_village_digest(v);
        let intents = plan_tick(&d, None, &p, d.now_ms, "roman");

        let train = intents.iter().any(|i| matches!(i, Intent::Train { .. }));
        assert!(!train, "garrison at floor → no training: {intents:?}");
    }

    // -----------------------------------------------------------------------
    // Rule 6: Settling
    // -----------------------------------------------------------------------

    fn settle_ready_village(id: &str, x: i32, y: i32) -> VillageDigest {
        let mut v = make_village(id, x, y);
        // Residence at level 10.
        v.buildings.push(SlotLevel {
            slot: 5,
            kind: "residence".into(),
            level: 10,
        });
        // 20 troops so garrison floor is met.
        v.garrison = vec![GarrisonEntry {
            unit: "legionnaire".into(),
            count: 20,
        }];
        v
    }

    fn culture_allows_more() -> Culture {
        Culture {
            cp: 1000,
            rate_per_hour: 10,
            villages_used: 1,
            villages_allowed: 2,
            next_threshold: 2000,
        }
    }

    #[test]
    fn rule6_train_settlers_when_none_present() {
        let v = settle_ready_village("v1", 0, 0);
        let now_ms = 1_700_000_000_000_i64;
        let p = persona(0);
        let mut d = single_village_digest(v);
        d.culture = culture_allows_more();

        let intents = plan_tick(&d, None, &p, now_ms, "roman");
        let ts = intents
            .iter()
            .find(|i| matches!(i, Intent::TrainSettlers { count: 3, .. }));
        assert!(ts.is_some(), "should train 3 settlers: {intents:?}");
    }

    #[test]
    fn rule6_train_remaining_settlers() {
        let mut v = settle_ready_village("v1", 0, 0);
        // 1 settler already in garrison.
        v.garrison.push(GarrisonEntry {
            unit: "settler".into(),
            count: 1,
        });

        let now_ms = 1_700_000_000_000_i64;
        let p = persona(0);
        let mut d = single_village_digest(v);
        d.culture = culture_allows_more();

        let intents = plan_tick(&d, None, &p, now_ms, "roman");
        let ts = intents
            .iter()
            .find(|i| matches!(i, Intent::TrainSettlers { count: 2, .. }));
        assert!(ts.is_some(), "should train 2 more settlers: {intents:?}");
    }

    #[test]
    fn rule6_settle_when_3_settlers_and_free_valley_in_map() {
        let mut v = settle_ready_village("v1", 0, 0);
        v.garrison = vec![
            GarrisonEntry {
                unit: "legionnaire".into(),
                count: 20,
            },
            GarrisonEntry {
                unit: "settler".into(),
                count: 3,
            },
        ];

        let map = MapWindow {
            center_x: 0,
            center_y: 0,
            r: 5,
            rows: vec![vec![MapCell {
                cell_class: "map-grid__cell".into(),
                label: "Empty valley (3, 4)".into(),
                href: None,
                settle: true,
                x: 3,
                y: 4,
            }]],
        };

        let now_ms = 1_700_000_000_000_i64;
        let p = persona(0);
        let mut d = single_village_digest(v);
        d.culture = culture_allows_more();

        let intents = plan_tick(&d, Some(&map), &p, now_ms, "roman");
        let settle = intents
            .iter()
            .find(|i| matches!(i, Intent::Settle { x: 3, y: 4, .. }));
        assert!(
            settle.is_some(),
            "should settle nearest free valley: {intents:?}"
        );
    }

    #[test]
    fn rule6_no_settle_when_culture_full() {
        let v = settle_ready_village("v1", 0, 0);
        let now_ms = 1_700_000_000_000_i64;
        let p = persona(0);
        // villages_used == villages_allowed → no settling.
        let d = single_village_digest(v);
        // d.culture.villages_used == villages_allowed (both 1 from helper).

        let intents = plan_tick(&d, None, &p, now_ms, "roman");
        let settling = intents
            .iter()
            .any(|i| matches!(i, Intent::TrainSettlers { .. } | Intent::Settle { .. }));
        assert!(!settling, "culture full → no settling: {intents:?}");
    }

    // -----------------------------------------------------------------------
    // Rule 7: Raiding
    // -----------------------------------------------------------------------

    fn map_with_inactive(x: i32, y: i32) -> MapWindow {
        MapWindow {
            center_x: 0,
            center_y: 0,
            r: 15,
            rows: vec![vec![MapCell {
                cell_class: "map-grid__cell".into(),
                label: format!("VillageName (inactive) ({x}, {y})"),
                href: None,
                settle: false,
                x,
                y,
            }]],
        }
    }

    #[test]
    fn rule7_raid_fires_for_aggression_1() {
        let mut v = make_village("v1", 0, 0);
        let p = persona(1); // aggression=1, floor=15+5=20, raid_range=varies
        // Enough garrison: floor=20, party=min(8+4,20/3)=min(12,6)=6
        v.garrison = vec![GarrisonEntry {
            unit: "legionnaire".into(),
            count: 30,
        }];

        let map = map_with_inactive(3, 3);
        let now_ms = 1_700_000_000_000_i64;
        let d = single_village_digest(v);
        let intents = plan_tick(&d, Some(&map), &p, now_ms, "roman");

        let raid = intents
            .iter()
            .find(|i| matches!(i, Intent::Raid { x: 3, y: 3, .. }));
        assert!(raid.is_some(), "should raid inactive target: {intents:?}");
    }

    #[test]
    fn rule7_no_raid_when_garrison_below_floor() {
        let mut v = make_village("v1", 0, 0);
        let p = Persona {
            window_start_hour: 0,
            window_len_hours: 24,
            tick_min_secs: 300,
            tick_max_secs: 720,
            aggression: 1,
            raid_range: 10,
        };
        // floor = 15 + 5*1 = 20; garrison = 15 (below floor)
        v.garrison = vec![GarrisonEntry {
            unit: "legionnaire".into(),
            count: 15,
        }];

        let map = map_with_inactive(2, 2);
        let now_ms = 1_700_000_000_000_i64;
        let d = single_village_digest(v);
        let intents = plan_tick(&d, Some(&map), &p, now_ms, "roman");

        let raid = intents.iter().any(|i| matches!(i, Intent::Raid { .. }));
        assert!(
            !raid,
            "garrison below floor must block raiding: {intents:?}"
        );
    }

    #[test]
    fn rule7_no_raid_when_aggression_0() {
        let mut v = make_village("v1", 0, 0);
        let p = persona(0); // aggression=0 → no raiding
        v.garrison = vec![GarrisonEntry {
            unit: "legionnaire".into(),
            count: 100,
        }];

        let map = map_with_inactive(2, 2);
        let now_ms = 1_700_000_000_000_i64;
        let d = single_village_digest(v);
        let intents = plan_tick(&d, Some(&map), &p, now_ms, "roman");

        let raid = intents.iter().any(|i| matches!(i, Intent::Raid { .. }));
        assert!(!raid, "aggression=0 → no raids: {intents:?}");
    }

    #[test]
    fn rule7_skip_in_flight_target() {
        let mut v = make_village("v1", 0, 0);
        let p = Persona {
            window_start_hour: 0,
            window_len_hours: 24,
            tick_min_secs: 300,
            tick_max_secs: 720,
            aggression: 2,
            raid_range: 10,
        };
        v.garrison = vec![GarrisonEntry {
            unit: "legionnaire".into(),
            count: 60,
        }];

        let map = map_with_inactive(3, 3);
        let now_ms = 1_700_000_000_000_i64;
        let mut d = single_village_digest(v);
        // Already raiding (3, 3).
        d.movements = vec![MovementEntry {
            kind: "raid".into(),
            dest_x: 3,
            dest_y: 3,
            arrive_at_ms: now_ms + 60_000,
            troops: Default::default(),
        }];

        let intents = plan_tick(&d, Some(&map), &p, now_ms, "roman");
        let raid = intents
            .iter()
            .any(|i| matches!(i, Intent::Raid { x: 3, y: 3, .. }));
        assert!(!raid, "in-flight target must be skipped: {intents:?}");
    }

    #[test]
    fn rule7_skip_out_of_range_target() {
        let mut v = make_village("v1", 0, 0);
        let p = Persona {
            window_start_hour: 0,
            window_len_hours: 24,
            tick_min_secs: 300,
            tick_max_secs: 720,
            aggression: 1,
            raid_range: 6, // Chebyshev 6
        };
        v.garrison = vec![GarrisonEntry {
            unit: "legionnaire".into(),
            count: 60,
        }];

        // Target at Chebyshev distance max(10, 0) = 10 > 6.
        let map = map_with_inactive(10, 0);
        let now_ms = 1_700_000_000_000_i64;
        let d = single_village_digest(v);
        let intents = plan_tick(&d, Some(&map), &p, now_ms, "roman");

        let raid = intents.iter().any(|i| matches!(i, Intent::Raid { .. }));
        assert!(!raid, "out-of-range target must be skipped: {intents:?}");
    }

    #[test]
    fn rule7_raids_up_to_aggression_targets() {
        let mut v = make_village("v1", 0, 0);
        let p = Persona {
            window_start_hour: 0,
            window_len_hours: 24,
            tick_min_secs: 300,
            tick_max_secs: 720,
            aggression: 2,
            raid_range: 15,
        };
        v.garrison = vec![GarrisonEntry {
            unit: "legionnaire".into(),
            count: 90,
        }];

        // Three inactive targets within range.
        let map = MapWindow {
            center_x: 0,
            center_y: 0,
            r: 15,
            rows: vec![vec![
                MapCell {
                    cell_class: "".into(),
                    label: "A (inactive) (1, 0)".into(),
                    settle: false,
                    x: 1,
                    y: 0,
                    href: None,
                },
                MapCell {
                    cell_class: "".into(),
                    label: "B (inactive) (2, 0)".into(),
                    settle: false,
                    x: 2,
                    y: 0,
                    href: None,
                },
                MapCell {
                    cell_class: "".into(),
                    label: "C (inactive) (3, 0)".into(),
                    settle: false,
                    x: 3,
                    y: 0,
                    href: None,
                },
            ]],
        };

        let now_ms = 1_700_000_000_000_i64;
        let d = single_village_digest(v);
        let intents = plan_tick(&d, Some(&map), &p, now_ms, "roman");

        let raids: Vec<_> = intents
            .iter()
            .filter(|i| matches!(i, Intent::Raid { .. }))
            .collect();
        // aggression=2 → at most 2 raids.
        assert_eq!(
            raids.len(),
            2,
            "should raid up to aggression=2 targets: {intents:?}"
        );
        // Should be the 2 nearest: (1,0) and (2,0).
        assert!(
            raids
                .iter()
                .any(|i| matches!(i, Intent::Raid { x: 1, y: 0, .. }))
        );
        assert!(
            raids
                .iter()
                .any(|i| matches!(i, Intent::Raid { x: 2, y: 0, .. }))
        );
    }

    // -----------------------------------------------------------------------
    // Priority ordering
    // -----------------------------------------------------------------------

    #[test]
    fn storage_beats_fields_in_priority() {
        let mut v = make_village("v1", 0, 0);
        // Both storage rule (crop near cap) and fields rule would apply.
        v.resources = full_resources_with_crop_low(); // crop 92%, low net
        // Fields below level 10.
        v.fields = vec![SlotLevel {
            slot: 0,
            kind: "crop".into(),
            level: 2,
        }];

        let p = persona(0);
        let d = single_village_digest(v);
        let intents = plan_tick(&d, None, &p, d.now_ms, "roman");

        // Storage (Granary) must win over field upgrade.
        let granary = intents
            .iter()
            .any(|i| matches!(i, Intent::Build { kind: Some(k), .. } if k == "granary"));
        assert!(
            granary,
            "Granary (rule 2) beats field upgrade (rule 3): {intents:?}"
        );

        let field_build = intents.iter().any(|i| {
            matches!(
                i,
                Intent::Build {
                    target: "field",
                    ..
                }
            )
        });
        assert!(
            !field_build,
            "field build must not fire when storage fires: {intents:?}"
        );
    }

    // -----------------------------------------------------------------------
    // M1: Doctrine walk test — verifies the DOCTRINE table is prereq-consistent
    //     against the classic preset (specs/balance/presets/classic/construction.toml).
    // -----------------------------------------------------------------------

    #[test]
    fn doctrine_table_walks_to_completion() {
        // Prereq-consistent against the classic preset; verified in this walk test.
        //   barracks  requires main_building≥3  → MB 3 comes before barracks
        //   academy   requires barracks≥3        → barracks 3 comes before academy
        //   residence requires main_building≥5   → MB 5 comes before residence
        //
        // Start: fields avg=2 (gate met), only main_building=1 + rally_point.
        let p = Persona {
            window_start_hour: 0,
            window_len_hours: 24,
            tick_min_secs: 300,
            tick_max_secs: 720,
            aggression: 0,
            raid_range: 8,
        };

        let mut buildings: Vec<SlotLevel> = vec![
            SlotLevel {
                slot: 0,
                kind: "main_building".into(),
                level: 1,
            },
            SlotLevel {
                slot: 1,
                kind: "rally_point".into(),
                level: 1,
            },
        ];
        let fields: Vec<SlotLevel> = vec![
            SlotLevel {
                slot: 0,
                kind: "wood".into(),
                level: 2,
            },
            SlotLevel {
                slot: 1,
                kind: "clay".into(),
                level: 2,
            },
            SlotLevel {
                slot: 2,
                kind: "iron".into(),
                level: 2,
            },
            SlotLevel {
                slot: 3,
                kind: "crop".into(),
                level: 2,
            },
        ];

        let mut sequence: Vec<(String, u8)> = vec![];
        let mut step_count = 0u32;

        loop {
            let v = VillageDigest {
                id: "v1".to_owned(),
                x: 0,
                y: 0,
                capital: false,
                resources: resources_at(50),
                fields: fields.clone(),
                buildings: buildings.clone(),
                build_queue: vec![], // empty → build intent always fires
                training: vec![],
                garrison: vec![GarrisonEntry {
                    unit: "legionnaire".into(),
                    count: 10,
                }],
                reinforcements_here: vec![],
                research: Default::default(),
            };
            let d = Digest {
                world: "world-0001".into(),
                player: "42".into(),
                now_ms: 1_700_000_000_000,
                villages: vec![v],
                culture: Culture {
                    villages_used: 1,
                    villages_allowed: 1,
                    ..Default::default()
                },
                ..Default::default()
            };

            let intents = plan_tick(&d, None, &p, d.now_ms, "roman");

            // Find the Build intent for a BUILDING (not a field).
            let build_result: Option<(String, u8)> = intents.iter().find_map(|i| {
                if let Intent::Build {
                    kind: Some(k),
                    slot: s,
                    target,
                    ..
                } = i
                {
                    if *target == "building" {
                        Some((k.clone(), *s))
                    } else {
                        None
                    }
                } else {
                    None
                }
            });

            let (kind, slot) = match build_result {
                None => break,
                Some(pair) => pair,
            };

            // Prereq checks at emission time (before updating state):
            //   academy emission → barracks must be ≥3  (construction.toml: academy prereqs barracks≥3)
            //   residence emission → main_building must be ≥5  (construction.toml: residence prereq MB≥5)
            if kind == "academy" {
                let barracks_lvl = buildings
                    .iter()
                    .find(|b| b.kind == "barracks")
                    .map(|b| b.level)
                    .unwrap_or(0);
                assert!(
                    barracks_lvl >= 3,
                    "step {step_count}: academy emitted before barracks≥3 (barracks={barracks_lvl}); seq={sequence:?}"
                );
            }
            if kind == "residence" {
                let mb_lvl = buildings
                    .iter()
                    .find(|b| b.kind == "main_building")
                    .map(|b| b.level)
                    .unwrap_or(0);
                assert!(
                    mb_lvl >= 5,
                    "step {step_count}: residence emitted before main_building≥5 (mb={mb_lvl}); seq={sequence:?}"
                );
            }

            // Simulate the build: increment existing building or place new one.
            if let Some(b) = buildings.iter_mut().find(|b| b.kind == kind) {
                b.level += 1;
                sequence.push((kind.clone(), b.level));
            } else {
                buildings.push(SlotLevel {
                    slot,
                    kind: kind.clone(),
                    level: 1,
                });
                sequence.push((kind.clone(), 1u8));
            }

            step_count += 1;
            assert!(
                step_count <= 30,
                "doctrine walk exceeded 30 steps; seq={sequence:?}"
            );

            // Done when residence reaches level 10.
            let res_level = buildings
                .iter()
                .find(|b| b.kind == "residence")
                .map(|b| b.level)
                .unwrap_or(0);
            if res_level >= 10 {
                break;
            }
        }

        // Final assertion: residence must be at level 10.
        let res_level = buildings
            .iter()
            .find(|b| b.kind == "residence")
            .map(|b| b.level)
            .unwrap_or(0);
        assert_eq!(
            res_level, 10,
            "walk did not reach residence=10; steps={step_count}, seq={sequence:?}"
        );
    }

    // -----------------------------------------------------------------------
    // M2: Raid overdraw test — total sent across raids ≤ garrison − floor
    // -----------------------------------------------------------------------

    #[test]
    fn rule7_raid_never_below_garrison_floor_across_multiple_raids() {
        // garrison=40, aggression=2, floor=15+5*2=25, remaining budget=15
        // party per target = min(8+8=16, 40/3=13, 15) = 13
        //   → first raid: 13 sent, remaining=2
        //   → second target: party = min(16,13,2)=2 < 4 → STOP
        // Total sent: 13 ≤ 15 (the budget). Floor (25) is never breached.
        let mut v = make_village("v1", 0, 0);
        let p = Persona {
            window_start_hour: 0,
            window_len_hours: 24,
            tick_min_secs: 300,
            tick_max_secs: 720,
            aggression: 2,
            raid_range: 15,
        };
        v.garrison = vec![GarrisonEntry {
            unit: "legionnaire".into(),
            count: 40,
        }];

        // Two inactive targets in range.
        let map = MapWindow {
            center_x: 0,
            center_y: 0,
            r: 15,
            rows: vec![vec![
                MapCell {
                    cell_class: "".into(),
                    label: "A (inactive) (1, 0)".into(),
                    settle: false,
                    x: 1,
                    y: 0,
                    href: None,
                },
                MapCell {
                    cell_class: "".into(),
                    label: "B (inactive) (2, 0)".into(),
                    settle: false,
                    x: 2,
                    y: 0,
                    href: None,
                },
            ]],
        };

        let now_ms = 1_700_000_000_000_i64;
        let d = single_village_digest(v);
        let intents = plan_tick(&d, Some(&map), &p, now_ms, "roman");

        let raids: Vec<_> = intents
            .iter()
            .filter(|i| matches!(i, Intent::Raid { .. }))
            .collect();
        // Only one raid (second target skipped: remaining=2 < min_party=4).
        assert_eq!(
            raids.len(),
            1,
            "second target must be skipped when budget exhausted: {intents:?}"
        );

        // Total units sent ≤ garrison − floor (15).
        let total_sent: u32 = raids
            .iter()
            .map(|i| {
                if let Intent::Raid { units, .. } = i {
                    units.values().sum()
                } else {
                    0
                }
            })
            .sum();
        assert!(
            total_sent <= 15,
            "total sent {total_sent} exceeds budget of 15: {intents:?}"
        );
    }

    // -----------------------------------------------------------------------
    // S5: In-training units count toward the garrison floor (no duplicate Train)
    //     and in-training settlers count toward the settler quota.
    // -----------------------------------------------------------------------

    #[test]
    fn rule5_no_train_when_batch_in_training_fills_floor() {
        // garrison=9, floor=10 (aggression=0). One legionnaire still training → effective=10 = floor.
        let mut v = make_village("v1", 0, 0);
        let p = persona(0); // floor = 10
        v.garrison = vec![GarrisonEntry {
            unit: "legionnaire".into(),
            count: 9,
        }];
        v.training = vec![crate::digest::TrainingEntry {
            building: "barracks".into(),
            unit: "legionnaire".into(),
            remaining: 1,
            next_complete_at_ms: 1_700_000_060_000,
        }];

        let d = single_village_digest(v);
        let intents = plan_tick(&d, None, &p, d.now_ms, "roman");

        let train = intents.iter().any(|i| matches!(i, Intent::Train { .. }));
        assert!(
            !train,
            "in-training unit fills floor → no duplicate Train: {intents:?}"
        );
    }

    #[test]
    fn rule6_no_train_settlers_when_one_in_training() {
        // 2 settlers in garrison + 1 in training → total=3 → no TrainSettlers.
        let mut v = settle_ready_village("v1", 0, 0);
        v.garrison.push(GarrisonEntry {
            unit: SETTLER_UNIT.into(),
            count: 2,
        });
        v.training = vec![crate::digest::TrainingEntry {
            building: "barracks".into(),
            unit: SETTLER_UNIT.into(),
            remaining: 1,
            next_complete_at_ms: 1_700_000_060_000,
        }];

        let now_ms = 1_700_000_000_000_i64;
        let p = persona(0);
        let mut d = single_village_digest(v);
        d.culture = culture_allows_more();

        let intents = plan_tick(&d, None, &p, now_ms, "roman");
        let ts = intents
            .iter()
            .any(|i| matches!(i, Intent::TrainSettlers { .. }));
        assert!(
            !ts,
            "2 in garrison + 1 in training = 3 total → no TrainSettlers: {intents:?}"
        );
    }

    // -----------------------------------------------------------------------
    // Determinism
    // -----------------------------------------------------------------------

    #[test]
    fn deterministic_same_inputs_same_output() {
        let v = make_village("v1", 0, 0);
        let p = persona(2);
        let d = single_village_digest(v);

        let result1 = plan_tick(&d, None, &p, d.now_ms, "roman");
        let result2 = plan_tick(&d, None, &p, d.now_ms, "roman");
        assert_eq!(
            result1, result2,
            "identical inputs must yield identical intents"
        );
    }
}
