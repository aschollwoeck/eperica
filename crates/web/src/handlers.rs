//! HTTP handlers for the register / login / village flow.

use crate::auth::{AuthUser, auth_cookie, clear_cookie};
use crate::state::AppState;
use crate::templates::{IndexTemplate, LoginTemplate, RegisterTemplate, VillageTemplate};
use askama::Template;
use axum::Form;
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum_extra::extract::PrivateCookieJar;
use eperica_application::{
    AccountRepository, LoginError, RegisterCommand, RegisterError, authenticate, register,
};
use eperica_domain::ResourceKind;
use serde::Deserialize;

/// Render a template to an HTML response (or 500 on failure).
fn page<T: Template>(template: &T) -> Response {
    match template.render() {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "template render failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response()
        }
    }
}

fn server_error() -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response()
}

/// Public landing page (Visitor).
pub async fn index() -> Response {
    page(&IndexTemplate)
}

/// Registration form (Visitor).
pub async fn register_form() -> Response {
    page(&RegisterTemplate { error: None })
}

/// Login form (Visitor).
pub async fn login_form() -> Response {
    page(&LoginTemplate { error: None })
}

/// The bundled stylesheet (see specs/ui-style-guide.md).
pub async fn app_css() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        include_str!("../static/app.css"),
    )
}

/// Registration form fields.
#[derive(Deserialize)]
pub struct RegisterForm {
    username: String,
    email: String,
    password: String,
}

/// Handle registration (AC1, AC3). On success (no confirmation required) logs the user in.
pub async fn register_submit(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Form(form): Form<RegisterForm>,
) -> Response {
    let cmd = RegisterCommand {
        username: form.username,
        email: form.email,
        password: form.password,
    };
    match register(
        state.accounts.as_ref(),
        state.hasher.as_ref(),
        state.template.as_ref(),
        state.require_email_confirmation,
        cmd,
    )
    .await
    {
        Ok(_) if state.require_email_confirmation => page(&LoginTemplate {
            error: Some("Account created. Confirm your email, then log in.".to_owned()),
        }),
        Ok(user) => {
            let jar = jar.add(auth_cookie(user.id.0));
            (jar, Redirect::to("/village")).into_response()
        }
        Err(RegisterError::Taken) => page(&RegisterTemplate {
            error: Some("That username or email is already taken.".to_owned()),
        }),
        Err(RegisterError::WorldFull) => page(&RegisterTemplate {
            error: Some("The world is full — no free tile to settle.".to_owned()),
        }),
        Err(RegisterError::Backend(e)) => {
            tracing::error!(error = %e, "register failed");
            server_error()
        }
    }
}

/// Login form fields.
#[derive(Deserialize)]
pub struct LoginForm {
    username: String,
    password: String,
}

/// Handle login (AC2). Sets the auth cookie on success.
pub async fn login_submit(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Form(form): Form<LoginForm>,
) -> Response {
    match authenticate(
        state.accounts.as_ref(),
        state.hasher.as_ref(),
        &form.username,
        &form.password,
    )
    .await
    {
        Ok(user) => {
            let jar = jar.add(auth_cookie(user.id.0));
            (jar, Redirect::to("/village")).into_response()
        }
        Err(LoginError::InvalidCredentials) => page(&LoginTemplate {
            error: Some("Invalid username or password.".to_owned()),
        }),
        Err(LoginError::EmailNotConfirmed) => page(&LoginTemplate {
            error: Some("Please confirm your email before logging in.".to_owned()),
        }),
        Err(LoginError::Backend(e)) => {
            tracing::error!(error = %e, "login failed");
            server_error()
        }
    }
}

/// Log out: clear the auth cookie (Player) and return to the landing page.
pub async fn logout(jar: PrivateCookieJar) -> Response {
    let jar = jar.remove(clear_cookie());
    (jar, Redirect::to("/")).into_response()
}

/// The player's starting village (Player only — AC3/AC4).
pub async fn village(State(state): State<AppState>, AuthUser(player): AuthUser) -> Response {
    let user = match state.accounts.find_user_by_id(player).await {
        Ok(Some(u)) => u,
        Ok(None) => return Redirect::to("/login").into_response(),
        Err(e) => {
            tracing::error!(error = %e, "lookup user failed");
            return server_error();
        }
    };
    let villages = match state.accounts.villages_of(player).await {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = %e, "lookup villages failed");
            return server_error();
        }
    };
    let Some(v) = villages.into_iter().next() else {
        return server_error();
    };

    let count = |kind: ResourceKind| v.fields.iter().filter(|f| f.kind == kind).count();
    let buildings = v
        .buildings
        .iter()
        .map(|b| format!("{:?} (level {})", b.kind, b.level))
        .collect();

    page(&VillageTemplate {
        username: user.username,
        x: v.coordinate.x,
        y: v.coordinate.y,
        wood: count(ResourceKind::Wood),
        clay: count(ResourceKind::Clay),
        iron: count(ResourceKind::Iron),
        crop: count(ResourceKind::Crop),
        buildings,
    })
}
