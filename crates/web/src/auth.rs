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

/// Name of the encrypted cookie holding the world the player has currently selected (043).
pub const WORLD_COOKIE: &str = "world";

/// Build the world-selection cookie for `world_id` (043) — set by the world switcher (045).
pub fn world_cookie(world_id: u128) -> Cookie<'static> {
    base_cookie(WORLD_COOKIE, world_id.to_string())
}

/// A removal cookie that clears the world selection (back to the home world).
pub fn clear_world_cookie() -> Cookie<'static> {
    base_cookie(WORLD_COOKIE, String::new())
}

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

/// The real logged-in human from the auth cookie, ignoring any sit (030) — `None` for a visitor.
fn real_player(jar: &PrivateCookieJar) -> Option<PlayerId> {
    Some(PlayerId(
        jar.get(AUTH_COOKIE)?.value().parse::<u128>().ok()?,
    ))
}

/// Extractor for an **optional real** player — the logged-in human, ignoring any sit (030); `None` for a
/// visitor. Never rejects. Used where an elevated capability must **not** be delegated through sitting —
/// e.g. the `/me` admin flag (so it matches the `RealUser`-gated admin console, 036).
pub struct MaybeRealUser(pub Option<PlayerId>);

impl FromRequestParts<AppState> for MaybeRealUser {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let real = match PrivateCookieJar::from_request_parts(parts, state).await {
            Ok(jar) => real_player(&jar),
            Err(_) => None,
        };
        Ok(MaybeRealUser(real))
    }
}

/// Extractor for an **optional** effective player — `Some` when logged in (the owner when sitting, else
/// the human), `None` for a visitor. Never rejects, so it suits best-effort, public-reachable endpoints
/// (e.g. the `/me` nav probe) that must answer for logged-out callers too.
pub struct MaybeAuthUser(pub Option<PlayerId>);

impl FromRequestParts<AppState> for MaybeAuthUser {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(MaybeAuthUser(effective_identity(parts, state).await.map(
            |(real, sitting_owner)| sitting_owner.unwrap_or(real),
        )))
    }
}

/// The **game** identity for a request (043): the selected world's repo/map/speed/radius + the account's
/// player in that world. Game handlers use this instead of the home `AppState` fields + `AuthUser`. The
/// selected world comes from the `world` cookie (default home); if the account has no player in it (or the
/// registry is not running it) it falls back to the **home** world (always joined). Resolves to the
/// effective account (sit-aware), so a sit transparently plays the owner's world. A missing/invalid auth
/// cookie redirects to `/login`.
pub struct GameContext {
    /// The selected world's account repository (world-scoped reads/writes).
    pub accounts: eperica_infrastructure::PgAccountRepository,
    /// The selected world's seeded map.
    pub map: std::sync::Arc<eperica_domain::WorldMap>,
    /// The account's player **in the selected world** (game state keys on this).
    pub player: PlayerId,
    /// The **account** (the human / effective user) — account-level reads (username, protection,
    /// activity) key on this. Equals `player` in the home world; differs in a second world.
    pub account: PlayerId,
    /// The selected world.
    pub world_id: eperica_domain::WorldId,
    /// The selected world's speed (P7) and radius (006).
    pub speed: eperica_domain::GameSpeed,
    pub radius: u32,
    /// The selected world's resolved rule bundle (050) — every per-world sim read keys on this preset.
    pub rules: std::sync::Arc<eperica_infrastructure::WorldRules>,
}

impl FromRequestParts<AppState> for GameContext {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        use eperica_infrastructure::application::AccountRepository;
        let Some((real, sitting_owner)) = effective_identity(parts, state).await else {
            return Err(Redirect::to("/login").into_response());
        };
        let account = sitting_owner.unwrap_or(real);

        // The selected world from the `world` cookie, defaulting to the home world.
        let jar: PrivateCookieJar = PrivateCookieJar::from_request_parts(parts, state)
            .await
            .map_err(|_| Redirect::to("/login").into_response())?;
        let selected = jar
            .get(WORLD_COOKIE)
            .and_then(|c| c.value().parse::<u128>().ok())
            .map(eperica_domain::WorldId)
            .unwrap_or(state.world_id);

        // Resolve the account's player in the selected world; fall back to home if not joined, or if the
        // registry is not running the selected world.
        let resolve = |world: eperica_domain::WorldId| async move {
            let player = state
                .accounts
                .player_in_world(account, world)
                .await
                .ok()??;
            let ctx = state.world_registry.context_for(world).await?;
            Some((world, player, ctx))
        };
        let (world_id, player, (accounts, map, speed, radius, rules)) =
            match resolve(selected).await {
                Some(found) => found,
                None => match resolve(state.world_id).await {
                    Some(home) => home,
                    // The account has no player even in the home world (should not happen for a real login).
                    None => return Err(Redirect::to("/login").into_response()),
                },
            };

        Ok(GameContext {
            accounts,
            map,
            player,
            account,
            world_id,
            speed,
            radius,
            rules,
        })
    }
}

/// A **player-less, login-less** world scope for the public read pages (046): the selected world's
/// repo/map/speed/radius, resolved from the `world` cookie (default = home), with no auth requirement — an
/// anonymous visitor sees the home world. Used by `leaderboard`/`wonder`/`search`/stat pages so a logged-in
/// player who selected a second world sees **that** world's boards, while public access is preserved.
pub struct WorldScope {
    pub accounts: eperica_infrastructure::PgAccountRepository,
    pub map: std::sync::Arc<eperica_domain::WorldMap>,
    pub world_id: eperica_domain::WorldId,
    pub speed: eperica_domain::GameSpeed,
    pub radius: u32,
    /// The selected world's resolved rule bundle (050) — the public read pages key on this preset.
    pub rules: std::sync::Arc<eperica_infrastructure::WorldRules>,
}

impl FromRequestParts<AppState> for WorldScope {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // The selected world from the `world` cookie, defaulting to home; no auth, never redirects.
        let jar: PrivateCookieJar = PrivateCookieJar::from_request_parts(parts, state)
            .await
            .map_err(|_| {
                (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    "internal error",
                )
                    .into_response()
            })?;
        let selected = jar
            .get(WORLD_COOKIE)
            .and_then(|c| c.value().parse::<u128>().ok())
            .map(eperica_domain::WorldId)
            .unwrap_or(state.world_id);
        // Build the selected world's context; fall back to home if it is not running.
        let (world_id, (accounts, map, speed, radius, rules)) =
            match state.world_registry.context_for(selected).await {
                Some(ctx) => (selected, ctx),
                None => match state.world_registry.context_for(state.world_id).await {
                    Some(home) => (state.world_id, home),
                    None => {
                        tracing::error!("world scope: home world has no context");
                        return Err((
                            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                            "internal error",
                        )
                            .into_response());
                    }
                },
            };
        Ok(WorldScope {
            accounts,
            map,
            world_id,
            speed,
            radius,
            rules,
        })
    }
}
