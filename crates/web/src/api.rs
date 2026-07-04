//! The Agent API (118, ADR 0036) — the JSON surface AI agents play through.
//!
//! Agents are **true clients**: a bearer key resolves to an AI account, and from there every request
//! flows through the **same** world-scope resolution and use-cases as a browser session (P4 — no
//! agent-only bypass). Failures are structured JSON (`ApiError`), never redirects: an agent must be
//! able to branch on a machine-readable `error` code.

use axum::Json;
use axum::extract::FromRequestParts;
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use eperica_application::{
    AccountRepository, AllianceRepository, BuildRepository, CombatError, CombatRepository,
    CommsError, MovementError, MovementRepository, OasisRepository, ResearchError, ScoutError,
    ScoutIntel, ScoutRepository, SettleError, TradeError, TradeRepository, TrainingRepository,
    UnitRepository, UpgradeError, conversation_list, open_dm, order_attack, order_reinforcement,
    order_research, order_return, order_scout, order_settle, order_smithy_upgrade, order_trade,
    parse_dm_key, send_dm,
};
use eperica_domain::{
    AttackMode, Coordinate, MovementKind, PlayerId, ResourceAmounts, ScoutTarget, Timestamp,
    TradeKind, UnitId, account_blocked,
};
use eperica_infrastructure::now;

use crate::state::AppState;
use crate::{apikey, auth};

/// The one JSON error shape every agent endpoint returns (plan Decision #6):
/// `{ "error": "<machine_code>", "reason": "<player-visible text>" }`.
#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    /// Stable snake_case machine code (`unauthorized`, `not_joined`, `rate_limited`, …).
    pub code: &'static str,
    /// The human-readable reason — the same message a browser player would see.
    pub reason: String,
}

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, reason: impl Into<String>) -> Self {
        Self {
            status,
            code,
            reason: reason.into(),
        }
    }
    pub fn unauthorized() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "Missing, malformed, revoked or unknown API key.",
        )
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(serde_json::json!({ "error": self.code, "reason": self.reason })),
        )
            .into_response()
    }
}

/// The parsed `(id, secret)` of a request's bearer token, or `None` when the header is missing,
/// non-`Bearer `, or malformed. **Strict about the `Bearer ` prefix** and shared with the rate guard
/// (`crate::agent_rate_guard`) so authentication and budgeting can never disagree on what counts as
/// a token (the review's M1: an unprefixed token must not authenticate while escaping the budget).
pub(crate) fn bearer_token(headers: &axum::http::HeaderMap) -> Option<(String, String)> {
    let token = headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")?;
    apikey::parse(token)
}

/// Resolve the `Authorization: Bearer epk_…` header to the bound **AI account** (AC1):
/// parse → key lookup by id → constant-time secret verify → not revoked → account exists,
/// **is_ai**, and not banned/suspended. Sanction enforcement lives here because agents never pass
/// the login chokepoint (019/022) — a blocked AI account is refused on **every** request. Backend
/// failures surface as 500 `internal` — never as a 401 that would make a bot retire a healthy key.
async fn bearer_account(parts: &Parts, state: &AppState) -> Result<PlayerId, ApiError> {
    let internal = |what: &'static str| {
        move |e| {
            tracing::error!(error = %e, "bearer resolution failed: {what}");
            ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", what)
        }
    };
    let (id, secret) = bearer_token(&parts.headers).ok_or_else(ApiError::unauthorized)?;
    let key = state
        .accounts
        .find_agent_key(&id)
        .await
        .map_err(internal("key lookup"))?
        .ok_or_else(ApiError::unauthorized)?;
    if key.revoked || !apikey::verify(&secret, &key.secret_hash) {
        return Err(ApiError::unauthorized());
    }
    let user = state
        .accounts
        .find_user_by_id(key.user)
        .await
        .map_err(internal("account lookup"))?
        .ok_or_else(ApiError::unauthorized)?;
    // Keys bind only to AI accounts (ADR 0036) — a key pointing anywhere else is refused outright.
    if !user.is_ai {
        return Err(ApiError::unauthorized());
    }
    if account_blocked(user.banned_at, user.suspended_until, Timestamp(now().0)) {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "account_blocked",
            "Your account is suspended or banned for a fair-play violation.",
        ));
    }
    // 123: a playing agent is an ACTIVE player — refresh last_activity exactly like the web
    // presence middleware does (the port is throttled: at most one small write per window).
    // Same posture as lib.rs: a failed touch is logged, never breaks the request (AC3).
    if let Err(e) = state
        .accounts
        .touch_activity(key.user, Timestamp(now().0))
        .await
    {
        tracing::error!(error = %e, "agent activity touch failed");
    }
    Ok(key.user)
}

/// Extractor: the bearer-authenticated AI **account** (no world scope) — `/api/me`.
pub struct AgentAccount(pub PlayerId);

impl FromRequestParts<AppState> for AgentAccount {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(AgentAccount(bearer_account(parts, state).await?))
    }
}

/// Extractor: the bearer-authenticated agent **in the selected world** — the agent-side twin of
/// [`auth::GameContext`]. Runs the same resolution core (world from path → `player_in_world` →
/// registry), so the two paths cannot drift (plan §Key risks); only the failure mapping differs
/// (JSON here, redirects there).
pub struct AgentGame(pub auth::GameContext);

impl FromRequestParts<AppState> for AgentGame {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let account = bearer_account(parts, state).await?;
        let ctx = auth::resolve_game_context(parts, state, account)
            .await
            .map_err(|f| match f {
                auth::WorldResolveFailure::NoWorld => ApiError::new(
                    StatusCode::NOT_FOUND,
                    "unknown_world",
                    "No such world in the path.",
                ),
                auth::WorldResolveFailure::NotJoined => ApiError::new(
                    StatusCode::FORBIDDEN,
                    "not_joined",
                    "This account has no player in that world.",
                ),
            })?;
        Ok(AgentGame(ctx))
    }
}

/// `GET /api/me` — key introspection (AC1): the bound account and its per-world players.
async fn me(
    axum::extract::State(state): axum::extract::State<AppState>,
    AgentAccount(account): AgentAccount,
) -> Result<Response, ApiError> {
    let internal = |what: &'static str| {
        move |e| {
            tracing::error!(error = %e, "/api/me read failed: {what}");
            ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", what)
        }
    };
    let user = state
        .accounts
        .find_user_by_id(account)
        .await
        .map_err(internal("account"))?
        .ok_or_else(ApiError::unauthorized)?;
    let worlds: Vec<serde_json::Value> = state
        .accounts
        .worlds_of_user(account)
        .await
        .map_err(internal("worlds"))?
        .into_iter()
        .map(|w| {
            serde_json::json!({
                "world": crate::handlers::world_id_str(w.world),
                "player": w.player.0.to_string(),
                "tribe": w.tribe.slug(),
            })
        })
        .collect();
    Ok(Json(serde_json::json!({
        "account": account.0.to_string(),
        "username": user.username,
        "is_ai": user.is_ai,
        "worlds": worlds,
    }))
    .into_response())
}

// ---------------------------------------------------------------------------
// The state digest (AC3) — one compact, fog-of-war-honest document per world.
// ---------------------------------------------------------------------------

/// One resource line in the digest: settled amount + hourly rate + storage cap (page truth — the
/// same numbers the resource ribbon renders).
#[derive(serde::Serialize)]
struct ResourceLine {
    amount: i64,
    rate: i64,
    capacity: i64,
}

#[derive(serde::Serialize)]
struct SlotLevel {
    slot: u8,
    kind: &'static str,
    level: u8,
}

#[derive(serde::Serialize)]
struct QueueEntry {
    target: &'static str,
    slot: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    kind: Option<&'static str>,
    level: u8,
    completes_at_ms: i64,
}

#[derive(serde::Serialize)]
struct TrainingEntry {
    building: &'static str,
    unit: String,
    remaining: u32,
    next_complete_at_ms: i64,
}

#[derive(serde::Serialize)]
struct GarrisonEntry {
    unit: String,
    count: u32,
}

#[derive(serde::Serialize)]
struct UnitLevel {
    unit: String,
    level: u8,
}

#[derive(serde::Serialize)]
struct ActiveOrderEntry {
    kind: &'static str,
    unit: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    target_level: Option<u8>,
    complete_at_ms: i64,
}

#[derive(serde::Serialize)]
struct ResearchDigest {
    researched: Vec<String>,
    levels: Vec<UnitLevel>,
    active: Vec<ActiveOrderEntry>,
}

#[derive(serde::Serialize)]
struct VillageDigest {
    id: String,
    x: i32,
    y: i32,
    capital: bool,
    resources: serde_json::Value,
    fields: Vec<SlotLevel>,
    buildings: Vec<SlotLevel>,
    build_queue: Vec<QueueEntry>,
    training: Vec<TrainingEntry>,
    garrison: Vec<GarrisonEntry>,
    research: ResearchDigest,
    /// Reinforcement groups stationed here from allied players — from `reinforcements_at`.
    reinforcements_here: Vec<serde_json::Value>,
}

/// `GET /api/w/{world}/state` — the digest (AC3): every number equals what the corresponding page
/// renders at the same instant, because it is assembled from the **same read models** (`load_economy`,
/// `active_builds`, `active_training`, `load_culture`, `incoming_against`, `reports_for`). Incoming
/// attacks carry the target village + arrival **only** (§7.3, P4).
async fn state_digest(AgentGame(ctx): AgentGame) -> Result<Response, ApiError> {
    use eperica_application::load_culture;
    use eperica_application::load_economy;
    let now_ts = Timestamp(now().0);
    let internal = |what: &'static str| {
        move |e| {
            tracing::error!(error = %e, "digest read failed: {what}");
            ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", what)
        }
    };

    let villages = ctx
        .accounts
        .villages_of(ctx.player)
        .await
        .map_err(internal("villages"))?;
    let ids: Vec<_> = villages.iter().map(|v| v.id).collect();

    let mut village_digests = Vec::with_capacity(villages.len());
    for v in &villages {
        // Per-village economy via the page's own loader (selected = this village) — digest = page truth.
        let Some(econ) = load_economy(
            &ctx.accounts,
            &ctx.rules.economy,
            &ctx.rules.units,
            ctx.speed,
            now_ts,
            ctx.player,
            Some(v.id),
        )
        .await
        .map_err(internal("economy"))?
        else {
            continue;
        };
        let e = &econ.economy;
        let builds = ctx
            .accounts
            .active_builds(v.id)
            .await
            .map_err(internal("builds"))?;
        let training = ctx
            .accounts
            .active_training(v.id)
            .await
            .map_err(internal("training"))?;
        let researched = ctx
            .accounts
            .researched_units(v.id)
            .await
            .map_err(internal("researched_units"))?;
        let unit_levels = ctx
            .accounts
            .unit_levels(v.id)
            .await
            .map_err(internal("unit_levels"))?;
        let unit_orders = ctx
            .accounts
            .active_unit_orders(v.id)
            .await
            .map_err(internal("unit_orders"))?;
        let reinf_here_raw = ctx
            .accounts
            .reinforcements_at(v.id)
            .await
            .map_err(internal("reinforcements_here"))?;
        let reinforcements_here: Vec<serde_json::Value> = reinf_here_raw
            .into_iter()
            .map(|g| {
                let troops: std::collections::BTreeMap<String, u32> = g
                    .troops
                    .iter()
                    .map(|(u, c)| (u.as_str().to_owned(), *c))
                    .collect();
                serde_json::json!({
                    // home_village = the guest/reinforcer's home village (hyphenated UUID, §064)
                    "home_village": crate::handlers::village_seg(g.home_village),
                    // other_coord = the guest's home coord (viewed by the host)
                    "x": g.other_coord.x,
                    "y": g.other_coord.y,
                    // other_owner = the guest/reinforcer's owner name
                    "owner": g.other_owner,
                    "troops": troops,
                })
            })
            .collect();
        village_digests.push(VillageDigest {
            id: crate::handlers::village_seg(v.id),
            x: v.coordinate.x,
            y: v.coordinate.y,
            capital: v.is_capital,
            resources: serde_json::json!({
                "wood": ResourceLine { amount: e.amounts.wood, rate: e.rates.wood, capacity: e.capacities.warehouse },
                "clay": ResourceLine { amount: e.amounts.clay, rate: e.rates.clay, capacity: e.capacities.warehouse },
                "iron": ResourceLine { amount: e.amounts.iron, rate: e.rates.iron, capacity: e.capacities.warehouse },
                "crop": ResourceLine { amount: e.amounts.crop, rate: e.rates.crop_net, capacity: e.capacities.granary },
            }),
            fields: v
                .fields
                .iter()
                .enumerate()
                .map(|(i, f)| SlotLevel {
                    slot: u8::try_from(i).unwrap_or(u8::MAX),
                    kind: crate::handlers::resource_slug(f.kind),
                    level: f.level,
                })
                .collect(),
            buildings: v
                .buildings
                .iter()
                .map(|b| SlotLevel {
                    slot: b.slot,
                    kind: crate::handlers::building_kind_id(b.kind),
                    level: b.level,
                })
                .collect(),
            build_queue: builds
                .into_iter()
                .map(|b| match b.target {
                    eperica_domain::BuildTarget::Field { slot } => QueueEntry {
                        target: "field",
                        slot,
                        kind: None,
                        level: b.target_level,
                        completes_at_ms: b.complete_at.0,
                    },
                    eperica_domain::BuildTarget::Building { slot, kind } => QueueEntry {
                        target: "building",
                        slot,
                        kind: Some(crate::handlers::building_kind_id(kind)),
                        level: b.target_level,
                        completes_at_ms: b.complete_at.0,
                    },
                })
                .collect(),
            training: training
                .into_iter()
                .map(|t| TrainingEntry {
                    building: crate::handlers::building_kind_id(t.building),
                    unit: t.unit.as_str().to_owned(),
                    remaining: t.count_total.saturating_sub(t.count_done),
                    next_complete_at_ms: t.next_complete_at.0,
                })
                .collect(),
            garrison: econ
                .garrison
                .iter()
                .map(|(unit, count)| GarrisonEntry {
                    unit: unit.as_str().to_owned(),
                    count: *count,
                })
                .collect(),
            research: ResearchDigest {
                researched: researched.into_iter().map(|u| u.as_str().to_owned()).collect(),
                levels: unit_levels
                    .into_iter()
                    .map(|(u, lvl)| UnitLevel {
                        unit: u.as_str().to_owned(),
                        level: lvl,
                    })
                    .collect(),
                active: unit_orders
                    .into_iter()
                    .map(|o| ActiveOrderEntry {
                        kind: match o.kind {
                            eperica_application::UnitOrderKind::Research => "research",
                            eperica_application::UnitOrderKind::SmithyUpgrade => "smithy",
                        },
                        unit: o.unit.as_str().to_owned(),
                        target_level: o.target_level,
                        complete_at_ms: o.complete_at.0,
                    })
                    .collect(),
            },
            reinforcements_here,
        });
    }

    let culture = load_culture(
        &ctx.accounts,
        &ctx.accounts,
        &ctx.rules.culture,
        now_ts,
        ctx.player,
    )
    .await
    .map_err(internal("culture"))?;
    // §7.3 / P4: target village + arrival ONLY — never origin or composition.
    let incoming: Vec<serde_json::Value> = ctx
        .accounts
        .incoming_against(&ids)
        .await
        .map_err(internal("incoming"))?
        .into_iter()
        .map(|a| {
            serde_json::json!({
                "village": crate::handlers::village_seg(a.target),
                "arrive_at_ms": a.arrive_at.0,
            })
        })
        .collect();
    // Latest battle report heads — now with kind (119 T4).
    let reports: Vec<serde_json::Value> = ctx
        .accounts
        .reports_for(ctx.player, 10)
        .await
        .map_err(internal("reports"))?
        .into_iter()
        .map(|r| {
            let kind = movement_kind_str(r.kind);
            serde_json::json!({
                "id": r.id.to_string(),
                "occurred_at_ms": r.occurred_at.0,
                "attacker_won": r.attacker_won,
                "kind": kind,
            })
        })
        .collect();

    // Per-player active movements (reuses the movement_json helper from T1/T3 actions).
    let movements: Vec<serde_json::Value> = ctx
        .accounts
        .active_movements(ctx.player)
        .await
        .map_err(internal("movements"))?
        .iter()
        .map(movement_json)
        .collect();

    // Reinforcement groups the player has stationed abroad (host = the foreign village).
    // StationedGroup viewed as the owner: other_coord = host's coord, other_owner = host's owner.
    let reinforcements_abroad: Vec<serde_json::Value> = ctx
        .accounts
        .reinforcements_of(ctx.player)
        .await
        .map_err(internal("reinforcements_abroad"))?
        .into_iter()
        .map(|g| {
            let troops: std::collections::BTreeMap<String, u32> = g
                .troops
                .iter()
                .map(|(u, c)| (u.as_str().to_owned(), *c))
                .collect();
            serde_json::json!({
                // host_village = where the troops are currently stationed (hyphenated UUID, §064)
                "host_village": crate::handlers::village_seg(g.host_village),
                // other_coord = the host's coordinate (viewed by the owner)
                "x": g.other_coord.x,
                "y": g.other_coord.y,
                // other_owner = the host village's owner
                "owner": g.other_owner,
                "troops": troops,
            })
        })
        .collect();

    // Latest scout report heads (ids + metadata) — intel is in the full /scout-report/{id} read.
    let scout_reports: Vec<serde_json::Value> = ctx
        .accounts
        .scout_reports_for(ctx.player, 10)
        .await
        .map_err(internal("scout_reports"))?
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id.to_string(),
                "occurred_at_ms": r.occurred_at.0,
                "viewer_is_scouter": r.viewer_is_scouter,
                "detected": r.detected,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "world": crate::handlers::world_id_str(ctx.world_id),
        "player": ctx.player.0.to_string(),
        "now_ms": now_ts.0,
        "villages": village_digests,
        "culture": {
            "cp": culture.cp,
            "rate_per_hour": culture.rate,
            "villages_used": culture.used_slots,
            "villages_allowed": culture.allowed_villages,
            "next_threshold": culture.next_threshold,
        },
        "incoming_attacks": incoming,
        "reports": reports,
        "movements": movements,
        "reinforcements_abroad": reinforcements_abroad,
        "scout_reports": scout_reports,
    }))
    .into_response())
}

/// Map-window query: centre + half-extent (`r`, clamped — P11).
#[derive(serde::Deserialize)]
struct MapWindowQuery {
    x: i32,
    y: i32,
    #[serde(default = "default_map_r")]
    r: i32,
}
fn default_map_r() -> i32 {
    7
}

/// `GET /api/w/{world}/map?x&y&r` — a bounded map window carrying exactly what the map page carries
/// (AC3: same `map_cells` builder as `/map/tiles`, 093).
async fn map_window(
    AgentGame(ctx): AgentGame,
    axum::extract::Query(q): axum::extract::Query<MapWindowQuery>,
) -> Result<Response, ApiError> {
    use eperica_application::{map_viewport_rect, viewport_coords_rect};
    let internal = |what: &'static str| {
        move |e| {
            tracing::error!(error = %e, "map window read failed: {what}");
            ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", what)
        }
    };
    let user = ctx
        .accounts
        .find_user_by_id(ctx.account)
        .await
        .map_err(internal("account"))?
        .ok_or_else(ApiError::unauthorized)?;
    let villages = ctx
        .accounts
        .villages_of(ctx.player)
        .await
        .map_err(internal("villages"))?;
    let capital_coord = villages.iter().find(|v| v.is_capital).map(|v| v.coordinate);
    let selected = villages
        .iter()
        .find(|v| v.is_capital)
        .or_else(|| villages.first());
    let acting_vid = selected.map(|v| crate::handlers::village_seg(v.id));
    let origin = selected.map(|v| v.coordinate);

    let radius = ctx.map.radius();
    let center = eperica_domain::Coordinate::new(q.x, q.y).wrapped(radius);
    let r = q.r.clamp(0, 10);
    let coords = viewport_coords_rect(center, r, r, radius);
    let markers = ctx
        .accounts
        .villages_at(&coords)
        .await
        .map_err(internal("markers"))?;
    let oasis_owners: std::collections::HashMap<_, _> = ctx
        .accounts
        .oasis_owners_at(&coords)
        .await
        .map_err(internal("oases"))?
        .into_iter()
        .collect();
    let viewport = map_viewport_rect(ctx.map.as_ref(), center, r, r, &markers);
    let rows = crate::handlers::map_cells(
        ctx.world_id,
        ctx.map.as_ref(),
        ctx.rules.lifecycle.inactive_after_secs,
        ctx.rules.lifecycle.presence_online_secs,
        &viewport,
        &oasis_owners,
        &user.username,
        capital_coord,
        origin,
        acting_vid.as_deref(),
        ctx.ai_labeled,
    );
    Ok(Json(serde_json::json!({
        "center_x": center.x,
        "center_y": center.y,
        "r": r,
        "rows": rows,
    }))
    .into_response())
}

// ---------------------------------------------------------------------------
// Economy actions (AC4) — thin JSON adapters onto the existing use-cases (P4).
// ---------------------------------------------------------------------------

/// Map a [`BuildError`] to the API error contract (plan Decision #6): a stable machine code + the
/// same player-visible message the form flash shows (`e.to_string()` — exact parity with
/// `build_submit`). Rule denials are 409; unknown targets 404; backend failures 500.
fn build_error(e: eperica_application::BuildError) -> ApiError {
    use eperica_application::BuildError as E;
    let (status, code) = match &e {
        E::Insufficient => (StatusCode::CONFLICT, "insufficient"),
        E::AlreadyBuilding => (StatusCode::CONFLICT, "lane_busy"),
        E::MaxLevel => (StatusCode::CONFLICT, "max_level"),
        E::PrereqUnmet => (StatusCode::CONFLICT, "prereq_unmet"),
        E::Exclusive => (StatusCode::CONFLICT, "exclusive"),
        E::Placement => (StatusCode::CONFLICT, "placement"),
        E::NotDemolishable => (StatusCode::CONFLICT, "not_demolishable"),
        E::MainBuildingTooLow => (StatusCode::CONFLICT, "main_building_too_low"),
        E::NotFound => (StatusCode::NOT_FOUND, "not_found"),
        E::Conflict => (StatusCode::CONFLICT, "conflict"),
        E::Backend(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    };
    ApiError::new(status, code, e.to_string())
}

/// Map a [`TrainError`] — same contract as [`build_error`], parity with `train_submit`'s flash.
fn train_error(e: eperica_application::TrainError) -> ApiError {
    use eperica_application::TrainError as E;
    let (status, code) = match &e {
        E::Insufficient => (StatusCode::CONFLICT, "insufficient"),
        E::QueueBusy => (StatusCode::CONFLICT, "lane_busy"),
        E::NotResearched => (StatusCode::CONFLICT, "not_researched"),
        E::BuildingMissing => (StatusCode::CONFLICT, "building_missing"),
        E::BuildingUnavailable => (StatusCode::CONFLICT, "building_unavailable"),
        E::CountOutOfRange => (StatusCode::BAD_REQUEST, "count_out_of_range"),
        E::NotFound => (StatusCode::NOT_FOUND, "not_found"),
        E::Conflict => (StatusCode::CONFLICT, "conflict"),
        E::Backend(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    };
    ApiError::new(status, code, e.to_string())
}

/// Map a [`CombatError`] to the API error contract (plan Decision #6): stable machine code + the
/// same player-visible message the form flash shows (`e.to_string()`). Rule denials 409 or 400;
/// unknown targets 404; backend failures 500.
fn combat_error(e: CombatError) -> ApiError {
    let (status, code) = match &e {
        CombatError::Insufficient => (StatusCode::CONFLICT, "insufficient"),
        CombatError::EmptyComposition => (StatusCode::BAD_REQUEST, "empty_composition"),
        CombatError::NoTargetThere => (StatusCode::NOT_FOUND, "no_target"),
        CombatError::SameTile => (StatusCode::BAD_REQUEST, "same_tile"),
        CombatError::TargetProtected => (StatusCode::CONFLICT, "target_protected"),
        CombatError::InvalidCatapultTarget => (StatusCode::BAD_REQUEST, "invalid_catapult_target"),
        CombatError::NotFound => (StatusCode::NOT_FOUND, "not_found"),
        CombatError::Backend(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    };
    ApiError::new(status, code, e.to_string())
}

/// Map a [`ScoutError`] — same contract as [`combat_error`], plus `not_all_scouts`.
fn scout_error(e: ScoutError) -> ApiError {
    let (status, code) = match &e {
        ScoutError::Insufficient => (StatusCode::CONFLICT, "insufficient"),
        ScoutError::EmptyComposition => (StatusCode::BAD_REQUEST, "empty_composition"),
        ScoutError::NotAllScouts => (StatusCode::BAD_REQUEST, "not_all_scouts"),
        ScoutError::NoTargetThere => (StatusCode::NOT_FOUND, "no_target"),
        ScoutError::SameTile => (StatusCode::BAD_REQUEST, "same_tile"),
        ScoutError::NotFound => (StatusCode::NOT_FOUND, "not_found"),
        ScoutError::Backend(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    };
    ApiError::new(status, code, e.to_string())
}

/// Map a [`MovementError`] — same contract as [`combat_error`], plus `nothing_stationed`.
fn movement_error(e: MovementError) -> ApiError {
    let (status, code) = match &e {
        MovementError::Insufficient => (StatusCode::CONFLICT, "insufficient"),
        MovementError::EmptyComposition => (StatusCode::BAD_REQUEST, "empty_composition"),
        MovementError::NoTargetThere => (StatusCode::NOT_FOUND, "no_target"),
        MovementError::SameTile => (StatusCode::BAD_REQUEST, "same_tile"),
        MovementError::NothingStationed => (StatusCode::NOT_FOUND, "nothing_stationed"),
        MovementError::NotFound => (StatusCode::NOT_FOUND, "not_found"),
        MovementError::Backend(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    };
    ApiError::new(status, code, e.to_string())
}

/// Map a [`TradeError`] — stable machine code + the same player-visible message the form flash
/// shows (`e.to_string()`). Rule denials 409; unknown targets 404; backend failures 500.
fn trade_error(e: TradeError) -> ApiError {
    let (status, code) = match &e {
        TradeError::NoMarketplace => (StatusCode::CONFLICT, "no_marketplace"),
        TradeError::EmptyBundle => (StatusCode::BAD_REQUEST, "empty_bundle"),
        TradeError::Insufficient => (StatusCode::CONFLICT, "insufficient"),
        TradeError::NotEnoughMerchants => (StatusCode::CONFLICT, "not_enough_merchants"),
        TradeError::NoTargetThere => (StatusCode::NOT_FOUND, "no_target"),
        TradeError::SameTile => (StatusCode::BAD_REQUEST, "same_tile"),
        TradeError::NotFound => (StatusCode::NOT_FOUND, "not_found"),
        TradeError::Backend(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    };
    ApiError::new(status, code, e.to_string())
}

/// Map a [`SettleError`] — stable machine code + the same player-visible message (`e.to_string()`).
fn settle_error(e: SettleError) -> ApiError {
    let (status, code) = match &e {
        SettleError::Insufficient => (StatusCode::CONFLICT, "insufficient"),
        SettleError::NotSettlerGroup => (StatusCode::CONFLICT, "not_settler_group"),
        SettleError::NoSlot => (StatusCode::CONFLICT, "no_slot"),
        SettleError::NotFreeValley => (StatusCode::CONFLICT, "not_free_valley"),
        SettleError::NotFound => (StatusCode::NOT_FOUND, "not_found"),
        SettleError::Backend(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    };
    ApiError::new(status, code, e.to_string())
}

/// Map a [`ResearchError`] — stable machine code + the same player-visible message (`e.to_string()`).
fn research_error(e: ResearchError) -> ApiError {
    let (status, code) = match &e {
        ResearchError::Insufficient => (StatusCode::CONFLICT, "insufficient"),
        ResearchError::InProgress => (StatusCode::CONFLICT, "in_progress"),
        ResearchError::AlreadyResearched => (StatusCode::CONFLICT, "already_researched"),
        ResearchError::RequirementsUnmet => (StatusCode::CONFLICT, "requirements_unmet"),
        ResearchError::NotFound => (StatusCode::NOT_FOUND, "not_found"),
        ResearchError::Conflict => (StatusCode::CONFLICT, "conflict"),
        ResearchError::Backend(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    };
    ApiError::new(status, code, e.to_string())
}

/// Map an [`UpgradeError`] — stable machine code + the same player-visible message (`e.to_string()`).
fn upgrade_error(e: UpgradeError) -> ApiError {
    let (status, code) = match &e {
        UpgradeError::Insufficient => (StatusCode::CONFLICT, "insufficient"),
        UpgradeError::InProgress => (StatusCode::CONFLICT, "in_progress"),
        UpgradeError::NotResearched => (StatusCode::CONFLICT, "not_researched"),
        UpgradeError::NoSmithy => (StatusCode::CONFLICT, "no_smithy"),
        UpgradeError::MaxLevel => (StatusCode::CONFLICT, "max_level"),
        UpgradeError::SmithyLevelTooLow => (StatusCode::CONFLICT, "smithy_level_too_low"),
        UpgradeError::NotFound => (StatusCode::NOT_FOUND, "not_found"),
        UpgradeError::Conflict => (StatusCode::CONFLICT, "conflict"),
        UpgradeError::Backend(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    };
    ApiError::new(status, code, e.to_string())
}

/// Convert a JSON unit map (`{ "<unit_id>": count }`) into the `Vec<(UnitId, u32)>` that every
/// send use-case expects, filtering out zero counts (rally-handler precedent).
fn unit_bundle(map: std::collections::BTreeMap<String, u32>) -> Vec<(UnitId, u32)> {
    map.into_iter()
        .filter(|(_, n)| *n > 0)
        .map(|(k, n)| (UnitId(k), n))
        .collect()
}

/// Serialize an in-flight [`MovementView`] to the compact shape the send-action responses carry.
/// Lowercase wire label for a [`MovementKind`] — one source of truth for echoes, digest heads and
/// report reads.
fn movement_kind_str(k: MovementKind) -> &'static str {
    match k {
        MovementKind::Reinforce => "reinforce",
        MovementKind::Return => "return",
        MovementKind::Attack => "attack",
        MovementKind::Raid => "raid",
        MovementKind::Scout => "scout",
        MovementKind::OasisAttack => "oasis_attack",
        MovementKind::OasisReinforce => "oasis_reinforce",
        MovementKind::Settle => "settle",
    }
}

fn movement_json(m: &eperica_application::MovementView) -> serde_json::Value {
    let kind = movement_kind_str(m.kind);
    let troops: std::collections::BTreeMap<String, u32> = m
        .troops
        .iter()
        .map(|(u, c)| (u.as_str().to_owned(), *c))
        .collect();
    serde_json::json!({
        "kind": kind,
        "dest_x": m.destination.x,
        "dest_y": m.destination.y,
        "arrive_at_ms": m.arrive_at.0,
        "troops": troops,
    })
}

/// Unwrap an `axum::Json` body, converting a malformed-JSON rejection into the API error shape
/// (plan §Key risks — never axum's plain-text default).
fn json_body<T>(
    body: Result<Json<T>, axum::extract::rejection::JsonRejection>,
) -> Result<T, ApiError> {
    body.map(|Json(v)| v)
        .map_err(|r| ApiError::new(StatusCode::BAD_REQUEST, "invalid_json", r.body_text()))
}

#[derive(serde::Deserialize)]
struct BuildBody {
    /// `"field"` or `"building"`.
    target: String,
    slot: u8,
    /// Building kind id (required when `target == "building"`).
    #[serde(default)]
    kind: Option<String>,
}

/// `POST /api/w/{world}/village/{village}/build` → `order_build` (AC4): the same call the form
/// handler makes — affordability, lanes, prerequisites, placement and ownership are all the
/// use-case's. Success returns the created queue entry with its completion time.
async fn build_action(
    AgentGame(ctx): AgentGame,
    axum::extract::Path((_world, village)): axum::extract::Path<(String, String)>,
    body: Result<Json<BuildBody>, axum::extract::rejection::JsonRejection>,
) -> Result<Response, ApiError> {
    use eperica_application::order_build;
    let body = json_body(body)?;
    let target = match body.target.as_str() {
        "field" => eperica_domain::BuildTarget::Field { slot: body.slot },
        "building" => match crate::handlers::parse_building_kind(body.kind.as_deref()) {
            Some(kind) => eperica_domain::BuildTarget::Building {
                slot: body.slot,
                kind,
            },
            None => {
                return Err(ApiError::new(
                    StatusCode::BAD_REQUEST,
                    "invalid_kind",
                    "Unknown or missing building kind.",
                ));
            }
        },
        _ => {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "invalid_target",
                "target must be \"field\" or \"building\".",
            ));
        }
    };
    let vid = owned_village(&ctx, &village).await?;
    order_build(
        &ctx.accounts,
        &ctx.accounts,
        &ctx.accounts,
        &ctx.rules.economy,
        &ctx.rules.build,
        &ctx.rules.units,
        ctx.speed,
        now(),
        ctx.player,
        Some(vid),
        target,
    )
    .await
    .map_err(build_error)?;
    // Read the created entry back through the digest's own read model (page truth, AC4).
    let entry = ctx
        .accounts
        .active_builds(vid)
        .await
        .unwrap_or_else(|e| {
            // The order committed — success stands; the missing echo is only a read-back glitch.
            tracing::warn!(error = %e, "post-order queue read-back failed");
            Vec::new()
        })
        .into_iter()
        .find(|b| b.target == target);
    Ok(Json(serde_json::json!({
        "ordered": true,
        "village": crate::handlers::village_seg(vid),
        "queue_entry": entry.map(|b| serde_json::json!({
            "level": b.target_level,
            "completes_at_ms": b.complete_at.0,
        })),
    }))
    .into_response())
}

/// The **strict** village resolution for agent actions (review M3): the path village must parse and
/// be owned by the acting player, else `404 not_found`. A machine client gets no silent
/// capital-fallback (the browser flow's convenience) — an agent must never have its order land on a
/// different village than it addressed.
async fn owned_village(
    ctx: &auth::GameContext,
    village: &str,
) -> Result<eperica_domain::VillageId, ApiError> {
    let not_found = || ApiError::new(StatusCode::NOT_FOUND, "not_found", "No such village.");
    let vid = crate::handlers::selected_village(Some(village)).ok_or_else(not_found)?;
    let owned = ctx.accounts.villages_of(ctx.player).await.map_err(|e| {
        tracing::error!(error = %e, "village ownership read failed");
        ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", "ownership")
    })?;
    if !owned.iter().any(|v| v.id == vid) {
        return Err(not_found());
    }
    Ok(vid)
}

#[derive(serde::Deserialize)]
struct TrainBody {
    unit: String,
    count: u32,
}

/// `POST /api/w/{world}/village/{village}/train` → `order_train` (AC4). Success returns the batch
/// with its next completion time.
async fn train_action(
    AgentGame(ctx): AgentGame,
    axum::extract::Path((_world, village)): axum::extract::Path<(String, String)>,
    body: Result<Json<TrainBody>, axum::extract::rejection::JsonRejection>,
) -> Result<Response, ApiError> {
    use eperica_application::order_train;
    let body = json_body(body)?;
    let unit = eperica_domain::UnitId(body.unit);
    let vid = owned_village(&ctx, &village).await?;
    order_train(
        &ctx.accounts,
        &ctx.accounts,
        &ctx.accounts,
        &ctx.accounts,
        &ctx.rules.economy,
        &ctx.rules.units,
        ctx.speed,
        now(),
        ctx.player,
        Some(vid),
        unit.clone(),
        body.count,
    )
    .await
    .map_err(train_error)?;
    // Read the batch back through the digest's own read model (page truth, AC4).
    let batch = ctx
        .accounts
        .active_training(vid)
        .await
        .unwrap_or_else(|e| {
            // The order committed — success stands; the missing echo is only a read-back glitch.
            tracing::warn!(error = %e, "post-train batch read-back failed");
            Vec::new()
        })
        .into_iter()
        .find(|t| t.unit == unit);
    Ok(Json(serde_json::json!({
        "ordered": true,
        "village": crate::handlers::village_seg(vid),
        "batch": batch.map(|t| serde_json::json!({
            "unit": t.unit.as_str(),
            "remaining": t.count_total.saturating_sub(t.count_done),
            "next_complete_at_ms": t.next_complete_at.0,
        })),
    }))
    .into_response())
}

// ---------------------------------------------------------------------------
// Military actions (119 AC1) — thin JSON adapters onto the existing use-cases (P4).
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
struct AttackBody {
    x: i32,
    y: i32,
    units: std::collections::BTreeMap<String, u32>,
    mode: String,
    #[serde(default)]
    catapult_target: Option<String>,
}

/// `POST /api/w/{world}/village/{village}/attack` → `order_attack` (119 AC1).
///
/// Strict village addressing: the path village must be owned by the agent (M3). Mode must be
/// `"attack"` or `"raid"`; anything else → 400 `invalid_mode`. Success returns the created
/// movement read back through `active_movements` (page truth); if the read-back glitches the
/// movement is `null` (the order committed — success stands).
async fn attack_action(
    AgentGame(ctx): AgentGame,
    axum::extract::Path((_world, village)): axum::extract::Path<(String, String)>,
    body: Result<Json<AttackBody>, axum::extract::rejection::JsonRejection>,
) -> Result<Response, ApiError> {
    let body = json_body(body)?;
    let mode = match body.mode.as_str() {
        "attack" => AttackMode::Attack,
        "raid" => AttackMode::Raid,
        _ => {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "invalid_mode",
                "mode must be \"attack\" or \"raid\".",
            ));
        }
    };
    let ordered_kind = match mode {
        AttackMode::Attack => MovementKind::Attack,
        AttackMode::Raid => MovementKind::Raid,
    };
    // A machine contract rejects a typo'd catapult target outright (unlike the browser's <select>,
    // which can't produce one) — the build endpoint's invalid_kind precedent.
    let catapult_target = match body.catapult_target.as_deref() {
        None => None,
        Some(s) => Some(
            crate::handlers::parse_building_kind(Some(s)).ok_or_else(|| {
                ApiError::new(
                    StatusCode::BAD_REQUEST,
                    "invalid_catapult_target",
                    "Unknown catapult target building.",
                )
            })?,
        ),
    };
    let troops = unit_bundle(body.units);
    let vid = owned_village(&ctx, &village).await?;
    let target = Coordinate::new(body.x, body.y);
    order_attack(
        &ctx.accounts,
        &ctx.accounts,
        &ctx.accounts,
        &ctx.accounts,
        &ctx.rules.economy,
        &ctx.rules.units,
        ctx.map.as_ref(),
        ctx.speed,
        now(),
        ctx.player,
        Some(vid),
        target,
        troops,
        mode,
        None, // scout_target — agent uses /scout for standalone scouting
        catapult_target,
    )
    .await
    .map_err(combat_error)?;
    // Read back the most-recently scheduled movement to this target (page truth, AC1).
    let movement = ctx
        .accounts
        .active_movements(ctx.player)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "post-attack movement read-back failed");
            Vec::new()
        })
        .into_iter()
        .filter(|m| m.destination == target && m.kind == ordered_kind)
        .max_by_key(|m| m.arrive_at.0);
    Ok(Json(serde_json::json!({
        "ordered": true,
        "movement": movement.as_ref().map(movement_json),
    }))
    .into_response())
}

#[derive(serde::Deserialize)]
struct ScoutBody {
    x: i32,
    y: i32,
    units: std::collections::BTreeMap<String, u32>,
    target: String,
}

/// `POST /api/w/{world}/village/{village}/scout` → `order_scout` (119 AC1).
///
/// `target` must be `"resources"` or `"defenses"` (010 slug); anything else → 400
/// `invalid_target`. Only Scout-role units are accepted; others → 400 `not_all_scouts`.
async fn scout_action(
    AgentGame(ctx): AgentGame,
    axum::extract::Path((_world, village)): axum::extract::Path<(String, String)>,
    body: Result<Json<ScoutBody>, axum::extract::rejection::JsonRejection>,
) -> Result<Response, ApiError> {
    let body = json_body(body)?;
    let scout_target = ScoutTarget::from_slug(&body.target).ok_or_else(|| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            "invalid_target",
            "target must be \"resources\" or \"defenses\".",
        )
    })?;
    let troops = unit_bundle(body.units);
    let vid = owned_village(&ctx, &village).await?;
    let target = Coordinate::new(body.x, body.y);
    order_scout(
        &ctx.accounts,
        &ctx.accounts,
        &ctx.accounts,
        &ctx.rules.economy,
        &ctx.rules.units,
        ctx.map.as_ref(),
        ctx.speed,
        now(),
        ctx.player,
        Some(vid),
        target,
        troops,
        scout_target,
    )
    .await
    .map_err(scout_error)?;
    let movement = ctx
        .accounts
        .active_movements(ctx.player)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "post-scout movement read-back failed");
            Vec::new()
        })
        .into_iter()
        .filter(|m| m.destination == target && m.kind == MovementKind::Scout)
        .max_by_key(|m| m.arrive_at.0);
    Ok(Json(serde_json::json!({
        "ordered": true,
        "movement": movement.as_ref().map(movement_json),
    }))
    .into_response())
}

#[derive(serde::Deserialize)]
struct ReinforceBody {
    x: i32,
    y: i32,
    units: std::collections::BTreeMap<String, u32>,
}

/// `POST /api/w/{world}/village/{village}/reinforce` → `order_reinforcement` (119 AC1).
async fn reinforce_action(
    AgentGame(ctx): AgentGame,
    axum::extract::Path((_world, village)): axum::extract::Path<(String, String)>,
    body: Result<Json<ReinforceBody>, axum::extract::rejection::JsonRejection>,
) -> Result<Response, ApiError> {
    let body = json_body(body)?;
    let troops = unit_bundle(body.units);
    let vid = owned_village(&ctx, &village).await?;
    let target = Coordinate::new(body.x, body.y);
    order_reinforcement(
        &ctx.accounts,
        &ctx.accounts,
        &ctx.accounts,
        &ctx.rules.economy,
        &ctx.rules.units,
        ctx.map.as_ref(),
        ctx.speed,
        now(),
        ctx.player,
        Some(vid),
        target,
        troops,
    )
    .await
    .map_err(movement_error)?;
    let movement = ctx
        .accounts
        .active_movements(ctx.player)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "post-reinforce movement read-back failed");
            Vec::new()
        })
        .into_iter()
        .filter(|m| m.destination == target && m.kind == MovementKind::Reinforce)
        .max_by_key(|m| m.arrive_at.0);
    Ok(Json(serde_json::json!({
        "ordered": true,
        "movement": movement.as_ref().map(movement_json),
    }))
    .into_response())
}

#[derive(serde::Deserialize)]
struct ReturnBody {
    host: String,
}

/// `POST /api/w/{world}/village/{village}/return` → `order_return` (119 AC2).
///
/// `host` is the hyphenated UUID of the village where the agent's troops are currently stationed.
/// Bad parse → 404 `not_found`. If no group is stationed there → 404 `nothing_stationed`.
async fn return_action(
    AgentGame(ctx): AgentGame,
    axum::extract::Path((_world, village)): axum::extract::Path<(String, String)>,
    body: Result<Json<ReturnBody>, axum::extract::rejection::JsonRejection>,
) -> Result<Response, ApiError> {
    let body = json_body(body)?;
    let host_vid = crate::handlers::selected_village(Some(&body.host)).ok_or_else(|| {
        ApiError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            "Invalid host village id.",
        )
    })?;
    // Strict addressing: verify the path village is owned, even though order_return keys on the
    // owner — this keeps the ownership check consistent with the other send actions (review M3).
    let _vid = owned_village(&ctx, &village).await?;
    order_return(
        &ctx.accounts,
        &ctx.accounts,
        &ctx.rules.units,
        ctx.map.as_ref(),
        ctx.speed,
        now(),
        ctx.player,
        host_vid,
    )
    .await
    .map_err(movement_error)?;
    // Read back the return movement: the most-recently scheduled Return (host coord → home).
    let movement = ctx
        .accounts
        .active_movements(ctx.player)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "post-return movement read-back failed");
            Vec::new()
        })
        .into_iter()
        .filter(|m| m.kind == MovementKind::Return)
        .max_by_key(|m| m.arrive_at.0);
    Ok(Json(serde_json::json!({
        "ordered": true,
        "movement": movement.as_ref().map(movement_json),
    }))
    .into_response())
}

// ---------------------------------------------------------------------------
// Trade & settle actions (119 AC3) — thin JSON adapters onto the existing use-cases (P4).
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
struct ResourceBundle {
    #[serde(default)]
    wood: i64,
    #[serde(default)]
    clay: i64,
    #[serde(default)]
    iron: i64,
    #[serde(default)]
    crop: i64,
}

#[derive(serde::Deserialize)]
struct TradeBody {
    x: i32,
    y: i32,
    give: ResourceBundle,
}

/// `POST /api/w/{world}/village/{village}/trade` → `order_trade` (119 AC3).
///
/// Negatives in the bundle are clamped to 0 (market_send precedent). Success returns the created
/// shipment echoed via `active_trades` (page truth). If the read-back glitches, `shipment` is
/// `null` — the order committed; success stands.
async fn trade_action(
    AgentGame(ctx): AgentGame,
    axum::extract::Path((_world, village)): axum::extract::Path<(String, String)>,
    body: Result<Json<TradeBody>, axum::extract::rejection::JsonRejection>,
) -> Result<Response, ApiError> {
    let body = json_body(body)?;
    // Clamp negatives to 0, matching market_send's `.filter(|n| *n > 0).unwrap_or(0)`.
    let bundle = ResourceAmounts {
        wood: body.give.wood.max(0),
        clay: body.give.clay.max(0),
        iron: body.give.iron.max(0),
        crop: body.give.crop.max(0),
    };
    let vid = owned_village(&ctx, &village).await?;
    let target = Coordinate::new(body.x, body.y);
    order_trade(
        &ctx.accounts,
        &ctx.accounts,
        &ctx.rules.economy,
        &ctx.rules.units,
        &ctx.rules.merchant,
        ctx.map.as_ref(),
        ctx.speed,
        now(),
        ctx.player,
        Some(vid),
        target,
        bundle,
    )
    .await
    .map_err(trade_error)?;
    // Read back the most-recently scheduled Deliver leg to the target (page truth, AC1).
    let shipment = ctx
        .accounts
        .active_trades(ctx.player)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "post-trade shipment read-back failed");
            Vec::new()
        })
        .into_iter()
        .filter(|t| t.destination == target && matches!(t.kind, TradeKind::Deliver))
        .max_by_key(|t| t.arrive_at.0);
    Ok(Json(serde_json::json!({
        "ordered": true,
        "shipment": shipment.as_ref().map(|t| serde_json::json!({
            "dest_x": t.destination.x,
            "dest_y": t.destination.y,
            "arrive_at_ms": t.arrive_at.0,
            "give": {
                "wood": t.bundle.wood,
                "clay": t.bundle.clay,
                "iron": t.bundle.iron,
                "crop": t.bundle.crop,
            },
        })),
    }))
    .into_response())
}

#[derive(serde::Deserialize)]
struct SettleBody {
    x: i32,
    y: i32,
}

/// `POST /api/w/{world}/village/{village}/settle` → `order_settle` (119 AC3).
///
/// Success returns the created settling movement echoed via `active_movements` (kind Settle).
/// If the read-back glitches, `movement` is `null` — the order committed; success stands.
async fn settle_action(
    AgentGame(ctx): AgentGame,
    axum::extract::Path((_world, village)): axum::extract::Path<(String, String)>,
    body: Result<Json<SettleBody>, axum::extract::rejection::JsonRejection>,
) -> Result<Response, ApiError> {
    let body = json_body(body)?;
    let vid = owned_village(&ctx, &village).await?;
    let target = Coordinate::new(body.x, body.y);
    order_settle(
        &ctx.accounts,
        &ctx.accounts,
        &ctx.accounts,
        &ctx.accounts,
        &ctx.rules.economy,
        &ctx.rules.units,
        &ctx.rules.culture,
        ctx.map.as_ref(),
        ctx.speed,
        now(),
        ctx.player,
        Some(vid),
        target,
    )
    .await
    .map_err(settle_error)?;
    // Read back the Settle movement to the target (page truth, AC1).
    let movement = ctx
        .accounts
        .active_movements(ctx.player)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "post-settle movement read-back failed");
            Vec::new()
        })
        .into_iter()
        .filter(|m| m.destination == target && m.kind == MovementKind::Settle)
        .max_by_key(|m| m.arrive_at.0);
    Ok(Json(serde_json::json!({
        "ordered": true,
        "movement": movement.as_ref().map(movement_json),
    }))
    .into_response())
}

// ---------------------------------------------------------------------------
// Research & smithy actions (119 AC4) — thin JSON adapters onto the existing use-cases (P4).
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
struct UnitBody {
    unit: String,
}

/// Serialize an [`ActiveUnitOrder`] to the compact shape the research/smithy responses carry.
fn unit_order_json(o: &eperica_application::ActiveUnitOrder) -> serde_json::Value {
    let kind = match o.kind {
        eperica_application::UnitOrderKind::Research => "research",
        eperica_application::UnitOrderKind::SmithyUpgrade => "smithy",
    };
    serde_json::json!({
        "kind": kind,
        "unit": o.unit.as_str(),
        "target_level": o.target_level,
        "complete_at_ms": o.complete_at.0,
    })
}

/// `POST /api/w/{world}/village/{village}/research` → `order_research` (119 AC4).
///
/// Success returns the created research order echoed via `active_unit_orders` (page truth). If the
/// read-back glitches, `order` is `null` — the order committed; success stands.
async fn research_action(
    AgentGame(ctx): AgentGame,
    axum::extract::Path((_world, village)): axum::extract::Path<(String, String)>,
    body: Result<Json<UnitBody>, axum::extract::rejection::JsonRejection>,
) -> Result<Response, ApiError> {
    let body = json_body(body)?;
    let unit = UnitId(body.unit);
    let vid = owned_village(&ctx, &village).await?;
    order_research(
        &ctx.accounts,
        &ctx.accounts,
        &ctx.accounts,
        &ctx.rules.economy,
        &ctx.rules.units,
        ctx.speed,
        now(),
        ctx.player,
        Some(vid),
        unit.clone(),
    )
    .await
    .map_err(research_error)?;
    // Read back the active research order (page truth, AC6).
    let order = ctx
        .accounts
        .active_unit_orders(vid)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "post-research order read-back failed");
            Vec::new()
        })
        .into_iter()
        .find(|o| o.unit == unit && matches!(o.kind, eperica_application::UnitOrderKind::Research));
    Ok(Json(serde_json::json!({
        "ordered": true,
        "order": order.as_ref().map(unit_order_json),
    }))
    .into_response())
}

/// `POST /api/w/{world}/village/{village}/smithy` → `order_smithy_upgrade` (119 AC4).
///
/// Success returns the created upgrade order echoed via `active_unit_orders` (page truth). If the
/// read-back glitches, `order` is `null` — the order committed; success stands.
async fn smithy_action(
    AgentGame(ctx): AgentGame,
    axum::extract::Path((_world, village)): axum::extract::Path<(String, String)>,
    body: Result<Json<UnitBody>, axum::extract::rejection::JsonRejection>,
) -> Result<Response, ApiError> {
    let body = json_body(body)?;
    let unit = UnitId(body.unit);
    let vid = owned_village(&ctx, &village).await?;
    order_smithy_upgrade(
        &ctx.accounts,
        &ctx.accounts,
        &ctx.accounts,
        &ctx.rules.economy,
        &ctx.rules.units,
        ctx.speed,
        now(),
        ctx.player,
        Some(vid),
        unit.clone(),
    )
    .await
    .map_err(upgrade_error)?;
    // Read back the active smithy-upgrade order (page truth, AC4).
    let order = ctx
        .accounts
        .active_unit_orders(vid)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "post-smithy order read-back failed");
            Vec::new()
        })
        .into_iter()
        .find(|o| {
            o.unit == unit && matches!(o.kind, eperica_application::UnitOrderKind::SmithyUpgrade)
        });
    Ok(Json(serde_json::json!({
        "ordered": true,
        "order": order.as_ref().map(unit_order_json),
    }))
    .into_response())
}

// ---------------------------------------------------------------------------
// Report detail endpoints (119 T4) — party-scoped reads (P4).
// ---------------------------------------------------------------------------

/// `GET /api/w/{world}/report/{id}` — full battle report, party-scoped by the port (P4).
///
/// `{id}` is the decimal `u128` string emitted by the digest's `reports[].id`. Parses as decimal
/// (matching `r.id.to_string()` in `state_digest`). The port's `report(id, player)` returns `None`
/// for non-parties — mapped to 404 `not_found` here (P4, no `forbidden` leak).
///
/// Both parties receive the identical full view; per-party differences (e.g. "which side I'm on")
/// are left to the caller. The report page itself applies no additional field-level redaction beyond
/// what the port pre-scopes: attacker and defender each see all forces, losses, loot, razed, and
/// loyalty data.
async fn report_get(
    AgentGame(ctx): AgentGame,
    axum::extract::Path((_world, id)): axum::extract::Path<(String, String)>,
) -> Result<Response, ApiError> {
    let internal = |what: &'static str| {
        move |e| {
            tracing::error!(error = %e, "report read failed: {what}");
            ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", what)
        }
    };
    let id: u128 = id
        .parse()
        .map_err(|_| ApiError::new(StatusCode::NOT_FOUND, "not_found", "Invalid report id."))?;
    let report = ctx
        .accounts
        .report(id, ctx.player)
        .await
        .map_err(internal("report"))?
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::NOT_FOUND,
                "not_found",
                "No such report or you are not a party to it.",
            )
        })?;
    let kind = movement_kind_str(report.kind);
    // Unit-count maps for forces/losses — deterministically ordered for stable JSON.
    let af: std::collections::BTreeMap<String, u32> = report
        .attacker_forces
        .iter()
        .map(|(u, c)| (u.as_str().to_owned(), *c))
        .collect();
    let al: std::collections::BTreeMap<String, u32> = report
        .attacker_losses
        .iter()
        .map(|(u, c)| (u.as_str().to_owned(), *c))
        .collect();
    let df: std::collections::BTreeMap<String, u32> = report
        .defender_forces
        .iter()
        .map(|(u, c)| (u.as_str().to_owned(), *c))
        .collect();
    let dl: std::collections::BTreeMap<String, u32> = report
        .defender_losses
        .iter()
        .map(|(u, c)| (u.as_str().to_owned(), *c))
        .collect();
    let razed = report.razed.map(|d| {
        serde_json::json!({
            "building": crate::handlers::building_kind_id(d.kind),
            "before": d.before,
            "after": d.after,
        })
    });
    Ok(Json(serde_json::json!({
        "id": report.id.to_string(),
        "occurred_at_ms": report.occurred_at.0,
        "kind": kind,
        "attacker_name": report.attacker_name,
        "attacker_coord": { "x": report.attacker_coord.x, "y": report.attacker_coord.y },
        "defender_name": report.defender_name,
        "defender_coord": { "x": report.defender_coord.x, "y": report.defender_coord.y },
        "attacker_won": report.attacker_won,
        "luck": report.luck,
        "morale": report.morale,
        "wall_before": report.wall_before,
        "wall_after": report.wall_after,
        "attacker_forces": af,
        "attacker_losses": al,
        "defender_forces": df,
        "defender_losses": dl,
        "scouted": report.scouted,
        "scout_target": report.scout_target.map(|t| t.as_str()),
        "loot": {
            "wood": report.loot.wood,
            "clay": report.loot.clay,
            "iron": report.loot.iron,
            "crop": report.loot.crop,
        },
        "razed": razed,
        "loyalty_before": report.loyalty_before,
        "loyalty_after": report.loyalty_after,
        "conquered": report.conquered,
    }))
    .into_response())
}

/// Serialize a [`ScoutIntel`] variant to a compact JSON value.
fn scout_intel_json(intel: &ScoutIntel) -> serde_json::Value {
    match intel {
        ScoutIntel::Resources(a) => serde_json::json!({
            "kind": "resources",
            "wood": a.wood,
            "clay": a.clay,
            "iron": a.iron,
            "crop": a.crop,
        }),
        ScoutIntel::Defenses { troops, wall_level } => {
            let troops_map: std::collections::BTreeMap<String, u32> = troops
                .iter()
                .map(|(u, c)| (u.as_str().to_owned(), *c))
                .collect();
            serde_json::json!({
                "kind": "defenses",
                "troops": troops_map,
                "wall_level": wall_level,
            })
        }
    }
}

/// `GET /api/w/{world}/scout-report/{id}` — full scout report, party-scoped by the port (P4).
///
/// `{id}` is the decimal `u128` string from the digest's `scout_reports[].id`. The port's
/// `scout_report(id, player)` applies redaction for a target viewer (strips intel + scouts_sent)
/// and returns `None` for non-parties — do NOT add extra field redaction here (010 rule: the port
/// pre-redacts).
async fn scout_report_get(
    AgentGame(ctx): AgentGame,
    axum::extract::Path((_world, id)): axum::extract::Path<(String, String)>,
) -> Result<Response, ApiError> {
    let internal = |what: &'static str| {
        move |e| {
            tracing::error!(error = %e, "scout report read failed: {what}");
            ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", what)
        }
    };
    let id: u128 = id.parse().map_err(|_| {
        ApiError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            "Invalid scout report id.",
        )
    })?;
    let r = ctx
        .accounts
        .scout_report(id, ctx.player)
        .await
        .map_err(internal("scout_report"))?
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::NOT_FOUND,
                "not_found",
                "No such scout report or you are not a party to it.",
            )
        })?;
    // Troops maps — scouts_sent is already empty for a target viewer (port pre-redacts, P4).
    let scouts_sent: std::collections::BTreeMap<String, u32> = r
        .scouts_sent
        .iter()
        .map(|(u, c)| (u.as_str().to_owned(), *c))
        .collect();
    let scouts_lost: std::collections::BTreeMap<String, u32> = r
        .scouts_lost
        .iter()
        .map(|(u, c)| (u.as_str().to_owned(), *c))
        .collect();
    Ok(Json(serde_json::json!({
        "id": r.id.to_string(),
        "occurred_at_ms": r.occurred_at.0,
        "scouter_name": r.scouter_name,
        "scouter_coord": { "x": r.scouter_coord.x, "y": r.scouter_coord.y },
        "target_name": r.target_name,
        "target_coord": { "x": r.target_coord.x, "y": r.target_coord.y },
        "target_type": r.target_type.as_str(),
        "scouts_sent": scouts_sent,
        "scouts_lost": scouts_lost,
        "detected": r.detected,
        "viewer_is_scouter": r.viewer_is_scouter,
        "intel": r.intel.as_ref().map(scout_intel_json),
    }))
    .into_response())
}

// ---------------------------------------------------------------------------
// Messages (119 AC6) — 024 DMs. Comms key by ACCOUNT id (users id, cross-world —
// 045/060): the sender is `ctx.account`, never `ctx.player`. Game actions
// elsewhere in this file key by `ctx.player`; do not mix the two.
// ---------------------------------------------------------------------------

/// Map a [`CommsError`] to the API contract (plan Decision #6).
fn comms_error(e: CommsError) -> ApiError {
    use CommsError as E;
    let (status, code) = match &e {
        E::Invalid => (StatusCode::BAD_REQUEST, "invalid"),
        E::SelfSend => (StatusCode::BAD_REQUEST, "self_send"),
        E::RecipientUnavailable => (StatusCode::NOT_FOUND, "recipient_unavailable"),
        E::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
        E::Backend(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    };
    ApiError::new(status, code, e.to_string())
}

#[derive(serde::Deserialize)]
struct MessageBody {
    /// Recipient **username** — resolved to the account id here (the browser links by id; a bot
    /// knows names from the map/boards).
    to: String,
    body: String,
}

/// `POST /api/w/{world}/message` → `send_dm` (AC6). Username → account id at the adapter; the
/// use-case owns body validation, self-send and abandoned-recipient rules (024, P4).
async fn message_send(
    AgentGame(ctx): AgentGame,
    body: Result<Json<MessageBody>, axum::extract::rejection::JsonRejection>,
) -> Result<Response, ApiError> {
    let body = json_body(body)?;
    let recipient = ctx
        .accounts
        .find_user_by_username(body.to.trim())
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "recipient lookup failed");
            ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", "recipient")
        })?
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::NOT_FOUND,
                "recipient_unavailable",
                "No such player.",
            )
        })?;
    let id = send_dm(
        &ctx.accounts,
        &ctx.accounts,
        ctx.account,
        recipient.id,
        &body.body,
        Timestamp(now().0),
    )
    .await
    .map_err(comms_error)?;
    Ok(Json(serde_json::json!({
        "sent": true,
        "message_id": id.to_string(),
        "to": recipient.id.0.to_string(),
    }))
    .into_response())
}

/// `GET /api/w/{world}/messages` → `conversation_list` (AC6): DM + channel summaries. DM entries
/// additionally carry the partner's decimal `account` id (derived from the `dm:<uuid>` key) so an
/// agent can follow up with `GET /api/w/{world}/messages/{account}` without uuid juggling.
async fn messages_list(AgentGame(ctx): AgentGame) -> Result<Response, ApiError> {
    let summaries = conversation_list(&ctx.accounts, &ctx.accounts, ctx.account, ctx.player)
        .await
        .map_err(comms_error)?;
    let rows: Vec<serde_json::Value> = summaries
        .into_iter()
        .map(|c| {
            let account = parse_dm_key(&c.key).map(|p| p.0.to_string());
            serde_json::json!({
                "key": c.key,
                "account": account,
                "title": c.title,
                "last_body": c.last_body,
                "last_ms": c.last_ms,
                "unread": c.unread,
            })
        })
        .collect();
    Ok(Json(serde_json::json!({ "conversations": rows })).into_response())
}

/// `GET /api/w/{world}/messages/{account}` → `open_dm` (AC6): the DM history with that account
/// (newest last), marking it read — the page's own semantics.
async fn messages_with(
    AgentGame(ctx): AgentGame,
    axum::extract::Path((_world, account)): axum::extract::Path<(String, String)>,
) -> Result<Response, ApiError> {
    let other: u128 = account
        .trim()
        .parse()
        .map_err(|_| ApiError::new(StatusCode::NOT_FOUND, "not_found", "No such conversation."))?;
    // The partner must exist — otherwise open_dm's mark-read would write a read cursor for a
    // nonexistent conversation on every garbage id.
    if ctx
        .accounts
        .find_user_by_id(PlayerId(other))
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "dm partner lookup failed");
            ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", "partner")
        })?
        .is_none()
    {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            "No such conversation.",
        ));
    }
    let history = open_dm(
        &ctx.accounts,
        ctx.account,
        PlayerId(other),
        50,
        Timestamp(now().0),
    )
    .await
    .map_err(comms_error)?;
    let rows: Vec<serde_json::Value> = history
        .into_iter()
        .map(|m| {
            serde_json::json!({
                "id": m.id.to_string(),
                "sender": m.sender.0.to_string(),
                "sender_name": m.sender_name,
                "body": m.body,
                "created_ms": m.created_ms,
            })
        })
        .collect();
    Ok(Json(serde_json::json!({ "messages": rows })).into_response())
}

/// The `/api` router (nested by [`crate::router`]). Every route answers JSON; unknown `/api` paths
/// get a JSON 404 (never the HTML fallback).
pub fn router() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/me", axum::routing::get(me))
        .route("/w/{world}/state", axum::routing::get(state_digest))
        .route("/w/{world}/map", axum::routing::get(map_window))
        .route(
            "/w/{world}/village/{village}/build",
            axum::routing::post(build_action),
        )
        .route(
            "/w/{world}/village/{village}/train",
            axum::routing::post(train_action),
        )
        .route("/w/{world}/message", axum::routing::post(message_send))
        .route("/w/{world}/messages", axum::routing::get(messages_list))
        .route(
            "/w/{world}/messages/{account}",
            axum::routing::get(messages_with),
        )
        .route(
            "/w/{world}/village/{village}/attack",
            axum::routing::post(attack_action),
        )
        .route(
            "/w/{world}/village/{village}/scout",
            axum::routing::post(scout_action),
        )
        .route(
            "/w/{world}/village/{village}/reinforce",
            axum::routing::post(reinforce_action),
        )
        .route(
            "/w/{world}/village/{village}/return",
            axum::routing::post(return_action),
        )
        .route(
            "/w/{world}/village/{village}/trade",
            axum::routing::post(trade_action),
        )
        .route(
            "/w/{world}/village/{village}/settle",
            axum::routing::post(settle_action),
        )
        .route(
            "/w/{world}/village/{village}/research",
            axum::routing::post(research_action),
        )
        .route(
            "/w/{world}/village/{village}/smithy",
            axum::routing::post(smithy_action),
        )
        .route("/w/{world}/report/{id}", axum::routing::get(report_get))
        .route(
            "/w/{world}/scout-report/{id}",
            axum::routing::get(scout_report_get),
        )
        .fallback(|| async {
            ApiError::new(
                StatusCode::NOT_FOUND,
                "unknown_endpoint",
                "No such API endpoint.",
            )
        })
}
