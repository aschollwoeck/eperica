//! Eperica web library — the HTTP router and handlers, exposed so integration tests can drive the
//! full stack. The binary (`main.rs`) wires configuration, persistence, and the scheduler around it.
#![forbid(unsafe_code)]

pub mod auth;
pub mod handlers;
pub mod state;
pub mod templates;

use axum::Router;
use axum::extract::{Request, State};
use axum::http::{Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use state::AppState;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

/// Round-freeze guard (021 AC7): once the world has been won, the server rejects mutating game
/// actions (`POST`s) — except authentication, so players can still log in to see the result. Reads
/// stay fully available. Server-authoritative (P4): the freeze is enforced here, not in the client.
async fn freeze_guard(State(state): State<AppState>, req: Request, next: Next) -> Response {
    use eperica_application::WonderRepository;
    let is_auth = matches!(req.uri().path(), "/login" | "/logout" | "/register");
    if req.method() == Method::POST && !is_auth {
        match state.accounts.world_ended().await {
            Ok(Some(_)) => {
                return (
                    StatusCode::FORBIDDEN,
                    "The round is over — the world has been won and is frozen.",
                )
                    .into_response();
            }
            Ok(None) => {}
            Err(e) => tracing::error!(error = %e, "freeze guard failed to read world state"),
        }
    }
    next.run(req).await
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
        .route("/stats/player/{id}", get(handlers::player_stats_page))
        .route("/stats/alliance/{id}", get(handlers::alliance_stats_page))
        .route("/styleguide", get(handlers::styleguide))
        .nest_service("/static", ServeDir::new("crates/web/static"))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            freeze_guard,
        ))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
