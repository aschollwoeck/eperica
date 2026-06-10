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
        .route("/styleguide", get(handlers::styleguide))
        .nest_service("/static", ServeDir::new("crates/web/static"))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
