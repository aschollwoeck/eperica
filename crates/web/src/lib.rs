//! Eperica web library — the HTTP router and handlers, exposed so integration tests can drive the
//! full stack. The binary (`main.rs`) wires configuration, persistence, and the scheduler around it.
#![forbid(unsafe_code)]

pub mod auth;
pub mod handlers;
pub mod state;
pub mod templates;

use auth::AUTH_COOKIE;
use axum::Router;
use axum::extract::{FromRequestParts, Request, State};
use axum::http::{Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum_extra::extract::PrivateCookieJar;
use eperica_domain::{PlayerId, Timestamp, account_blocked};
use eperica_infrastructure::now;
use state::AppState;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

/// Extract the authenticated player id from the request's encrypted cookie, if any (read-only — the
/// `AuthUser` extractor enforces presence on Player pages; here we only need to *know* who is acting).
async fn session_player(
    parts: &mut axum::http::request::Parts,
    state: &AppState,
) -> Option<PlayerId> {
    let jar: PrivateCookieJar = PrivateCookieJar::from_request_parts(parts, state)
        .await
        .ok()?;
    let id: u128 = jar.get(AUTH_COOKIE)?.value().parse().ok()?;
    Some(PlayerId(id))
}

/// Server-authoritative action guard (P4) on mutating `POST`s, except authentication (so a player can
/// always log in / out):
/// - **Round freeze** (021 AC7): once the world is won, every mutating action is rejected.
/// - **Sanction enforcement** (022 AC5): a banned or currently-suspended logged-in player's mutating
///   actions are rejected.
///
/// Reads (`GET`) always pass; enforcement lives here, never in the client.
async fn action_guard(State(state): State<AppState>, req: Request, next: Next) -> Response {
    use eperica_application::{AccountRepository, WonderRepository};
    let is_auth = matches!(req.uri().path(), "/login" | "/logout" | "/register");
    if req.method() != Method::POST || is_auth {
        return next.run(req).await;
    }

    // Round freeze (021).
    match state.accounts.world_ended().await {
        Ok(Some(_)) => {
            return (
                StatusCode::FORBIDDEN,
                "The round is over — the world has been won and is frozen.",
            )
                .into_response();
        }
        Ok(None) => {}
        Err(e) => tracing::error!(error = %e, "action guard failed to read world state"),
    }

    // Per-account sanction (022): reject a blocked logged-in player's mutating action.
    let (mut parts, body) = req.into_parts();
    if let Some(player) = session_player(&mut parts, &state).await {
        match state.accounts.find_user_by_id(player).await {
            Ok(Some(u)) if account_blocked(u.banned_at, u.suspended_until, Timestamp(now().0)) => {
                return (
                    StatusCode::FORBIDDEN,
                    "Your account is suspended or banned for a fair-play violation.",
                )
                    .into_response();
            }
            Ok(_) => {}
            Err(e) => tracing::error!(error = %e, "action guard failed to read account"),
        }
    }
    next.run(Request::from_parts(parts, body)).await
}

/// Build the application router for the given state.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(handlers::index))
        .route(
            "/register",
            get(handlers::register_form).post(handlers::register_submit),
        )
        .route(
            "/login",
            get(handlers::login_form).post(handlers::login_submit),
        )
        .route("/logout", post(handlers::logout))
        .route("/village", get(handlers::village))
        .route("/village/build", post(handlers::build_submit))
        .route("/map", get(handlers::map))
        .route("/village/academy", get(handlers::academy))
        .route("/village/academy/research", post(handlers::research_submit))
        .route("/village/smithy", get(handlers::smithy))
        .route(
            "/village/smithy/upgrade",
            post(handlers::smithy_upgrade_submit),
        )
        .route("/village/troops/{building}", get(handlers::troops))
        .route("/village/train", post(handlers::train_submit))
        .route("/village/rally", get(handlers::rally))
        .route("/village/rally/send", post(handlers::rally_send))
        .route("/village/rally/return", post(handlers::rally_return))
        .route("/village/oasis/recall", post(handlers::oasis_recall))
        .route("/village/market", get(handlers::market))
        .route("/village/market/send", post(handlers::market_send))
        .route("/alliance", get(handlers::alliance))
        .route("/alliance/found", post(handlers::alliance_found))
        .route("/alliance/invite", post(handlers::alliance_invite))
        .route("/alliance/revoke", post(handlers::alliance_revoke))
        .route("/alliance/respond", post(handlers::alliance_respond))
        .route("/alliance/leave", post(handlers::alliance_leave))
        .route("/alliance/expel", post(handlers::alliance_expel))
        .route("/alliance/role", post(handlers::alliance_role))
        .route("/alliance/transfer", post(handlers::alliance_transfer))
        .route("/alliance/disband", post(handlers::alliance_disband))
        .route("/alliance/diplomacy", post(handlers::alliance_diplomacy))
        .route("/quests", get(handlers::quests_page))
        .route("/reports", get(handlers::reports))
        .route("/reports/scout/{id}", get(handlers::scout_report_detail))
        .route("/reports/{id}", get(handlers::report_detail))
        .route("/leaderboard", get(handlers::leaderboard))
        .route("/wonder", get(handlers::wonder))
        .route("/wonder/build", post(handlers::wonder_build_submit))
        .route("/stats/player/{id}", get(handlers::player_stats_page))
        .route("/stats/alliance/{id}", get(handlers::alliance_stats_page))
        .route("/styleguide", get(handlers::styleguide))
        .nest_service("/static", ServeDir::new("crates/web/static"))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            action_guard,
        ))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
