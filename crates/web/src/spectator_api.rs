//! The Spectator API (125, T4) — the read-only JSON surface behind the `/spectate` dashboard.
//!
//! Mirrors the Agent API's shape (bearer auth, the `{ error, reason }` contract, a world-scoped
//! extractor) but authenticates a **different** credential kind (`spk_` spectator keys, never
//! `epk_` agent keys) and exposes **no mutating route at all** — read-only by construction, not by
//! permission check (P4; AC6). Every handler here is a thin JSON adapter over the exact same
//! [`eperica_application::spectate`] aggregation the dashboard (T3) renders, so the two surfaces can
//! never drift.
//!
//! Deliberately **no activity side effects** (AC6): unlike the agent API's 123 `touch_activity` call
//! on every request, spectating never calls `touch_activity` on anyone — not the spectator's own
//! account, and never a watched player's (these handlers never even resolve a watched player's
//! identity as an *actor*, only as data to read).

use axum::Json;
use axum::extract::{FromRequestParts, Path, Query, State};
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use eperica_application::{
    AccountRepository, PLAYERS_PER_PAGE, SpectatorRepository, players as spectate_player_index,
    village_detail, world_feed,
};
use eperica_domain::{BuildTarget, PlayerId, Timestamp, TradeKind, VillageId, account_blocked};
use eperica_infrastructure::now;

use crate::api::{ApiError, movement_kind_str};
use crate::state::AppState;
use crate::{apikey, auth};

/// The parsed `(id, secret)` of a request's `spk_` bearer token, or `None` when the header is
/// missing, non-`Bearer `, or not a valid **spectator** token — an `epk_` agent token parses to
/// `None` here (the T1 `apikey::parse_spectator` prefix guarantee, AC2). Shared with the spectator
/// rate guard (`crate::agent_rate_guard`) so authentication and budgeting can never disagree on what
/// counts as a token (the 118 M1 lesson, reapplied to this surface).
pub(crate) fn bearer_token(headers: &axum::http::HeaderMap) -> Option<(String, String)> {
    let token = headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")?;
    apikey::parse_spectator(token)
}

/// Resolve the `Authorization: Bearer spk_…` header to the bound **spectator account**: parse →
/// key lookup by id → constant-time secret verify → not revoked → account exists → **the account
/// holds the Spectator role right now** (plan §Key decisions — the role is re-checked at auth time,
/// not only at mint, so revoking it dead-ends every key that account holds instantly) → not
/// banned/suspended. Deliberately does **not** call `touch_activity` anywhere (AC6).
async fn bearer_spectator(parts: &Parts, state: &AppState) -> Result<PlayerId, ApiError> {
    let internal = |what: &'static str| {
        move |e| {
            tracing::error!(error = %e, "spectator bearer resolution failed: {what}");
            ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", what)
        }
    };
    let (id, secret) = bearer_token(&parts.headers).ok_or_else(ApiError::unauthorized)?;
    let key = state
        .accounts
        .find_spectator_key(&id)
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
    // The auth-time role re-check (plan §Key decisions): a key whose account lost the Spectator role
    // is refused exactly like an unknown key — no separate "role lost" error code (mirrors the agent
    // API's `is_ai` re-check, AC2).
    if !user.is_spectator {
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

/// Extractor: the bearer-authenticated spectator **account** (no world scope) — `/spectator/me`.
pub struct SpectatorAccount(pub PlayerId);

impl FromRequestParts<AppState> for SpectatorAccount {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(SpectatorAccount(bearer_spectator(parts, state).await?))
    }
}

/// Extractor: the bearer-authenticated spectator **in the selected world** — the read-only,
/// player-less twin of [`auth::WorldScope`] (there is no per-world "player" for a spectator; the
/// role itself is the only gate). World resolution mirrors the agent API's `AgentGame`: path uuid →
/// registry lookup → JSON 404 if the world is unknown or not running (no `NotJoined` case — a
/// spectator has standing on every world by construction, AC1).
pub struct SpectatorWorld {
    pub accounts: eperica_infrastructure::PgAccountRepository,
    pub rules: std::sync::Arc<eperica_infrastructure::WorldRules>,
    pub world_id: eperica_domain::WorldId,
    pub speed: eperica_domain::GameSpeed,
    /// Whether AI players are labeled as NPCs on this world (120 Decision #3) — the AC7 disguise gate.
    pub ai_labeled: bool,
}

fn unknown_world() -> ApiError {
    ApiError::new(
        StatusCode::NOT_FOUND,
        "unknown_world",
        "No such world in the path.",
    )
}

impl FromRequestParts<AppState> for SpectatorWorld {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        bearer_spectator(parts, state).await?;
        let world = auth::world_from_path(parts)
            .await
            .ok_or_else(unknown_world)?;
        let Some((accounts, _map, speed, _radius, rules, ai_labeled)) =
            state.world_registry.context_for(world).await
        else {
            return Err(unknown_world());
        };
        Ok(SpectatorWorld {
            accounts,
            rules,
            world_id: world,
            speed,
            ai_labeled,
        })
    }
}

/// `GET /spectator/me` — key introspection (AC2): the bound account and its role.
async fn me(
    State(state): State<AppState>,
    SpectatorAccount(account): SpectatorAccount,
) -> Result<Response, ApiError> {
    let user = state
        .accounts
        .find_user_by_id(account)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "/spectator/me read failed");
            ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", "account")
        })?
        .ok_or_else(ApiError::unauthorized)?;
    Ok(Json(serde_json::json!({
        "account": account.0.to_string(),
        "username": user.username,
        "is_spectator": user.is_spectator,
    }))
    .into_response())
}

/// Lowercase wire label for a [`TradeKind`] leg — the shipment analogue of [`movement_kind_str`].
fn trade_kind_str(kind: TradeKind) -> &'static str {
    match kind {
        TradeKind::Deliver => "deliver",
        TradeKind::Return => "return",
    }
}

/// A build/upgrade target as `(target, slot, kind)` for the wire — the world-feed twin of the agent
/// digest's `QueueEntry` (`crate::api`), rebuilt here since the feed's [`WorldBuildOrder`] rows carry
/// no `Village` to build the field's resource-kind label from (only the raw slot).
///
/// [`WorldBuildOrder`]: eperica_application::WorldBuildOrder
fn build_target_json(target: BuildTarget) -> (&'static str, u8, Option<&'static str>) {
    match target {
        BuildTarget::Field { slot } => ("field", slot, None),
        BuildTarget::Building { slot, kind } => (
            "building",
            slot,
            Some(crate::handlers::building_kind_id(kind)),
        ),
    }
}

/// `GET /spectator/w/{world}/feed` — the capped activity snapshot (AC4/AC5): movements (both
/// directions, full composition — never redacted, unlike a defender's own arrival-only view),
/// shipments, build orders, training batches, and recent reports, each capped and soonest-first.
/// Deadlines are absolute Unix-ms (agent-digest convention) — compute countdowns client-side.
async fn feed(world: SpectatorWorld) -> Result<Response, ApiError> {
    let internal = |what: &'static str| {
        move |e| {
            tracing::error!(error = %e, "spectator feed read failed: {what}");
            ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", what)
        }
    };
    let f = world_feed(&world.accounts)
        .await
        .map_err(internal("feed"))?;

    let movements: Vec<serde_json::Value> = f
        .movements
        .iter()
        .map(|m| {
            let troops: std::collections::BTreeMap<String, u32> = m
                .troops
                .iter()
                .map(|(u, c)| (u.as_str().to_owned(), *c))
                .collect();
            serde_json::json!({
                "id": m.id.to_string(),
                "kind": movement_kind_str(m.kind),
                "origin": {
                    "village": crate::handlers::village_seg(m.origin_village),
                    "x": m.origin_coord.x,
                    "y": m.origin_coord.y,
                    "owner": m.origin_owner,
                },
                "destination": {
                    "village": m.destination_village.map(crate::handlers::village_seg),
                    "x": m.destination_coord.x,
                    "y": m.destination_coord.y,
                    "owner": m.destination_owner,
                },
                "arrive_at_ms": m.arrive_at.0,
                "troops": troops,
            })
        })
        .collect();

    let shipments: Vec<serde_json::Value> = f
        .shipments
        .iter()
        .map(|s| {
            serde_json::json!({
                "id": s.id.to_string(),
                "kind": trade_kind_str(s.kind),
                "origin": {
                    "village": crate::handlers::village_seg(s.origin_village),
                    "x": s.origin_coord.x,
                    "y": s.origin_coord.y,
                    "owner": s.origin_owner,
                },
                "destination": {
                    "village": crate::handlers::village_seg(s.destination_village),
                    "x": s.destination_coord.x,
                    "y": s.destination_coord.y,
                    "owner": s.destination_owner,
                },
                "arrive_at_ms": s.arrive_at.0,
                "give": {
                    "wood": s.bundle.wood,
                    "clay": s.bundle.clay,
                    "iron": s.bundle.iron,
                    "crop": s.bundle.crop,
                },
                "merchants": s.merchants,
            })
        })
        .collect();

    let builds: Vec<serde_json::Value> = f
        .builds
        .iter()
        .map(|b| {
            let (target, slot, kind) = build_target_json(b.target);
            serde_json::json!({
                "village": crate::handlers::village_seg(b.village),
                "x": b.village_coord.x,
                "y": b.village_coord.y,
                "owner": b.owner,
                "target": target,
                "slot": slot,
                "kind": kind,
                "target_level": b.target_level,
                "completes_at_ms": b.complete_at.0,
            })
        })
        .collect();

    let trainings: Vec<serde_json::Value> = f
        .trainings
        .iter()
        .map(|t| {
            serde_json::json!({
                "village": crate::handlers::village_seg(t.village),
                "x": t.village_coord.x,
                "y": t.village_coord.y,
                "owner": t.owner,
                "unit": t.unit.as_str(),
                "remaining": t.remaining,
                "next_complete_at_ms": t.next_complete_at.0,
            })
        })
        .collect();

    let reports: Vec<serde_json::Value> = f
        .reports
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id.to_string(),
                "occurred_at_ms": r.occurred_at.0,
                "kind": movement_kind_str(r.kind),
                "attacker": {
                    "name": r.attacker_name,
                    "x": r.attacker_coord.x,
                    "y": r.attacker_coord.y,
                },
                "defender": {
                    "name": r.defender_name,
                    "x": r.defender_coord.x,
                    "y": r.defender_coord.y,
                },
                "outcome": r.outcome,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "world": crate::handlers::world_id_str(world.world_id),
        "now_ms": now().0,
        "movements": movements,
        "shipments": shipments,
        "builds": builds,
        "trainings": trainings,
        "reports": reports,
    }))
    .into_response())
}

/// Paging query for the spectator player index (AC5): `?page=` (1-based; missing/invalid ⇒ page 1).
#[derive(serde::Deserialize)]
struct PlayersQuery {
    #[serde(default)]
    page: Option<i64>,
}

/// `GET /spectator/w/{world}/players?page=` — the paged, population-descending player index (AC5/
/// AC7). `npc` is computed as `is_ai && world.ai_labeled` — the raw `is_ai` truth is read only to
/// compute this and is never itself serialized, on either a labeled or a disguised world (AC7).
async fn players(
    world: SpectatorWorld,
    Query(q): Query<PlayersQuery>,
) -> Result<Response, ApiError> {
    let page_no = q.page.unwrap_or(1).max(1);
    let rows = spectate_player_index(&world.accounts, &world.rules.economy, page_no)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "spectator players read failed");
            ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", "players")
        })?;
    let has_next = rows.len() as i64 == PLAYERS_PER_PAGE;
    let players: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "player": r.player.0.to_string(),
                "username": r.username,
                "tribe": r.tribe.map(eperica_domain::Tribe::slug),
                "population": r.population,
                "villages": r.village_count,
                "alliance_tag": r.alliance_tag,
                "npc": r.is_ai && world.ai_labeled,
            })
        })
        .collect();
    Ok(Json(serde_json::json!({
        "world": crate::handlers::world_id_str(world.world_id),
        "page": page_no,
        "has_next": has_next,
        "players": players,
    }))
    .into_response())
}

/// `GET /spectator/w/{world}/village/{id}` — the omniscient village detail (AC3): resources
/// (computed on read), fields/buildings, build queue with deadlines, training batches, garrison,
/// stationed reinforcements, loyalty and researched units — reusing the exact owner-view read-model
/// ([`village_detail`]) with the village's true owner substituted for the caller, so these values can
/// never drift from what the owner's own `/village` page (or the dashboard drill-down, T3) shows.
async fn village(
    world: SpectatorWorld,
    Path((_world, village_id)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let internal = |what: &'static str| {
        move |e| {
            tracing::error!(error = %e, "spectator village read failed: {what}");
            ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", what)
        }
    };
    let not_found = || ApiError::new(StatusCode::NOT_FOUND, "not_found", "No such village.");
    let village_uuid = uuid::Uuid::parse_str(village_id.trim()).map_err(|_| not_found())?;
    let village_id = VillageId(village_uuid.as_u128());
    let now_ts = Timestamp(now().0);

    let detail = village_detail(
        &world.accounts,
        &world.rules.economy,
        &world.rules.units,
        world.speed,
        now_ts,
        village_id,
    )
    .await
    .map_err(internal("village_detail"))?
    .ok_or_else(not_found)?;

    let owner = world
        .accounts
        .find_user_by_id(detail.economy.village.owner)
        .await
        .map_err(internal("owner lookup"))?
        .map(|u| u.username)
        .unwrap_or_else(|| "unknown".to_owned());

    let v = &detail.economy.village;
    let e = &detail.economy.economy;

    let fields: Vec<serde_json::Value> = v
        .fields
        .iter()
        .enumerate()
        .map(|(i, f)| {
            serde_json::json!({
                "slot": u8::try_from(i).unwrap_or(u8::MAX),
                "kind": crate::handlers::resource_slug(f.kind),
                "level": f.level,
            })
        })
        .collect();
    let buildings: Vec<serde_json::Value> = v
        .buildings
        .iter()
        .map(|b| {
            serde_json::json!({
                "slot": b.slot,
                "kind": crate::handlers::building_kind_id(b.kind),
                "level": b.level,
            })
        })
        .collect();
    let build_queue: Vec<serde_json::Value> = detail
        .builds
        .iter()
        .map(|b| {
            let (target, slot, kind) = build_target_json(b.target);
            serde_json::json!({
                "target": target,
                "slot": slot,
                "kind": kind,
                "level": b.target_level,
                "completes_at_ms": b.complete_at.0,
            })
        })
        .collect();
    let training: Vec<serde_json::Value> = detail
        .trainings
        .iter()
        .map(|t| {
            serde_json::json!({
                "unit": t.unit.as_str(),
                "remaining": t.count_total.saturating_sub(t.count_done),
                "next_complete_at_ms": t.next_complete_at.0,
            })
        })
        .collect();
    let garrison: Vec<serde_json::Value> = detail
        .economy
        .garrison
        .iter()
        .map(|(unit, count)| {
            serde_json::json!({
                "unit": unit.as_str(),
                "count": count,
            })
        })
        .collect();
    let reinforcements: Vec<serde_json::Value> = detail
        .reinforcements
        .iter()
        .map(|g| {
            let troops: std::collections::BTreeMap<String, u32> = g
                .troops
                .iter()
                .map(|(u, c)| (u.as_str().to_owned(), *c))
                .collect();
            serde_json::json!({
                "home_village": crate::handlers::village_seg(g.home_village),
                "x": g.other_coord.x,
                "y": g.other_coord.y,
                "owner": g.other_owner,
                "troops": troops,
            })
        })
        .collect();
    let loyalty = match detail.loyalty {
        Some((value, updated)) => eperica_domain::regenerate_loyalty(
            value,
            (now_ts.0 - updated.0) / 1000,
            &world.rules.loyalty,
            world.speed,
        ),
        None => world.rules.loyalty.starting_loyalty,
    };
    let researched: Vec<&str> = detail.researched.iter().map(|u| u.as_str()).collect();

    Ok(Json(serde_json::json!({
        "world": crate::handlers::world_id_str(world.world_id),
        "village": crate::handlers::village_seg(village_id),
        "owner": owner,
        "x": v.coordinate.x,
        "y": v.coordinate.y,
        "capital": v.is_capital,
        "tribe": v.tribe.map(eperica_domain::Tribe::slug),
        "resources": {
            "wood": { "amount": e.amounts.wood, "rate": e.rates.wood, "capacity": e.capacities.warehouse },
            "clay": { "amount": e.amounts.clay, "rate": e.rates.clay, "capacity": e.capacities.warehouse },
            "iron": { "amount": e.amounts.iron, "rate": e.rates.iron, "capacity": e.capacities.warehouse },
            "crop": { "amount": e.amounts.crop, "rate": e.rates.crop_net, "capacity": e.capacities.granary },
        },
        "fields": fields,
        "buildings": buildings,
        "build_queue": build_queue,
        "training": training,
        "garrison": garrison,
        "reinforcements": reinforcements,
        "loyalty": loyalty,
        "researched": researched,
    }))
    .into_response())
}

/// The `/spectator` router (nested by [`crate::router`]) — **GET-only, by construction** (AC6): no
/// mutating handler exists to register, so a `POST` to any path here either 405s (a registered path,
/// wrong method) or 404s via the fallback (an unregistered path) — never a write. Unknown paths get a
/// JSON 404, never the HTML fallback (mirrors `crate::api::router`).
pub fn router() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/me", axum::routing::get(me))
        .route("/w/{world}/feed", axum::routing::get(feed))
        .route("/w/{world}/players", axum::routing::get(players))
        .route("/w/{world}/village/{id}", axum::routing::get(village))
        .fallback(|| async {
            ApiError::new(
                StatusCode::NOT_FOUND,
                "unknown_endpoint",
                "No such spectator endpoint.",
            )
        })
}
