//! HTTP handlers for the register / login / village flow.

use crate::auth::{AuthUser, auth_cookie, clear_cookie};
use crate::state::AppState;
use crate::templates::{
    ActiveView, BuildRow, IndexTemplate, LoginTemplate, RegisterTemplate, StyleGuideTemplate,
    VillageTemplate,
};
use askama::Template;
use axum::Form;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum_extra::extract::PrivateCookieJar;
use eperica_application::{
    AccountRepository, BuildRepository, LoginError, RegisterCommand, RegisterError, authenticate,
    load_economy, order_build, register,
};
use eperica_domain::{
    BuildTarget, BuildingKind, QueueLane, ResourceAmounts, ResourceKind, Tribe, Village,
    can_afford, queue_lane,
};
use eperica_infrastructure::now;
use serde::Deserialize;

fn resource_label(kind: ResourceKind) -> &'static str {
    match kind {
        ResourceKind::Wood => "Wood",
        ResourceKind::Clay => "Clay",
        ResourceKind::Iron => "Iron",
        ResourceKind::Crop => "Crop",
    }
}

fn tribe_label(tribe: Option<Tribe>) -> &'static str {
    match tribe {
        Some(Tribe::Romans) => "Romans",
        Some(Tribe::Teutons) => "Teutons",
        Some(Tribe::Gauls) => "Gauls",
        None => "—",
    }
}

fn building_label(kind: BuildingKind) -> &'static str {
    match kind {
        BuildingKind::MainBuilding => "Main Building",
        BuildingKind::RallyPoint => "Rally Point",
        BuildingKind::Warehouse => "Warehouse",
        BuildingKind::Granary => "Granary",
        BuildingKind::Barracks => "Barracks",
        BuildingKind::Academy => "Academy",
        BuildingKind::Smithy => "Smithy",
        BuildingKind::Stable => "Stable",
        BuildingKind::Workshop => "Workshop",
        BuildingKind::Residence => "Residence",
    }
}

fn building_kind_id(kind: BuildingKind) -> &'static str {
    match kind {
        BuildingKind::MainBuilding => "main_building",
        BuildingKind::RallyPoint => "rally_point",
        BuildingKind::Warehouse => "warehouse",
        BuildingKind::Granary => "granary",
        BuildingKind::Barracks => "barracks",
        BuildingKind::Academy => "academy",
        BuildingKind::Smithy => "smithy",
        BuildingKind::Stable => "stable",
        BuildingKind::Workshop => "workshop",
        BuildingKind::Residence => "residence",
    }
}

/// Fixed center slot per building kind (founding places Main Building/Rally Point at 0/1).
fn building_slot(kind: BuildingKind) -> u8 {
    match kind {
        BuildingKind::MainBuilding => 0,
        BuildingKind::RallyPoint => 1,
        BuildingKind::Warehouse => 2,
        BuildingKind::Granary => 3,
        BuildingKind::Barracks => 4,
        BuildingKind::Academy => 5,
        BuildingKind::Smithy => 6,
        BuildingKind::Stable => 7,
        BuildingKind::Workshop => 8,
        BuildingKind::Residence => 9,
    }
}

fn parse_building_kind(s: Option<&str>) -> Option<BuildingKind> {
    match s {
        Some("main_building") => Some(BuildingKind::MainBuilding),
        Some("rally_point") => Some(BuildingKind::RallyPoint),
        Some("warehouse") => Some(BuildingKind::Warehouse),
        Some("granary") => Some(BuildingKind::Granary),
        Some("barracks") => Some(BuildingKind::Barracks),
        Some("academy") => Some(BuildingKind::Academy),
        Some("smithy") => Some(BuildingKind::Smithy),
        _ => None,
    }
}

fn target_label(village: &Village, target: BuildTarget) -> String {
    match target {
        BuildTarget::Field { slot } => match village.fields.get(slot as usize) {
            Some(f) => format!("{} field #{slot}", resource_label(f.kind)),
            None => format!("field #{slot}"),
        },
        BuildTarget::Building { kind, .. } => building_label(kind).to_owned(),
    }
}

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

/// Living component gallery rendering the canonical theme (see specs/ui-style-guide.md).
pub async fn styleguide() -> Response {
    page(&StyleGuideTemplate)
}

/// Registration form fields.
#[derive(Deserialize)]
pub struct RegisterForm {
    username: String,
    email: String,
    password: String,
    /// Tribe slug; validated server-side (004 AC1, P4).
    #[serde(default)]
    tribe: String,
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
        tribe: form.tribe,
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
        Err(RegisterError::Invalid(message)) => page(&RegisterTemplate {
            error: Some(message),
        }),
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

/// The player's starting village with its live economy (Player only — AC3/AC4/AC7).
pub async fn village(State(state): State<AppState>, AuthUser(player): AuthUser) -> Response {
    let user = match state.accounts.find_user_by_id(player).await {
        Ok(Some(u)) => u,
        Ok(None) => return Redirect::to("/login").into_response(),
        Err(e) => {
            tracing::error!(error = %e, "lookup user failed");
            return server_error();
        }
    };

    let economy = match load_economy(
        state.accounts.as_ref(),
        state.rules.as_ref(),
        state.world.speed,
        now(),
        player,
    )
    .await
    {
        Ok(Some(e)) => e,
        Ok(None) => {
            tracing::error!(?player, "authenticated user has no village/economy");
            return server_error();
        }
        Err(e) => {
            tracing::error!(error = %e, "load economy failed");
            return server_error();
        }
    };

    let village = economy.village;
    let amounts = economy.economy.amounts;
    let rates = economy.economy.rates;
    let caps = economy.economy.capacities;

    let active = match state.accounts.active_builds(village.id).await {
        Ok(a) => a,
        Err(e) => {
            tracing::error!(error = %e, "active build lookup failed");
            return server_error();
        }
    };
    let build_rules = state.build_rules.as_ref();

    // A target is orderable only if its queue lane is free — Romans get a field and a building
    // lane, other tribes one shared lane (004 AC13). Server-side re-validation happens on POST.
    let tribe = village.tribe;
    let lane_of = |target: BuildTarget| tribe.map_or(QueueLane::All, |t| queue_lane(t, target));
    let lane_busy = |target: BuildTarget| {
        let lane = lane_of(target);
        active.iter().any(|a| lane_of(a.target) == lane)
    };

    let make_row = |table: &'static str,
                    slot: u8,
                    kind: &'static str,
                    label: String,
                    level: u8,
                    target: BuildTarget|
     -> BuildRow {
        let cost = build_rules.cost(target, level);
        let at_max = cost.is_none();
        let can_order = !lane_busy(target) && cost.is_some_and(|c| can_afford(amounts, c));
        let c = cost.unwrap_or(ResourceAmounts {
            wood: 0,
            clay: 0,
            iron: 0,
            crop: 0,
        });
        BuildRow {
            table,
            slot,
            kind,
            label,
            level,
            cost_wood: c.wood,
            cost_clay: c.clay,
            cost_iron: c.iron,
            cost_crop: c.crop,
            at_max,
            can_order,
        }
    };

    let fields = village
        .fields
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let slot = u8::try_from(i).unwrap_or(0);
            make_row(
                "field",
                slot,
                "",
                format!("{} field #{slot}", resource_label(f.kind)),
                f.level,
                BuildTarget::Field { slot },
            )
        })
        .collect();

    let buildings = [
        BuildingKind::MainBuilding,
        BuildingKind::RallyPoint,
        BuildingKind::Warehouse,
        BuildingKind::Granary,
        BuildingKind::Barracks,
        BuildingKind::Academy,
        BuildingKind::Smithy,
    ]
    .into_iter()
    .map(|kind| {
        let slot = building_slot(kind);
        let level = village
            .buildings
            .iter()
            .find(|b| b.kind == kind)
            .map_or(0, |b| b.level);
        make_row(
            "building",
            slot,
            building_kind_id(kind),
            building_label(kind).to_owned(),
            level,
            BuildTarget::Building { slot, kind },
        )
    })
    .collect();

    let active_view = active
        .iter()
        .map(|a| ActiveView {
            label: target_label(&village, a.target),
            target_level: a.target_level,
            complete_ms: a.complete_at.0,
        })
        .collect();

    page(&VillageTemplate {
        username: user.username,
        tribe: tribe_label(village.tribe),
        x: village.coordinate.x,
        y: village.coordinate.y,
        wood: amounts.wood,
        clay: amounts.clay,
        iron: amounts.iron,
        crop: amounts.crop,
        wood_rate: rates.wood,
        clay_rate: rates.clay,
        iron_rate: rates.iron,
        crop_rate: rates.crop_net,
        warehouse: caps.warehouse,
        granary: caps.granary,
        active: active_view,
        fields,
        buildings,
    })
}

/// Build-order form fields.
#[derive(Deserialize)]
pub struct BuildForm {
    table: String,
    slot: u8,
    #[serde(default)]
    kind: Option<String>,
}

/// Order an upgrade/construction for the player's village, then return to it (Player only, P4).
pub async fn build_submit(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
    Form(form): Form<BuildForm>,
) -> Response {
    let target = match form.table.as_str() {
        "field" => BuildTarget::Field { slot: form.slot },
        "building" => match parse_building_kind(form.kind.as_deref()) {
            // Slot is derived server-side from the kind — never trusted from the client (P4), so a
            // crafted request cannot place a building in (clobber) another building's slot.
            Some(kind) => BuildTarget::Building {
                slot: building_slot(kind),
                kind,
            },
            None => return Redirect::to("/village").into_response(),
        },
        _ => return Redirect::to("/village").into_response(),
    };

    if let Err(e) = order_build(
        state.accounts.as_ref(),
        state.accounts.as_ref(),
        state.rules.as_ref(),
        state.build_rules.as_ref(),
        state.world.speed,
        now(),
        player,
        target,
    )
    .await
    {
        tracing::warn!(error = %e, "build order rejected");
    }
    Redirect::to("/village").into_response()
}
