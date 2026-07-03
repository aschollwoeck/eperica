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
    AccountRepository, AllianceRepository, BuildRepository, CombatRepository, OasisRepository,
    TrainingRepository,
};
use eperica_domain::{PlayerId, Timestamp, account_blocked};
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

/// Resolve the `Authorization: Bearer epk_…` header to the bound **AI account** (AC1):
/// parse → key lookup by id → constant-time secret verify → not revoked → account exists,
/// **is_ai**, and not banned/suspended. Sanction enforcement lives here because agents never pass
/// the login chokepoint (019/022) — a blocked AI account is refused on **every** request.
async fn bearer_account(parts: &Parts, state: &AppState) -> Result<PlayerId, ApiError> {
    let header = parts
        .headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(ApiError::unauthorized)?;
    let token = header.strip_prefix("Bearer ").unwrap_or(header);
    let (id, secret) = apikey::parse(token).ok_or_else(ApiError::unauthorized)?;
    let key = state
        .accounts
        .find_agent_key(&id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "agent key lookup failed");
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                "Key lookup failed.",
            )
        })?
        .ok_or_else(ApiError::unauthorized)?;
    if key.revoked || !apikey::verify(&secret, &key.secret_hash) {
        return Err(ApiError::unauthorized());
    }
    let user = state
        .accounts
        .find_user_by_id(key.user)
        .await
        .ok()
        .flatten()
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
    let username = state
        .accounts
        .find_user_by_id(account)
        .await
        .ok()
        .flatten()
        .map(|u| u.username);
    let worlds: Vec<serde_json::Value> = state
        .accounts
        .worlds_of_user(account)
        .await
        .unwrap_or_default()
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
        "username": username,
        "is_ai": true,
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
    // Latest report heads (ids + occurrence); the full report read arrives with 119.
    let reports: Vec<serde_json::Value> = ctx
        .accounts
        .reports_for(ctx.player, 10)
        .await
        .map_err(internal("reports"))?
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id.to_string(),
                "occurred_at_ms": r.occurred_at.0,
                "attacker_won": r.attacker_won,
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
    );
    Ok(Json(serde_json::json!({
        "center_x": center.x,
        "center_y": center.y,
        "r": r,
        "rows": rows,
    }))
    .into_response())
}

/// The `/api` router (nested by [`crate::router`]). Every route answers JSON; unknown `/api` paths
/// get a JSON 404 (never the HTML fallback).
pub fn router() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/me", axum::routing::get(me))
        .route("/w/{world}/state", axum::routing::get(state_digest))
        .route("/w/{world}/map", axum::routing::get(map_window))
        .fallback(|| async {
            ApiError::new(
                StatusCode::NOT_FOUND,
                "unknown_endpoint",
                "No such API endpoint.",
            )
        })
}
