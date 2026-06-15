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

/// Presence-touch guard (025): on a logged-in request, refresh the player's `last_activity` (throttled in
/// the repo) so presence reflects real navigation. Excludes static assets and the background pollers
/// (the unread-badge poll + the SSE stream) so an idle open tab is not perpetually "online" — which also
/// preserves the 019 inactivity/abandonment signal that reads the same `last_activity`.
async fn presence_touch(State(state): State<AppState>, req: Request, next: Next) -> Response {
    use eperica_application::AccountRepository;
    let path = req.uri().path();
    let background = path.starts_with("/static")
        || path == "/messages/unread"
        || path.starts_with("/messages/stream")
        || path == "/notifications/unread"
        || path == "/notifications/stream"
        || path == "/sitting/status";
    if background {
        return next.run(req).await;
    }
    let (mut parts, body) = req.into_parts();
    // 030: touch the *effective* player — operating an account via a sit keeps the owner active.
    if let Some((real, sitting_owner)) = auth::effective_identity(&mut parts, &state).await
        && let Err(e) = state
            .accounts
            .touch_activity(sitting_owner.unwrap_or(real), Timestamp(now().0))
            .await
    {
        tracing::error!(error = %e, "presence touch failed");
    }
    next.run(Request::from_parts(parts, body)).await
}

/// The client IP for rate-limit/detection keying. Behind a trusted proxy (`trust_proxy`), prefer the
/// first `X-Forwarded-For` hop then `X-Real-IP`; otherwise those headers are **spoofable** and ignored,
/// and the `peer` socket address is used (022 — server-authoritative attribution, P4).
pub(crate) fn client_ip(headers: &axum::http::HeaderMap, peer: &str, trust_proxy: bool) -> String {
    if trust_proxy
        && let Some(ip) = headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.split(',').next())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .or_else(|| headers.get("x-real-ip").and_then(|v| v.to_str().ok()))
    {
        return ip.to_owned();
    }
    peer.to_owned()
}

/// The peer socket IP from the request's `ConnectInfo` extension (set by
/// `into_make_service_with_connect_info`), or `"unknown"` if absent.
fn peer_ip(parts: &axum::http::request::Parts) -> String {
    parts
        .extensions
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
        .map(|ci| ci.0.ip().to_string())
        .unwrap_or_else(|| "unknown".to_owned())
}

/// Server-authoritative rate-limit guard (022 AC6, P4/P5): each mutating `POST` is counted in a
/// DB-backed fixed window and rejected with **429** when over the configured limit. `/login` +
/// `/register` are keyed by **IP** (brute-force / signup-spam); other actions by the **session player**.
/// `/logout` and reads are never limited.
async fn rate_limit_guard(State(state): State<AppState>, req: Request, next: Next) -> Response {
    use eperica_application::{ModerationError, check_rate_limit};
    let path = req.uri().path();
    if req.method() != Method::POST || path == "/logout" {
        return next.run(req).await;
    }
    let rules = state.fair_play_rules.clone();
    let by_ip = matches!(path, "/login" | "/register");
    let (mut parts, body) = req.into_parts();
    let (subject, action, limit) = if by_ip {
        let ip = client_ip(&parts.headers, &peer_ip(&parts), state.trust_proxy);
        (ip, "login", rules.login_limit_per_window)
    } else {
        match session_player(&mut parts, &state).await {
            Some(p) => (p.0.to_string(), "action", rules.rate_limit_per_window),
            // No session ⇒ nothing to key on; the page's own auth will redirect.
            None => return next.run(Request::from_parts(parts, body)).await,
        }
    };
    match check_rate_limit(
        state.accounts.as_ref(),
        &rules,
        &subject,
        action,
        limit,
        Timestamp(now().0),
    )
    .await
    {
        Err(ModerationError::RateLimited) => (
            StatusCode::TOO_MANY_REQUESTS,
            "Too many requests — slow down.",
        )
            .into_response(),
        Ok(()) => next.run(Request::from_parts(parts, body)).await,
        // Fail-open on a backend error: a counter glitch must not lock players out.
        Err(e) => {
            tracing::error!(error = %e, "rate-limit check failed");
            next.run(Request::from_parts(parts, body)).await
        }
    }
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

    // Per-account sanction (022): reject when either the real actor **or** the effective player (030 — the
    // owner being sat) is banned/suspended. A banned sitter cannot act; a sanctioned owner cannot be
    // operated via a sit.
    let (mut parts, body) = req.into_parts();
    if let Some((real, sitting_owner)) = auth::effective_identity(&mut parts, &state).await {
        let now_ts = Timestamp(now().0);
        for who in std::iter::once(real).chain(sitting_owner) {
            match state.accounts.find_user_by_id(who).await {
                Ok(Some(u)) if account_blocked(u.banned_at, u.suspended_until, now_ts) => {
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
    }
    next.run(Request::from_parts(parts, body)).await
}

/// Mutating actions a sitter may **not** take on the owner's account (030 AC4) — managing the owner's
/// sitters, changing the owner's settings/profile, or starting a nested sit.
fn sitter_restricted(path: &str) -> bool {
    matches!(
        path,
        "/settings/notifications"
            | "/profile/bio"
            | "/sitting/grant"
            | "/sitting/revoke"
            | "/sitting/start"
    )
}

/// Account-sitting guard (030): on a mutating `POST` made while actively sitting, **refuse** the
/// owner-only actions (AC4) and **audit** everything else (AC5). Runs after the freeze/sanction guard, so
/// only actions that would proceed are recorded.
async fn sitting_guard(State(state): State<AppState>, req: Request, next: Next) -> Response {
    if req.method() != Method::POST {
        return next.run(req).await;
    }
    let path = req.uri().path().to_owned();
    let (mut parts, body) = req.into_parts();
    if let Some((real, Some(owner))) = auth::effective_identity(&mut parts, &state).await {
        // Actively sitting `owner` as `real`.
        if sitter_restricted(&path) {
            return (
                StatusCode::FORBIDDEN,
                "A sitter cannot change the owner's account settings.",
            )
                .into_response();
        }
        // Audit the action (best-effort — a logging glitch must not block gameplay).
        if let Err(e) = eperica_application::record_sitter_action(
            state.accounts.as_ref(),
            owner,
            real,
            &format!("POST {path}"),
            Timestamp(now().0),
        )
        .await
        {
            tracing::error!(error = %e, "failed to record sitter action");
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
        .route("/alliance/forum", get(handlers::forum_page))
        .route("/alliance/forum/new", post(handlers::forum_new))
        .route("/alliance/forum/{id}", get(handlers::forum_thread_page))
        .route("/alliance/forum/{id}/reply", post(handlers::forum_reply))
        .route("/quests", get(handlers::quests_page))
        .route("/reports", get(handlers::reports))
        .route("/reports/scout/{id}", get(handlers::scout_report_detail))
        .route("/reports/{id}", get(handlers::report_detail))
        .route("/leaderboard", get(handlers::leaderboard))
        .route("/search", get(handlers::search_page))
        .route("/messages", get(handlers::messages))
        .route("/messages/unread", get(handlers::messages_unread))
        .route("/messages/send", post(handlers::messages_send))
        .route("/messages/with/{id}", get(handlers::messages_with))
        .route("/messages/c/{key}", get(handlers::conversation))
        .route("/messages/stream/{key}", get(handlers::messages_stream))
        .route("/notifications", get(handlers::notifications_page))
        .route("/notifications/unread", get(handlers::notifications_unread))
        .route("/notifications/read", post(handlers::notifications_read))
        .route("/notifications/stream", get(handlers::notifications_stream))
        .route("/wonder", get(handlers::wonder))
        .route("/wonder/build", post(handlers::wonder_build_submit))
        .route("/stats/player/{id}", get(handlers::player_stats_page))
        .route("/stats/alliance/{id}", get(handlers::alliance_stats_page))
        .route("/profile", get(handlers::profile_page))
        .route("/profile/bio", post(handlers::profile_bio_submit))
        .route("/settings", get(handlers::settings_page))
        .route(
            "/settings/notifications",
            post(handlers::settings_notifications_submit),
        )
        .route("/sitting", get(handlers::sitting_page))
        .route("/sitting/status", get(handlers::sitting_status))
        .route("/sitting/grant", post(handlers::sitting_grant))
        .route("/sitting/revoke", post(handlers::sitting_revoke))
        .route("/sitting/start", post(handlers::sitting_start))
        .route("/sitting/stop", post(handlers::sitting_stop))
        .route("/report", post(handlers::report_submit))
        .route("/mod", get(handlers::mod_queue))
        .route("/mod/account/{id}", get(handlers::mod_account))
        .route("/mod/resolve", post(handlers::mod_resolve_submit))
        .route("/mod/sanction", post(handlers::mod_sanction_submit))
        .route("/styleguide", get(handlers::styleguide))
        .nest_service("/static", ServeDir::new("crates/web/static"))
        // Innermost guard (runs just before the handler, after the freeze/sanction guard): restrict +
        // audit sitter actions (030).
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            sitting_guard,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            action_guard,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            rate_limit_guard,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            presence_touch,
        ))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
