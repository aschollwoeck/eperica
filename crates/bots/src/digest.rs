//! Serde DTOs mirroring the agent-API wire shapes (docs/agent-api.md).
//!
//! Every struct derives `Default` and every field carries `#[serde(default)]`
//! so that additive server changes (new fields, new top-level keys) never
//! break parsing — unknown fields are silently ignored by serde.

use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Shared building blocks
// ---------------------------------------------------------------------------

/// One resource line: current amount, hourly net rate, and storage capacity.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct ResourceLine {
    #[serde(default)]
    pub amount: i64,
    #[serde(default)]
    pub rate: i64,
    #[serde(default)]
    pub capacity: i64,
}

/// The four resource lines for a village (wood / clay / iron / crop).
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct Resources {
    #[serde(default)]
    pub wood: ResourceLine,
    #[serde(default)]
    pub clay: ResourceLine,
    #[serde(default)]
    pub iron: ResourceLine,
    #[serde(default)]
    pub crop: ResourceLine,
}

/// A field or building slot with its current level.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct SlotLevel {
    #[serde(default)]
    pub slot: u8,
    /// Kind slug (e.g. `"wood"`, `"main_building"`).
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub level: u8,
}

/// One entry in the construction queue.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct QueueEntry {
    /// `"field"` or `"building"`.
    #[serde(default)]
    pub target: String,
    #[serde(default)]
    pub slot: u8,
    /// Present only for building orders (absent for field orders — `None` via `#[serde(default)]`).
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub level: u8,
    #[serde(default)]
    pub completes_at_ms: i64,
}

/// One active training batch.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct TrainingEntry {
    /// Building slug (e.g. `"barracks"`, `"stable"`).
    #[serde(default)]
    pub building: String,
    /// Unit slug (e.g. `"legionnaire"`).
    #[serde(default)]
    pub unit: String,
    /// Units still to complete.
    #[serde(default)]
    pub remaining: u32,
    #[serde(default)]
    pub next_complete_at_ms: i64,
}

/// One unit stack in the garrison.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct GarrisonEntry {
    #[serde(default)]
    pub unit: String,
    #[serde(default)]
    pub count: u32,
}

// ---------------------------------------------------------------------------
// Research
// ---------------------------------------------------------------------------

/// Per-unit smithy/academy level.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct UnitLevel {
    #[serde(default)]
    pub unit: String,
    #[serde(default)]
    pub level: u8,
}

/// An active research or smithy order.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct ActiveOrderEntry {
    /// `"research"` or `"smithy"`.
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub unit: String,
    /// Present only for smithy orders; absent for research (`None` via `#[serde(default)]`).
    #[serde(default)]
    pub target_level: Option<u8>,
    #[serde(default)]
    pub complete_at_ms: i64,
}

/// Research state for one village.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct Research {
    /// Fully researched unit slugs.
    #[serde(default)]
    pub researched: Vec<String>,
    /// Per-unit smithy levels.
    #[serde(default)]
    pub levels: Vec<UnitLevel>,
    /// Active research / smithy orders.
    #[serde(default)]
    pub active: Vec<ActiveOrderEntry>,
}

// ---------------------------------------------------------------------------
// Reinforcement groups
// ---------------------------------------------------------------------------

/// A reinforcement group stationed at this village (owned by another player).
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct ReinforcementHere {
    /// Home village UUID of the reinforcing player (hyphenated).
    #[serde(default)]
    pub home_village: String,
    #[serde(default)]
    pub x: i32,
    #[serde(default)]
    pub y: i32,
    /// Owner username of the reinforcing player.
    #[serde(default)]
    pub owner: String,
    #[serde(default)]
    pub troops: HashMap<String, u32>,
}

/// A reinforcement group the player has stationed at a foreign village.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct ReinforcementAbroad {
    /// The host village UUID (where the troops are stationed, hyphenated).
    #[serde(default)]
    pub host_village: String,
    /// Host village x coordinate.
    #[serde(default)]
    pub x: i32,
    /// Host village y coordinate.
    #[serde(default)]
    pub y: i32,
    /// Owner username of the host village.
    #[serde(default)]
    pub owner: String,
    #[serde(default)]
    pub troops: HashMap<String, u32>,
}

// ---------------------------------------------------------------------------
// Village digest
// ---------------------------------------------------------------------------

/// Full state for one village — part of the top-level [`Digest`].
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct VillageDigest {
    /// Hyphenated village UUID.
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub x: i32,
    #[serde(default)]
    pub y: i32,
    #[serde(default)]
    pub capital: bool,
    #[serde(default)]
    pub resources: Resources,
    /// Resource fields (up to 18 slots).
    #[serde(default)]
    pub fields: Vec<SlotLevel>,
    /// Infrastructure buildings.
    #[serde(default)]
    pub buildings: Vec<SlotLevel>,
    /// Queue (0–2 entries depending on tribe / Main Building level).
    #[serde(default)]
    pub build_queue: Vec<QueueEntry>,
    /// Active training batches.
    #[serde(default)]
    pub training: Vec<TrainingEntry>,
    /// Own troops currently stationed here.
    #[serde(default)]
    pub garrison: Vec<GarrisonEntry>,
    /// Allied reinforcement groups stationed here.
    #[serde(default)]
    pub reinforcements_here: Vec<ReinforcementHere>,
    /// Research and smithy state.
    #[serde(default)]
    pub research: Research,
}

// ---------------------------------------------------------------------------
// Player-level digest fields
// ---------------------------------------------------------------------------

/// Culture-point tally and village-slot budget.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct Culture {
    #[serde(default)]
    pub cp: i64,
    #[serde(default)]
    pub rate_per_hour: i64,
    #[serde(default)]
    pub villages_used: u32,
    #[serde(default)]
    pub villages_allowed: u32,
    #[serde(default)]
    pub next_threshold: i64,
}

/// One incoming attack head (fog-of-war: target village + arrival time only).
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct Incoming {
    /// Target village UUID.
    #[serde(default)]
    pub village: String,
    #[serde(default)]
    pub arrive_at_ms: i64,
}

/// Summary of a past battle report.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct ReportHead {
    /// Decimal report ID string.
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub occurred_at_ms: i64,
    #[serde(default)]
    pub attacker_won: bool,
    /// Movement kind (e.g. `"attack"`, `"raid"`).
    #[serde(default)]
    pub kind: String,
}

/// One in-flight movement owned by this player.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct MovementEntry {
    /// `"attack"`, `"raid"`, `"reinforce"`, `"return"`, `"settle"`, etc.
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub dest_x: i32,
    #[serde(default)]
    pub dest_y: i32,
    #[serde(default)]
    pub arrive_at_ms: i64,
    /// Unit composition map (`unit_slug → count`).
    #[serde(default)]
    pub troops: HashMap<String, u32>,
}

/// Summary of a scout report (intel is in the full `/scout-report/{id}` read).
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct ScoutReportHead {
    /// Decimal report ID string.
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub occurred_at_ms: i64,
    #[serde(default)]
    pub viewer_is_scouter: bool,
    #[serde(default)]
    pub detected: bool,
}

// ---------------------------------------------------------------------------
// Top-level digest
// ---------------------------------------------------------------------------

/// The full state digest returned by `GET /api/w/{world}/state`.
///
/// Every field defaults to a zero value so additive server changes never break
/// parsing (unknown JSON keys are silently ignored).
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct Digest {
    #[serde(default)]
    pub world: String,
    #[serde(default)]
    pub player: String,
    /// Server clock at the time the digest was assembled (Unix milliseconds).
    #[serde(default)]
    pub now_ms: i64,
    #[serde(default)]
    pub villages: Vec<VillageDigest>,
    #[serde(default)]
    pub culture: Culture,
    #[serde(default)]
    pub incoming_attacks: Vec<Incoming>,
    #[serde(default)]
    pub reports: Vec<ReportHead>,
    /// Own in-flight movements (attacks, reinforcements, returns, settling).
    #[serde(default)]
    pub movements: Vec<MovementEntry>,
    /// Own troops stationed at foreign villages.
    #[serde(default)]
    pub reinforcements_abroad: Vec<ReinforcementAbroad>,
    #[serde(default)]
    pub scout_reports: Vec<ScoutReportHead>,
}

// ---------------------------------------------------------------------------
// /api/me response
// ---------------------------------------------------------------------------

/// One world entry in the `/api/me` response.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct WorldEntry {
    #[serde(default)]
    pub world: String,
    #[serde(default)]
    pub player: String,
    #[serde(default)]
    pub tribe: String,
}

/// Response from `GET /api/me`.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct MeResponse {
    #[serde(default)]
    pub account: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub is_ai: bool,
    #[serde(default)]
    pub worlds: Vec<WorldEntry>,
}

// ---------------------------------------------------------------------------
// Map window
// ---------------------------------------------------------------------------

/// One cell in a map window row.  Only the fields the runner needs are named;
/// any additional fields the server adds are silently discarded.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct MapCell {
    #[serde(default)]
    pub cell_class: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub href: Option<String>,
    /// `true` when this tile is a free valley the runner can settle on.
    #[serde(default)]
    pub settle: bool,
    #[serde(default)]
    pub x: i32,
    #[serde(default)]
    pub y: i32,
}

/// Response from `GET /api/w/{world}/map?x&y&r`.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct MapWindow {
    #[serde(default)]
    pub center_x: i32,
    #[serde(default)]
    pub center_y: i32,
    #[serde(default)]
    pub r: i32,
    /// Rows ordered north→south, each west→east.
    #[serde(default)]
    pub rows: Vec<Vec<MapCell>>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Comprehensive fixture: shaped exactly like the agent-API examples in
    /// docs/agent-api.md. A `__bogus` key is sprinkled throughout to prove
    /// that unknown fields are silently ignored.
    #[test]
    fn fixture_parse() {
        let digest_json = r#"{
            "__bogus_top_level": "ignored",
            "world": "world-0001",
            "player": "42",
            "now_ms": 1700000000000,
            "villages": [{
                "__bogus_village": "ignored",
                "id": "aaaaaaaa-0000-0000-0000-000000000001",
                "x": 0,
                "y": 0,
                "capital": true,
                "resources": {
                    "wood": {"amount": 800, "rate": 20, "capacity": 1000},
                    "clay": {"amount": 700, "rate": 15, "capacity": 1000},
                    "iron": {"amount": 600, "rate": 12, "capacity": 1000},
                    "crop": {"amount": 500, "rate": 8,  "capacity": 1000}
                },
                "fields": [
                    {"slot": 0, "kind": "wood", "level": 1},
                    {"slot": 1, "kind": "clay", "level": 1}
                ],
                "buildings": [
                    {"slot": 19, "kind": "main_building", "level": 1}
                ],
                "build_queue": [
                    {"target": "field",    "slot": 0,  "level": 2, "completes_at_ms": 1700000060000},
                    {"target": "building", "slot": 19, "kind": "main_building", "level": 2, "completes_at_ms": 1700000120000}
                ],
                "training": [
                    {"building": "barracks", "unit": "legionnaire", "remaining": 5, "next_complete_at_ms": 1700000300000}
                ],
                "garrison": [
                    {"unit": "legionnaire", "count": 10}
                ],
                "reinforcements_here": [
                    {"home_village": "bbbbbbbb-0000-0000-0000-000000000002", "x": 1, "y": 2, "owner": "ally", "troops": {"legionnaire": 5}}
                ],
                "research": {
                    "researched": ["legionnaire"],
                    "levels": [{"unit": "legionnaire", "level": 1}],
                    "active": [
                        {"kind": "smithy", "unit": "legionnaire", "complete_at_ms": 1700000500000}
                    ]
                }
            }],
            "culture": {
                "cp": 50, "rate_per_hour": 10,
                "villages_used": 1, "villages_allowed": 1, "next_threshold": 200
            },
            "incoming_attacks": [
                {"village": "aaaaaaaa-0000-0000-0000-000000000001", "arrive_at_ms": 1700000600000}
            ],
            "reports": [
                {"id": "99999", "occurred_at_ms": 1699999000000, "attacker_won": true, "kind": "attack"}
            ],
            "movements": [
                {"kind": "attack", "dest_x": 5, "dest_y": 3, "arrive_at_ms": 1700000600000, "troops": {"legionnaire": 8}}
            ],
            "reinforcements_abroad": [
                {"host_village": "bbbbbbbb-0000-0000-0000-000000000002", "x": 1, "y": 2, "owner": "ally", "troops": {"legionnaire": 3}}
            ],
            "scout_reports": [
                {"id": "88888", "occurred_at_ms": 1699999100000, "viewer_is_scouter": true, "detected": false}
            ]
        }"#;

        let digest: Digest = serde_json::from_str(digest_json).expect("digest parses");

        assert_eq!(digest.world, "world-0001");
        assert_eq!(digest.now_ms, 1700000000000);
        assert_eq!(digest.villages.len(), 1);

        let v = &digest.villages[0];
        assert_eq!(v.x, 0);
        assert!(v.capital);
        assert_eq!(v.resources.wood.amount, 800);
        assert_eq!(v.resources.crop.rate, 8);
        assert_eq!(v.fields.len(), 2);
        assert_eq!(v.fields[0].kind, "wood");
        assert_eq!(v.build_queue.len(), 2);
        // Field entry has no kind; building entry has kind.
        assert!(v.build_queue[0].kind.is_none());
        assert_eq!(v.build_queue[1].kind.as_deref(), Some("main_building"));
        assert_eq!(v.garrison[0].unit, "legionnaire");
        assert_eq!(v.garrison[0].count, 10);
        assert_eq!(
            v.reinforcements_here[0].home_village,
            "bbbbbbbb-0000-0000-0000-000000000002"
        );
        assert_eq!(v.research.researched, ["legionnaire"]);
        // smithy active order has no target_level in JSON → defaults to None.
        assert!(v.research.active[0].target_level.is_none());

        assert_eq!(digest.culture.villages_allowed, 1);
        assert_eq!(digest.incoming_attacks[0].arrive_at_ms, 1700000600000);
        assert!(digest.reports[0].attacker_won);
        assert_eq!(digest.movements[0].troops["legionnaire"], 8);
        assert_eq!(
            digest.reinforcements_abroad[0].host_village,
            "bbbbbbbb-0000-0000-0000-000000000002"
        );
        assert!(!digest.scout_reports[0].detected);
    }

    #[test]
    fn me_response_parse() {
        let json = r#"{
            "__bogus": "ignored",
            "account": "1",
            "username": "bot_01",
            "is_ai": true,
            "worlds": [{"world": "world-0001", "player": "42", "tribe": "roman"}]
        }"#;

        let me: MeResponse = serde_json::from_str(json).expect("me parses");
        assert_eq!(me.username, "bot_01");
        assert!(me.is_ai);
        assert_eq!(me.worlds[0].tribe, "roman");
    }

    #[test]
    fn map_window_parse() {
        let json = r#"{
            "__bogus": "ignored",
            "center_x": 0,
            "center_y": 0,
            "r": 3,
            "rows": [[{
                "__bogus_cell": "ignored",
                "cell_class": "map-grid__cell",
                "label": "Empty valley (0, 0)",
                "href": null,
                "settle": true,
                "x": 0,
                "y": 0
            }]]
        }"#;

        let win: MapWindow = serde_json::from_str(json).expect("map window parses");
        assert_eq!(win.r, 3);
        assert_eq!(win.rows.len(), 1);
        let cell = &win.rows[0][0];
        assert!(cell.settle);
        assert_eq!(cell.x, 0);
        assert!(cell.href.is_none());
        assert_eq!(cell.label, "Empty valley (0, 0)");
    }

    #[test]
    fn unknown_extra_fields_ignored() {
        // A minimal digest with only unknown fields and none of the expected ones
        // — the defensive defaults kick in and parsing succeeds.
        let json = r#"{"completely_unknown": 42, "also_unknown": []}"#;
        let d: Digest = serde_json::from_str(json).expect("parses with all-defaults");
        assert_eq!(d.world, "");
        assert_eq!(d.now_ms, 0);
        assert!(d.villages.is_empty());
    }
}
