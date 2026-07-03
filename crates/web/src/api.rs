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
use eperica_application::AccountRepository;
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

/// The `/api` router (nested by [`crate::router`]). Every route answers JSON; unknown `/api` paths
/// get a JSON 404 (never the HTML fallback).
pub fn router() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/me", axum::routing::get(me))
        .fallback(|| async {
            ApiError::new(
                StatusCode::NOT_FOUND,
                "unknown_endpoint",
                "No such API endpoint.",
            )
        })
}
