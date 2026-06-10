//! Authentication — the encrypted auth cookie and the `AuthUser` extractor (P4: server-enforced).

use crate::state::AppState;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Redirect, Response};
use axum_extra::extract::PrivateCookieJar;
use axum_extra::extract::cookie::{Cookie, SameSite};
use eperica_domain::PlayerId;

/// Name of the encrypted cookie holding the authenticated player's id.
pub const AUTH_COOKIE: &str = "uid";

/// Build the auth cookie for `player_id` (encrypted by the `PrivateCookieJar`).
pub fn auth_cookie(player_id: u128) -> Cookie<'static> {
    Cookie::build((AUTH_COOKIE, player_id.to_string()))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .build()
}

/// A removal cookie that clears the auth cookie on logout.
pub fn clear_cookie() -> Cookie<'static> {
    Cookie::build((AUTH_COOKIE, ""))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .build()
}

/// Extractor for the authenticated player. A missing/invalid cookie redirects to `/login`
/// (Visitors cannot reach Player-only pages — roles.md, AC7).
pub struct AuthUser(pub PlayerId);

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let jar: PrivateCookieJar = PrivateCookieJar::from_request_parts(parts, state)
            .await
            .map_err(|_| Redirect::to("/login").into_response())?;

        let Some(cookie) = jar.get(AUTH_COOKIE) else {
            return Err(Redirect::to("/login").into_response());
        };
        let id: u128 = cookie
            .value()
            .parse()
            .map_err(|_| Redirect::to("/login").into_response())?;
        Ok(AuthUser(PlayerId(id)))
    }
}
