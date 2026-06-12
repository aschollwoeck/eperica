//! HTTP handlers for the register / login / village flow.

use crate::auth::{AuthUser, auth_cookie, clear_cookie};
use crate::state::AppState;
use crate::templates::{
    AcademyRow, AcademyTemplate, ActiveView, BuildRow, ForceRow, GarrisonRow, IndexTemplate,
    LoginTemplate, MapCellView, MapTemplate, MarketTemplate, MovementRow, QueueView, RallyTemplate,
    RallyUnitRow, RegisterTemplate, ReinforcementRow, ReportRow, ReportTemplate, ReportsTemplate,
    ScoutReportTemplate, ScoutResourceRow, ShipmentRow, SmithyRow, SmithyTemplate,
    StyleGuideTemplate, TrainRow, TroopsTemplate, VillageTemplate,
};
use askama::Template;
use axum::Form;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum_extra::extract::PrivateCookieJar;
use eperica_application::{
    AccountRepository, BattleReportView, BuildRepository, CombatRepository, LoginError,
    MovementRepository, RegisterCommand, RegisterError, ScoutIntel, ScoutReportView,
    ScoutRepository, TradeRepository, TrainingRepository, UnitOrderKind, UnitRepository,
    authenticate, load_economy, map_viewport, order_attack, order_build, order_reinforcement,
    order_research, order_return, order_scout, order_smithy_upgrade, order_trade, order_train,
    register, viewport_coords,
};
use eperica_domain::{
    AttackMode, BuildTarget, BuildingKind, Coordinate, MovementKind, OasisBonus, QueueLane,
    ResearchDenied, ResourceAmounts, ResourceKind, ScoutTarget, TileKind, TradeKind, Tribe, UnitId,
    UnitRole, UnitRules, UpgradeDenied, Village, VillageId, can_afford, can_research, can_upgrade,
    garrison_upkeep, per_unit_time_secs, queue_lane, scaled_time_secs,
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
        BuildingKind::Marketplace => "Marketplace",
        BuildingKind::Wall => "Wall",
        BuildingKind::Barracks => "Barracks",
        BuildingKind::Academy => "Academy",
        BuildingKind::Smithy => "Smithy",
        BuildingKind::Stable => "Stable",
        BuildingKind::Workshop => "Workshop",
        BuildingKind::Residence => "Residence",
        BuildingKind::Cranny => "Cranny",
    }
}

fn building_kind_id(kind: BuildingKind) -> &'static str {
    match kind {
        BuildingKind::MainBuilding => "main_building",
        BuildingKind::RallyPoint => "rally_point",
        BuildingKind::Warehouse => "warehouse",
        BuildingKind::Granary => "granary",
        BuildingKind::Marketplace => "marketplace",
        BuildingKind::Wall => "wall",
        BuildingKind::Barracks => "barracks",
        BuildingKind::Academy => "academy",
        BuildingKind::Smithy => "smithy",
        BuildingKind::Stable => "stable",
        BuildingKind::Workshop => "workshop",
        BuildingKind::Residence => "residence",
        BuildingKind::Cranny => "cranny",
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
        BuildingKind::Marketplace => 10,
        BuildingKind::Wall => 11,
        BuildingKind::Cranny => 12,
    }
}

fn parse_building_kind(s: Option<&str>) -> Option<BuildingKind> {
    match s {
        Some("main_building") => Some(BuildingKind::MainBuilding),
        Some("rally_point") => Some(BuildingKind::RallyPoint),
        Some("warehouse") => Some(BuildingKind::Warehouse),
        Some("granary") => Some(BuildingKind::Granary),
        Some("marketplace") => Some(BuildingKind::Marketplace),
        Some("wall") => Some(BuildingKind::Wall),
        Some("barracks") => Some(BuildingKind::Barracks),
        Some("academy") => Some(BuildingKind::Academy),
        Some("smithy") => Some(BuildingKind::Smithy),
        Some("stable") => Some(BuildingKind::Stable),
        Some("workshop") => Some(BuildingKind::Workshop),
        Some("cranny") => Some(BuildingKind::Cranny),
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
        state.unit_rules.as_ref(),
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

    // The garrison panel + total upkeep (005 AC6/AC9); names resolved via the tribe's roster.
    let roster = village
        .tribe
        .map_or(&[][..], |t| state.unit_rules.roster(t));
    let garrison_rows: Vec<GarrisonRow> = economy
        .garrison
        .iter()
        .map(|(unit, count)| {
            let spec = roster.iter().find(|s| &s.id == unit);
            GarrisonRow {
                name: spec.map_or_else(|| unit.as_str().to_owned(), |s| s.name.clone()),
                count: *count,
                upkeep: spec.map_or(0, |s| i64::from(s.crop_upkeep) * i64::from(*count)),
            }
        })
        .collect();
    let total_upkeep = garrison_upkeep(&economy.garrison, roster);
    let troop_links: Vec<(&'static str, &'static str)> = [
        (
            BuildingKind::Barracks,
            "Barracks",
            "/village/troops/barracks",
        ),
        (BuildingKind::Stable, "Stable", "/village/troops/stable"),
        (
            BuildingKind::Workshop,
            "Workshop",
            "/village/troops/workshop",
        ),
    ]
    .into_iter()
    .filter(|(kind, _, _)| {
        village
            .buildings
            .iter()
            .any(|b| b.kind == *kind && b.level > 0)
    })
    .map(|(_, label, href)| (label, href))
    .collect();

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
        BuildingKind::Marketplace,
        BuildingKind::Wall,
        BuildingKind::Barracks,
        BuildingKind::Academy,
        BuildingKind::Smithy,
        BuildingKind::Stable,
        BuildingKind::Workshop,
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

    // Troop movements + stationed reinforcements (007 AC7).
    let movements_view = match state.accounts.active_movements(player).await {
        Ok(m) => m,
        Err(e) => {
            tracing::error!(error = %e, "movements lookup failed");
            return server_error();
        }
    };
    let here = match state.accounts.reinforcements_at(village.id).await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "reinforcements-here lookup failed");
            return server_error();
        }
    };
    let abroad = match state.accounts.reinforcements_of(player).await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "reinforcements-abroad lookup failed");
            return server_error();
        }
    };
    let unit_rules: &UnitRules = state.unit_rules.as_ref();
    let movements: Vec<MovementRow> = movements_view
        .iter()
        .map(|m| MovementRow {
            label: match m.kind {
                MovementKind::Reinforce => {
                    format!("Reinforcement to ({}|{})", m.destination.x, m.destination.y)
                }
                MovementKind::Return => {
                    format!("Returning to ({}|{})", m.destination.x, m.destination.y)
                }
                MovementKind::Attack => {
                    format!("Attack on ({}|{})", m.destination.x, m.destination.y)
                }
                MovementKind::Raid => {
                    format!("Raid on ({}|{})", m.destination.x, m.destination.y)
                }
                MovementKind::Scout => {
                    format!("Scouting ({}|{})", m.destination.x, m.destination.y)
                }
            },
            troops: troops_summary(unit_rules, &m.troops),
            arrive_ms: m.arrive_at.0,
        })
        .collect();
    let reinforcements_here: Vec<ReinforcementRow> = here
        .iter()
        .map(|g| ReinforcementRow {
            owner: g.other_owner.clone(),
            coord: format!("({}|{})", g.other_coord.x, g.other_coord.y),
            troops: troops_summary(unit_rules, &g.troops),
            host_id: String::new(),
        })
        .collect();
    let reinforcements_abroad: Vec<ReinforcementRow> = abroad
        .iter()
        .map(|g| ReinforcementRow {
            owner: g.other_owner.clone(),
            coord: format!("({}|{})", g.other_coord.x, g.other_coord.y),
            troops: troops_summary(unit_rules, &g.troops),
            host_id: g.host_village.0.to_string(),
        })
        .collect();

    let trades = match state.accounts.active_trades(player).await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(error = %e, "trades lookup failed");
            return server_error();
        }
    };
    let shipments: Vec<ShipmentRow> = trades
        .iter()
        .map(|t| ShipmentRow {
            label: match t.kind {
                TradeKind::Deliver => {
                    format!("Shipment to ({}|{})", t.destination.x, t.destination.y)
                }
                TradeKind::Return => format!(
                    "Merchants returning from ({}|{})",
                    t.destination.x, t.destination.y
                ),
            },
            contents: bundle_summary(t.bundle),
            merchants: t.merchants,
            arrive_ms: t.arrive_at.0,
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
        has_academy: village
            .buildings
            .iter()
            .any(|b| b.kind == BuildingKind::Academy && b.level > 0),
        has_smithy: village
            .buildings
            .iter()
            .any(|b| b.kind == BuildingKind::Smithy && b.level > 0),
        troop_links,
        garrison: garrison_rows,
        garrison_upkeep: total_upkeep,
        movements,
        reinforcements_here,
        reinforcements_abroad,
        shipments,
        fields,
        buildings,
    })
}

/// The viewport half-extent: the map view shows a `(2·HALF + 1)`-square grid.
const MAP_HALF: i32 = 4;

/// Optional map-view center (defaults to the player's village).
#[derive(Deserialize)]
pub struct MapQuery {
    x: Option<i32>,
    y: Option<i32>,
}

fn oasis_label(b: OasisBonus) -> String {
    let parts: Vec<String> = [
        ("wood", b.wood),
        ("clay", b.clay),
        ("iron", b.iron),
        ("crop", b.crop),
    ]
    .into_iter()
    .filter(|(_, pct)| *pct > 0)
    .map(|(name, pct)| format!("+{pct}% {name}"))
    .collect();
    if parts.is_empty() {
        "Oasis".to_owned()
    } else {
        format!("Oasis {}", parts.join(", "))
    }
}

/// The terrain `(modifier-class, glyph, label)` for a tile.
fn tile_view(tile: TileKind) -> (&'static str, &'static str, String) {
    match tile {
        TileKind::Valley(d) => (
            "map-grid__cell--valley",
            "·",
            format!("Valley {}·{}·{}·{}", d.wood, d.clay, d.iron, d.crop),
        ),
        TileKind::Oasis(b) => ("map-grid__cell--oasis", "❀", oasis_label(b)),
        TileKind::Natar => ("map-grid__cell--natar", "N", "Natar".to_owned()),
    }
}

/// The seeded world map around a center (006 AC7; Player only — Visitor redirected to login, P4).
pub async fn map(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
    Query(q): Query<MapQuery>,
) -> Response {
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
            tracing::error!(error = %e, "villages lookup failed");
            return server_error();
        }
    };
    let radius = state.map.radius();
    // Center on the query (if given) or the player's first village, wrapped into bounds.
    let center = match (q.x, q.y) {
        (Some(x), Some(y)) => Coordinate::new(x, y).wrapped(radius),
        _ => villages
            .first()
            .map_or(Coordinate::new(0, 0), |v| v.coordinate),
    };

    let coords = viewport_coords(center, MAP_HALF, radius);
    let markers = match state.accounts.villages_at(&coords).await {
        Ok(m) => m,
        Err(e) => {
            tracing::error!(error = %e, "map markers lookup failed");
            return server_error();
        }
    };
    let viewport = map_viewport(state.map.as_ref(), center, MAP_HALF, &markers);

    let rows: Vec<Vec<MapCellView>> = viewport
        .rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|cell| {
                    let (kind_class, glyph, base_label) = tile_view(cell.tile);
                    let coord = cell.coordinate;
                    let mut class = format!("map-grid__cell {kind_class}");
                    let mut glyph = glyph;
                    let mut label = format!("{base_label} ({}|{})", coord.x, coord.y);
                    if let Some(marker) = &cell.marker {
                        class.push_str(" map-grid__cell--village");
                        if marker.owner_name == user.username {
                            class.push_str(" map-grid__cell--self");
                        }
                        glyph = "★";
                        label = format!(
                            "{} — {} ({}|{})",
                            base_label, marker.owner_name, coord.x, coord.y
                        );
                    }
                    MapCellView {
                        cell_class: class,
                        glyph,
                        label,
                    }
                })
                .collect()
        })
        .collect();

    let span = 2 * MAP_HALF + 1;
    page(&MapTemplate {
        center_x: center.x,
        center_y: center.y,
        radius: i32::try_from(radius).unwrap_or(i32::MAX),
        north_y: Coordinate::new(center.x, center.y.saturating_add(span))
            .wrapped(radius)
            .y,
        south_y: Coordinate::new(center.x, center.y.saturating_sub(span))
            .wrapped(radius)
            .y,
        east_x: Coordinate::new(center.x.saturating_add(span), center.y)
            .wrapped(radius)
            .x,
        west_x: Coordinate::new(center.x.saturating_sub(span), center.y)
            .wrapped(radius)
            .x,
        rows,
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
        state.accounts.as_ref(),
        state.rules.as_ref(),
        state.build_rules.as_ref(),
        state.unit_rules.as_ref(),
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

fn role_label(role: UnitRole) -> &'static str {
    match role {
        UnitRole::Infantry => "Infantry",
        UnitRole::Cavalry => "Cavalry",
        UnitRole::Scout => "Scout",
        UnitRole::Siege => "Siege",
        UnitRole::Expansion => "Expansion",
    }
}

/// Resolve a unit's display name across all tribes' rosters (a stationed reinforcement may come
/// from a different tribe than the viewer), falling back to the slug.
fn unit_name(unit_rules: &UnitRules, unit: &UnitId) -> String {
    [Tribe::Romans, Tribe::Teutons, Tribe::Gauls]
        .into_iter()
        .find_map(|t| unit_rules.unit(t, unit))
        .map_or_else(|| unit.as_str().to_owned(), |s| s.name.clone())
}

/// Summarise a resource bundle as "300 wood, 50 clay" ("—" when empty), 008 AC6.
fn bundle_summary(bundle: ResourceAmounts) -> String {
    let parts: Vec<String> = [
        ("wood", bundle.wood),
        ("clay", bundle.clay),
        ("iron", bundle.iron),
        ("crop", bundle.crop),
    ]
    .into_iter()
    .filter(|(_, n)| *n > 0)
    .map(|(name, n)| format!("{n} {name}"))
    .collect();
    if parts.is_empty() {
        "—".to_owned()
    } else {
        parts.join(", ")
    }
}

/// Summarise a composition as "4 Phalanx, 2 Swordsman" (007 AC7).
fn troops_summary(unit_rules: &UnitRules, troops: &[(UnitId, u32)]) -> String {
    troops
        .iter()
        .map(|(u, n)| format!("{n} {}", unit_name(unit_rules, u)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Format a duration in seconds as `h:mm:ss` (matching the countdown display).
fn fmt_duration(secs: i64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{h}:{m:02}:{s:02}")
}

fn building_level(village: &Village, kind: BuildingKind) -> u8 {
    village
        .buildings
        .iter()
        .find(|b| b.kind == kind)
        .map_or(0, |b| b.level)
}

/// The player's village + settled amounts, or an error response.
async fn village_view_data(
    state: &AppState,
    player: eperica_domain::PlayerId,
) -> Result<(Village, ResourceAmounts), Response> {
    match load_economy(
        state.accounts.as_ref(),
        state.rules.as_ref(),
        state.unit_rules.as_ref(),
        state.world.speed,
        now(),
        player,
    )
    .await
    {
        Ok(Some(e)) => {
            let amounts = e.economy.amounts;
            Ok((e.village, amounts))
        }
        Ok(None) => {
            tracing::error!(?player, "authenticated user has no village/economy");
            Err(server_error())
        }
        Err(e) => {
            tracing::error!(error = %e, "load economy failed");
            Err(server_error())
        }
    }
}

/// The Academy: the tribe's roster with research state and actions (004 AC15; Player only, P4).
pub async fn academy(State(state): State<AppState>, AuthUser(player): AuthUser) -> Response {
    let (village, amounts) = match village_view_data(&state, player).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let Some(tribe) = village.tribe else {
        tracing::error!(?player, "village has no tribe");
        return server_error();
    };
    let (researched, orders) = match tokio::try_join!(
        state.accounts.researched_units(village.id),
        state.accounts.active_unit_orders(village.id),
    ) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(error = %e, "academy state lookup failed");
            return server_error();
        }
    };
    let unit_rules: &UnitRules = state.unit_rules.as_ref();
    let research_active = orders.iter().find(|o| o.kind == UnitOrderKind::Research);
    let active = research_active.map(|o| QueueView {
        label: format!(
            "Researching {}",
            unit_rules
                .unit(tribe, &o.unit)
                .map_or(o.unit.as_str(), |s| s.name.as_str())
        ),
        complete_ms: o.complete_at.0,
    });

    let rows = unit_rules
        .roster(tribe)
        .iter()
        .map(|spec| {
            let is_researched = spec.researched_by_default() || researched.contains(&spec.id);
            let (cost, time_secs) = spec.research.as_ref().map_or((None, 0), |r| {
                (
                    Some(r.cost),
                    scaled_time_secs(r.time_secs, state.world.speed),
                )
            });
            let mut gate = String::new();
            let mut can_order = false;
            if !is_researched {
                match can_research(spec, false, &village.buildings) {
                    Ok(()) => {
                        if research_active.is_some() {
                            gate = "research in progress".to_owned();
                        } else if !cost.is_some_and(|c| can_afford(amounts, c)) {
                            gate = "insufficient resources".to_owned();
                        } else {
                            can_order = true;
                        }
                    }
                    Err(ResearchDenied::NoAcademy | ResearchDenied::RequirementsUnmet) => {
                        let unmet: Vec<String> = spec
                            .research
                            .as_ref()
                            .map(|r| {
                                r.requirements
                                    .iter()
                                    .filter(|(k, l)| building_level(&village, *k) < *l)
                                    .map(|(k, l)| format!("{} {l}", building_label(*k)))
                                    .collect()
                            })
                            .unwrap_or_default();
                        gate = format!("requires {}", unmet.join(", "));
                    }
                    Err(ResearchDenied::AlreadyResearched) => {}
                }
            }
            let c = cost.unwrap_or(ResourceAmounts {
                wood: 0,
                clay: 0,
                iron: 0,
                crop: 0,
            });
            AcademyRow {
                id: spec.id.as_str().to_owned(),
                name: spec.name.clone(),
                role: role_label(spec.role),
                attack: spec.attack,
                def_inf: spec.defense_infantry,
                def_cav: spec.defense_cavalry,
                speed: spec.speed,
                carry: spec.carry_capacity,
                upkeep: spec.crop_upkeep,
                researched: is_researched,
                can_order,
                gate,
                cost_wood: c.wood,
                cost_clay: c.clay,
                cost_iron: c.iron,
                cost_crop: c.crop,
                time: fmt_duration(time_secs),
            }
        })
        .collect();

    page(&AcademyTemplate {
        has_academy: building_level(&village, BuildingKind::Academy) > 0,
        rows,
        active,
    })
}

/// The Smithy: researched units with upgrade levels and actions (004 AC15; Player only, P4).
pub async fn smithy(State(state): State<AppState>, AuthUser(player): AuthUser) -> Response {
    let (village, amounts) = match village_view_data(&state, player).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let Some(tribe) = village.tribe else {
        tracing::error!(?player, "village has no tribe");
        return server_error();
    };
    let (researched, levels, orders) = match tokio::try_join!(
        state.accounts.researched_units(village.id),
        state.accounts.unit_levels(village.id),
        state.accounts.active_unit_orders(village.id),
    ) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(error = %e, "smithy state lookup failed");
            return server_error();
        }
    };
    let unit_rules: &UnitRules = state.unit_rules.as_ref();
    let upgrade_active = orders
        .iter()
        .find(|o| o.kind == UnitOrderKind::SmithyUpgrade);
    let active = upgrade_active.map(|o| QueueView {
        label: format!(
            "Upgrading {} to level {}",
            unit_rules
                .unit(tribe, &o.unit)
                .map_or(o.unit.as_str(), |s| s.name.as_str()),
            o.target_level.unwrap_or(0)
        ),
        complete_ms: o.complete_at.0,
    });

    let rows = unit_rules
        .roster(tribe)
        .iter()
        .filter(|spec| spec.researched_by_default() || researched.contains(&spec.id))
        .map(|spec| {
            let level = levels
                .iter()
                .find(|(u, _)| u == &spec.id)
                .map_or(0, |(_, l)| *l);
            let cost = unit_rules.smithy.upgrade_cost(spec, level);
            let time_secs = unit_rules
                .smithy
                .base_time_secs(level)
                .map_or(0, |t| scaled_time_secs(t, state.world.speed));
            let mut gate = String::new();
            let mut can_order = false;
            match can_upgrade(spec, true, level, &village.buildings, &unit_rules.smithy) {
                Ok(()) => {
                    if upgrade_active.is_some() {
                        gate = "upgrade in progress".to_owned();
                    } else if !cost.is_some_and(|c| can_afford(amounts, c)) {
                        gate = "insufficient resources".to_owned();
                    } else {
                        can_order = true;
                    }
                }
                Err(UpgradeDenied::AtMaxLevel) => gate = "max level".to_owned(),
                Err(UpgradeDenied::SmithyLevelTooLow) => {
                    gate = "raise the Smithy first".to_owned();
                }
                Err(UpgradeDenied::NoSmithy | UpgradeDenied::NotResearched) => {}
            }
            let c = cost.unwrap_or(ResourceAmounts {
                wood: 0,
                clay: 0,
                iron: 0,
                crop: 0,
            });
            SmithyRow {
                id: spec.id.as_str().to_owned(),
                name: spec.name.clone(),
                level,
                can_order,
                gate,
                cost_wood: c.wood,
                cost_clay: c.clay,
                cost_iron: c.iron,
                cost_crop: c.crop,
                time: fmt_duration(time_secs),
            }
        })
        .collect();

    page(&SmithyTemplate {
        has_smithy: building_level(&village, BuildingKind::Smithy) > 0,
        smithy_level: building_level(&village, BuildingKind::Smithy),
        rows,
        active,
    })
}

/// Research/upgrade form fields.
#[derive(Deserialize)]
pub struct UnitForm {
    unit: String,
}

/// Order a unit research for the player's village, then return to the Academy (Player only, P4).
pub async fn research_submit(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
    Form(form): Form<UnitForm>,
) -> Response {
    if let Err(e) = order_research(
        state.accounts.as_ref(),
        state.accounts.as_ref(),
        state.accounts.as_ref(),
        state.rules.as_ref(),
        state.unit_rules.as_ref(),
        state.world.speed,
        now(),
        player,
        UnitId(form.unit),
    )
    .await
    {
        tracing::warn!(error = %e, "research order rejected");
    }
    Redirect::to("/village/academy").into_response()
}

fn parse_troop_building(slug: &str) -> Option<BuildingKind> {
    match slug {
        "barracks" => Some(BuildingKind::Barracks),
        "stable" => Some(BuildingKind::Stable),
        "workshop" => Some(BuildingKind::Workshop),
        _ => None,
    }
}

/// A troop building's training view: researched units it trains, the running batch (005 AC9;
/// Player only, P4).
pub async fn troops(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
    axum::extract::Path(building_slug): axum::extract::Path<String>,
) -> Response {
    let Some(building) = parse_troop_building(&building_slug) else {
        return Redirect::to("/village").into_response();
    };
    let (village, _amounts) = match village_view_data(&state, player).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let Some(tribe) = village.tribe else {
        tracing::error!(?player, "village has no tribe");
        return server_error();
    };
    let (researched, active) = match tokio::try_join!(
        state.accounts.researched_units(village.id),
        state.accounts.active_training(village.id),
    ) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(error = %e, "troop view lookup failed");
            return server_error();
        }
    };
    let unit_rules: &UnitRules = state.unit_rules.as_ref();
    let building_level = building_level(&village, building);
    let batch = active.iter().find(|t| t.building == building);
    let active_view = batch.map(|t| QueueView {
        label: format!(
            "Training {} × {} — {} remaining",
            t.count_total,
            unit_rules
                .unit(tribe, &t.unit)
                .map_or(t.unit.as_str(), |s| s.name.as_str()),
            t.count_total - t.count_done
        ),
        complete_ms: t.next_complete_at.0,
    });

    let rows = if building_level == 0 {
        Vec::new() // the template only renders the building-required notice
    } else {
        unit_rules
            .roster(tribe)
            .iter()
            .filter(|spec| spec.trained_in == building)
            .filter(|spec| spec.researched_by_default() || researched.contains(&spec.id))
            .map(|spec| TrainRow {
                id: spec.id.as_str().to_owned(),
                name: spec.name.clone(),
                attack: spec.attack,
                def_inf: spec.defense_infantry,
                def_cav: spec.defense_cavalry,
                upkeep: spec.crop_upkeep,
                cost_wood: spec.cost.wood,
                cost_clay: spec.cost.clay,
                cost_iron: spec.cost.iron,
                cost_crop: spec.cost.crop,
                time: fmt_duration(per_unit_time_secs(
                    spec.train_secs,
                    building_level,
                    &unit_rules.training,
                    state.world.speed,
                )),
                can_order: batch.is_none(),
                gate: if batch.is_some() {
                    "training in progress".to_owned()
                } else {
                    String::new()
                },
            })
            .collect()
    };

    page(&TroopsTemplate {
        building: building_label(building),
        has_building: building_level > 0,
        rows,
        active: active_view,
    })
}

/// Training form fields.
#[derive(Deserialize)]
pub struct TrainForm {
    unit: String,
    count: u32,
}

/// Order a training batch for the player's village, then return to the building page (Player
/// only, P4).
pub async fn train_submit(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
    Form(form): Form<TrainForm>,
) -> Response {
    let unit = UnitId(form.unit);
    if let Err(e) = order_train(
        state.accounts.as_ref(),
        state.accounts.as_ref(),
        state.accounts.as_ref(),
        state.accounts.as_ref(),
        state.rules.as_ref(),
        state.unit_rules.as_ref(),
        state.world.speed,
        now(),
        player,
        unit.clone(),
        form.count,
    )
    .await
    {
        tracing::warn!(error = %e, "training order rejected");
    }
    // Land back on the unit's building page (the same kind across tribes).
    let building = [Tribe::Romans, Tribe::Teutons, Tribe::Gauls]
        .into_iter()
        .find_map(|t| state.unit_rules.unit(t, &unit))
        .map(|s| s.trained_in);
    let target = match building {
        Some(BuildingKind::Barracks) => "/village/troops/barracks",
        Some(BuildingKind::Stable) => "/village/troops/stable",
        Some(BuildingKind::Workshop) => "/village/troops/workshop",
        _ => "/village",
    };
    Redirect::to(target).into_response()
}

/// Order a Smithy upgrade for the player's village, then return to the Smithy (Player only, P4).
pub async fn smithy_upgrade_submit(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
    Form(form): Form<UnitForm>,
) -> Response {
    if let Err(e) = order_smithy_upgrade(
        state.accounts.as_ref(),
        state.accounts.as_ref(),
        state.accounts.as_ref(),
        state.rules.as_ref(),
        state.unit_rules.as_ref(),
        state.world.speed,
        now(),
        player,
        UnitId(form.unit),
    )
    .await
    {
        tracing::warn!(error = %e, "smithy upgrade rejected");
    }
    Redirect::to("/village/smithy").into_response()
}

/// The Rally Point: the garrison troops that can be sent to reinforce (007 AC7; Player only, P4).
pub async fn rally(State(state): State<AppState>, AuthUser(player): AuthUser) -> Response {
    let (village, _amounts) = match village_view_data(&state, player).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let Some(tribe) = village.tribe else {
        tracing::error!(?player, "village has no tribe");
        return server_error();
    };
    let garrison = match state.accounts.garrison(village.id).await {
        Ok(g) => g,
        Err(e) => {
            tracing::error!(error = %e, "garrison lookup failed");
            return server_error();
        }
    };
    let roster = state.unit_rules.roster(tribe);
    let units = garrison
        .iter()
        .filter(|(_, n)| *n > 0)
        .map(|(u, n)| {
            let spec = roster.iter().find(|s| &s.id == u);
            RallyUnitRow {
                id: u.as_str().to_owned(),
                name: spec.map_or_else(|| u.as_str().to_owned(), |s| s.name.clone()),
                available: *n,
            }
        })
        .collect();
    page(&RallyTemplate { units })
}

/// Send a reinforcement from the Rally Point, then return to the village (Player only, P4).
///
/// The composition arrives as `count_<unit-slug>` fields alongside the target `x`/`y`; counts are
/// parsed and re-validated server-side (P4) — the use-case rejects anything over the garrison.
pub async fn rally_send(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
    Form(form): Form<std::collections::HashMap<String, String>>,
) -> Response {
    let x = form.get("x").and_then(|s| s.trim().parse::<i32>().ok());
    let y = form.get("y").and_then(|s| s.trim().parse::<i32>().ok());
    let (Some(x), Some(y)) = (x, y) else {
        return Redirect::to("/village/rally").into_response();
    };
    let troops: Vec<(UnitId, u32)> = form
        .iter()
        .filter_map(|(k, v)| {
            let id = k.strip_prefix("count_")?;
            let n = v.trim().parse::<u32>().ok()?;
            (n > 0).then(|| (UnitId(id.to_owned()), n))
        })
        .collect();
    let target = Coordinate::new(x, y);
    // What attached scouts spy on (010); used by `scout` missions and by attacks carrying scouts.
    let scout_target = form
        .get("scout_target")
        .and_then(|s| ScoutTarget::from_slug(s));
    // The mode selects the use-case: reinforce (007) defends; attack/raid (009) fight; scout (010) spies.
    match form.get("mode").map(String::as_str) {
        Some("scout") => {
            if let Err(e) = order_scout(
                state.accounts.as_ref(),
                state.accounts.as_ref(),
                state.accounts.as_ref(),
                state.rules.as_ref(),
                state.unit_rules.as_ref(),
                state.map.as_ref(),
                state.world.speed,
                now(),
                player,
                target,
                troops,
                scout_target.unwrap_or(ScoutTarget::Defenses),
            )
            .await
            {
                tracing::warn!(error = %e, "scout order rejected");
            }
        }
        Some(mode @ ("attack" | "raid")) => {
            let mode = if mode == "raid" {
                AttackMode::Raid
            } else {
                AttackMode::Attack
            };
            if let Err(e) = order_attack(
                state.accounts.as_ref(),
                state.accounts.as_ref(),
                state.accounts.as_ref(),
                state.rules.as_ref(),
                state.unit_rules.as_ref(),
                state.map.as_ref(),
                state.world.speed,
                now(),
                player,
                target,
                troops,
                mode,
                scout_target,
            )
            .await
            {
                tracing::warn!(error = %e, "attack order rejected");
            }
        }
        _ => {
            if let Err(e) = order_reinforcement(
                state.accounts.as_ref(),
                state.accounts.as_ref(),
                state.accounts.as_ref(),
                state.rules.as_ref(),
                state.unit_rules.as_ref(),
                state.map.as_ref(),
                state.world.speed,
                now(),
                player,
                target,
                troops,
            )
            .await
            {
                tracing::warn!(error = %e, "reinforcement order rejected");
            }
        }
    }
    Redirect::to("/village").into_response()
}

/// Send-back form fields (the host village whose stationed troops to recall).
#[derive(Deserialize)]
pub struct RallyReturnForm {
    host: String,
}

/// Recall the player's troops stationed at a host, then return to the village (Player only, P4).
pub async fn rally_return(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
    Form(form): Form<RallyReturnForm>,
) -> Response {
    let Ok(host) = form.host.trim().parse::<u128>() else {
        return Redirect::to("/village").into_response();
    };
    if let Err(e) = order_return(
        state.accounts.as_ref(),
        state.accounts.as_ref(),
        state.unit_rules.as_ref(),
        state.map.as_ref(),
        state.world.speed,
        now(),
        player,
        VillageId(host),
    )
    .await
    {
        tracing::warn!(error = %e, "return order rejected");
    }
    Redirect::to("/village").into_response()
}

/// The Marketplace: the merchant pool (free/total + capacity) and a send-resources form (008 AC6;
/// Player only, P4).
pub async fn market(State(state): State<AppState>, AuthUser(player): AuthUser) -> Response {
    let (village, _amounts) = match village_view_data(&state, player).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let Some(tribe) = village.tribe else {
        tracing::error!(?player, "village has no tribe");
        return server_error();
    };
    let level = building_level(&village, BuildingKind::Marketplace);
    if level == 0 {
        return page(&MarketTemplate {
            has_marketplace: false,
            capacity: 0,
            free: 0,
            total: 0,
        });
    }
    let committed = match state.accounts.committed_merchants(village.id).await {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, "committed merchants lookup failed");
            return server_error();
        }
    };
    let total = state.merchant_rules.merchants_total(level);
    page(&MarketTemplate {
        has_marketplace: true,
        capacity: state.merchant_rules.profile(tribe).capacity,
        free: total.saturating_sub(committed),
        total,
    })
}

/// Send a resource shipment from the Marketplace, then return to the village (Player only, P4).
///
/// The amounts arrive as `amount_<resource>` fields alongside the target `x`/`y`; they are parsed
/// and re-validated server-side (P4) — the use-case rejects an over-stored or over-merchant load.
pub async fn market_send(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
    Form(form): Form<std::collections::HashMap<String, String>>,
) -> Response {
    let x = form.get("x").and_then(|s| s.trim().parse::<i32>().ok());
    let y = form.get("y").and_then(|s| s.trim().parse::<i32>().ok());
    let (Some(x), Some(y)) = (x, y) else {
        return Redirect::to("/village/market").into_response();
    };
    let amount = |k: &str| {
        form.get(k)
            .and_then(|s| s.trim().parse::<i64>().ok())
            .filter(|n| *n > 0)
            .unwrap_or(0)
    };
    let bundle = ResourceAmounts {
        wood: amount("amount_wood"),
        clay: amount("amount_clay"),
        iron: amount("amount_iron"),
        crop: amount("amount_crop"),
    };
    if let Err(e) = order_trade(
        state.accounts.as_ref(),
        state.accounts.as_ref(),
        state.rules.as_ref(),
        state.unit_rules.as_ref(),
        state.merchant_rules.as_ref(),
        state.map.as_ref(),
        state.world.speed,
        now(),
        player,
        Coordinate::new(x, y),
        bundle,
    )
    .await
    {
        tracing::warn!(error = %e, "trade order rejected");
    }
    Redirect::to("/village").into_response()
}

/// A one-line headline for a report from this player's perspective.
fn report_headline(r: &BattleReportView, i_attacked: bool) -> String {
    let kind = if r.kind == MovementKind::Raid {
        "Raid"
    } else {
        "Attack"
    };
    if i_attacked {
        format!(
            "{kind} on {} ({}|{})",
            r.defender_name, r.defender_coord.x, r.defender_coord.y
        )
    } else {
        format!(
            "{kind} from {} ({}|{})",
            r.attacker_name, r.attacker_coord.x, r.attacker_coord.y
        )
    }
}

/// The outcome from this player's perspective.
fn report_outcome(r: &BattleReportView, i_attacked: bool) -> String {
    let i_won = if i_attacked {
        r.attacker_won
    } else {
        !r.attacker_won
    };
    if i_won { "Victory" } else { "Defeat" }.to_owned()
}

/// Build a side's force table (sent/defending + lost) with unit display names.
fn force_rows(
    unit_rules: &UnitRules,
    forces: &[(UnitId, u32)],
    losses: &[(UnitId, u32)],
) -> Vec<ForceRow> {
    forces
        .iter()
        .map(|(id, count)| ForceRow {
            name: unit_name(unit_rules, id),
            count: *count,
            lost: losses.iter().find(|(u, _)| u == id).map_or(0, |(_, l)| *l),
        })
        .collect()
}

/// One inbox row for a scout report from the viewer's perspective (010 AC12).
fn scout_report_row(r: &ScoutReportView) -> ReportRow {
    let (name, coord) = if r.viewer_is_scouter {
        (&r.target_name, r.target_coord)
    } else {
        (&r.scouter_name, r.scouter_coord)
    };
    let headline = if r.viewer_is_scouter {
        format!("Scouted {name} ({}|{})", coord.x, coord.y)
    } else {
        format!("Scouted by {name} ({}|{})", coord.x, coord.y)
    };
    let lost: u32 = r.scouts_lost.iter().map(|(_, n)| n).sum();
    let outcome = if r.viewer_is_scouter {
        if r.intel.is_some() {
            "Intel gathered".to_owned()
        } else {
            "No intel — all scouts lost".to_owned()
        }
    } else {
        format!("Detected — {lost} destroyed")
    };
    ReportRow {
        when_ms: r.occurred_at.0,
        headline,
        outcome,
        href: format!("/reports/scout/{}", r.id),
    }
}

/// The player's reports inbox — battle reports (009) and scout reports (010), newest first (P4).
pub async fn reports(State(state): State<AppState>, AuthUser(player): AuthUser) -> Response {
    let battle = match state.accounts.reports_for(player, 50).await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "reports lookup failed");
            return server_error();
        }
    };
    let scouts = match state.accounts.scout_reports_for(player, 50).await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "scout reports lookup failed");
            return server_error();
        }
    };
    let mut rows: Vec<ReportRow> = battle
        .iter()
        .map(|r| {
            let i_attacked = r.attacker_player == player;
            ReportRow {
                when_ms: r.occurred_at.0,
                headline: report_headline(r, i_attacked),
                outcome: report_outcome(r, i_attacked),
                href: format!("/reports/{}", r.id),
            }
        })
        .collect();
    rows.extend(scouts.iter().map(scout_report_row));
    rows.sort_by_key(|r| std::cmp::Reverse(r.when_ms));
    page(&ReportsTemplate { reports: rows })
}

/// One scout report's detail — scouter sees the intel, a detected target sees only the notification;
/// redaction is enforced by the repository (010 AC11, P4).
pub async fn scout_report_detail(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    let Ok(id) = id.parse::<u128>() else {
        return Redirect::to("/reports").into_response();
    };
    let r = match state.accounts.scout_report(id, player).await {
        Ok(Some(r)) => r,
        Ok(None) => return Redirect::to("/reports").into_response(),
        Err(e) => {
            tracing::error!(error = %e, "scout report lookup failed");
            return server_error();
        }
    };
    let unit_rules = state.unit_rules.as_ref();
    let target_type = match r.target_type {
        ScoutTarget::Resources => "Resources",
        ScoutTarget::Defenses => "Defenses",
    };
    let lost: u32 = r.scouts_lost.iter().map(|(_, n)| n).sum();
    let headline = if r.viewer_is_scouter {
        format!(
            "You scouted {} ({}|{})",
            r.target_name, r.target_coord.x, r.target_coord.y
        )
    } else {
        format!(
            "{} ({}|{}) scouted your village",
            r.scouter_name, r.scouter_coord.x, r.scouter_coord.y
        )
    };
    let summary = if r.viewer_is_scouter {
        let sent: u32 = r.scouts_sent.iter().map(|(_, n)| n).sum();
        format!("{sent} scouts sent, {lost} lost")
    } else {
        format!("{lost} enemy scouts destroyed")
    };
    let (intel_kind, resources, troops, wall_level) = match &r.intel {
        Some(ScoutIntel::Resources(a)) => (
            "resources",
            vec![
                ScoutResourceRow {
                    name: "Wood".to_owned(),
                    amount: a.wood,
                },
                ScoutResourceRow {
                    name: "Clay".to_owned(),
                    amount: a.clay,
                },
                ScoutResourceRow {
                    name: "Iron".to_owned(),
                    amount: a.iron,
                },
                ScoutResourceRow {
                    name: "Crop".to_owned(),
                    amount: a.crop,
                },
            ],
            Vec::new(),
            0u8,
        ),
        Some(ScoutIntel::Defenses { troops, wall_level }) => (
            "defenses",
            Vec::new(),
            force_rows(unit_rules, troops, &[]),
            *wall_level,
        ),
        None => ("none", Vec::new(), Vec::new(), 0),
    };
    page(&ScoutReportTemplate {
        headline,
        summary,
        is_scouter: r.viewer_is_scouter,
        target_type,
        intel_kind,
        resources,
        troops,
        wall_level,
    })
}

/// One battle report's detail — only a party to it may view it (009 AC8, P4).
pub async fn report_detail(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    let Ok(id) = id.parse::<u128>() else {
        return Redirect::to("/reports").into_response();
    };
    let report = match state.accounts.report(id, player).await {
        Ok(Some(r)) => r,
        Ok(None) => return Redirect::to("/reports").into_response(),
        Err(e) => {
            tracing::error!(error = %e, "report lookup failed");
            return server_error();
        }
    };
    let unit_rules = state.unit_rules.as_ref();
    let i_attacked = report.attacker_player == player;
    // The defender of a combined attack learns scouting also occurred only when detected (010 AC8).
    let scouted_note = (report.scouted && !i_attacked).then(|| {
        let what = match report.scout_target {
            Some(ScoutTarget::Resources) => "resources",
            _ => "defenses",
        };
        format!("The enemy also scouted your {what}.")
    });
    page(&ReportTemplate {
        kind: if report.kind == MovementKind::Raid {
            "Raid"
        } else {
            "Attack"
        },
        headline: report_headline(&report, i_attacked),
        outcome: report_outcome(&report, i_attacked),
        luck_pct: ((report.luck - 1.0) * 100.0).round() as i64,
        morale_pct: (report.morale * 100.0).round() as i64,
        wall_before: report.wall_before,
        wall_after: report.wall_after,
        attacker_name: report.attacker_name.clone(),
        attacker_rows: force_rows(unit_rules, &report.attacker_forces, &report.attacker_losses),
        defender_name: report.defender_name.clone(),
        defender_rows: force_rows(unit_rules, &report.defender_forces, &report.defender_losses),
        scouted_note,
    })
}
