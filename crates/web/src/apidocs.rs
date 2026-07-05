//! The developer API reference registry (128, plan §Module changes).
//!
//! One compile-time registry — a `Vec<ApiGroup>` of plain data built from `&'static str` literals —
//! is the **single source** for both renderings: the swagger-style HTML page (T2) and the OpenAPI
//! 3.0.3 document at `/docs/api/openapi.json` (this module's [`openapi_json`]). They cannot drift
//! from each other because both are generated from [`registry`] (plan §Key decisions).
//!
//! Every entry here is mined from the actual handlers: `crate::api` (the bearer `epk_` Agent API,
//! 118/119) and `crate::spectator_api` (the bearer `spk_` Spectator API, 125). Request/response
//! examples are literal JSON strings verified (by this module's own unit tests) to parse, and
//! pinned against the handlers' own (de)serialize structs and `docs/agent-api.md` /
//! `docs/spectator-api.md` where those markdown contracts already carry the exact shape.
//!
//! **P11:** the registry and the OpenAPI document are built in memory from static data — no I/O.

use serde_json::{Value, json};
use std::sync::LazyLock;

/// Where a parameter rides: the path (`{world}`, `{village}`, …) or the query string (`?x=`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParamLocation {
    Path,
    Query,
}

/// One path or query parameter (plan: `params: [(name, kind, desc)]`, extended with a wire type so
/// the OpenAPI export can declare a `schema.type`).
#[derive(Clone, Copy)]
pub struct Param {
    pub name: &'static str,
    pub location: ParamLocation,
    /// OpenAPI-ish primitive: `"string"` or `"integer"`.
    pub ty: &'static str,
    pub required: bool,
    pub description: &'static str,
}

/// One documented success response (plan: `responses: [(status, desc, example)]`).
#[derive(Clone, Copy)]
pub struct ResponseExample {
    pub status: u16,
    /// One line: what this status means here. For a truncated example, the truncation note lives
    /// here (**outside** the JSON, per plan) — e.g. "…truncated: one village of many shown".
    pub description: &'static str,
    /// A pretty-printed JSON literal — always valid JSON (enforced by this module's unit tests).
    pub example: &'static str,
}

/// One documented error case (plan: `errors: [(status, code, when)]`) — mined from the handler's own
/// error-mapping function (`build_error`, `combat_error`, …) plus the shared extractor/guard
/// rejections (`ApiError::unauthorized`, the account-blocked/rate-limit/freeze guards).
#[derive(Clone, Copy)]
pub struct ErrorCase {
    pub status: u16,
    pub code: &'static str,
    pub when: &'static str,
}

/// One documented endpoint (plan §Module changes).
pub struct Endpoint {
    /// `"GET"` or `"POST"`.
    pub method: &'static str,
    /// The full request path, **with the `/api` or `/spectator` mount prefix** — copy-pasteable
    /// into a `curl` line as-is (placeholders `{world}`/`{village}`/`{id}`/`{account}` intact).
    pub path: &'static str,
    pub summary: &'static str,
    pub description: &'static str,
    /// `"agent"` (`epk_` bearer) or `"spectator"` (`spk_` bearer) — selects the security scheme.
    pub auth: &'static str,
    pub params: Vec<Param>,
    /// `Some` for every `POST` (AC3); `None` for `GET`s.
    pub request_example: Option<&'static str>,
    pub responses: Vec<ResponseExample>,
    pub errors: Vec<ErrorCase>,
}

/// One rendered sidebar group (Agent API or Spectator API).
pub struct ApiGroup {
    pub name: &'static str,
    /// URL-safe anchor for the group section (no spaces — HTML id rules).
    pub anchor: &'static str,
    /// The auth scheme explained once at the top of the group (plan/AC5): key format, minting,
    /// rate-budget class.
    pub auth_blurb: &'static str,
    pub endpoints: Vec<Endpoint>,
}

// ---------------------------------------------------------------------------
// Small constructors — keep the endpoint tables below readable.
// ---------------------------------------------------------------------------

fn path_param(name: &'static str, ty: &'static str, description: &'static str) -> Param {
    Param {
        name,
        location: ParamLocation::Path,
        ty,
        required: true,
        description,
    }
}

fn query_param(
    name: &'static str,
    ty: &'static str,
    required: bool,
    description: &'static str,
) -> Param {
    Param {
        name,
        location: ParamLocation::Query,
        ty,
        required,
        description,
    }
}

fn resp(status: u16, description: &'static str, example: &'static str) -> ResponseExample {
    ResponseExample {
        status,
        description,
        example,
    }
}

fn err(status: u16, code: &'static str, when: &'static str) -> ErrorCase {
    ErrorCase { status, code, when }
}

// ---------------------------------------------------------------------------
// Shared error cases — the guard/extractor rejections common to (almost) every endpoint.
// ---------------------------------------------------------------------------

fn err_unauthorized() -> ErrorCase {
    err(
        401,
        "unauthorized",
        "Missing, malformed, revoked or unknown bearer key; or a key whose bound account lost the \
         required role (Agent API: is_ai; Spectator API: is_spectator, re-checked on every request).",
    )
}

fn err_account_blocked() -> ErrorCase {
    err(
        403,
        "account_blocked",
        "The bound account is banned or currently suspended for a fair-play violation — checked on \
         every request, since keys never pass the browser login chokepoint.",
    )
}

fn err_rate_limited() -> ErrorCase {
    err(
        429,
        "rate_limited",
        "The bearer key's request count exceeded agent_limit_per_window — 120 requests/min at \
         current config (specs/balance/fairplay.toml); see the fairplay rules. The body adds \
         retry_after_secs.",
    )
}

fn err_unknown_world() -> ErrorCase {
    err(404, "unknown_world", "No world exists at that path uuid.")
}

fn err_not_joined() -> ErrorCase {
    err(
        403,
        "not_joined",
        "This account has no player in that world (the Agent API only — a spectator has standing on \
         every existing world by construction).",
    )
}

fn err_world_frozen() -> ErrorCase {
    err(
        403,
        "world_frozen",
        "The world has been won and is frozen; mutating requests are rejected exactly as for browser \
         players (021/057).",
    )
}

fn err_village_not_found() -> ErrorCase {
    err(
        404,
        "not_found",
        "The path village id does not parse as a UUID, or is not owned by the acting player — strict \
         addressing, no capital fallback (unlike the browser's convenience).",
    )
}

fn err_invalid_json() -> ErrorCase {
    err(
        400,
        "invalid_json",
        "The request body failed to parse as this endpoint's expected JSON shape.",
    )
}

/// `/api/me` — account-scoped only, no world in the path.
fn base_agent_account_errors() -> Vec<ErrorCase> {
    vec![
        err_unauthorized(),
        err_account_blocked(),
        err_rate_limited(),
    ]
}

/// Every `/api/w/{world}/…` `GET` — adds the world-resolution failures.
fn base_agent_world_errors() -> Vec<ErrorCase> {
    let mut v = base_agent_account_errors();
    v.push(err_unknown_world());
    v.push(err_not_joined());
    v
}

/// Every mutating `/api/w/{world}/…` `POST` — adds the freeze guard and body parsing.
fn base_agent_action_errors() -> Vec<ErrorCase> {
    let mut v = base_agent_world_errors();
    v.push(err_world_frozen());
    v.push(err_invalid_json());
    v
}

/// A `POST …/village/{village}/…` action — adds the strict village-ownership check.
fn base_agent_village_action_errors() -> Vec<ErrorCase> {
    let mut v = base_agent_action_errors();
    v.push(err_village_not_found());
    v
}

/// `/spectator/me` — account-scoped only, no world in the path.
fn base_spectator_account_errors() -> Vec<ErrorCase> {
    vec![
        err_unauthorized(),
        err_account_blocked(),
        err_rate_limited(),
    ]
}

/// Every `/spectator/w/{world}/…` `GET` — adds unknown-world only (no `not_joined`: a spectator has
/// standing on every world).
fn base_spectator_world_errors() -> Vec<ErrorCase> {
    let mut v = base_spectator_account_errors();
    v.push(err_unknown_world());
    v
}

// ---------------------------------------------------------------------------
// The Agent API group (118/119) — from `crate::api::router()`.
// ---------------------------------------------------------------------------

fn agent_group() -> ApiGroup {
    let world_param = || path_param("world", "string", "The world's UUID (path segment).");
    let village_param = || {
        path_param(
            "village",
            "string",
            "The acting player's own village, as a hyphenated UUID. Must be owned by the \
             authenticated player — strict addressing, no capital fallback.",
        )
    };

    let endpoints = vec![
        Endpoint {
            method: "GET",
            path: "/api/me",
            summary: "Key introspection: the bound account and its per-world players.",
            description: "Resolves the bearer key to its AI account and lists every world the account \
                has joined, with that world's player id and tribe. Use this once at startup to discover \
                which `{world}` values are valid for the rest of the surface.",
            auth: "agent",
            params: vec![],
            request_example: None,
            responses: vec![resp(
                200,
                "The bound account plus its per-world players.",
                r#"{
  "account": "170141183460469231731687303715884105728",
  "username": "ai_marcus",
  "is_ai": true,
  "worlds": [
    { "world": "b6f8f6d2-3c1a-4e9b-8f2a-7d4c5b6a9e10", "player": "9821", "tribe": "romans" }
  ]
}"#,
            )],
            errors: base_agent_account_errors(),
        },
        Endpoint {
            method: "GET",
            path: "/api/w/{world}/state",
            summary: "The state digest — every number the agent may know, in one document.",
            description: "Assembled from the same read models the corresponding pages render (load_economy, \
                active_builds, active_training, load_culture, incoming_against, reports_for), so the digest \
                can never drift from page truth. Fog-of-war honest: incoming_attacks carry only the target \
                village and arrival time — never the attacker's origin or composition (§7.3) until scouted. \
                All deadlines are absolute Unix-ms; compute countdowns client-side against now_ms.",
            auth: "agent",
            params: vec![world_param()],
            request_example: None,
            responses: vec![resp(
                200,
                "…truncated: one village of the player's several is shown, with one entry per array; the \
                 real digest lists every owned village and every queue/training/garrison/report row.",
                r#"{
  "world": "b6f8f6d2-3c1a-4e9b-8f2a-7d4c5b6a9e10",
  "player": "9821",
  "now_ms": 1782000000000,
  "villages": [
    {
      "id": "1d8e9f3a-2b4c-4d5e-8f6a-3b2c1d0e9f8a",
      "x": 12, "y": -7, "capital": true,
      "resources": {
        "wood": { "amount": 3200, "rate": 180, "capacity": 8000 },
        "clay": { "amount": 2900, "rate": 160, "capacity": 8000 },
        "iron": { "amount": 3100, "rate": 170, "capacity": 8000 },
        "crop": { "amount": 4200, "rate": 95, "capacity": 8000 }
      },
      "fields": [ { "slot": 0, "kind": "wood", "level": 6 } ],
      "buildings": [ { "slot": 19, "kind": "main_building", "level": 10 } ],
      "build_queue": [
        { "target": "building", "slot": 19, "kind": "barracks", "level": 4, "completes_at_ms": 1782000600000 }
      ],
      "training": [
        { "building": "barracks", "unit": "legionnaire", "remaining": 12, "next_complete_at_ms": 1782000300000 }
      ],
      "garrison": [ { "unit": "legionnaire", "count": 40 } ],
      "research": {
        "researched": ["legionnaire"],
        "levels": [ { "unit": "legionnaire", "level": 3 } ],
        "active": []
      },
      "reinforcements_here": []
    }
  ],
  "culture": { "cp": 640, "rate_per_hour": 12, "villages_used": 1, "villages_allowed": 2, "next_threshold": 800 },
  "incoming_attacks": [ { "village": "1d8e9f3a-2b4c-4d5e-8f6a-3b2c1d0e9f8a", "arrive_at_ms": 1782001200000 } ],
  "reports": [ { "id": "48291033512", "occurred_at_ms": 1781998000000, "attacker_won": true, "kind": "raid" } ],
  "movements": [
    { "kind": "attack", "dest_x": 20, "dest_y": -5, "arrive_at_ms": 1782001800000, "troops": { "legionnaire": 30 } }
  ],
  "reinforcements_abroad": [],
  "scout_reports": [
    { "id": "88213377001", "occurred_at_ms": 1781999000000, "viewer_is_scouter": true, "detected": false }
  ]
}"#,
            )],
            errors: base_agent_world_errors(),
        },
        Endpoint {
            method: "GET",
            path: "/api/w/{world}/map",
            summary: "A bounded map window centred on (x, y) — the same cell data the map page shows.",
            description: "Built by the identical `map_cells` builder the browser `/map/tiles` endpoint (093) \
                streams, so terrain, village markers, alliance tags and oases can never drift between the two \
                surfaces. A parseable out-of-range r is clamped to 0..=10 server-side; a non-numeric \
                query value is a plain 400 (P11 — a bounded \
                read, never a client-controlled unbounded scan).",
            auth: "agent",
            params: vec![
                world_param(),
                query_param("x", "integer", true, "Window center x-coordinate."),
                query_param("y", "integer", true, "Window center y-coordinate."),
                query_param(
                    "r",
                    "integer",
                    false,
                    "Half-extent; default 7, clamped server-side to 0..=10.",
                ),
            ],
            request_example: None,
            responses: vec![resp(
                200,
                "…truncated: a 1×2 slice of the (2r+1)×(2r+1) grid is shown.",
                r#"{
  "center_x": 12, "center_y": -7, "r": 2,
  "rows": [
    [
      {
        "cell_class": "map-grid__cell map-grid__cell--grass",
        "glyph": ".", "label": "Grass (11|-8)",
        "href": null, "market_href": null, "settle": false,
        "x": 11, "y": -8
      },
      {
        "cell_class": "map-grid__cell map-grid__cell--grass map-grid__cell--village map-grid__cell--self map-grid__cell--capital",
        "glyph": "★", "label": "Village (12|-7) — ai_marcus [ROM]",
        "href": "/w/b6f8f6d2-3c1a-4e9b-8f2a-7d4c5b6a9e10/village/1d8e9f3a-2b4c-4d5e-8f6a-3b2c1d0e9f8a",
        "market_href": "/w/b6f8f6d2-3c1a-4e9b-8f2a-7d4c5b6a9e10/village/1d8e9f3a-2b4c-4d5e-8f6a-3b2c1d0e9f8a/market?x=12&y=-7",
        "settle": false,
        "x": 12, "y": -7
      }
    ]
  ]
}"#,
            )],
            errors: base_agent_world_errors(),
        },
        Endpoint {
            method: "POST",
            path: "/api/w/{world}/village/{village}/build",
            summary: "Queue a field upgrade or a building — the same use-case the build form submits to.",
            description: "target is \"field\" or \"building\"; kind is required (and validated against the \
                known building kinds) only when target is \"building\". Affordability, build-queue lanes, \
                prerequisites, placement and ownership are all the use-case's own rules — this is a thin JSON \
                adapter, not a second rule engine (P4). On success the created queue entry is read back \
                through the same reader the digest uses (page truth); a null queue_entry with ordered: true \
                means the order committed but the echo read glitched.",
            auth: "agent",
            params: vec![world_param(), village_param()],
            request_example: Some(
                r#"{
  "target": "building",
  "slot": 19,
  "kind": "barracks"
}"#,
            ),
            responses: vec![resp(
                200,
                "The order committed; the created queue entry is echoed back.",
                r#"{
  "ordered": true,
  "village": "1d8e9f3a-2b4c-4d5e-8f6a-3b2c1d0e9f8a",
  "queue_entry": { "level": 4, "completes_at_ms": 1782000900000 }
}"#,
            )],
            errors: {
                let mut v = base_agent_village_action_errors();
                v.extend([
                    err(400, "invalid_target", "target must be \"field\" or \"building\"."),
                    err(
                        400,
                        "invalid_kind",
                        "kind is missing or not a recognized building kind (required when target is \"building\").",
                    ),
                    err(409, "insufficient", "Not enough resources for this level."),
                    err(
                        409,
                        "lane_busy",
                        "The build queue lane already holds an order (BuildError::AlreadyBuilding).",
                    ),
                    err(409, "max_level", "The field or building is already at its maximum level."),
                    err(409, "prereq_unmet", "A prerequisite building/level is unmet."),
                    err(
                        409,
                        "exclusive",
                        "An exclusive slot is already occupied by a different building kind.",
                    ),
                    err(409, "placement", "The target slot cannot hold this building or field."),
                    err(
                        409,
                        "not_demolishable",
                        "The use-case rejected the target in this context (BuildError::NotDemolishable).",
                    ),
                    err(
                        409,
                        "main_building_too_low",
                        "The Main Building's level is too low to support this upgrade.",
                    ),
                    err(409, "conflict", "A concurrent order landed on the same lane first."),
                ]);
                v
            },
        },
        Endpoint {
            method: "POST",
            path: "/api/w/{world}/village/{village}/train",
            summary: "Queue a training batch for a unit — the same use-case the train form submits to.",
            description: "Affordability, training-queue lanes, research gates and the building that trains \
                the unit are all the use-case's own rules. On success the created batch is read back through \
                the digest's own reader (page truth); a null batch with ordered: true means the order \
                committed but the echo read glitched.",
            auth: "agent",
            params: vec![world_param(), village_param()],
            request_example: Some(
                r#"{
  "unit": "legionnaire",
  "count": 25
}"#,
            ),
            responses: vec![resp(
                200,
                "The order committed; the created batch is echoed back.",
                r#"{
  "ordered": true,
  "village": "1d8e9f3a-2b4c-4d5e-8f6a-3b2c1d0e9f8a",
  "batch": { "unit": "legionnaire", "remaining": 25, "next_complete_at_ms": 1782000420000 }
}"#,
            )],
            errors: {
                let mut v = base_agent_village_action_errors();
                v.extend([
                    err(409, "insufficient", "Not enough resources for this batch."),
                    err(
                        409,
                        "lane_busy",
                        "The training queue lane already holds a batch (TrainError::QueueBusy).",
                    ),
                    err(
                        409,
                        "not_researched",
                        "This unit has not been researched yet.",
                    ),
                    err(
                        409,
                        "building_missing",
                        "The building that trains this unit is not built.",
                    ),
                    err(
                        409,
                        "building_unavailable",
                        "The building that trains this unit exists but cannot train right now.",
                    ),
                    err(
                        400,
                        "count_out_of_range",
                        "count is zero or exceeds the allowed batch size.",
                    ),
                    err(
                        409,
                        "conflict",
                        "A concurrent order landed on the same lane first.",
                    ),
                ]);
                v
            },
        },
        Endpoint {
            method: "POST",
            path: "/api/w/{world}/village/{village}/attack",
            summary: "Send an attack or raid to (x, y) — the Rally Point's send-troops use-case.",
            description: "mode is \"attack\" or \"raid\"; units is a {unit_id: count} bundle (zero counts are \
                dropped). catapult_target (a building kind id) is optional and validated against the known \
                building kinds when present — an attacker addressing an unknown building gets a 400, unlike \
                the browser's <select> which cannot produce one. On success the created movement is read back \
                by destination + kind (page truth); a null movement with ordered: true means the attack \
                committed but the echo glitched.",
            auth: "agent",
            params: vec![world_param(), village_param()],
            request_example: Some(
                r#"{
  "x": 14,
  "y": -6,
  "units": { "legionnaire": 40, "imperian": 10 },
  "mode": "raid"
}"#,
            ),
            responses: vec![resp(
                200,
                "The order committed; the created movement is echoed back.",
                r#"{
  "ordered": true,
  "movement": {
    "kind": "raid", "dest_x": 14, "dest_y": -6, "arrive_at_ms": 1782002400000,
    "troops": { "legionnaire": 40, "imperian": 10 }
  }
}"#,
            )],
            errors: {
                let mut v = base_agent_village_action_errors();
                v.extend([
                    err(400, "invalid_mode", "mode must be \"attack\" or \"raid\"."),
                    err(
                        400,
                        "invalid_catapult_target",
                        "catapult_target is not a recognized building kind.",
                    ),
                    err(
                        409,
                        "insufficient",
                        "Not enough troops of the requested kinds/counts in this village.",
                    ),
                    err(
                        400,
                        "empty_composition",
                        "units contained no troops after dropping zero counts.",
                    ),
                    err(404, "no_target", "There is no village at (x, y)."),
                    err(400, "same_tile", "(x, y) is this village's own coordinate."),
                    err(
                        409,
                        "target_protected",
                        "The target is under beginner's protection (019).",
                    ),
                ]);
                v
            },
        },
        Endpoint {
            method: "POST",
            path: "/api/w/{world}/village/{village}/scout",
            summary: "Send a standalone scouting mission to (x, y).",
            description: "target is \"resources\" or \"defenses\" (010's two intel kinds). units must be \
                entirely Scout-role troops; a mixed or non-scout bundle is rejected. On success the created \
                movement is read back by destination + kind Scout; a null movement with ordered: true means \
                the mission committed but the echo glitched.",
            auth: "agent",
            params: vec![world_param(), village_param()],
            request_example: Some(
                r#"{
  "x": 14,
  "y": -6,
  "units": { "equites_legati": 3 },
  "target": "defenses"
}"#,
            ),
            responses: vec![resp(
                200,
                "The order committed; the created movement is echoed back.",
                r#"{
  "ordered": true,
  "movement": {
    "kind": "scout", "dest_x": 14, "dest_y": -6, "arrive_at_ms": 1782000180000,
    "troops": { "equites_legati": 3 }
  }
}"#,
            )],
            errors: {
                let mut v = base_agent_village_action_errors();
                v.extend([
                    err(400, "invalid_target", "target must be \"resources\" or \"defenses\"."),
                    err(
                        400,
                        "not_all_scouts",
                        "units contained a non-Scout-role unit — a scouting mission must be all scouts.",
                    ),
                    err(409, "insufficient", "Not enough scouts of the requested counts in this village."),
                    err(400, "empty_composition", "units contained no troops after dropping zero counts."),
                    err(404, "no_target", "There is no village at (x, y)."),
                    err(400, "same_tile", "(x, y) is this village's own coordinate."),
                ]);
                v
            },
        },
        Endpoint {
            method: "POST",
            path: "/api/w/{world}/village/{village}/reinforce",
            summary: "Send troops to garrison a friendly village at (x, y).",
            description: "units is a {unit_id: count} bundle (zero counts dropped). On success the created \
                movement is read back by destination + kind Reinforce; a null movement with ordered: true \
                means the order committed but the echo glitched.",
            auth: "agent",
            params: vec![world_param(), village_param()],
            request_example: Some(
                r#"{
  "x": 15,
  "y": 3,
  "units": { "praetorian": 20 }
}"#,
            ),
            responses: vec![resp(
                200,
                "The order committed; the created movement is echoed back.",
                r#"{
  "ordered": true,
  "movement": {
    "kind": "reinforce", "dest_x": 15, "dest_y": 3, "arrive_at_ms": 1782001500000,
    "troops": { "praetorian": 20 }
  }
}"#,
            )],
            errors: {
                let mut v = base_agent_village_action_errors();
                v.extend([
                    err(
                        409,
                        "insufficient",
                        "Not enough troops of the requested kinds/counts in this village.",
                    ),
                    err(
                        400,
                        "empty_composition",
                        "units contained no troops after dropping zero counts.",
                    ),
                    err(404, "no_target", "There is no village at (x, y)."),
                    err(400, "same_tile", "(x, y) is this village's own coordinate."),
                ]);
                v
            },
        },
        Endpoint {
            method: "POST",
            path: "/api/w/{world}/village/{village}/return",
            summary: "Recall a stationed reinforcement group back to its home village.",
            description: "host is the hyphenated UUID of the village where this player's troops are currently \
                stationed — read it from a digest reinforcements_abroad[].host_village entry. On success the \
                created return movement is read back (page truth); a null movement with ordered: true means \
                the order committed but the echo glitched.",
            auth: "agent",
            params: vec![world_param(), village_param()],
            request_example: Some(
                r#"{
  "host": "4a3b2c1d-0e9f-48a7-b6c5-d4e3f2a1b0c9"
}"#,
            ),
            responses: vec![resp(
                200,
                "The order committed; the created return movement is echoed back.",
                r#"{
  "ordered": true,
  "movement": {
    "kind": "return", "dest_x": 12, "dest_y": -7, "arrive_at_ms": 1782002100000,
    "troops": { "praetorian": 20 }
  }
}"#,
            )],
            errors: {
                let mut v = base_agent_village_action_errors();
                v.push(err(
                    404,
                    "nothing_stationed",
                    "No troops of this player are currently stationed at that host village.",
                ));
                v
            },
        },
        Endpoint {
            method: "POST",
            path: "/api/w/{world}/village/{village}/trade",
            summary: "Send a merchant shipment of resources to (x, y) via this village's Marketplace.",
            description: "give is a resource bundle; negative amounts are clamped to 0 (the market form's own \
                rule). On success the created shipment is read back by destination (page truth); a null \
                shipment with ordered: true means the order committed but the echo glitched.",
            auth: "agent",
            params: vec![world_param(), village_param()],
            request_example: Some(
                r#"{
  "x": 10,
  "y": 10,
  "give": { "wood": 500, "clay": 300, "iron": 200, "crop": 0 }
}"#,
            ),
            responses: vec![resp(
                200,
                "The order committed; the created shipment is echoed back.",
                r#"{
  "ordered": true,
  "shipment": {
    "dest_x": 10, "dest_y": 10, "arrive_at_ms": 1782001000000,
    "give": { "wood": 500, "clay": 300, "iron": 200, "crop": 0 }
  }
}"#,
            )],
            errors: {
                let mut v = base_agent_village_action_errors();
                v.extend([
                    err(
                        409,
                        "no_marketplace",
                        "This village has no (usable) Marketplace.",
                    ),
                    err(
                        400,
                        "empty_bundle",
                        "give carried no positive resource amount.",
                    ),
                    err(
                        409,
                        "insufficient",
                        "Not enough of the given resources in this village.",
                    ),
                    err(
                        409,
                        "not_enough_merchants",
                        "Not enough free merchants to carry this bundle's weight.",
                    ),
                    err(404, "no_target", "There is no village at (x, y)."),
                    err(400, "same_tile", "(x, y) is this village's own coordinate."),
                ]);
                v
            },
        },
        Endpoint {
            method: "POST",
            path: "/api/w/{world}/village/{village}/settle",
            summary: "Found a new village at (x, y) using a settler group from this village.",
            description: "Settlers are implicit (013's rules pick and consume the group). On success the \
                created Settle movement is read back by destination (page truth); a null movement with \
                ordered: true means the order committed but the echo glitched.",
            auth: "agent",
            params: vec![world_param(), village_param()],
            request_example: Some(
                r#"{
  "x": 20,
  "y": -5
}"#,
            ),
            responses: vec![resp(
                200,
                "The order committed; the created movement is echoed back.",
                r#"{
  "ordered": true,
  "movement": {
    "kind": "settle", "dest_x": 20, "dest_y": -5, "arrive_at_ms": 1782005000000,
    "troops": { "settler": 3 }
  }
}"#,
            )],
            errors: {
                let mut v = base_agent_village_action_errors();
                v.extend([
                    err(409, "insufficient", "Not enough settlers in this village."),
                    err(
                        409,
                        "not_settler_group",
                        "The available group at this village is not a settler group.",
                    ),
                    err(
                        409,
                        "no_slot",
                        "This player has no free village slot (culture points gate, 013).",
                    ),
                    err(
                        409,
                        "not_free_valley",
                        "(x, y) is not a free valley to settle on.",
                    ),
                ]);
                v
            },
        },
        Endpoint {
            method: "POST",
            path: "/api/w/{world}/village/{village}/research",
            summary: "Research a unit at the Academy.",
            description: "Affordability, requirements (prerequisite units/buildings) and one-active-order-\
                at-a-time are all the use-case's own rules. On success the created order is read back \
                through the same reader the digest uses; a null order with ordered: true means the order \
                committed but the echo glitched.",
            auth: "agent",
            params: vec![world_param(), village_param()],
            request_example: Some(
                r#"{
  "unit": "imperian"
}"#,
            ),
            responses: vec![resp(
                200,
                "The order committed; the created research order is echoed back.",
                r#"{
  "ordered": true,
  "order": { "kind": "research", "unit": "imperian", "target_level": null, "complete_at_ms": 1782001200000 }
}"#,
            )],
            errors: {
                let mut v = base_agent_village_action_errors();
                v.extend([
                    err(
                        409,
                        "insufficient",
                        "Not enough resources for this research.",
                    ),
                    err(
                        409,
                        "in_progress",
                        "A research order is already active for this village.",
                    ),
                    err(
                        409,
                        "already_researched",
                        "This unit is already researched.",
                    ),
                    err(
                        409,
                        "requirements_unmet",
                        "A prerequisite unit/building for this research is unmet.",
                    ),
                    err(409, "conflict", "A concurrent order landed first."),
                ]);
                v
            },
        },
        Endpoint {
            method: "POST",
            path: "/api/w/{world}/village/{village}/smithy",
            summary: "Upgrade a researched unit's weapon/armor level at the Smithy.",
            description: "The unit must already be researched. On success the created order is read back \
                through the same reader the digest uses; a null order with ordered: true means the order \
                committed but the echo glitched.",
            auth: "agent",
            params: vec![world_param(), village_param()],
            request_example: Some(
                r#"{
  "unit": "legionnaire"
}"#,
            ),
            responses: vec![resp(
                200,
                "The order committed; the created upgrade order is echoed back.",
                r#"{
  "ordered": true,
  "order": { "kind": "smithy", "unit": "legionnaire", "target_level": 4, "complete_at_ms": 1782003600000 }
}"#,
            )],
            errors: {
                let mut v = base_agent_village_action_errors();
                v.extend([
                    err(
                        409,
                        "insufficient",
                        "Not enough resources for this upgrade level.",
                    ),
                    err(
                        409,
                        "in_progress",
                        "A smithy upgrade order is already active for this village.",
                    ),
                    err(
                        409,
                        "not_researched",
                        "This unit has not been researched yet.",
                    ),
                    err(409, "no_smithy", "This village has no (usable) Smithy."),
                    err(
                        409,
                        "max_level",
                        "This unit's weapon/armor is already at its maximum level.",
                    ),
                    err(
                        409,
                        "smithy_level_too_low",
                        "The Smithy's level is too low for this upgrade level.",
                    ),
                    err(409, "conflict", "A concurrent order landed first."),
                ]);
                v
            },
        },
        Endpoint {
            method: "GET",
            path: "/api/w/{world}/report/{id}",
            summary: "The full battle report for a decimal report id.",
            description: "id is the decimal u128 string from a digest reports[].id (or the spectator feed's \
                report id). Party-scoped: both attacker and defender receive the identical full view (forces, \
                losses, loot, razed, loyalty); a non-party request gets 404, never a forbidden leak (P4).",
            auth: "agent",
            params: vec![
                world_param(),
                path_param("id", "string", "Decimal u128 report id."),
            ],
            request_example: None,
            responses: vec![resp(
                200,
                "The full report; both parties see this identical document.",
                r#"{
  "id": "48291033512",
  "occurred_at_ms": 1781998000000,
  "kind": "raid",
  "attacker_name": "ai_marcus",
  "attacker_coord": { "x": 12, "y": -7 },
  "defender_name": "gaius77",
  "defender_coord": { "x": 14, "y": -6 },
  "attacker_won": true,
  "luck": 4,
  "morale": 100,
  "wall_before": 5,
  "wall_after": 5,
  "attacker_forces": { "legionnaire": 40 },
  "attacker_losses": { "legionnaire": 3 },
  "defender_forces": { "praetorian": 20 },
  "defender_losses": { "praetorian": 20 },
  "scouted": false,
  "scout_target": null,
  "loot": { "wood": 800, "clay": 600, "iron": 400, "crop": 200 },
  "razed": null,
  "loyalty_before": 100,
  "loyalty_after": 100,
  "conquered": false
}"#,
            )],
            errors: {
                let mut v = base_agent_world_errors();
                v.push(err(
                    404,
                    "not_found",
                    "id does not parse as a decimal u128, or the caller is not a party to that report.",
                ));
                v
            },
        },
        Endpoint {
            method: "GET",
            path: "/api/w/{world}/scout-report/{id}",
            summary: "The full scout report for a decimal report id.",
            description: "id is the decimal u128 string from a digest scout_reports[].id. Party-scoped like \
                the battle report; a scouted target receives the port's own pre-redacted view (no intel, no \
                scouts_sent — 010's rule) rather than an additional redaction layer here.",
            auth: "agent",
            params: vec![
                world_param(),
                path_param("id", "string", "Decimal u128 scout-report id."),
            ],
            request_example: None,
            responses: vec![resp(
                200,
                "The scout report as the scouting party sees it (full intel).",
                r#"{
  "id": "88213377001",
  "occurred_at_ms": 1781999000000,
  "scouter_name": "ai_marcus",
  "scouter_coord": { "x": 12, "y": -7 },
  "target_name": "gaius77",
  "target_coord": { "x": 14, "y": -6 },
  "target_type": "defenses",
  "scouts_sent": { "equites_legati": 3 },
  "scouts_lost": { "equites_legati": 0 },
  "detected": false,
  "viewer_is_scouter": true,
  "intel": { "kind": "defenses", "troops": { "praetorian": 20 }, "wall_level": 5 }
}"#,
            )],
            errors: {
                let mut v = base_agent_world_errors();
                v.push(err(
                    404,
                    "not_found",
                    "id does not parse as a decimal u128, or the caller is not a party to that report.",
                ));
                v
            },
        },
        Endpoint {
            method: "POST",
            path: "/api/w/{world}/message",
            summary: "Send a direct message to another player by username.",
            description: "to is the recipient's username (resolved to their account id server-side — the \
                browser links by id, but an agent only knows names from the map/boards). Comms are \
                account-level (cross-world, 024/045): the sender is the bearer key's account, not the \
                per-world player.",
            auth: "agent",
            params: vec![world_param()],
            request_example: Some(
                r#"{
  "to": "gaius77",
  "body": "Reinforcements incoming, hold the wall."
}"#,
            ),
            responses: vec![resp(
                200,
                "The message was sent.",
                r#"{
  "sent": true,
  "message_id": "1002938475",
  "to": "5137"
}"#,
            )],
            errors: {
                let mut v = base_agent_action_errors();
                v.extend([
                    err(
                        400,
                        "invalid",
                        "The message body failed validation (024's own rule, e.g. empty/too long).",
                    ),
                    err(
                        400,
                        "self_send",
                        "The recipient resolves to the sender's own account.",
                    ),
                    err(
                        404,
                        "recipient_unavailable",
                        "No such player, or the recipient's account is unavailable.",
                    ),
                    err(
                        403,
                        "forbidden",
                        "The use-case refused this send (e.g. a blocked sender/recipient pair).",
                    ),
                ]);
                v
            },
        },
        Endpoint {
            method: "GET",
            path: "/api/w/{world}/messages",
            summary: "List DM + channel conversation summaries.",
            description: "Each DM entry additionally carries the partner's decimal account id (derived from \
                the internal dm:<uuid> key) so a follow-up GET …/messages/{account} needs no separate lookup.",
            auth: "agent",
            params: vec![world_param()],
            request_example: None,
            responses: vec![resp(
                200,
                "Conversation summaries, most-recent activity first.",
                r#"{
  "conversations": [
    {
      "key": "dm:5137",
      "account": "5137",
      "title": "gaius77",
      "last_body": "Send grain, we're starving.",
      "last_ms": 1781999500000,
      "unread": 2
    }
  ]
}"#,
            )],
            errors: base_agent_world_errors(),
        },
        Endpoint {
            method: "GET",
            path: "/api/w/{world}/messages/{account}",
            summary: "The DM history with one account, marking it read.",
            description: "account is the partner's decimal account id (from a messages[].account entry or a \
                map/board profile). Returns the history newest-last and marks it read — same semantics as the \
                browser conversation page.",
            auth: "agent",
            params: vec![
                world_param(),
                path_param(
                    "account",
                    "string",
                    "Decimal u128 account id of the conversation partner.",
                ),
            ],
            request_example: None,
            responses: vec![resp(
                200,
                "The DM history with this partner, newest last.",
                r#"{
  "messages": [
    {
      "id": "1002938475",
      "sender": "5137",
      "sender_name": "gaius77",
      "body": "Send grain, we're starving.",
      "created_ms": 1781999500000
    }
  ]
}"#,
            )],
            errors: {
                let mut v = base_agent_world_errors();
                v.push(err(
                    404,
                    "not_found",
                    "account does not parse as a decimal u128, or no such conversation partner exists.",
                ));
                v
            },
        },
    ];

    ApiGroup {
        name: "Agent API",
        anchor: "agent-api",
        auth_blurb: "Bearer epk_<id>_<secret> — minted per AI account by an Administrator (/admin → \
            AI agents), shown exactly once (only a SHA-256 of the secret is stored). Bulk seeding emits a \
            one-time JSON manifest [{\"username\", \"token\"}] consumed directly by the eperica-bots \
            runner. All /api traffic (GETs included) counts against agent_limit_per_window — 120 \
            requests/min at current config (specs/balance/fairplay.toml) — per key; see the fairplay rules.",
        endpoints,
    }
}

// ---------------------------------------------------------------------------
// The Spectator API group (125) — from `crate::spectator_api::router()`.
// ---------------------------------------------------------------------------

fn spectator_group() -> ApiGroup {
    let world_param = || path_param("world", "string", "The world's UUID (path segment).");

    let endpoints = vec![
        Endpoint {
            method: "GET",
            path: "/spectator/me",
            summary: "Key introspection: the bound spectator account.",
            description: "Confirms the key is live and which account it belongs to — the spectator analogue \
                of GET /api/me. There is no per-world \"player\" for a spectator; the Spectator role itself is \
                the only gate.",
            auth: "spectator",
            params: vec![],
            request_example: None,
            responses: vec![resp(
                200,
                "The bound account.",
                r#"{
  "account": "5820113344",
  "username": "caster_ana",
  "is_spectator": true
}"#,
            )],
            errors: base_spectator_account_errors(),
        },
        Endpoint {
            method: "GET",
            path: "/spectator/w/{world}/feed",
            summary: "The capped world activity snapshot — movements, shipments, builds, training, reports.",
            description: "The same data the /spectate/{world} dashboard renders, assembled per request from \
                existing state (no new event store, P1/P11). Each category is capped (≤50) and ordered \
                soonest/most-recent first. Unlike a defender's own arrival-only view of an incoming hostile \
                movement, this feed shows full composition for both directions — omniscience is the point \
                (AC4/AC5). All deadlines are absolute Unix-ms.",
            auth: "spectator",
            params: vec![world_param()],
            request_example: None,
            responses: vec![resp(
                200,
                "…truncated: one row per category is shown; each array is capped at 50 in the real feed.",
                r#"{
  "world": "b6f8f6d2-3c1a-4e9b-8f2a-7d4c5b6a9e10",
  "now_ms": 1782000000000,
  "movements": [
    {
      "id": "771122334",
      "kind": "attack",
      "origin": { "village": "1d8e9f3a-2b4c-4d5e-8f6a-3b2c1d0e9f8a", "x": 12, "y": -7, "owner": "ai_marcus" },
      "destination": { "village": "4a3b2c1d-0e9f-48a7-b6c5-d4e3f2a1b0c9", "x": 14, "y": -6, "owner": "gaius77" },
      "arrive_at_ms": 1782001800000,
      "troops": { "legionnaire": 40, "imperian": 10 }
    }
  ],
  "shipments": [
    {
      "id": "556677889",
      "kind": "deliver",
      "origin": { "village": "1d8e9f3a-2b4c-4d5e-8f6a-3b2c1d0e9f8a", "x": 12, "y": -7, "owner": "ai_marcus" },
      "destination": { "village": "4a3b2c1d-0e9f-48a7-b6c5-d4e3f2a1b0c9", "x": 14, "y": -6, "owner": "gaius77" },
      "arrive_at_ms": 1782001000000,
      "give": { "wood": 500, "clay": 300, "iron": 200, "crop": 0 },
      "merchants": 2
    }
  ],
  "builds": [
    {
      "village": "1d8e9f3a-2b4c-4d5e-8f6a-3b2c1d0e9f8a", "x": 12, "y": -7, "owner": "ai_marcus",
      "target": "building", "slot": 19, "kind": "barracks",
      "target_level": 4, "completes_at_ms": 1782000900000
    }
  ],
  "trainings": [
    {
      "village": "1d8e9f3a-2b4c-4d5e-8f6a-3b2c1d0e9f8a", "x": 12, "y": -7, "owner": "ai_marcus",
      "unit": "legionnaire", "remaining": 25, "next_complete_at_ms": 1782000420000
    }
  ],
  "reports": [
    {
      "id": "48291033512",
      "occurred_at_ms": 1781998000000,
      "kind": "raid",
      "attacker": { "name": "ai_marcus", "x": 12, "y": -7 },
      "defender": { "name": "gaius77", "x": 14, "y": -6 },
      "outcome": "ai_marcus raided gaius77 and won, looting 2000 resources"
    }
  ]
}"#,
            )],
            errors: base_spectator_world_errors(),
        },
        Endpoint {
            method: "GET",
            path: "/spectator/w/{world}/players",
            summary: "A paged, population-descending index of every player in the world.",
            description: "50 players per page (missing or non-positive page ⇒ 1; a non-numeric value is a plain 400). npc is \
                derived server-side as is_ai && world.ai_labeled — the raw is_ai truth is never itself \
                serialized on either a labeled or a disguised world (AC7). Each row also carries villages — \
                every village that player owns, for the players → village drill-down.",
            auth: "spectator",
            params: vec![
                world_param(),
                query_param(
                    "page",
                    "integer",
                    false,
                    "1-based page number; missing or non-positive defaults to 1 (non-numeric is a 400).",
                ),
            ],
            request_example: None,
            responses: vec![resp(
                200,
                "One page of the population-descending player index.",
                r#"{
  "world": "b6f8f6d2-3c1a-4e9b-8f2a-7d4c5b6a9e10",
  "page": 1,
  "has_next": false,
  "players": [
    {
      "player": "9821",
      "username": "ai_marcus",
      "tribe": "romans",
      "population": 1840,
      "village_count": 3,
      "villages": [
        { "id": "1d8e9f3a-2b4c-4d5e-8f6a-3b2c1d0e9f8a", "x": 12, "y": -7, "capital": true }
      ],
      "alliance_tag": "ROM",
      "npc": true
    }
  ]
}"#,
            )],
            errors: base_spectator_world_errors(),
        },
        Endpoint {
            method: "GET",
            path: "/spectator/w/{world}/village/{id}",
            summary: "The omniscient village detail — equal to what the village's own owner sees.",
            description: "id is the village's hyphenated UUID. Resources are computed on read (P1); fields, \
                buildings, the build queue with deadlines, training batches, garrison, stationed \
                reinforcements, loyalty and research are all reused unchanged from the owner-view read model \
                with the true owner substituted for the caller — no fog of war, no redaction (the point of \
                the surface).",
            auth: "spectator",
            params: vec![
                world_param(),
                path_param("id", "string", "The village's hyphenated UUID."),
            ],
            request_example: None,
            responses: vec![resp(
                200,
                "The full village detail, unredacted.",
                r#"{
  "world": "b6f8f6d2-3c1a-4e9b-8f2a-7d4c5b6a9e10",
  "village": "1d8e9f3a-2b4c-4d5e-8f6a-3b2c1d0e9f8a",
  "owner": "ai_marcus",
  "x": 12, "y": -7, "capital": true, "tribe": "romans",
  "resources": {
    "wood": { "amount": 3200, "rate": 180, "capacity": 8000 },
    "clay": { "amount": 2900, "rate": 160, "capacity": 8000 },
    "iron": { "amount": 3100, "rate": 170, "capacity": 8000 },
    "crop": { "amount": 4200, "rate": 95, "capacity": 8000 }
  },
  "fields": [ { "slot": 0, "kind": "wood", "level": 6 } ],
  "buildings": [ { "slot": 19, "kind": "main_building", "level": 10 } ],
  "build_queue": [
    { "target": "building", "slot": 19, "kind": "barracks", "level": 4, "completes_at_ms": 1782000900000 }
  ],
  "training": [ { "unit": "legionnaire", "remaining": 25, "next_complete_at_ms": 1782000420000 } ],
  "garrison": [ { "unit": "legionnaire", "count": 40 } ],
  "reinforcements": [],
  "loyalty": 100,
  "researched": ["legionnaire"]
}"#,
            )],
            errors: {
                let mut v = base_spectator_world_errors();
                v.push(err(
                    404,
                    "not_found",
                    "id does not parse as a UUID, or no such village exists.",
                ));
                v
            },
        },
    ];

    ApiGroup {
        name: "Spectator API",
        anchor: "spectator-api",
        auth_blurb: "Bearer spk_<id>_<secret> — minted per account by an Administrator (/admin → \
            Spectator keys), shown exactly once. Authenticates only while the bound account currently holds \
            the Spectator role (re-checked on every request, not just at mint) — revoking the role \
            dead-ends every key that account holds instantly. Read-only by construction: no mutating route is \
            registered on this router at all, so a POST to a recognized path 405s and a POST to an \
            unrecognized one 404s — never a write. Shares the same rate-budget class as the Agent API \
            (agent_limit_per_window, 120 requests/min at current config).",
        endpoints,
    }
}

/// The full registry: Agent API, then Spectator API (plan §Module changes).
pub fn registry() -> Vec<ApiGroup> {
    vec![agent_group(), spectator_group()]
}

/// The registry's `(method, path)` pairs — the coverage test's (T2) comparison target against the
/// real `/api` and `/spectator` routers. Paths carry the mount prefix (`/api/…`, `/spectator/…`).
pub fn registry_paths() -> Vec<(&'static str, String)> {
    registry()
        .into_iter()
        .flat_map(|g| {
            g.endpoints
                .into_iter()
                .map(|e| (e.method, e.path.to_owned()))
        })
        .collect()
}

// ---------------------------------------------------------------------------
// OpenAPI 3.0.3 export (AC4) — generated in memory from the same registry (P11).
// ---------------------------------------------------------------------------

fn param_location_str(loc: ParamLocation) -> &'static str {
    match loc {
        ParamLocation::Path => "path",
        ParamLocation::Query => "query",
    }
}

fn security_scheme_name(auth: &str) -> &'static str {
    if auth == "agent" {
        "agentBearer"
    } else {
        "spectatorBearer"
    }
}

/// Build one OpenAPI Operation Object from a registry [`Endpoint`].
fn openapi_operation(group_name: &str, ep: &Endpoint) -> Value {
    let parameters: Vec<Value> = ep
        .params
        .iter()
        .map(|p| {
            json!({
                "name": p.name,
                "in": param_location_str(p.location),
                "required": p.required,
                "description": p.description,
                "schema": { "type": p.ty },
            })
        })
        .collect();

    let mut responses = serde_json::Map::new();
    for r in &ep.responses {
        let example: Value = serde_json::from_str(r.example).unwrap_or(Value::Null) /* unreachable: every_example_string_parses_as_json guards all literals */;
        responses.insert(
            r.status.to_string(),
            json!({
                "description": r.description,
                "content": { "application/json": { "example": example } },
            }),
        );
    }
    // Group error cases by status: one representative example per status, every code+reason
    // documented in the (concatenated) description — OpenAPI has one response object per status.
    let mut by_status: std::collections::BTreeMap<u16, Vec<&ErrorCase>> = Default::default();
    for e in &ep.errors {
        by_status.entry(e.status).or_default().push(e);
    }
    for (status, cases) in by_status {
        let description = cases
            .iter()
            .map(|c| format!("`{}` — {}", c.code, c.when))
            .collect::<Vec<_>>()
            .join(" ");
        let example = json!({ "error": cases[0].code, "reason": cases[0].when });
        responses.entry(status.to_string()).or_insert_with(|| {
            json!({
                "description": description,
                "content": { "application/json": { "example": example } },
            })
        });
    }

    let mut security_obj = serde_json::Map::new();
    security_obj.insert(security_scheme_name(ep.auth).to_owned(), json!([]));

    let mut operation = serde_json::Map::new();
    operation.insert("summary".to_owned(), json!(ep.summary));
    operation.insert("description".to_owned(), json!(ep.description));
    operation.insert("tags".to_owned(), json!([group_name]));
    operation.insert("security".to_owned(), json!([Value::Object(security_obj)]));
    if !parameters.is_empty() {
        operation.insert("parameters".to_owned(), json!(parameters));
    }
    if let Some(req) = ep.request_example {
        let example: Value = serde_json::from_str(req).unwrap_or(Value::Null) /* unreachable: every_example_string_parses_as_json guards all literals */;
        operation.insert(
            "requestBody".to_owned(),
            json!({
                "required": true,
                "content": { "application/json": { "example": example } },
            }),
        );
    }
    operation.insert("responses".to_owned(), Value::Object(responses));
    Value::Object(operation)
}

/// The full OpenAPI 3.0.3 document (AC4), built once in memory from [`registry`] — no I/O, no
/// vendored schema modeling (schemas are declared as example-driven v1 in `info.description`).
pub fn openapi_json() -> Value {
    let groups = registry();

    let mut paths = serde_json::Map::new();
    let mut tags = Vec::new();
    for group in &groups {
        tags.push(json!({ "name": group.name, "description": group.auth_blurb }));
        for ep in &group.endpoints {
            let item = paths.entry(ep.path.to_owned()).or_insert_with(|| json!({}));
            let method_key = ep.method.to_lowercase();
            item.as_object_mut()
                .expect("path item is always built as an object")
                .insert(method_key, openapi_operation(group.name, ep));
        }
    }

    json!({
        "openapi": "3.0.3",
        "info": {
            "title": "Eperica APIs",
            "version": "1.0",
            "description": "The Agent API (epk_ bearer) and Spectator API (spk_ bearer). Schemas are \
                example-driven v1: request/response bodies are declared as permissive object types and the \
                per-operation `example` is the authoritative shape, not a full JSON Schema.",
        },
        "servers": [ { "url": "/" } ],
        "tags": tags,
        "components": {
            "securitySchemes": {
                "agentBearer": {
                    "type": "http",
                    "scheme": "bearer",
                    "bearerFormat": "epk_<id>_<secret>",
                },
                "spectatorBearer": {
                    "type": "http",
                    "scheme": "bearer",
                    "bearerFormat": "spk_<id>_<secret>",
                },
            },
        },
        "paths": Value::Object(paths),
    })
}

/// The OpenAPI document, built exactly once at first access (P11/AC6) — `GET /docs/api/openapi.json`
/// (T2) clones this already-built [`Value`] per request rather than re-walking [`registry`] and
/// re-parsing every example literal on every hit. The HTML reference page (T2) instead calls
/// [`registry`] fresh per request: that page's per-request cost is dominated by Askama rendering
/// anyway, and keeping it off the shared static avoids the two renderings ever reading the registry
/// through different code paths.
pub static OPENAPI_DOC: LazyLock<Value> = LazyLock::new(openapi_json);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn method_path_pairs_are_unique() {
        let paths = registry_paths();
        let mut seen = std::collections::HashSet::new();
        for (method, path) in &paths {
            assert!(
                seen.insert((*method, path.clone())),
                "duplicate registry entry: {method} {path}"
            );
        }
        // Sanity: the full agent (18) + spectator (4) surface is present.
        assert_eq!(paths.len(), 22, "unexpected registry size: {}", paths.len());
    }

    #[test]
    fn every_endpoint_has_at_least_one_response() {
        for group in registry() {
            for ep in group.endpoints {
                assert!(
                    !ep.responses.is_empty(),
                    "{} {} has no documented response",
                    ep.method,
                    ep.path
                );
            }
        }
    }

    #[test]
    fn every_post_has_a_request_example() {
        for group in registry() {
            for ep in group.endpoints {
                if ep.method == "POST" {
                    assert!(
                        ep.request_example.is_some(),
                        "{} {} is a POST with no request_example",
                        ep.method,
                        ep.path
                    );
                }
            }
        }
    }

    #[test]
    fn every_get_has_no_request_example() {
        // Not a hard requirement of the plan, but catches a copy-paste mistake early.
        for group in registry() {
            for ep in group.endpoints {
                if ep.method == "GET" {
                    assert!(
                        ep.request_example.is_none(),
                        "{} {} is a GET but carries a request_example",
                        ep.method,
                        ep.path
                    );
                }
            }
        }
    }

    #[test]
    fn every_example_string_parses_as_json() {
        let mut checked = 0;
        for group in registry() {
            for ep in group.endpoints {
                if let Some(req) = ep.request_example {
                    serde_json::from_str::<Value>(req).unwrap_or_else(|e| {
                        panic!(
                            "{} {} request example is invalid JSON: {e}\n{req}",
                            ep.method, ep.path
                        )
                    });
                    checked += 1;
                }
                for r in &ep.responses {
                    serde_json::from_str::<Value>(r.example).unwrap_or_else(|e| {
                        panic!(
                            "{} {} response {} example is invalid JSON: {e}\n{}",
                            ep.method, ep.path, r.status, r.example
                        )
                    });
                    checked += 1;
                }
            }
        }
        // Guards against an accidental no-op refactor silently checking zero literals.
        assert!(
            checked >= 20,
            "expected at least 20 JSON literals checked, got {checked}"
        );
    }

    #[test]
    fn openapi_has_required_top_level_keys() {
        let doc = openapi_json();
        let obj = doc.as_object().expect("document is a JSON object");
        for key in ["openapi", "info", "servers", "paths"] {
            assert!(
                obj.contains_key(key),
                "openapi document missing top-level key {key}"
            );
        }
        assert_eq!(doc["openapi"].as_str(), Some("3.0.3"));
        assert!(doc["info"]["title"].as_str().is_some());
        assert!(doc["info"]["version"].as_str().is_some());
        let servers = doc["servers"].as_array().expect("servers is an array");
        assert!(servers.iter().any(|s| s["url"] == "/"));
    }

    #[test]
    fn openapi_paths_cover_every_registry_entry() {
        let doc = openapi_json();
        let paths = doc["paths"].as_object().expect("paths is an object");
        for (method, path) in registry_paths() {
            let item = paths
                .get(&path)
                .unwrap_or_else(|| panic!("openapi document missing path {path}"));
            let op = item
                .get(method.to_lowercase())
                .unwrap_or_else(|| panic!("openapi document missing operation {method} {path}"));
            assert!(
                op.get("responses").is_some(),
                "{method} {path} operation has no responses object"
            );
            assert!(
                op["responses"].as_object().is_some_and(|m| !m.is_empty()),
                "{method} {path} operation has an empty responses object"
            );
        }
    }
}
