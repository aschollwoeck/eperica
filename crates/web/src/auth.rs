//! Authentication — the encrypted auth cookie + the `AuthUser` / `RealUser` extractors (P4:
//! server-enforced). Account sitting (030) layers a second, optional "sit" cookie on top: while a sitter
//! is operating an owner's account, the **effective** player is the owner, while the **real** player stays
//! the human. `AuthUser` resolves the effective player (so every gameplay handler acts as the owner
//! transparently); `RealUser` is the human (for sitting management).

use crate::state::AppState;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Redirect, Response};
use axum_extra::extract::PrivateCookieJar;
use axum_extra::extract::cookie::{Cookie, SameSite};
use eperica_application::authorize_sit;
use eperica_domain::{PlayerId, Timestamp};
use eperica_infrastructure::now;

/// Name of the encrypted cookie holding the authenticated player's id.
pub const AUTH_COOKIE: &str = "uid";

/// Name of the encrypted cookie holding the owner a sitter is currently operating (030).
pub const SIT_COOKIE: &str = "sit";

/// Build the auth cookie for `player_id` (encrypted by the `PrivateCookieJar`).
pub fn auth_cookie(player_id: u128) -> Cookie<'static> {
    base_cookie(AUTH_COOKIE, player_id.to_string())
}

/// A removal cookie that clears the auth cookie on logout.
pub fn clear_cookie() -> Cookie<'static> {
    base_cookie(AUTH_COOKIE, String::new())
}

/// Build the sit cookie for `owner_id` — the account a sitter is operating (030).
pub fn sit_cookie(owner_id: u128) -> Cookie<'static> {
    base_cookie(SIT_COOKIE, owner_id.to_string())
}

/// A removal cookie that clears the sit cookie (stop sitting).
pub fn clear_sit_cookie() -> Cookie<'static> {
    base_cookie(SIT_COOKIE, String::new())
}

fn base_cookie(name: &'static str, value: String) -> Cookie<'static> {
    Cookie::build((name, value))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .build()
}

/// The logged-in human + the owner they are sitting for (if any, and still authorised). `None` when not
/// logged in. The owner is `Some` only when the sit cookie is set **and** the sit is currently authorised
/// (030 AC3 — re-checked every request, so a revoke ends the sit). Performs a DB authorisation check only
/// when the sit cookie is present.
pub(crate) async fn effective_identity(
    parts: &mut Parts,
    state: &AppState,
) -> Option<(PlayerId, Option<PlayerId>)> {
    let jar: PrivateCookieJar = PrivateCookieJar::from_request_parts(parts, state)
        .await
        .ok()?;
    let real = PlayerId(jar.get(AUTH_COOKIE)?.value().parse::<u128>().ok()?);
    let sitting_owner = match jar
        .get(SIT_COOKIE)
        .and_then(|c| c.value().parse::<u128>().ok())
    {
        Some(owner_id) => {
            let owner = PlayerId(owner_id);
            match authorize_sit(state.accounts.as_ref(), owner, real, Timestamp(now().0)).await {
                Ok(true) => Some(owner),
                _ => None,
            }
        }
        None => None,
    };
    Some((real, sitting_owner))
}

/// Extractor for the **effective** player — the owner when sitting (030), else the logged-in player. Every
/// gameplay handler uses this, so a sit transparently acts as the owner. A missing/invalid auth cookie
/// redirects to `/login` (Visitors cannot reach Player-only pages — roles.md, AC7).
pub struct AuthUser(pub PlayerId);

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        match effective_identity(parts, state).await {
            Some((real, sitting_owner)) => Ok(AuthUser(sitting_owner.unwrap_or(real))),
            None => Err(Redirect::to("/login").into_response()),
        }
    }
}

/// Extractor for the **real** logged-in human (ignores any sit cookie) — used by sitting management so a
/// sitter always acts on their own account there. A missing/invalid auth cookie redirects to `/login`.
pub struct RealUser(pub PlayerId);

impl FromRequestParts<AppState> for RealUser {
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
        Ok(RealUser(PlayerId(id)))
    }
}
