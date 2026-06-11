//! Eperica web library — the HTTP router and handlers, exposed so integration tests can drive the
//! full stack. The binary (`main.rs`) wires configuration, persistence, and the scheduler around it.
#![forbid(unsafe_code)]

pub mod auth;
pub mod handlers;
pub mod state;
pub mod templates;

use axum::Router;
use axum::routing::{get, post};
use state::AppState;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

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
        .route("/village/market", get(handlers::market))
        .route("/village/market/send", post(handlers::market_send))
        .route("/styleguide", get(handlers::styleguide))
        .nest_service("/static", ServeDir::new("crates/web/static"))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
