//! HTTP handlers for the register / login / village flow.

use crate::auth::{AuthUser, MaybeAuthUser, MaybeRealUser, RealUser, auth_cookie, clear_cookie};
use crate::state::AppState;
use crate::templates::{
    AcademyRow, AcademyTemplate, AchievementRowView, ActiveView, AdminAccountRow, AdminTemplate,
    AdminWorldRow, AllianceStatsTemplate, AllianceTemplate, AlliedVillageView, ArtifactRowView,
    AuditRow, BuildRow, ChatLineView, CompletedQuestView, ConversationRow, ConversationTemplate,
    CurrentQuestView, DiploRowView, ForceRow, ForumPostRow, ForumTemplate, ForumThreadRow,
    ForumThreadTemplate, GarrisonRow, HistoryPointView, IncomingView, IndexTemplate,
    LeaderboardRowView, LeaderboardTemplate, LoginTemplate, MapCellView, MapTemplate,
    MarketTemplate, MedalRowView, MemberStatRow, MessagesTemplate, ModAccountTemplate,
    ModQueueTemplate, ModReportRow, MovementRow, NotificationRowView, NotificationsTemplate,
    OasisRow, OutgoingInviteView, PendingInviteView, PlayerStatsTemplate, ProfileTemplate,
    QuestsTemplate, QueueView, RallyTemplate, RallyUnitRow, RegisterTemplate, ReinforcementRow,
    ReportRow, ReportTemplate, ReportsTemplate, RosterRowView, ScoutReportTemplate,
    ScoutResourceRow, SearchHitRow, SearchTemplate, SettingsTemplate, SettingsToggleRow,
    ShipmentRow, SitterRow, SittingTemplate, SmithyRow, SmithyTemplate, StyleGuideTemplate,
    TrainRow, TroopsTemplate, VillageStatRow, VillageSwitchRow, VillageTemplate,
    WonderStandingView, WonderTemplate,
};
use askama::Template;
use axum::Form;
use axum::extract::{ConnectInfo, Path, Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum_extra::extract::PrivateCookieJar;
use eperica_application::{
    AccountRepository, AchievementRepository, AdminError, AllianceLeaderboardRow,
    AllianceRepository, ArtifactRepository, BattleReportView, BoardScope, BuildRepository,
    CombatRepository, CommsError, ConflictMetric, ConquestRepository, DiplomacyCommand,
    ElevatedRole, ForumError, LeaderboardRow, LoginError, MedalRepository, MedalSubjectKind,
    ModerationError, ModerationRepository, MovementRepository, OasisRepository, PlayerHit,
    QuestRepository, RegisterCommand, RegisterError, ScoutIntel, ScoutReportView, ScoutRepository,
    TradeRepository, TrainingRepository, UnitOrderKind, UnitRepository, Window, WonderRepository,
    account_signals, admin_overview, alliance_conflict_leaderboard,
    alliance_population_leaderboard, alliance_statistics, alliance_view, authenticate,
    authorize_sit, climbers_leaderboard, conflict_leaderboard, conversation_list,
    create_world as admin_create_world_uc, disband_alliance, dm_key, dm_pair_key, edit_bio,
    end_protection_if_established, evaluate_achievements, evaluate_quests, expel_member,
    file_report, found_alliance, grant_sitter, invite_player, leave_alliance,
    list_accounts as admin_list_accounts, list_forum, list_notifications, list_sitters,
    list_sitting_for, list_worlds as admin_list_worlds, load_culture, load_economy, map_viewport,
    mark_notifications_read, notif_key, notification_settings, notification_unread, open_chat,
    open_dm, open_thread, order_attack, order_build, order_oasis_attack, order_oasis_recall,
    order_oasis_reinforce, order_reinforcement, order_research, order_return, order_scout,
    order_settle, order_smithy_upgrade, order_trade, order_train, order_wonder_build, parse_dm_key,
    player_statistics, population_history, population_leaderboard, register, reinforcement_reports,
    reply, require_admin, resolve_report, respond_invite, review_queue, revoke_invite,
    revoke_sitter, sanction_account, search, search_accounts as admin_search_accounts, send_chat,
    send_dm, set_diplomacy, set_member_role, set_notification_pref, set_role as admin_set_role_uc,
    sitter_log, start_thread, transfer_founder, unread_badge, view_profile, viewport_coords,
};
use eperica_domain::{
    AllianceId, AllianceRight, AllianceRole, AttackMode, BuildTarget, BuildingKind, ChatChannel,
    Coordinate, DiplomacyStance, DiplomacyStatus, MedalCategory, MovementKind, OasisBonus,
    PlayerId, Presence, Quadrant, QuestReward, QueueLane, ReportReason, ResearchDenied,
    ResourceAmounts, ResourceKind, RightSet, SanctionKind, ScoutTarget, TileKind, Timestamp,
    TradeKind, Tribe, UnitId, UnitRole, UnitRules, UpgradeDenied, Village, VillageId,
    can_access_channel, can_afford, can_research, can_upgrade, current_quest, expansion_slots,
    garrison_upkeep, is_inactive, per_unit_time_secs, presence, queue_lane, regenerate_loyalty,
    scaled_time_secs,
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
        BuildingKind::Embassy => "Embassy",
        BuildingKind::Wall => "Wall",
        BuildingKind::Barracks => "Barracks",
        BuildingKind::Academy => "Academy",
        BuildingKind::Smithy => "Smithy",
        BuildingKind::Stable => "Stable",
        BuildingKind::Workshop => "Workshop",
        BuildingKind::Residence => "Residence",
        BuildingKind::Cranny => "Cranny",
        BuildingKind::Outpost => "Outpost",
        BuildingKind::TownHall => "Town Hall",
        BuildingKind::Palace => "Palace",
        BuildingKind::Treasury => "Treasury",
        BuildingKind::Wonder => "Wonder of the World",
    }
}

fn building_kind_id(kind: BuildingKind) -> &'static str {
    match kind {
        BuildingKind::MainBuilding => "main_building",
        BuildingKind::RallyPoint => "rally_point",
        BuildingKind::Warehouse => "warehouse",
        BuildingKind::Granary => "granary",
        BuildingKind::Marketplace => "marketplace",
        BuildingKind::Embassy => "embassy",
        BuildingKind::Wall => "wall",
        BuildingKind::Barracks => "barracks",
        BuildingKind::Academy => "academy",
        BuildingKind::Smithy => "smithy",
        BuildingKind::Stable => "stable",
        BuildingKind::Workshop => "workshop",
        BuildingKind::Residence => "residence",
        BuildingKind::Cranny => "cranny",
        BuildingKind::Outpost => "outpost",
        BuildingKind::TownHall => "town_hall",
        BuildingKind::Palace => "palace",
        BuildingKind::Treasury => "treasury",
        BuildingKind::Wonder => "wonder",
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
        BuildingKind::Outpost => 13,
        BuildingKind::TownHall => 14,
        BuildingKind::Palace => 15,
        BuildingKind::Embassy => 16,
        BuildingKind::Treasury => 17,
        BuildingKind::Wonder => 18,
    }
}

fn parse_building_kind(s: Option<&str>) -> Option<BuildingKind> {
    match s {
        Some("main_building") => Some(BuildingKind::MainBuilding),
        Some("rally_point") => Some(BuildingKind::RallyPoint),
        Some("warehouse") => Some(BuildingKind::Warehouse),
        Some("granary") => Some(BuildingKind::Granary),
        Some("marketplace") => Some(BuildingKind::Marketplace),
        Some("embassy") => Some(BuildingKind::Embassy),
        Some("wall") => Some(BuildingKind::Wall),
        Some("barracks") => Some(BuildingKind::Barracks),
        Some("academy") => Some(BuildingKind::Academy),
        Some("smithy") => Some(BuildingKind::Smithy),
        Some("stable") => Some(BuildingKind::Stable),
        Some("workshop") => Some(BuildingKind::Workshop),
        Some("cranny") => Some(BuildingKind::Cranny),
        Some("outpost") => Some(BuildingKind::Outpost),
        Some("town_hall") => Some(BuildingKind::TownHall),
        Some("residence") => Some(BuildingKind::Residence),
        Some("palace") => Some(BuildingKind::Palace),
        Some("treasury") => Some(BuildingKind::Treasury),
        Some("wonder") => Some(BuildingKind::Wonder),
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

/// 403 for a non-administrator reaching an admin-only surface (036 AC2, P4).
fn admin_forbidden() -> Response {
    (StatusCode::FORBIDDEN, "Administrators only.").into_response()
}

/// 403 for a non-moderator reaching a moderator-only surface (022 AC1, P4).
fn forbidden() -> Response {
    (StatusCode::FORBIDDEN, "Moderators only.").into_response()
}

fn not_found() -> Response {
    (StatusCode::NOT_FOUND, "not found").into_response()
}

/// A display label for a medal category (017).
fn medal_label(c: MedalCategory) -> &'static str {
    match c {
        MedalCategory::Attacker => "Top attacker",
        MedalCategory::Defender => "Top defender",
        MedalCategory::Raider => "Top raider",
        MedalCategory::Climber => "Top climber",
        MedalCategory::AlliancePopulation => "Top alliance (population)",
        MedalCategory::AllianceAttacker => "Top alliance (attack)",
        MedalCategory::AllianceDefender => "Top alliance (defense)",
    }
}

/// A display label for an achievement id (017). Falls back to the id for unknown entries.
fn achievement_label(id: &str) -> &'static str {
    match id {
        "second_village" => "Founded a second village",
        "defender_100" => "Won 100 defensive battles",
        "first_oasis" => "Occupied a first oasis",
        "population_1000" => "Reached 1000 population",
        "research_all_units" => "Researched every unit of your tribe",
        _ => "Achievement",
    }
}

/// A human-readable summary of a quest reward (018): resources, culture, and any troop grant.
fn quest_reward_label(reward: &QuestReward) -> String {
    let mut parts: Vec<String> = Vec::new();
    let r = &reward.resources;
    for (amount, name) in [
        (r.wood, "wood"),
        (r.clay, "clay"),
        (r.iron, "iron"),
        (r.crop, "crop"),
    ] {
        if amount > 0 {
            parts.push(format!("{amount} {name}"));
        }
    }
    if reward.culture > 0 {
        parts.push(format!("{} culture", reward.culture));
    }
    if let Some((unit, count)) = &reward.troops {
        parts.push(format!("{count}× {}", unit.0));
    }
    if parts.is_empty() {
        "none".to_owned()
    } else {
        parts.join(", ")
    }
}

/// Optional village selector for the multi-village pages (013 AC11): `?village=<id>` chooses which of
/// the player's villages to act on; absent ⇒ the capital / first village (single-village default).
/// The id rides as a **string** because the form/query decoder (`serde_urlencoded`) cannot parse the
/// 128-bit village id.
#[derive(Deserialize)]
pub struct VillageQuery {
    #[serde(default)]
    village: Option<String>,
}

/// The selected village as a domain id (server re-validates ownership in the use-case, P4). An
/// absent or unparseable id ⇒ `None` (the capital / first-village default).
fn selected_village(village: Option<&str>) -> Option<VillageId> {
    village
        .and_then(|s| s.trim().parse::<u128>().ok())
        .map(VillageId)
}

/// Redirect to `path`, preserving the selected village (013 AC11) so the user stays on that village.
fn redirect_with_village(path: &str, village: Option<&str>) -> Response {
    match village.and_then(|s| s.trim().parse::<u128>().ok()) {
        Some(id) => Redirect::to(&format!("{path}?village={id}")).into_response(),
        None => Redirect::to(path).into_response(),
    }
}

/// Redirect back to the village page, preserving the selected village so the user stays on it.
fn redirect_to_village(village: Option<&str>) -> Response {
    redirect_with_village("/village", village)
}

/// A short-lived, JS-readable cookie carrying a one-shot user-facing message (034). The `base.html`
/// banner reads it, shows it, and clears it.
const FLASH_COOKIE: &str = "flash";

/// Turn a use-case error's `Display` into a user-facing message, hiding internal storage errors (034).
/// The error strings are lowercase sentence fragments (e.g. "not enough resources"); capitalize the
/// first letter so the banner reads as a sentence.
fn user_msg(err: String) -> String {
    if err.starts_with("storage error") {
        return "Something went wrong — please try again.".to_owned();
    }
    let mut chars = err.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => err,
    }
}

/// Percent-encode a flash message so it is a valid cookie value (the JS reads it with
/// `decodeURIComponent`). Encodes everything outside the unreserved set.
fn pct_encode(s: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

/// Attach a one-shot flash message to a response (typically a redirect, 034) — the next page shows it.
/// `None` leaves the response unchanged (the action succeeded).
fn with_flash(resp: Response, msg: Option<String>) -> Response {
    let Some(msg) = msg else { return resp };
    let cookie = format!(
        "{FLASH_COOKIE}={}; Path=/; Max-Age=30; SameSite=Lax",
        pct_encode(&msg)
    );
    let mut resp = resp;
    if let Ok(value) = axum::http::HeaderValue::from_str(&cookie) {
        resp.headers_mut()
            .append(axum::http::header::SET_COOKIE, value);
    }
    resp
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
    headers: axum::http::HeaderMap,
    ConnectInfo(peer): ConnectInfo<std::net::SocketAddr>,
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
        Ok(user) => {
            // Capture the registration IP — the shared-IP detection key (022, P4 server-side) — for
            // every created account, whether or not email confirmation gates the first login.
            let ip = crate::client_ip(&headers, &peer.ip().to_string(), state.trust_proxy);
            if let Err(e) = state.accounts.record_registration_ip(user.id, &ip).await {
                tracing::error!(error = %e, "failed to record registration IP");
            }
            if state.require_email_confirmation {
                page(&LoginTemplate {
                    error: Some("Account created. Confirm your email, then log in.".to_owned()),
                })
            } else {
                let jar = jar.add(auth_cookie(user.id.0));
                (jar, Redirect::to("/village")).into_response()
            }
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
        now(),
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
        Err(LoginError::Abandoned) => page(&LoginTemplate {
            error: Some("This account has been retired after a long inactivity.".to_owned()),
        }),
        Err(LoginError::Sanctioned) => page(&LoginTemplate {
            error: Some(
                "This account is suspended or banned for a fair-play violation.".to_owned(),
            ),
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

/// A player's village with its live economy, switchable across all their villages (Player only —
/// AC3/AC4/AC7, 013 AC11). `?village=<id>` selects which to show; absent ⇒ the capital / first.
pub async fn village(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
    Query(q): Query<VillageQuery>,
) -> Response {
    let selected = selected_village(q.village.as_deref());
    let user = match state.accounts.find_user_by_id(player).await {
        Ok(Some(u)) => u,
        Ok(None) => return Redirect::to("/login").into_response(),
        Err(e) => {
            tracing::error!(error = %e, "lookup user failed");
            return server_error();
        }
    };

    // 017: lazily grant any achievements this player has newly earned (server-authoritative,
    // idempotent). Best-effort — a failure here must not break the village view.
    if let Err(e) = evaluate_achievements(
        state.accounts.as_ref(),
        state.rules.as_ref(),
        state.unit_rules.as_ref(),
        state.achievement_catalogue.as_ref(),
        player,
    )
    .await
    {
        tracing::error!(error = %e, "achievement evaluation failed");
    }

    // 018: lazily complete any onboarding quests now satisfied (server-authoritative, idempotent,
    // stage-gated). Best-effort — a failure here must not break the village view.
    if let Err(e) = evaluate_quests(
        state.accounts.as_ref(),
        state.rules.as_ref(),
        state.quest_chain.as_ref(),
        player,
    )
    .await
    {
        tracing::error!(error = %e, "quest evaluation failed");
    }

    // 019: this authenticated view is the activity signal (throttled), and the natural place to end
    // beginner's protection once the player is established. Best-effort.
    if let Err(e) = state.accounts.touch_activity(player, now()).await {
        tracing::error!(error = %e, "activity touch failed");
    }
    if let Err(e) = end_protection_if_established(
        state.accounts.as_ref(),
        state.rules.as_ref(),
        state.lifecycle_rules.as_ref(),
        player,
        now(),
    )
    .await
    {
        tracing::error!(error = %e, "protection threshold check failed");
    }
    // The remaining protection window, if any, for the view.
    let protected_until = match state.accounts.protection_of(player).await {
        Ok(p) => p,
        Err(e) => {
            tracing::error!(error = %e, "protection lookup failed");
            None
        }
    };

    // 020 AC8: the artifacts this player holds, with the holding village's coordinate.
    let held = state
        .accounts
        .held_by_player(player)
        .await
        .unwrap_or_default();
    let owned = state.accounts.villages_of(player).await.unwrap_or_default();
    let artifacts: Vec<ArtifactRowView> = held
        .iter()
        .map(|h| ArtifactRowView {
            label: artifact_label(&h.def),
            holder: owned
                .iter()
                .find(|v| v.id == h.holder)
                .map(|v| format!("({}|{})", v.coordinate.x, v.coordinate.y))
                .unwrap_or_default(),
        })
        .collect();

    let economy = match load_economy(
        state.accounts.as_ref(),
        state.rules.as_ref(),
        state.unit_rules.as_ref(),
        state.world.speed,
        now(),
        player,
        selected,
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

    // 031: the effect of the *next* level, so a player sees what an upgrade does — not just its cost. Pure
    // reads off the economy rules (scaled by world speed for production, to match the displayed rates).
    let econ = state.rules.as_ref();
    let speed = state.world.speed;
    let field_effect = |kind: ResourceKind, level: u8| -> String {
        let cur = econ.field_production_per_hour(kind, level, speed);
        let next = econ.field_production_per_hour(kind, level + 1, speed);
        let dpop = econ.field_population(level + 1) - econ.field_population(level);
        let mut s = format!("Production {cur} → {next}/h");
        if dpop != 0 {
            s.push_str(&format!(" · +{dpop} pop"));
        }
        s
    };
    // Effects for buildings whose rules live outside the economy (combat / trade / culture / build /
    // training). Read-only lookups; the village's tribe selects the (tribe-flavoured) Wall profile.
    let combat = state.combat_rules.as_ref();
    let merchants = state.merchant_rules.as_ref();
    let culture = state.culture_rules.as_ref();
    let training = &state.unit_rules.training;
    let building_effect = |kind: BuildingKind, level: u8| -> String {
        let next = level + 1;
        let special = match kind {
            BuildingKind::Warehouse => Some(format!(
                "Storage {} → {}",
                econ.warehouse_capacity(level),
                econ.warehouse_capacity(next)
            )),
            BuildingKind::Granary => Some(format!(
                "Crop storage {} → {}",
                econ.granary_capacity(level),
                econ.granary_capacity(next)
            )),
            BuildingKind::Outpost => Some(format!(
                "Holds {} → {} oases",
                econ.outpost_capacity(level),
                econ.outpost_capacity(next)
            )),
            BuildingKind::Wall => tribe.map(|t| {
                // One decimal: tribe wall bonuses differ by half-percent steps (e.g. Gaul 2.5 % vs
                // Teuton 2.0 %), which a whole-percent display would collapse.
                format!(
                    "Wall defence {:+.1}% → {:+.1}%",
                    combat.wall_bonus(t, level) * 100.0,
                    combat.wall_bonus(t, next) * 100.0
                )
            }),
            BuildingKind::Cranny => Some(format!(
                "Hides {} → {} of each resource",
                combat.cranny_capacity(level),
                combat.cranny_capacity(next)
            )),
            BuildingKind::Marketplace => Some(format!(
                "Merchants {} → {}",
                merchants.merchants_total(level),
                merchants.merchants_total(next)
            )),
            BuildingKind::TownHall => Some(format!(
                "Culture +{} → +{}/h",
                culture.town_hall_cp(level),
                culture.town_hall_cp(next)
            )),
            BuildingKind::Residence | BuildingKind::Palace => Some(format!(
                "Expansion slots {} → {}",
                expansion_slots(&[level], culture),
                expansion_slots(&[next], culture)
            )),
            BuildingKind::MainBuilding => Some(format!(
                "Build speed ×{:.2} → ×{:.2}",
                build_rules.main_building_factor(level),
                build_rules.main_building_factor(next)
            )),
            BuildingKind::Barracks | BuildingKind::Stable | BuildingKind::Workshop => {
                Some(format!(
                    "Training speed ×{:.2} → ×{:.2}",
                    training.building_factor(level),
                    training.building_factor(next)
                ))
            }
            _ => None,
        };
        let dpop =
            econ.building_population_at(kind, level + 1) - econ.building_population_at(kind, level);
        let pop = (dpop != 0).then(|| format!("+{dpop} pop"));
        [special, pop]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ")
    };

    let make_row = |table: &'static str,
                    slot: u8,
                    kind: &'static str,
                    label: String,
                    level: u8,
                    target: BuildTarget,
                    effect: String|
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
            // Blank at max (no next level to describe).
            effect: if at_max { String::new() } else { effect },
        }
    };

    // The capital may raise its resource fields past the normal cap (013 AC10); a non-capital stops
    // at the normal cap. The cost table extends to the capital cap, so gate the rows on `field_cap`.
    let field_cap = build_rules.field_max_level(village.is_capital);
    let fields = village
        .fields
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let slot = u8::try_from(i).unwrap_or(0);
            let mut row = make_row(
                "field",
                slot,
                "",
                format!("{} field #{slot}", resource_label(f.kind)),
                f.level,
                BuildTarget::Field { slot },
                field_effect(f.kind, f.level),
            );
            if f.level >= field_cap {
                // A non-capital field caps below the cost table's end (which runs to the capital cap), so
                // also blank the effect — there is no buildable next level here (AC1).
                row.at_max = true;
                row.can_order = false;
                row.effect = String::new();
            }
            row
        })
        .collect();

    let buildings = [
        BuildingKind::MainBuilding,
        BuildingKind::RallyPoint,
        BuildingKind::Warehouse,
        BuildingKind::Granary,
        BuildingKind::Marketplace,
        BuildingKind::Embassy,
        BuildingKind::Wall,
        BuildingKind::Cranny,
        BuildingKind::Outpost,
        BuildingKind::TownHall,
        BuildingKind::Residence,
        BuildingKind::Palace,
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
            building_effect(kind, level),
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
                MovementKind::OasisAttack => {
                    format!("Oasis attack on ({}|{})", m.destination.x, m.destination.y)
                }
                MovementKind::OasisReinforce => {
                    format!(
                        "Oasis reinforcement to ({}|{})",
                        m.destination.x, m.destination.y
                    )
                }
                MovementKind::Settle => {
                    format!("Settlers to ({}|{})", m.destination.x, m.destination.y)
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

    // The oases this village holds (012 AC12): their tile, bonus, and a recall action.
    let oases: Vec<OasisRow> = match state.accounts.occupied_oases(village.id).await {
        Ok(o) => o
            .iter()
            .map(|(c, b)| OasisRow {
                x: c.x,
                y: c.y,
                bonus: oasis_label(*b),
            })
            .collect(),
        Err(e) => {
            tracing::error!(error = %e, "occupied oases lookup failed");
            return server_error();
        }
    };

    // The village switcher: every owned village, the capital badged (013 AC11).
    let owned = match state.accounts.villages_of(player).await {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = %e, "villages lookup failed");
            return server_error();
        }
    };
    let switcher: Vec<VillageSwitchRow> = owned
        .iter()
        .map(|v| VillageSwitchRow {
            id: v.id.0.to_string(),
            label: format!("({}|{})", v.coordinate.x, v.coordinate.y),
            is_capital: v.is_capital,
            is_current: v.id == village.id,
        })
        .collect();

    // The player's pooled culture points + the expansion-slot gate (013 AC1/AC4/AC11).
    let culture = match load_culture(
        state.accounts.as_ref(),
        state.accounts.as_ref(),
        state.culture_rules.as_ref(),
        now(),
        player,
    )
    .await
    {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, "culture lookup failed");
            return server_error();
        }
    };
    let has_free_slot = culture.used_slots < culture.allowed_villages;

    // The shown village's loyalty, regenerated to now (014 AC1).
    let loyalty = match state.accounts.village_loyalty(village.id).await {
        Ok(Some((value, updated))) => regenerate_loyalty(
            value,
            (now().0 - updated.0) / 1000,
            state.loyalty_rules.as_ref(),
            state.world.speed,
        ),
        Ok(None) => state.loyalty_rules.starting_loyalty,
        Err(e) => {
            tracing::error!(error = %e, "loyalty lookup failed");
            return server_error();
        }
    };

    // The round-over notice (021 AC7) — best-effort; a lookup error must not break the village view.
    let world_won = matches!(state.accounts.world_ended().await, Ok(Some(_)));

    page(&VillageTemplate {
        username: user.username,
        world_won,
        is_wonder_site: village.is_wonder_site,
        village_id: village.id.0.to_string(),
        is_capital: village.is_capital,
        loyalty,
        villages: switcher,
        cp: culture.cp,
        cp_rate: culture.rate,
        slots_used: culture.used_slots,
        slots_allowed: culture.allowed_villages,
        next_threshold: culture.next_threshold,
        has_free_slot,
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
        oases,
        fields,
        buildings,
        protection: protection_notice(protected_until, now()),
        artifacts,
    })
}

/// A human label for a held artifact (020 AC8): "Speed (large) — ×2.0".
fn artifact_label(def: &eperica_domain::ArtifactDef) -> String {
    use eperica_domain::{ArtifactKind, ArtifactScope};
    let kind = match def.kind {
        ArtifactKind::Speed => "Speed",
        ArtifactKind::Storage => "Storage",
        ArtifactKind::Sustenance => "Sustenance",
        ArtifactKind::Trainer => "Trainer",
        ArtifactKind::Architect => "Architect",
        ArtifactKind::Eyes => "Eyes",
        ArtifactKind::Confuser => "Confuser",
        ArtifactKind::Fool => "Fool",
    };
    let scope = match def.scope {
        ArtifactScope::Small => "small",
        ArtifactScope::Large => "large",
        ArtifactScope::Unique => "unique",
    };
    format!("{kind} ({scope}) — ×{:.2}", def.magnitude)
}

/// A human notice of the remaining beginner's-protection window (019 AC9), or `None` once it has ended.
fn protection_notice(
    protected_until: Option<eperica_domain::Timestamp>,
    now: eperica_domain::Timestamp,
) -> Option<String> {
    let until = protected_until?;
    let remaining_secs = (until.0 - now.0) / 1000;
    if remaining_secs <= 0 {
        return None;
    }
    let label = if remaining_secs >= 86_400 {
        format!("{} day(s)", remaining_secs / 86_400)
    } else if remaining_secs >= 3_600 {
        format!("{} hour(s)", remaining_secs / 3_600)
    } else {
        format!("{} minute(s)", (remaining_secs / 60).max(1))
    };
    Some(format!(
        "Under beginner's protection — you cannot be attacked for about {label}."
    ))
}

/// The viewport half-extent: the map view shows a `(2·HALF + 1)`-square grid.
const MAP_HALF: i32 = 4;

/// Optional map-view center (defaults to the player's village). On the Rally Point, `village` also
/// selects which of the player's villages the troops are sent from (013 AC11).
#[derive(Deserialize)]
pub struct MapQuery {
    x: Option<i32>,
    y: Option<i32>,
    #[serde(default)]
    village: Option<String>,
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
    // The player's capital tile, distinguished on the map (013 AC9/AC11).
    let capital_coord = villages.iter().find(|v| v.is_capital).map(|v| v.coordinate);
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

    // Which oases in view are occupied, and by whom (012 AC12).
    let oasis_owners: std::collections::HashMap<Coordinate, String> =
        match state.accounts.oasis_owners_at(&coords).await {
            Ok(o) => o.into_iter().collect(),
            Err(e) => {
                tracing::error!(error = %e, "oasis owners lookup failed");
                return server_error();
            }
        };

    // 033: distances on the map are measured from the player's home (capital, else first village).
    let origin = capital_coord.or_else(|| villages.first().map(|v| v.coordinate));
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
                    let mut href = None;
                    if let Some(marker) = &cell.marker {
                        class.push_str(" map-grid__cell--village");
                        if marker.owner_name == user.username {
                            class.push_str(" map-grid__cell--self");
                        }
                        let is_capital = Some(coord) == capital_coord;
                        if is_capital {
                            class.push_str(" map-grid__cell--capital");
                        }
                        glyph = "★";
                        // 019 AC6: an inactive owner's village is farmable — grey it and flag it.
                        let inactive = is_inactive(
                            marker.owner_last_activity,
                            now(),
                            state.lifecycle_rules.inactive_after_secs,
                            state.world.speed,
                        );
                        if inactive {
                            class.push_str(" map-grid__cell--inactive");
                        }
                        // Alliance membership is public (§7.3): show the owner's alliance tag if any.
                        let tag = marker
                            .alliance_tag
                            .as_deref()
                            .map(|t| format!(" [{t}]"))
                            .unwrap_or_default();
                        // 025: the owner's presence, surfaced in the marker label.
                        let (online, presence_label) = presence_view(
                            marker.owner_last_activity,
                            now(),
                            state.lifecycle_rules.presence_online_secs,
                        );
                        let presence = if online {
                            " · online".to_owned()
                        } else {
                            format!(" · {presence_label}")
                        };
                        label = format!(
                            "{} — {}{}{}{}{} ({}|{})",
                            base_label,
                            marker.owner_name,
                            tag,
                            if is_capital { " (capital)" } else { "" },
                            if inactive { " (inactive)" } else { "" },
                            presence,
                            coord.x,
                            coord.y
                        );
                        // A send shortcut to another player's village (you can't target your own).
                        if marker.owner_name != user.username {
                            href = Some(format!("/village/rally?x={}&y={}", coord.x, coord.y));
                        }
                    } else if matches!(cell.tile, TileKind::Oasis(_)) {
                        // An oasis links to the Rally Point pre-filled with the tile (attack, or
                        // reinforce your own); its owner (if any) is shown in the label.
                        if let Some(owner) = oasis_owners.get(&coord) {
                            class.push_str(" map-grid__cell--occupied");
                            if owner == &user.username {
                                class.push_str(" map-grid__cell--self");
                            }
                            label =
                                format!("{base_label} — held by {owner} ({}|{})", coord.x, coord.y);
                        } else {
                            label =
                                format!("{base_label} — wild animals ({}|{})", coord.x, coord.y);
                        }
                        href = Some(format!("/village/rally?x={}&y={}", coord.x, coord.y));
                    }
                    // Distance from home (toroidal, rounded) — helps judge travel time at a glance.
                    if let Some(o) = origin {
                        let d = state.map.distance(o, coord);
                        if d >= 0.5 {
                            label.push_str(&format!(" · {} fields away", d.round() as i64));
                        }
                    }
                    MapCellView {
                        cell_class: class,
                        glyph,
                        label,
                        href,
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
    /// Which of the player's villages to build in (013 AC11); absent ⇒ capital / first.
    #[serde(default)]
    village: Option<String>,
}

/// Order an upgrade/construction for the selected village, then return to it (Player only, P4).
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
            None => return redirect_to_village(form.village.as_deref()),
        },
        _ => return redirect_to_village(form.village.as_deref()),
    };

    let flash = order_build(
        state.accounts.as_ref(),
        state.accounts.as_ref(),
        state.accounts.as_ref(),
        state.rules.as_ref(),
        state.build_rules.as_ref(),
        state.unit_rules.as_ref(),
        state.world.speed,
        now(),
        player,
        selected_village(form.village.as_deref()),
        target,
    )
    .await
    .err()
    .map(|e| {
        tracing::warn!(error = %e, "build order rejected");
        user_msg(e.to_string())
    });
    with_flash(redirect_to_village(form.village.as_deref()), flash)
}

fn role_label(role: UnitRole) -> &'static str {
    match role {
        UnitRole::Infantry => "Infantry",
        UnitRole::Cavalry => "Cavalry",
        UnitRole::Scout => "Scout",
        UnitRole::Siege => "Siege",
        UnitRole::Expansion => "Expansion",
        UnitRole::Wild => "Wild",
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

/// The selected village + settled amounts, or an error response (013 AC11; `selected` ⇒ that village).
async fn village_view_data(
    state: &AppState,
    player: eperica_domain::PlayerId,
    selected: Option<VillageId>,
) -> Result<(Village, ResourceAmounts), Response> {
    match load_economy(
        state.accounts.as_ref(),
        state.rules.as_ref(),
        state.unit_rules.as_ref(),
        state.world.speed,
        now(),
        player,
        selected,
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
pub async fn academy(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
    Query(q): Query<VillageQuery>,
) -> Response {
    let (village, amounts) =
        match village_view_data(&state, player, selected_village(q.village.as_deref())).await {
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
        village_id: village.id.0.to_string(),
        has_academy: building_level(&village, BuildingKind::Academy) > 0,
        rows,
        active,
    })
}

/// The Smithy: researched units with upgrade levels and actions (004 AC15; Player only, P4).
pub async fn smithy(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
    Query(q): Query<VillageQuery>,
) -> Response {
    let (village, amounts) =
        match village_view_data(&state, player, selected_village(q.village.as_deref())).await {
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
            // 031: the stat gain this upgrade grants (Smithy scales attack + defence per level).
            let effect = if cost.is_none() {
                String::new()
            } else {
                let stat = |base: u32, lvl: u8| {
                    (f64::from(base) * state.combat_rules.smithy_factor(lvl)).round() as u32
                };
                format!(
                    "Att {}→{} · Def {}/{}→{}/{}",
                    stat(spec.attack, level),
                    stat(spec.attack, level + 1),
                    stat(spec.defense_infantry, level),
                    stat(spec.defense_cavalry, level),
                    stat(spec.defense_infantry, level + 1),
                    stat(spec.defense_cavalry, level + 1),
                )
            };
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
                effect,
            }
        })
        .collect();

    page(&SmithyTemplate {
        village_id: village.id.0.to_string(),
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
    /// Which of the player's villages to act on (013 AC11); absent ⇒ capital / first.
    #[serde(default)]
    village: Option<String>,
}

/// Order a unit research for the player's village, then return to the Academy (Player only, P4).
pub async fn research_submit(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
    Form(form): Form<UnitForm>,
) -> Response {
    let flash = order_research(
        state.accounts.as_ref(),
        state.accounts.as_ref(),
        state.accounts.as_ref(),
        state.rules.as_ref(),
        state.unit_rules.as_ref(),
        state.world.speed,
        now(),
        player,
        selected_village(form.village.as_deref()),
        UnitId(form.unit),
    )
    .await
    .err()
    .map(|e| {
        tracing::warn!(error = %e, "research order rejected");
        user_msg(e.to_string())
    });
    with_flash(
        redirect_with_village("/village/academy", form.village.as_deref()),
        flash,
    )
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
    Query(q): Query<VillageQuery>,
) -> Response {
    let Some(building) = parse_troop_building(&building_slug) else {
        return Redirect::to("/village").into_response();
    };
    let (village, _amounts) =
        match village_view_data(&state, player, selected_village(q.village.as_deref())).await {
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
                time_secs: per_unit_time_secs(
                    spec.train_secs,
                    building_level,
                    &unit_rules.training,
                    state.world.speed,
                ),
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
        village_id: village.id.0.to_string(),
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
    /// Which of the player's villages to train in (013 AC11); absent ⇒ capital / first.
    #[serde(default)]
    village: Option<String>,
}

/// Order a training batch for the player's village, then return to the building page (Player
/// only, P4).
pub async fn train_submit(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
    Form(form): Form<TrainForm>,
) -> Response {
    let unit = UnitId(form.unit);
    let flash = order_train(
        state.accounts.as_ref(),
        state.accounts.as_ref(),
        state.accounts.as_ref(),
        state.accounts.as_ref(),
        state.rules.as_ref(),
        state.unit_rules.as_ref(),
        state.world.speed,
        now(),
        player,
        selected_village(form.village.as_deref()),
        unit.clone(),
        form.count,
    )
    .await
    .err()
    .map(|e| {
        tracing::warn!(error = %e, "training order rejected");
        user_msg(e.to_string())
    });
    // Land back on the unit's building page (the same kind across tribes), keeping the village.
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
    with_flash(
        redirect_with_village(target, form.village.as_deref()),
        flash,
    )
}

/// Order a Smithy upgrade for the player's village, then return to the Smithy (Player only, P4).
pub async fn smithy_upgrade_submit(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
    Form(form): Form<UnitForm>,
) -> Response {
    let flash = order_smithy_upgrade(
        state.accounts.as_ref(),
        state.accounts.as_ref(),
        state.accounts.as_ref(),
        state.rules.as_ref(),
        state.unit_rules.as_ref(),
        state.world.speed,
        now(),
        player,
        selected_village(form.village.as_deref()),
        UnitId(form.unit),
    )
    .await
    .err()
    .map(|e| {
        tracing::warn!(error = %e, "smithy upgrade rejected");
        user_msg(e.to_string())
    });
    with_flash(
        redirect_with_village("/village/smithy", form.village.as_deref()),
        flash,
    )
}

/// The Rally Point: the garrison troops that can be sent to reinforce (007 AC7; Player only, P4).
pub async fn rally(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
    Query(q): Query<MapQuery>,
) -> Response {
    let (village, _amounts) =
        match village_view_data(&state, player, selected_village(q.village.as_deref())).await {
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
            // For the army-power preview, count only units that fight the main battle: scouts + oasis
            // animals never do, and a ram's attack feeds the wall, not the field battle (009/010).
            let main_battle =
                spec.is_some_and(|s| !matches!(s.role, UnitRole::Scout | UnitRole::Wild));
            let counts_attack = main_battle
                && spec.is_some_and(|s| s.siege_kind != Some(eperica_domain::SiegeKind::Ram));
            RallyUnitRow {
                id: u.as_str().to_owned(),
                name: spec.map_or_else(|| u.as_str().to_owned(), |s| s.name.clone()),
                available: *n,
                speed: spec.map_or(0, |s| s.speed),
                carry: spec.map_or(0, |s| s.carry_capacity),
                attack: if counts_attack {
                    spec.map_or(0, |s| s.attack)
                } else {
                    0
                },
                def_inf: if main_battle {
                    spec.map_or(0, |s| s.defense_infantry)
                } else {
                    0
                },
                def_cav: if main_battle {
                    spec.map_or(0, |s| s.defense_cavalry)
                } else {
                    0
                },
            }
        })
        .collect();
    // Pre-fill the target from the map link, and flag when it is an oasis (so the form can hint
    // attack/reinforce instead of the village modes, 012 AC12).
    let target = match (q.x, q.y) {
        (Some(x), Some(y)) => Some(Coordinate::new(x, y)),
        _ => None,
    };
    let target_is_oasis = target.is_some_and(|c| state.map.oasis_bonus_at(c).is_some());
    // The Settle order is offered only with a free expansion slot (013 AC11).
    let can_settle = match load_culture(
        state.accounts.as_ref(),
        state.accounts.as_ref(),
        state.culture_rules.as_ref(),
        now(),
        player,
    )
    .await
    {
        Ok(c) => c.used_slots < c.allowed_villages,
        Err(e) => {
            tracing::error!(error = %e, "culture lookup failed");
            return server_error();
        }
    };
    page(&RallyTemplate {
        village_id: village.id.0.to_string(),
        units,
        target_x: q.x,
        target_y: q.y,
        target_is_oasis,
        can_settle,
        settlers_per_village: state.culture_rules.settlers_per_village,
        origin_x: village.coordinate.x,
        origin_y: village.coordinate.y,
        radius: i32::try_from(state.world.radius).unwrap_or(i32::MAX),
        speed_mult: state.world.speed.multiplier(),
    })
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
    let village = form.get("village").map(String::as_str);
    let selected = selected_village(village);
    let x = form.get("x").and_then(|s| s.trim().parse::<i32>().ok());
    let y = form.get("y").and_then(|s| s.trim().parse::<i32>().ok());
    let (Some(x), Some(y)) = (x, y) else {
        return redirect_with_village("/village/rally", village);
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
    // The building attached catapults aim at (011); ignored unless catapults are in the composition.
    let catapult_target = parse_building_kind(form.get("catapult_target").map(String::as_str));
    // The mode selects the use-case: reinforce (007) defends; attack/raid (009) fight; scout (010) spies.
    let flash = match form.get("mode").map(String::as_str) {
        Some("settle") => {
            // Found a new village: send the settler group to a free valley tile (013 AC6/AC11).
            order_settle(
                state.accounts.as_ref(),
                state.accounts.as_ref(),
                state.accounts.as_ref(),
                state.accounts.as_ref(),
                state.rules.as_ref(),
                state.unit_rules.as_ref(),
                state.culture_rules.as_ref(),
                state.map.as_ref(),
                state.world.speed,
                now(),
                player,
                selected,
                target,
            )
            .await
            .err()
            .map(|e| {
                tracing::warn!(error = %e, "settle order rejected");
                user_msg(e.to_string())
            })
        }
        Some("scout") => order_scout(
            state.accounts.as_ref(),
            state.accounts.as_ref(),
            state.accounts.as_ref(),
            state.rules.as_ref(),
            state.unit_rules.as_ref(),
            state.map.as_ref(),
            state.world.speed,
            now(),
            player,
            selected,
            target,
            troops,
            scout_target.unwrap_or(ScoutTarget::Defenses),
        )
        .await
        .err()
        .map(|e| {
            tracing::warn!(error = %e, "scout order rejected");
            user_msg(e.to_string())
        }),
        Some(mode @ ("attack" | "raid")) => {
            // An oasis tile (no village) routes to the oasis-attack use-case (012); a village tile
            // to the 009 attack/raid.
            if state.map.oasis_bonus_at(target).is_some() {
                order_oasis_attack(
                    state.accounts.as_ref(),
                    state.accounts.as_ref(),
                    state.accounts.as_ref(),
                    state.rules.as_ref(),
                    state.unit_rules.as_ref(),
                    state.map.as_ref(),
                    state.world.speed,
                    now(),
                    player,
                    selected,
                    target,
                    troops,
                )
                .await
                .err()
                .map(|e| {
                    tracing::warn!(error = %e, "oasis attack rejected");
                    user_msg(e.to_string())
                })
            } else {
                let mode = if mode == "raid" {
                    AttackMode::Raid
                } else {
                    AttackMode::Attack
                };
                order_attack(
                    state.accounts.as_ref(),
                    state.accounts.as_ref(),
                    state.accounts.as_ref(),
                    state.accounts.as_ref(),
                    state.rules.as_ref(),
                    state.unit_rules.as_ref(),
                    state.map.as_ref(),
                    state.world.speed,
                    now(),
                    player,
                    selected,
                    target,
                    troops,
                    mode,
                    scout_target,
                    catapult_target,
                )
                .await
                .err()
                .map(|e| {
                    tracing::warn!(error = %e, "attack order rejected");
                    user_msg(e.to_string())
                })
            }
        }
        _ => {
            // Reinforcing an oasis tile stations troops on it (012); a village tile defends it (007).
            if state.map.oasis_bonus_at(target).is_some() {
                order_oasis_reinforce(
                    state.accounts.as_ref(),
                    state.accounts.as_ref(),
                    state.accounts.as_ref(),
                    state.rules.as_ref(),
                    state.unit_rules.as_ref(),
                    state.map.as_ref(),
                    state.world.speed,
                    now(),
                    player,
                    selected,
                    target,
                    troops,
                )
                .await
                .err()
                .map(|e| {
                    tracing::warn!(error = %e, "oasis reinforcement rejected");
                    user_msg(e.to_string())
                })
            } else {
                order_reinforcement(
                    state.accounts.as_ref(),
                    state.accounts.as_ref(),
                    state.accounts.as_ref(),
                    state.rules.as_ref(),
                    state.unit_rules.as_ref(),
                    state.map.as_ref(),
                    state.world.speed,
                    now(),
                    player,
                    selected,
                    target,
                    troops,
                )
                .await
                .err()
                .map(|e| {
                    tracing::warn!(error = %e, "reinforcement order rejected");
                    user_msg(e.to_string())
                })
            }
        }
    };
    with_flash(redirect_to_village(village), flash)
}

/// Send-back form fields (the host village whose stationed troops to recall).
#[derive(Deserialize)]
pub struct RallyReturnForm {
    host: String,
    /// The village page to return to (013 AC11); absent ⇒ capital / first.
    #[serde(default)]
    village: Option<String>,
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
    let flash = order_return(
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
    .err()
    .map(|e| {
        tracing::warn!(error = %e, "return order rejected");
        user_msg(e.to_string())
    });
    with_flash(redirect_to_village(form.village.as_deref()), flash)
}

/// Recall form fields (the oasis tile to recall stationed troops from).
#[derive(Deserialize)]
pub struct OasisRecallForm {
    x: i32,
    y: i32,
    /// The village to recall the troops to (013 AC11); absent ⇒ capital / first.
    #[serde(default)]
    village: Option<String>,
}

/// Recall the player's troops stationed at one of their oases, then return to the village (012 AC7;
/// Player only, P4).
pub async fn oasis_recall(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
    Form(form): Form<OasisRecallForm>,
) -> Response {
    let target = Coordinate::new(form.x, form.y);
    let flash = order_oasis_recall(
        state.accounts.as_ref(),
        state.accounts.as_ref(),
        state.unit_rules.as_ref(),
        state.map.as_ref(),
        state.world.speed,
        now(),
        player,
        selected_village(form.village.as_deref()),
        target,
    )
    .await
    .err()
    .map(|e| {
        tracing::warn!(error = %e, "oasis recall rejected");
        user_msg(e.to_string())
    });
    with_flash(redirect_to_village(form.village.as_deref()), flash)
}

/// The Marketplace: the merchant pool (free/total + capacity) and a send-resources form (008 AC6;
/// Player only, P4).
pub async fn market(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
    Query(q): Query<VillageQuery>,
) -> Response {
    let (village, _amounts) =
        match village_view_data(&state, player, selected_village(q.village.as_deref())).await {
            Ok(v) => v,
            Err(r) => return r,
        };
    let village_id = village.id.0.to_string();
    let Some(tribe) = village.tribe else {
        tracing::error!(?player, "village has no tribe");
        return server_error();
    };
    let level = building_level(&village, BuildingKind::Marketplace);
    if level == 0 {
        return page(&MarketTemplate {
            village_id,
            has_marketplace: false,
            capacity: 0,
            free: 0,
            total: 0,
            merchant_speed: 0,
            origin_x: 0,
            origin_y: 0,
            radius: 0,
            speed_mult: 1.0,
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
    let profile = state.merchant_rules.profile(tribe);
    page(&MarketTemplate {
        village_id,
        has_marketplace: true,
        capacity: profile.capacity,
        free: total.saturating_sub(committed),
        total,
        merchant_speed: profile.speed,
        origin_x: village.coordinate.x,
        origin_y: village.coordinate.y,
        radius: i32::try_from(state.world.radius).unwrap_or(i32::MAX),
        speed_mult: state.world.speed.multiplier(),
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
    let village = form.get("village").map(String::as_str);
    let x = form.get("x").and_then(|s| s.trim().parse::<i32>().ok());
    let y = form.get("y").and_then(|s| s.trim().parse::<i32>().ok());
    let (Some(x), Some(y)) = (x, y) else {
        return redirect_with_village("/village/market", village);
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
    let flash = order_trade(
        state.accounts.as_ref(),
        state.accounts.as_ref(),
        state.rules.as_ref(),
        state.unit_rules.as_ref(),
        state.merchant_rules.as_ref(),
        state.map.as_ref(),
        state.world.speed,
        now(),
        player,
        selected_village(village),
        Coordinate::new(x, y),
        bundle,
    )
    .await
    .err()
    .map(|e| {
        tracing::warn!(error = %e, "trade order rejected");
        user_msg(e.to_string())
    });
    with_flash(redirect_to_village(village), flash)
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
    // 016 AC3/AC12: battles where the player **reinforced** an ally — their own report (the owner's
    // own defenses are already above as `defender_player`). Informational rows (no separate detail).
    let defended =
        match reinforcement_reports(state.accounts.as_ref(), &state.ranking_rules, player).await {
            Ok(d) => d,
            Err(e) => {
                tracing::error!(error = %e, "defender reports lookup failed");
                return server_error();
            }
        };
    rows.extend(defended.iter().filter(|d| !d.is_owner).map(|d| {
        let lost: u32 = d.losses.iter().map(|(_, n)| n).sum();
        ReportRow {
            when_ms: d.occurred_at.0,
            headline: format!(
                "Reinforced an allied defense (+{} defense points)",
                d.defense_points
            ),
            outcome: if lost == 0 {
                "No losses".to_owned()
            } else {
                format!("Lost {lost} troops")
            },
            href: String::new(),
        }
    }));
    rows.sort_by_key(|r| std::cmp::Reverse(r.when_ms));
    page(&ReportsTemplate { reports: rows })
}

/// Query for the public leaderboard page (016): category, region scope, time window.
#[derive(Deserialize)]
pub struct LeaderboardQuery {
    cat: Option<String>,
    scope: Option<String>,
    window: Option<String>,
}

/// The map-quadrant scope for a leaderboard query string.
fn parse_scope(s: &str) -> BoardScope {
    match s {
        "ne" => BoardScope::Quadrant(Quadrant::Ne),
        "nw" => BoardScope::Quadrant(Quadrant::Nw),
        "sw" => BoardScope::Quadrant(Quadrant::Sw),
        "se" => BoardScope::Quadrant(Quadrant::Se),
        _ => BoardScope::World,
    }
}

/// The time window for a leaderboard query string, validated against config (P7): an `"<n>d"` key is
/// honored only if `<n>` days is a configured window; anything else (incl. "all") is all-time. This
/// keeps the selector and the use-case in agreement, so a config change can never 500 the page.
fn parse_window(s: &str, rules: &eperica_domain::RankingRules) -> Window {
    let secs = s
        .strip_suffix('d')
        .and_then(|n| n.parse::<i64>().ok())
        .map(|days| days * 86_400);
    match secs {
        Some(secs) if rules.windows_secs.contains(&secs) => Window::Last(secs),
        _ => Window::AllTime,
    }
}

/// The window selector options, built from config (P7): "All-time" plus each configured window.
fn window_options(rules: &eperica_domain::RankingRules) -> Vec<(String, String)> {
    let mut out = vec![("all".to_owned(), "All-time".to_owned())];
    for secs in &rules.windows_secs {
        let days = secs / 86_400;
        out.push((format!("{days}d"), format!("{days} days")));
    }
    out
}

/// Map player leaderboard rows to view rows (rank + stat-page link + 025 presence indicator).
fn player_rows(
    rows: Vec<LeaderboardRow>,
    now: Timestamp,
    online_secs: i64,
) -> Vec<LeaderboardRowView> {
    rows.into_iter()
        .enumerate()
        .map(|(i, r)| {
            let (online, presence_label) = presence_view(r.last_activity, now, online_secs);
            LeaderboardRowView {
                rank: i + 1,
                name: r.name,
                tag: String::new(),
                href: format!("/stats/player/{}", r.player.0),
                value: r.value,
                has_presence: true,
                online,
                presence_label,
            }
        })
        .collect()
}

/// Map alliance leaderboard rows to view rows (rank + tag + stat-page link). Alliances have no
/// presence — `has_presence` is false so the template renders no indicator.
fn alliance_rows(rows: Vec<AllianceLeaderboardRow>) -> Vec<LeaderboardRowView> {
    rows.into_iter()
        .enumerate()
        .map(|(i, r)| LeaderboardRowView {
            rank: i + 1,
            name: r.name,
            tag: r.tag,
            href: format!("/stats/alliance/{}", r.alliance.0),
            value: r.value,
            has_presence: false,
            online: false,
            presence_label: String::new(),
        })
        .collect()
}

/// Public leaderboards (016 AC2/AC5/AC6/AC8): population / attackers / defenders / raiders + the
/// alliance aggregates, filterable by quadrant and (for conflict boards) time window.
pub async fn leaderboard(
    State(state): State<AppState>,
    Query(q): Query<LeaderboardQuery>,
) -> Response {
    let scope_key = q.scope.unwrap_or_else(|| "world".to_owned());
    let window_key = q.window.unwrap_or_else(|| "all".to_owned());
    let scope = parse_scope(&scope_key);
    let repo = state.accounts.as_ref();
    let econ = state.rules.as_ref();
    let rules = state.ranking_rules.as_ref();
    let window = parse_window(&window_key, rules);
    let now_ts = now();
    let online_secs = state.lifecycle_rules.presence_online_secs;

    let categories = vec![
        ("population", "Population"),
        ("attackers", "Top attackers"),
        ("defenders", "Top defenders"),
        ("raiders", "Top raiders"),
        ("climbers", "Top climbers"),
        ("alliances", "Alliances"),
        ("alliance-atk", "Alliance attack"),
        ("alliance-def", "Alliance defense"),
    ];
    let cat = q.cat.unwrap_or_else(|| "population".to_owned());
    let category = if categories.iter().any(|(k, _)| *k == cat) {
        cat
    } else {
        "population".to_owned()
    };

    let (result, value_label, is_alliance, windowed): (Result<_, _>, &str, bool, bool) =
        match category.as_str() {
            "attackers" => (
                conflict_leaderboard(repo, rules, ConflictMetric::Attack, scope, window, now_ts)
                    .await
                    .map(|r| player_rows(r, now_ts, online_secs)),
                "Attack points",
                false,
                true,
            ),
            "defenders" => (
                conflict_leaderboard(repo, rules, ConflictMetric::Defense, scope, window, now_ts)
                    .await
                    .map(|r| player_rows(r, now_ts, online_secs)),
                "Defense points",
                false,
                true,
            ),
            "raiders" => (
                conflict_leaderboard(repo, rules, ConflictMetric::Raided, scope, window, now_ts)
                    .await
                    .map(|r| player_rows(r, now_ts, online_secs)),
                "Resources looted",
                false,
                true,
            ),
            "climbers" => (
                climbers_leaderboard(repo, rules, scope)
                    .await
                    .map(|r| player_rows(r, now_ts, online_secs)),
                "Population gained",
                false,
                false,
            ),
            "alliances" => (
                alliance_population_leaderboard(repo, econ, rules, scope)
                    .await
                    .map(alliance_rows),
                "Population",
                true,
                false,
            ),
            "alliance-atk" => (
                alliance_conflict_leaderboard(
                    repo,
                    rules,
                    ConflictMetric::Attack,
                    scope,
                    window,
                    now_ts,
                )
                .await
                .map(alliance_rows),
                "Attack points",
                true,
                true,
            ),
            "alliance-def" => (
                alliance_conflict_leaderboard(
                    repo,
                    rules,
                    ConflictMetric::Defense,
                    scope,
                    window,
                    now_ts,
                )
                .await
                .map(alliance_rows),
                "Defense points",
                true,
                true,
            ),
            _ => (
                population_leaderboard(repo, econ, rules, scope)
                    .await
                    .map(|r| player_rows(r, now_ts, online_secs)),
                "Population",
                false,
                false,
            ),
        };
    let rows = match result {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "leaderboard query failed");
            return server_error();
        }
    };
    page(&LeaderboardTemplate {
        category,
        categories,
        scope: scope_key,
        scopes: vec![
            ("world", "World"),
            ("ne", "NE"),
            ("nw", "NW"),
            ("sw", "SW"),
            ("se", "SE"),
        ],
        window: window_key,
        windows: window_options(rules),
        is_alliance,
        windowed,
        value_label,
        rows,
    })
}

/// The Wonder-of-the-World race page (021 AC9): the alliances by Wonder level, plus the winner banner
/// once the round is won.
pub async fn wonder(State(state): State<AppState>) -> Response {
    let repo = state.accounts.as_ref();
    let winner = match repo.world_ended().await {
        Ok(Some(outcome)) => match repo.alliance_summary(outcome.winner).await {
            Ok(summary) => summary,
            Err(e) => {
                tracing::error!(error = %e, "winner alliance lookup failed");
                return server_error();
            }
        },
        Ok(None) => None,
        Err(e) => {
            tracing::error!(error = %e, "world-ended lookup failed");
            return server_error();
        }
    };
    let standings = match repo.top_wonders().await {
        Ok(s) => s
            .into_iter()
            .enumerate()
            .map(|(i, s)| WonderStandingView {
                rank: i + 1,
                name: s.name,
                tag: s.tag,
                level: s.level,
            })
            .collect(),
        Err(e) => {
            tracing::error!(error = %e, "wonder standings query failed");
            return server_error();
        }
    };
    page(&WonderTemplate {
        winner,
        max_level: eperica_domain::MAX_WONDER_LEVEL,
        standings,
    })
}

/// Order one level of Wonder construction on a controlled site (021 AC4) — the only path that builds a
/// Wonder; gating (site control + alliance holds a plan + level < 100) is server-side.
pub async fn wonder_build_submit(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
    Form(form): Form<WonderBuildForm>,
) -> Response {
    let flash = order_wonder_build(
        state.accounts.as_ref(),
        state.accounts.as_ref(),
        state.accounts.as_ref(),
        state.accounts.as_ref(),
        state.accounts.as_ref(),
        state.rules.as_ref(),
        state.build_rules.as_ref(),
        state.unit_rules.as_ref(),
        state.world.speed,
        now(),
        player,
        selected_village(form.village.as_deref()),
    )
    .await
    .err()
    .map(|e| {
        tracing::warn!(error = %e, "Wonder build order rejected");
        user_msg(e.to_string())
    });
    with_flash(redirect_to_village(form.village.as_deref()), flash)
}

/// The Wonder-build form: which controlled site village to build the Wonder in (013 AC11).
#[derive(Deserialize)]
pub struct WonderBuildForm {
    #[serde(default)]
    village: Option<String>,
}

/// The moderator review queue (022 AC3/AC9) — open reports, oldest first. Moderator-gated.
pub async fn mod_queue(State(state): State<AppState>, AuthUser(player): AuthUser) -> Response {
    let reports = match review_queue(
        state.accounts.as_ref(),
        state.accounts.as_ref(),
        player,
        100,
    )
    .await
    {
        Ok(r) => r,
        Err(ModerationError::NotAuthorized) => return forbidden(),
        Err(e) => {
            tracing::error!(error = %e, "review queue failed");
            return server_error();
        }
    };
    let rows = reports
        .into_iter()
        .map(|r| ModReportRow {
            id: r.id.to_string(),
            reporter_name: r.reporter_name,
            subject_id: r.subject.0.to_string(),
            subject_name: r.subject_name,
            reason: report_reason_label(r.reason),
            note: r.note,
        })
        .collect();
    page(&ModQueueTemplate { reports: rows })
}

/// The moderator account-inspect page (022 AC7/AC9) — sanction status + detection signals. Gated.
pub async fn mod_account(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
    Path(id): Path<String>,
) -> Response {
    let Some(subject) = id.trim().parse::<u128>().ok().map(PlayerId) else {
        return (StatusCode::BAD_REQUEST, "invalid account id").into_response();
    };
    let signals = match account_signals(
        state.accounts.as_ref(),
        state.accounts.as_ref(),
        state.fair_play_rules.as_ref(),
        player,
        subject,
    )
    .await
    {
        Ok(s) => s,
        Err(ModerationError::NotAuthorized) => return forbidden(),
        Err(e) => {
            tracing::error!(error = %e, "account signals failed");
            return server_error();
        }
    };
    let user = match state.accounts.find_user_by_id(subject).await {
        Ok(Some(u)) => u,
        Ok(None) => return server_error(),
        Err(e) => {
            tracing::error!(error = %e, "subject lookup failed");
            return server_error();
        }
    };
    let now = now();
    page(&ModAccountTemplate {
        subject_id: subject.0.to_string(),
        username: user.username,
        banned: user.banned_at.is_some(),
        suspended: user.suspended_until.is_some_and(|u| now.0 < u.0),
        ip_association_count: signals.ip_association_count,
        shared_ip_flagged: signals.shared_ip_flagged,
        peak_action_count: signals.peak_action_count,
        inhuman_action_rate: signals.inhuman_action_rate,
    })
}

/// Resolve a report (+ optional sanction) from the queue (022 AC4). Moderator-gated.
pub async fn mod_resolve_submit(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
    Form(form): Form<ResolveForm>,
) -> Response {
    let Some(report_id) = form.report_id.trim().parse::<u128>().ok() else {
        return Redirect::to("/mod").into_response();
    };
    let sanction = form
        .sanction
        .as_deref()
        .filter(|s| !s.is_empty())
        .and_then(SanctionKind::parse);
    match resolve_report(
        state.accounts.as_ref(),
        state.accounts.as_ref(),
        state.fair_play_rules.as_ref(),
        player,
        report_id,
        now(),
        &form.resolution,
        sanction,
        None,
    )
    .await
    {
        Ok(_) => Redirect::to("/mod").into_response(),
        Err(ModerationError::NotAuthorized) => forbidden(),
        Err(e) => {
            tracing::error!(error = %e, "resolve report failed");
            server_error()
        }
    }
}

/// Apply a sanction directly from the account-inspect page (022 AC4). Moderator-gated.
pub async fn mod_sanction_submit(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
    Form(form): Form<SanctionForm>,
) -> Response {
    let Some(subject) = form.subject.trim().parse::<u128>().ok().map(PlayerId) else {
        return Redirect::to("/mod").into_response();
    };
    let Some(kind) = SanctionKind::parse(&form.kind) else {
        return Redirect::to(&format!("/mod/account/{}", subject.0)).into_response();
    };
    match sanction_account(
        state.accounts.as_ref(),
        state.accounts.as_ref(),
        state.fair_play_rules.as_ref(),
        player,
        subject,
        now(),
        kind,
        None,
    )
    .await
    {
        Ok(()) => Redirect::to(&format!("/mod/account/{}", subject.0)).into_response(),
        Err(ModerationError::NotAuthorized) => forbidden(),
        Err(e) => {
            tracing::error!(error = %e, "sanction failed");
            server_error()
        }
    }
}

/// The admin console (036) — read-only world/server status + account role administration. Admin-gated on
/// the **real** human (`RealUser`, so admin powers are never delegated through a 030 sit): a non-admin
/// gets 403, a visitor is redirected to `/login`. An optional `?q=` searches accounts (028) so an admin
/// can manage the roles of *any* account, not only the recent listing.
pub async fn admin(
    State(state): State<AppState>,
    RealUser(player): RealUser,
    Query(q): Query<AdminQuery>,
) -> Response {
    let overview =
        match admin_overview(state.accounts.as_ref(), state.accounts.as_ref(), player).await {
            Ok(o) => o,
            Err(AdminError::NotAuthorized) => return admin_forbidden(),
            Err(e) => {
                tracing::error!(error = %e, "admin overview failed");
                return server_error();
            }
        };
    let query = q.q.unwrap_or_default();
    let trimmed = query.trim();
    let searched = !trimmed.is_empty();
    // A search lists matching accounts (any account); otherwise the recent-accounts listing.
    let listing = if searched {
        admin_search_accounts(
            state.accounts.as_ref(),
            state.accounts.as_ref(),
            player,
            trimmed,
            50,
        )
        .await
    } else {
        admin_list_accounts(
            state.accounts.as_ref(),
            state.accounts.as_ref(),
            player,
            100,
        )
        .await
    };
    let accounts = match listing {
        Ok(a) => a,
        Err(AdminError::NotAuthorized) => return forbidden(),
        Err(e) => {
            tracing::error!(error = %e, "admin account listing failed");
            return server_error();
        }
    };
    let rows = accounts
        .into_iter()
        .map(|a| AdminAccountRow {
            id: a.id.0.to_string(),
            username: a.username,
            is_moderator: a.is_moderator,
            is_admin: a.is_admin,
            abandoned: a.abandoned,
            is_self: a.id == player,
        })
        .collect();
    // The worlds the registry runs (041 AC3).
    let worlds =
        match admin_list_worlds(state.accounts.as_ref(), state.accounts.as_ref(), player).await {
            Ok(w) => w
                .into_iter()
                .map(|w| AdminWorldRow {
                    id: w.id.0.to_string(),
                    speed: w.speed,
                    radius: w.radius,
                    created_ms: w.created_ms,
                    won: w.won_ms.is_some(),
                    is_home: w.id == state.world_id,
                })
                .collect(),
            Err(e) => {
                tracing::error!(error = %e, "admin worlds listing failed");
                Vec::new()
            }
        };
    page(&AdminTemplate {
        speed: overview.speed,
        radius: overview.radius,
        seed: overview.seed,
        created_ms: overview.created_ms,
        artifact_release_ms: overview.artifact_release_ms,
        wonder_release_ms: overview.wonder_release_ms,
        won_ms: overview.won_ms,
        accounts: overview.accounts,
        villages: overview.villages,
        pending_events: overview.pending_events,
        max_radius: eperica_application::MAX_WORLD_RADIUS,
        worlds,
        query: trimmed.to_owned(),
        searched,
        rows,
    })
}

/// The create-world form (041 AC1).
#[derive(Deserialize)]
pub struct CreateWorldForm {
    speed: f64,
    radius: u32,
}

/// Create a new world from the admin console and start it running live (041 AC1/AC2). Admin-gated on the
/// real human; the new world's scheduler is started through the registry — no restart.
pub async fn admin_world_submit(
    State(state): State<AppState>,
    RealUser(player): RealUser,
    Form(form): Form<CreateWorldForm>,
) -> Response {
    match admin_create_world_uc(
        state.accounts.as_ref(),
        state.accounts.as_ref(),
        player,
        form.speed,
        form.radius,
    )
    .await
    {
        Ok(world_id) => {
            // Start the new world's scheduler live (AC2). The row exists regardless; if the spawn fails it
            // will start on the next restart (the registry loads all worlds at startup).
            let flash = match state.world_registry.start_world(world_id).await {
                Ok(()) => "World created and started.".to_owned(),
                Err(e) => {
                    tracing::error!(error = %e, "world created but scheduler failed to start");
                    "World created; its scheduler will start on the next restart.".to_owned()
                }
            };
            with_flash(Redirect::to("/admin").into_response(), Some(flash))
        }
        Err(AdminError::NotAuthorized) => admin_forbidden(),
        Err(e) => {
            tracing::warn!(error = %e, "create world rejected");
            with_flash(
                Redirect::to("/admin").into_response(),
                Some(user_msg(e.to_string())),
            )
        }
    }
}

/// The admin console search query (036 AC3).
#[derive(Deserialize)]
pub struct AdminQuery {
    #[serde(default)]
    q: Option<String>,
}

/// The admin role-change form (036 AC3): grant/revoke Moderator or Administrator on a target account.
#[derive(Deserialize)]
pub struct AdminRoleForm {
    target: String,
    /// `"moderator"` or `"admin"`.
    role: String,
    grant: bool,
}

/// Grant/revoke an elevated role from the admin console (036 AC3). Admin-gated on the **real** human
/// (`RealUser`); the gate runs before malformed-input handling so a non-admin never learns the input was
/// parsed. The self-demotion guard and not-found surface as a flash on `/admin`.
pub async fn admin_role_submit(
    State(state): State<AppState>,
    RealUser(player): RealUser,
    Form(form): Form<AdminRoleForm>,
) -> Response {
    if let Err(AdminError::NotAuthorized) = require_admin(state.accounts.as_ref(), player).await {
        return admin_forbidden();
    }
    let (Some(role), Ok(target)) = (
        ElevatedRole::from_slug(&form.role),
        form.target.trim().parse::<u128>(),
    ) else {
        return Redirect::to("/admin").into_response();
    };
    match admin_set_role_uc(
        state.accounts.as_ref(),
        state.accounts.as_ref(),
        state.accounts.as_ref(),
        player,
        PlayerId(target),
        role,
        form.grant,
    )
    .await
    {
        Ok(()) => Redirect::to("/admin").into_response(),
        Err(AdminError::NotAuthorized) => admin_forbidden(),
        Err(e) => {
            tracing::warn!(error = %e, "admin role change rejected");
            with_flash(
                Redirect::to("/admin").into_response(),
                Some(user_msg(e.to_string())),
            )
        }
    }
}

/// A player reports another account (022 AC2). Redirects back to the subject's stats page.
pub async fn report_submit(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
    Form(form): Form<ReportForm>,
) -> Response {
    let Some(subject) = form.subject.trim().parse::<u128>().ok().map(PlayerId) else {
        return Redirect::to("/leaderboard").into_response();
    };
    let reason = ReportReason::parse(&form.reason).unwrap_or(ReportReason::Other);
    let note = form.note.unwrap_or_default();
    let flash = file_report(state.accounts.as_ref(), player, subject, reason, &note)
        .await
        .err()
        .map(|e| {
            // A self-report or backend error is non-fatal — just return to the page.
            tracing::warn!(error = %e, "report rejected");
            user_msg(e.to_string())
        });
    with_flash(
        Redirect::to(&format!("/stats/player/{}", subject.0)).into_response(),
        flash,
    )
}

/// Label a report reason for display.
fn report_reason_label(reason: ReportReason) -> String {
    match reason {
        ReportReason::Pushing => "Pushing / multi-account",
        ReportReason::Botting => "Botting",
        ReportReason::Abuse => "Abuse",
        ReportReason::Other => "Other",
    }
    .to_owned()
}

/// The resolve-report form (022 AC4).
#[derive(Deserialize)]
pub struct ResolveForm {
    report_id: String,
    resolution: String,
    #[serde(default)]
    sanction: Option<String>,
}

/// The direct-sanction form (022 AC4).
#[derive(Deserialize)]
pub struct SanctionForm {
    subject: String,
    kind: String,
}

/// The player report form (022 AC2).
#[derive(Deserialize)]
pub struct ReportForm {
    subject: String,
    reason: String,
    #[serde(default)]
    note: Option<String>,
}

/// Render a presence value (025) as an `(online, label)` pair for templates: `(true, "online")` when
/// the player acted within the configured window, else `(false, "last seen …")` with a coarse,
/// human-friendly age. Time math is on the pure `Timestamp` ms values; no wall-clock formatting.
fn presence_view(last_activity: Timestamp, now: Timestamp, online_secs: i64) -> (bool, String) {
    match presence(last_activity, now, online_secs) {
        Presence::Online => (true, "online".to_owned()),
        Presence::LastSeen(seen) => {
            let secs = ((now.0 - seen.0) / 1000).max(0);
            let label = if secs < 3600 {
                format!("last seen {}m ago", (secs / 60).max(1))
            } else if secs < 86_400 {
                format!("last seen {}h ago", secs / 3600)
            } else {
                format!("last seen {}d ago", secs / 86_400)
            };
            (false, label)
        }
    }
}

/// The search query string.
#[derive(Deserialize)]
pub struct SearchQuery {
    #[serde(default)]
    q: Option<String>,
}

/// Public who-is search (028): players (username prefix), alliances (name/tag prefix), and a coordinate
/// jump. A public read — no login required.
pub async fn search_page(State(state): State<AppState>, Query(sq): Query<SearchQuery>) -> Response {
    let query = sq.q.unwrap_or_default();
    let trimmed = query.trim().to_owned();
    if trimmed.is_empty() {
        return page(&SearchTemplate {
            query,
            searched: false,
            players: Vec::new(),
            alliances: Vec::new(),
            coordinate_href: None,
            coordinate_label: String::new(),
        });
    }
    let results = match search(state.accounts.as_ref(), state.accounts.as_ref(), &trimmed).await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "search failed");
            return server_error();
        }
    };
    let players = results
        .players
        .into_iter()
        .map(|p| SearchHitRow {
            href: format!("/stats/player/{}", p.player.0),
            label: p.name,
        })
        .collect();
    let alliances = results
        .alliances
        .into_iter()
        .map(|a| SearchHitRow {
            href: format!("/stats/alliance/{}", a.alliance.0),
            label: format!("{} [{}]", a.name, a.tag),
        })
        .collect();
    let (coordinate_href, coordinate_label) = match results.coordinate {
        Some(c) => (
            Some(format!("/map?x={}&y={}", c.x, c.y)),
            format!("({}|{})", c.x, c.y),
        ),
        None => (None, String::new()),
    };
    page(&SearchTemplate {
        query,
        searched: true,
        players,
        alliances,
        coordinate_href,
        coordinate_label,
    })
}

/// Public player statistics page (016 AC9).
pub async fn player_stats_page(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    let Ok(pid) = id.parse::<u128>() else {
        return not_found();
    };
    let repo = state.accounts.as_ref();
    let s = match player_statistics(repo, state.rules.as_ref(), PlayerId(pid)).await {
        Ok(Some(s)) => s,
        Ok(None) => return not_found(),
        Err(e) => {
            tracing::error!(error = %e, "player stats failed");
            return server_error();
        }
    };
    // 017: medals, achievements, and population history (all public, derived from persisted state).
    let medals = match repo.medals_for(MedalSubjectKind::Player, pid).await {
        Ok(m) => m,
        Err(e) => {
            tracing::error!(error = %e, "player medals failed");
            return server_error();
        }
    };
    let held = match repo.held_achievements(PlayerId(pid)).await {
        Ok(h) => h,
        Err(e) => {
            tracing::error!(error = %e, "player achievements failed");
            return server_error();
        }
    };
    let history = match population_history(repo, PlayerId(pid)).await {
        Ok(h) => h,
        Err(e) => {
            tracing::error!(error = %e, "player history failed");
            return server_error();
        }
    };
    // 025: bio + presence, derived from the account's profile and last activity.
    let profile = match view_profile(repo, PlayerId(pid)).await {
        Ok(p) => p,
        Err(e) => {
            tracing::error!(error = %e, "player profile failed");
            return server_error();
        }
    };
    let (online, presence_label) = presence_view(
        profile.last_activity,
        now(),
        state.lifecycle_rules.presence_online_secs,
    );
    let mut achievements: Vec<AchievementRowView> = held
        .iter()
        .map(|a| AchievementRowView {
            label: achievement_label(&a.0).to_owned(),
        })
        .collect();
    achievements.sort_by(|a, b| a.label.cmp(&b.label));
    page(&PlayerStatsTemplate {
        subject_id: pid.to_string(),
        name: s.name,
        bio: profile.bio,
        online,
        presence_label,
        population: s.population,
        attack_points: s.attack_points,
        defense_points: s.defense_points,
        loot_total: s.loot_total,
        villages: s
            .villages
            .into_iter()
            .map(|(_, c, population)| VillageStatRow {
                x: c.x,
                y: c.y,
                population,
            })
            .collect(),
        medals: medals
            .into_iter()
            .map(|m| MedalRowView {
                category: medal_label(m.category).to_owned(),
                rank: m.rank,
                period: m.period,
            })
            .collect(),
        achievements,
        history: history
            .into_iter()
            .map(|(period, population)| HistoryPointView { period, population })
            .collect(),
    })
}

/// The bio-edit form (025 AC2, owner only).
#[derive(Deserialize)]
pub struct BioForm {
    bio: String,
}

/// The signed-in player's own profile page (025, Player only): renders the editable bio. The public
/// view lives on `/stats/player/{id}`; this page is purely for self-editing.
pub async fn profile_page(State(state): State<AppState>, AuthUser(player): AuthUser) -> Response {
    match view_profile(state.accounts.as_ref(), player).await {
        Ok(p) => page(&ProfileTemplate { bio: p.bio }),
        Err(e) => {
            tracing::error!(error = %e, "profile load failed");
            server_error()
        }
    }
}

/// Save the signed-in player's bio (025 AC2, owner-scoped, P4). Validation (length) lives in the pure
/// domain via `edit_bio`; an invalid bio re-renders the form rather than persisting.
pub async fn profile_bio_submit(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
    Form(form): Form<BioForm>,
) -> Response {
    match edit_bio(state.accounts.as_ref(), player, &form.bio).await {
        Ok(()) => Redirect::to("/profile").into_response(),
        Err(eperica_application::ProfileError::Invalid) => {
            // Re-render with the rejected text so the player can fix it.
            page(&ProfileTemplate { bio: form.bio })
        }
        Err(e) => {
            tracing::error!(error = %e, "bio save failed");
            server_error()
        }
    }
}

/// The player's settings page (029, Player only) — currently per-kind notification preferences.
pub async fn settings_page(State(state): State<AppState>, AuthUser(player): AuthUser) -> Response {
    match notification_settings(state.accounts.as_ref(), player).await {
        Ok(prefs) => page(&SettingsTemplate {
            notifications: prefs
                .into_iter()
                .map(|(kind, enabled)| SettingsToggleRow {
                    token: kind.as_str().to_owned(),
                    label: kind.label().to_owned(),
                    enabled,
                })
                .collect(),
        }),
        Err(e) => {
            tracing::error!(error = %e, "settings load failed");
            server_error()
        }
    }
}

/// Save notification preferences (029 AC2, owner-scoped). A checkbox present in the form ⇒ that kind is
/// enabled; absent ⇒ muted. Iterates every kind so unchecking is honoured.
pub async fn settings_notifications_submit(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
    Form(form): Form<std::collections::HashMap<String, String>>,
) -> Response {
    for kind in eperica_domain::NotificationKind::ALL {
        let enabled = form.contains_key(kind.as_str());
        if let Err(e) = set_notification_pref(state.accounts.as_ref(), player, kind, enabled).await
        {
            tracing::error!(error = %e, "notification pref save failed");
            return server_error();
        }
    }
    Redirect::to("/settings").into_response()
}

// ---- Account sitting (030) ----

/// A coarse "how long ago" label for an audit timestamp (030 AC5).
fn ago(then_ms: i64, now_ms: i64) -> String {
    let secs = ((now_ms - then_ms) / 1000).max(0);
    if secs < 60 {
        "just now".to_owned()
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86_400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86_400)
    }
}

/// The owner's name if the player is currently (and validly) sitting — from the sit cookie + a live
/// authorisation check. Shared by the page + the status poll.
async fn sit_owner_name(state: &AppState, jar: &PrivateCookieJar, me: PlayerId) -> Option<String> {
    let owner = PlayerId(
        jar.get(crate::auth::SIT_COOKIE)?
            .value()
            .parse::<u128>()
            .ok()?,
    );
    match authorize_sit(state.accounts.as_ref(), owner, me, now()).await {
        Ok(true) => view_profile(state.accounts.as_ref(), owner)
            .await
            .ok()
            .map(|p| p.name),
        _ => None,
    }
}

/// The account-sitting page (030): your sitters, the accounts you sit for, and your audit log. Always
/// operates on the **real** logged-in player (`RealUser`), even mid-sit.
pub async fn sitting_page(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    RealUser(me): RealUser,
) -> Response {
    let repo = state.accounts.as_ref();
    let to_rows = |hits: Vec<PlayerHit>| {
        hits.into_iter()
            .map(|h| SitterRow {
                id: h.player.0.to_string(),
                name: h.name,
            })
            .collect::<Vec<_>>()
    };
    let my_sitters = match list_sitters(repo, me).await {
        Ok(s) => to_rows(s),
        Err(e) => {
            tracing::error!(error = %e, "list sitters failed");
            return server_error();
        }
    };
    let sitting_for = match list_sitting_for(repo, me).await {
        Ok(s) => to_rows(s),
        Err(e) => {
            tracing::error!(error = %e, "list sitting-for failed");
            return server_error();
        }
    };
    let now_ms = now().0;
    let audit = match sitter_log(repo, me).await {
        Ok(log) => log
            .into_iter()
            .map(|a| AuditRow {
                sitter: a.sitter_name,
                action: a.action,
                when: ago(a.created_ms, now_ms),
            })
            .collect(),
        Err(e) => {
            tracing::error!(error = %e, "sitter log failed");
            return server_error();
        }
    };
    page(&SittingTemplate {
        my_sitters,
        sitting_for,
        audit,
        currently_sitting: sit_owner_name(&state, &jar, me).await,
    })
}

/// Nav probe (035) — who the viewer is, for the topbar: whether logged in, a moderator, and (036) an
/// administrator. Best-effort and reachable by visitors (returns `authed:false`), so the JS can render
/// the right link set without threading auth state through every page template. Excluded from
/// presence-touch.
pub async fn me(
    State(state): State<AppState>,
    MaybeAuthUser(effective): MaybeAuthUser,
    MaybeRealUser(real): MaybeRealUser,
) -> Response {
    use eperica_application::AccountRepository;
    // Moderator follows the *effective* player (035 — sitting a moderator lets you moderate as them).
    // Admin follows the *real* human only (036 — admin grants persist, so they are never delegated
    // through a sit; matches the `RealUser`-gated console).
    let eff_rec = match effective {
        Some(p) => state.accounts.find_user_by_id(p).await.ok().flatten(),
        None => None,
    };
    let admin = match real {
        Some(p) if Some(p) == effective => eff_rec.as_ref().is_some_and(|u| u.is_admin),
        Some(p) => state
            .accounts
            .find_user_by_id(p)
            .await
            .ok()
            .flatten()
            .is_some_and(|u| u.is_admin),
        None => false,
    };
    let authed = effective.is_some();
    let moderator = eff_rec.as_ref().is_some_and(|u| u.is_moderator);
    axum::Json(serde_json::json!({ "authed": authed, "moderator": moderator, "admin": admin }))
        .into_response()
}

/// Live sitting status (030) — the owner's name when actively sitting, else empty. Drives the persistent
/// banner; excluded from presence-touch. Uses the sit cookie + a live authorisation check.
pub async fn sitting_status(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    RealUser(me): RealUser,
) -> Response {
    (
        StatusCode::OK,
        sit_owner_name(&state, &jar, me).await.unwrap_or_default(),
    )
        .into_response()
}

/// The grant-sitter form (030).
#[derive(Deserialize)]
pub struct GrantForm {
    username: String,
}

/// Authorise a sitter by username (030 AC1, owner = the real player).
pub async fn sitting_grant(
    State(state): State<AppState>,
    RealUser(me): RealUser,
    Form(form): Form<GrantForm>,
) -> Response {
    let flash = grant_sitter(state.accounts.as_ref(), me, &form.username)
        .await
        .err()
        .map(|e| {
            tracing::warn!(error = %e, "grant sitter rejected");
            user_msg(e.to_string())
        });
    with_flash(Redirect::to("/sitting").into_response(), flash)
}

/// The revoke / start forms carry a target player id.
#[derive(Deserialize)]
pub struct SitterTargetForm {
    #[serde(default)]
    sitter: Option<String>,
    #[serde(default)]
    owner: Option<String>,
}

/// Revoke a sitter (030 AC1, owner = the real player). Ends any in-progress sit on the next request.
pub async fn sitting_revoke(
    State(state): State<AppState>,
    RealUser(me): RealUser,
    Form(form): Form<SitterTargetForm>,
) -> Response {
    if let Some(sitter) = form.sitter.and_then(|s| s.parse::<u128>().ok())
        && let Err(e) = revoke_sitter(state.accounts.as_ref(), me, PlayerId(sitter)).await
    {
        tracing::error!(error = %e, "revoke sitter failed");
    }
    Redirect::to("/sitting").into_response()
}

/// Start sitting an owner (030 AC2): authorise, then set the sit cookie. Acts on the real player.
pub async fn sitting_start(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    RealUser(me): RealUser,
    Form(form): Form<SitterTargetForm>,
) -> Response {
    let Some(owner) = form
        .owner
        .and_then(|s| s.parse::<u128>().ok())
        .map(PlayerId)
    else {
        return Redirect::to("/sitting").into_response();
    };
    match authorize_sit(state.accounts.as_ref(), owner, me, now()).await {
        Ok(true) => {
            let jar = jar.add(crate::auth::sit_cookie(owner.0));
            (jar, Redirect::to("/village")).into_response()
        }
        // Not authorised (not a sitter / blocked owner) — refuse.
        Ok(false) => forbidden(),
        Err(e) => {
            tracing::error!(error = %e, "authorize sit failed");
            server_error()
        }
    }
}

/// Stop sitting (030 AC2): clear the sit cookie, returning to the real account.
pub async fn sitting_stop(jar: PrivateCookieJar) -> Response {
    let jar = jar.remove(crate::auth::clear_sit_cookie());
    (jar, Redirect::to("/sitting")).into_response()
}

/// The player's onboarding quests (018 AC8, Player only): the current quest with its reward, the
/// completed list, and the all-done state. Evaluates lazily on view (server-authoritative,
/// idempotent) so newly-satisfied quests are completed before rendering.
pub async fn quests_page(State(state): State<AppState>, AuthUser(player): AuthUser) -> Response {
    // Lazily complete anything now satisfied — best-effort, must not break the page.
    if let Err(e) = evaluate_quests(
        state.accounts.as_ref(),
        state.rules.as_ref(),
        state.quest_chain.as_ref(),
        player,
    )
    .await
    {
        tracing::error!(error = %e, "quest evaluation failed");
    }
    let repo = state.accounts.as_ref();
    let completed = match repo.completed_quests(player).await {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, "completed quests lookup failed");
            return server_error();
        }
    };
    let villages = match repo.villages_of(player).await {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = %e, "villages lookup failed");
            return server_error();
        }
    };
    // Capital (else first) village — only used for the nav back-link.
    let village_id = villages
        .iter()
        .find(|v| v.is_capital)
        .or_else(|| villages.first())
        .map(|v| v.id.0.to_string())
        .unwrap_or_default();
    let chain = state.quest_chain.as_ref();
    let current = current_quest(chain, &completed).map(|q| CurrentQuestView {
        description: q.description.clone(),
        reward: quest_reward_label(&q.reward),
    });
    // Completed quests in chain order, with their descriptions.
    let done: Vec<CompletedQuestView> = chain
        .iter()
        .filter(|q| completed.contains(&q.id))
        .map(|q| CompletedQuestView {
            description: q.description.clone(),
        })
        .collect();
    page(&QuestsTemplate {
        village_id,
        current,
        completed: done,
    })
}

/// Public alliance statistics page (016 AC10).
pub async fn alliance_stats_page(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    let Ok(aid) = id.parse::<u128>() else {
        return not_found();
    };
    let repo = state.accounts.as_ref();
    let s = match alliance_statistics(repo, state.rules.as_ref(), AllianceId(aid)).await {
        Ok(Some(s)) => s,
        Ok(None) => return not_found(),
        Err(e) => {
            tracing::error!(error = %e, "alliance stats failed");
            return server_error();
        }
    };
    let medals = match repo.medals_for(MedalSubjectKind::Alliance, aid).await {
        Ok(m) => m,
        Err(e) => {
            tracing::error!(error = %e, "alliance medals failed");
            return server_error();
        }
    };
    page(&AllianceStatsTemplate {
        name: s.name,
        tag: s.tag,
        population: s.population,
        attack_points: s.attack_points,
        defense_points: s.defense_points,
        members: s
            .members
            .into_iter()
            .map(
                |(player, name, population, attack_points, defense_points)| MemberStatRow {
                    name,
                    href: format!("/stats/player/{}", player.0),
                    population,
                    attack_points,
                    defense_points,
                },
            )
            .collect(),
        medals: medals
            .into_iter()
            .map(|m| MedalRowView {
                category: medal_label(m.category).to_owned(),
                rank: m.rank,
                period: m.period,
            })
            .collect(),
    })
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
    // Loot + building damage (011): shown when present.
    let l = report.loot;
    let loot = (l.wood != 0 || l.clay != 0 || l.iron != 0 || l.crop != 0).then(|| {
        format!(
            "{} wood, {} clay, {} iron, {} crop",
            l.wood, l.clay, l.iron, l.crop
        )
    });
    let razed = report
        .razed
        .map(|d| format!("{} {} → {}", building_label(d.kind), d.before, d.after));
    // The loyalty change from an administrator strike (014 AC10), shown when present.
    let loyalty = match (report.loyalty_before, report.loyalty_after) {
        (Some(before), Some(after)) => Some(format!("{before} → {after}")),
        _ => None,
    };
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
        loot,
        razed,
        loyalty,
        conquered: report.conquered,
    })
}

// ---------------------------------------------------------------- alliances (015)

fn role_name(role: AllianceRole) -> &'static str {
    match role {
        AllianceRole::Founder => "Founder",
        AllianceRole::Leader => "Leader",
        AllianceRole::Member => "Member",
    }
}

/// A human summary of a member effective rights for the roster.
fn rights_summary(role: AllianceRole, rights: RightSet) -> String {
    match role {
        AllianceRole::Founder => "all".to_owned(),
        AllianceRole::Member => "—".to_owned(),
        AllianceRole::Leader => {
            let names: Vec<&str> = [
                (AllianceRight::Invite, "invite"),
                (AllianceRight::Expel, "expel"),
                (AllianceRight::Diplomacy, "diplomacy"),
                (AllianceRight::Announce, "announce"),
                (AllianceRight::ManageRoles, "manage"),
            ]
            .into_iter()
            .filter(|(r, _)| rights.contains(*r))
            .map(|(_, n)| n)
            .collect();
            if names.is_empty() {
                "—".to_owned()
            } else {
                names.join(", ")
            }
        }
    }
}

/// The alliance / Embassy page (015 AC8/AC9/AC11): the founder/join controls when alliance-less, or the
/// roster + diplomacy + incoming-defence overview + management controls when in one.
pub async fn alliance(State(state): State<AppState>, AuthUser(player): AuthUser) -> Response {
    let repo = state.accounts.as_ref();
    let rules = state.alliance_rules.as_ref();
    match alliance_view(repo, player).await {
        Ok(Some(ov)) => {
            let me = ov.membership.alliance;
            let roster = ov
                .roster
                .iter()
                .map(|e| RosterRowView {
                    player_id: e.player.0.to_string(),
                    name: e.name.clone(),
                    role: role_name(e.role),
                    rights: rights_summary(e.role, e.rights),
                    is_self: e.player == player,
                })
                .collect();
            let diplomacy = ov
                .diplomacy
                .iter()
                .map(|d| {
                    let label = match (d.stance, d.status) {
                        (DiplomacyStance::War, _) => "War".to_owned(),
                        (DiplomacyStance::Confederation, DiplomacyStatus::Active) => {
                            "Confederation".to_owned()
                        }
                        (DiplomacyStance::Confederation, DiplomacyStatus::Proposed) => {
                            if d.proposed_by == Some(me) {
                                "Confederation (proposed by you)".to_owned()
                            } else {
                                "Confederation (proposed by them)".to_owned()
                            }
                        }
                    };
                    let can_accept = d.stance == DiplomacyStance::Confederation
                        && d.status == DiplomacyStatus::Proposed
                        && d.proposed_by == Some(d.other);
                    DiploRowView {
                        other_id: d.other.0.to_string(),
                        other: format!("{} [{}]", d.other_name, d.other_tag),
                        label,
                        can_accept,
                    }
                })
                .collect();
            let allied_villages = ov
                .allied_villages
                .iter()
                .map(|v| AlliedVillageView {
                    owner: v.owner_name.clone(),
                    x: v.coordinate.x,
                    y: v.coordinate.y,
                })
                .collect();
            let incoming = ov
                .incoming
                .iter()
                .map(|i| IncomingView {
                    x: i.coordinate.x,
                    y: i.coordinate.y,
                    arrive_ms: i.arrive_at.0,
                })
                .collect();
            let role = ov.membership.role;
            let rights = ov.membership.rights;
            let outgoing_invites = match repo.invites_of(me).await {
                Ok(invs) => invs
                    .into_iter()
                    .map(|o| OutgoingInviteView {
                        invitee_name: o.invitee_name,
                    })
                    .collect(),
                Err(e) => {
                    tracing::error!(error = %e, "alliance invites lookup failed");
                    return server_error();
                }
            };
            page(&AllianceTemplate {
                in_alliance: true,
                can_found: false,
                embassy_level: 0,
                found_level: rules.found_embassy_level,
                join_level: rules.join_embassy_level,
                pending: Vec::new(),
                name: ov.name,
                tag: ov.tag,
                my_role: role_name(role),
                is_founder: role == AllianceRole::Founder,
                can_invite: eperica_domain::has_right(role, rights, AllianceRight::Invite),
                can_diplomacy: eperica_domain::has_right(role, rights, AllianceRight::Diplomacy),
                can_expel: eperica_domain::has_right(role, rights, AllianceRight::Expel),
                can_manage: eperica_domain::has_right(role, rights, AllianceRight::ManageRoles),
                roster,
                diplomacy,
                allied_villages,
                incoming,
                outgoing_invites,
            })
        }
        Ok(None) => {
            let embassy = repo.max_embassy_level(player).await.unwrap_or(0);
            let pending = match repo.pending_invites_for(player).await {
                Ok(invs) => invs
                    .into_iter()
                    .map(|i| PendingInviteView {
                        alliance_id: i.alliance.0.to_string(),
                        name: i.alliance_name,
                        tag: i.alliance_tag,
                    })
                    .collect(),
                Err(e) => {
                    tracing::error!(error = %e, "pending invites lookup failed");
                    return server_error();
                }
            };
            page(&AllianceTemplate {
                in_alliance: false,
                can_found: rules.can_found(embassy),
                embassy_level: embassy,
                found_level: rules.found_embassy_level,
                join_level: rules.join_embassy_level,
                pending,
                name: String::new(),
                tag: String::new(),
                my_role: "",
                is_founder: false,
                can_invite: false,
                can_diplomacy: false,
                can_expel: false,
                can_manage: false,
                roster: Vec::new(),
                diplomacy: Vec::new(),
                allied_villages: Vec::new(),
                incoming: Vec::new(),
                outgoing_invites: Vec::new(),
            })
        }
        Err(e) => {
            tracing::error!(error = %e, "alliance view failed");
            server_error()
        }
    }
}

#[derive(Deserialize)]
pub struct FoundForm {
    name: String,
    tag: String,
}

pub async fn alliance_found(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
    Form(form): Form<FoundForm>,
) -> Response {
    let flash = found_alliance(
        state.accounts.as_ref(),
        state.alliance_rules.as_ref(),
        player,
        form.name.trim(),
        form.tag.trim(),
    )
    .await
    .err()
    .map(|e| {
        tracing::warn!(error = %e, "found alliance rejected");
        user_msg(e.to_string())
    });
    with_flash(Redirect::to("/alliance").into_response(), flash)
}

#[derive(Deserialize)]
pub struct UsernameForm {
    username: String,
}

/// Resolve a username to a player id, or `None` (logged) if it does not exist.
async fn resolve_player(state: &AppState, username: &str) -> Option<PlayerId> {
    match state.accounts.find_user_by_username(username.trim()).await {
        Ok(Some(u)) => Some(u.id),
        Ok(None) => None,
        Err(e) => {
            tracing::error!(error = %e, "username lookup failed");
            None
        }
    }
}

pub async fn alliance_invite(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
    Form(form): Form<UsernameForm>,
) -> Response {
    let flash = match resolve_player(&state, &form.username).await {
        Some(invitee) => invite_player(state.accounts.as_ref(), player, invitee)
            .await
            .err()
            .map(|e| {
                tracing::warn!(error = %e, "invite rejected");
                user_msg(e.to_string())
            }),
        None => Some("No player with that name.".to_owned()),
    };
    with_flash(Redirect::to("/alliance").into_response(), flash)
}

pub async fn alliance_revoke(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
    Form(form): Form<UsernameForm>,
) -> Response {
    let flash = match resolve_player(&state, &form.username).await {
        Some(invitee) => revoke_invite(state.accounts.as_ref(), player, invitee)
            .await
            .err()
            .map(|e| {
                tracing::warn!(error = %e, "revoke rejected");
                user_msg(e.to_string())
            }),
        None => Some("No player with that name.".to_owned()),
    };
    with_flash(Redirect::to("/alliance").into_response(), flash)
}

#[derive(Deserialize)]
pub struct RespondForm {
    alliance: String,
    accept: bool,
}

pub async fn alliance_respond(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
    Form(form): Form<RespondForm>,
) -> Response {
    let flash = match form.alliance.parse::<u128>() {
        Ok(id) => respond_invite(
            state.accounts.as_ref(),
            state.alliance_rules.as_ref(),
            player,
            eperica_domain::AllianceId(id),
            form.accept,
        )
        .await
        .err()
        .map(|e| {
            tracing::warn!(error = %e, "respond invite rejected");
            user_msg(e.to_string())
        }),
        Err(_) => None,
    };
    with_flash(Redirect::to("/alliance").into_response(), flash)
}

pub async fn alliance_leave(State(state): State<AppState>, AuthUser(player): AuthUser) -> Response {
    let flash = leave_alliance(state.accounts.as_ref(), player)
        .await
        .err()
        .map(|e| {
            tracing::warn!(error = %e, "leave rejected");
            user_msg(e.to_string())
        });
    with_flash(Redirect::to("/alliance").into_response(), flash)
}

pub async fn alliance_disband(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
) -> Response {
    let flash = disband_alliance(state.accounts.as_ref(), player)
        .await
        .err()
        .map(|e| {
            tracing::warn!(error = %e, "disband rejected");
            user_msg(e.to_string())
        });
    with_flash(Redirect::to("/alliance").into_response(), flash)
}

#[derive(Deserialize)]
pub struct TargetForm {
    target: String,
}

pub async fn alliance_expel(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
    Form(form): Form<TargetForm>,
) -> Response {
    let flash = match form.target.parse::<u128>() {
        Ok(id) => expel_member(state.accounts.as_ref(), player, PlayerId(id))
            .await
            .err()
            .map(|e| {
                tracing::warn!(error = %e, "expel rejected");
                user_msg(e.to_string())
            }),
        Err(_) => None,
    };
    with_flash(Redirect::to("/alliance").into_response(), flash)
}

pub async fn alliance_transfer(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
    Form(form): Form<TargetForm>,
) -> Response {
    let flash = match form.target.parse::<u128>() {
        Ok(id) => transfer_founder(state.accounts.as_ref(), player, PlayerId(id))
            .await
            .err()
            .map(|e| {
                tracing::warn!(error = %e, "transfer rejected");
                user_msg(e.to_string())
            }),
        Err(_) => None,
    };
    with_flash(Redirect::to("/alliance").into_response(), flash)
}

#[derive(Deserialize)]
pub struct RoleForm {
    target: String,
    role: String,
    #[serde(default)]
    invite: bool,
    #[serde(default)]
    expel: bool,
    #[serde(default)]
    diplomacy: bool,
    #[serde(default)]
    announce: bool,
    #[serde(default)]
    manage_roles: bool,
}

pub async fn alliance_role(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
    Form(form): Form<RoleForm>,
) -> Response {
    let Ok(id) = form.target.parse::<u128>() else {
        return Redirect::to("/alliance").into_response();
    };
    let role = match form.role.as_str() {
        "leader" => AllianceRole::Leader,
        _ => AllianceRole::Member,
    };
    let mut rights = RightSet::empty();
    if form.invite {
        rights = rights.with(AllianceRight::Invite);
    }
    if form.expel {
        rights = rights.with(AllianceRight::Expel);
    }
    if form.diplomacy {
        rights = rights.with(AllianceRight::Diplomacy);
    }
    if form.announce {
        rights = rights.with(AllianceRight::Announce);
    }
    if form.manage_roles {
        rights = rights.with(AllianceRight::ManageRoles);
    }
    let flash = set_member_role(state.accounts.as_ref(), player, PlayerId(id), role, rights)
        .await
        .err()
        .map(|e| {
            tracing::warn!(error = %e, "role change rejected");
            user_msg(e.to_string())
        });
    with_flash(Redirect::to("/alliance").into_response(), flash)
}

#[derive(Deserialize)]
pub struct DiplomacyForm {
    other: String,
    command: String,
}

pub async fn alliance_diplomacy(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
    Form(form): Form<DiplomacyForm>,
) -> Response {
    let Ok(id) = form.other.parse::<u128>() else {
        return Redirect::to("/alliance").into_response();
    };
    let command = match form.command.as_str() {
        "declare_war" => DiplomacyCommand::DeclareWar,
        "propose_confederation" => DiplomacyCommand::ProposeConfederation,
        "accept_confederation" => DiplomacyCommand::AcceptConfederation,
        "cancel" => DiplomacyCommand::Cancel,
        _ => return Redirect::to("/alliance").into_response(),
    };
    let flash = set_diplomacy(
        state.accounts.as_ref(),
        player,
        eperica_domain::AllianceId(id),
        command,
    )
    .await
    .err()
    .map(|e| {
        tracing::warn!(error = %e, "diplomacy change rejected");
        user_msg(e.to_string())
    });
    with_flash(Redirect::to("/alliance").into_response(), flash)
}

// ---- Alliance forum (027) ----

/// The alliance forum thread list (027 AC1, members only). Shows the announcement checkbox only when the
/// viewer holds the `Announce` right.
pub async fn forum_page(State(state): State<AppState>, AuthUser(player): AuthUser) -> Response {
    let repo = state.accounts.as_ref();
    let threads = match list_forum(repo, player).await {
        Ok(t) => t,
        Err(ForumError::NotAMember) => return forbidden(),
        Err(e) => {
            tracing::error!(error = %e, "forum list failed");
            return server_error();
        }
    };
    // Whether the viewer may post announcements (the Announce right).
    let can_announce = match repo.alliance_of(player).await {
        Ok(Some(m)) => eperica_domain::has_right(m.role, m.rights, AllianceRight::Announce),
        _ => false,
    };
    let threads = threads
        .into_iter()
        .map(|t| ForumThreadRow {
            id: t.id.to_string(),
            title: t.title,
            author: t.author_name,
            announcement: t.announcement,
            post_count: t.post_count,
        })
        .collect();
    page(&ForumTemplate {
        threads,
        can_announce,
    })
}

/// The new-thread form (027 AC2/AC4).
#[derive(Deserialize)]
pub struct NewThreadForm {
    title: String,
    body: String,
    #[serde(default)]
    announcement: Option<String>,
}

/// Start a forum thread (027 AC2/AC4, member; announcement needs `Announce`).
pub async fn forum_new(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
    Form(form): Form<NewThreadForm>,
) -> Response {
    let announcement = form.announcement.as_deref() == Some("1");
    match start_thread(
        state.accounts.as_ref(),
        player,
        &form.title,
        &form.body,
        announcement,
        now(),
    )
    .await
    {
        Ok(id) => Redirect::to(&format!("/alliance/forum/{id}")).into_response(),
        Err(ForumError::NotAMember | ForumError::MissingRight) => forbidden(),
        Err(_) => Redirect::to("/alliance/forum").into_response(),
    }
}

/// A single forum thread + its posts (027 AC1, members of the owning alliance only).
pub async fn forum_thread_page(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
    Path(id): Path<String>,
) -> Response {
    let Ok(tid) = id.parse::<u128>() else {
        return not_found();
    };
    match open_thread(state.accounts.as_ref(), player, tid).await {
        Ok((head, posts)) => page(&ForumThreadTemplate {
            thread_id: id,
            title: head.title,
            locked: head.announcement,
            posts: posts
                .into_iter()
                .map(|p| ForumPostRow {
                    author: p.author_name,
                    body: p.body,
                })
                .collect(),
        }),
        Err(ForumError::NotAMember) => forbidden(),
        Err(ForumError::NotFound) => not_found(),
        Err(e) => {
            tracing::error!(error = %e, "forum thread load failed");
            server_error()
        }
    }
}

/// The reply form (027 AC3).
#[derive(Deserialize)]
pub struct ForumReplyForm {
    body: String,
}

/// Reply to a forum thread (027 AC3, member; locked threads rejected).
pub async fn forum_reply(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
    Path(id): Path<String>,
    Form(form): Form<ForumReplyForm>,
) -> Response {
    let Ok(tid) = id.parse::<u128>() else {
        return not_found();
    };
    match reply(state.accounts.as_ref(), player, tid, &form.body, now()).await {
        Ok(_) => Redirect::to(&format!("/alliance/forum/{id}")).into_response(),
        Err(ForumError::NotAMember) => forbidden(),
        Err(ForumError::NotFound) => not_found(),
        Err(_) => Redirect::to(&format!("/alliance/forum/{id}")).into_response(),
    }
}

// ---------------------------------------------------------------------------------------------------
// Communication: conversations (DMs + chat channels) — 024.
// ---------------------------------------------------------------------------------------------------

/// The viewer's alliance, if any (for channel-access checks).
async fn viewer_alliance(state: &AppState, player: PlayerId) -> Option<AllianceId> {
    state
        .accounts
        .alliance_of(player)
        .await
        .ok()
        .flatten()
        .map(|m| m.alliance)
}

/// The conversations list (024 AC3).
pub async fn messages(State(state): State<AppState>, AuthUser(player): AuthUser) -> Response {
    let now_ts = now();
    let online_secs = state.lifecycle_rules.presence_online_secs;
    match conversation_list(state.accounts.as_ref(), state.accounts.as_ref(), player).await {
        Ok(list) => page(&MessagesTemplate {
            conversations: list
                .into_iter()
                .map(|c| {
                    // DM rows carry the other party's activity; channels do not.
                    let (has_presence, online, presence_label) = match c.other_last_activity {
                        Some(ms) => {
                            let (online, label) = presence_view(Timestamp(ms), now_ts, online_secs);
                            (true, online, label)
                        }
                        None => (false, false, String::new()),
                    };
                    ConversationRow {
                        key: c.key,
                        title: c.title,
                        last_body: c.last_body,
                        unread: c.unread,
                        has_presence,
                        online,
                        presence_label,
                    }
                })
                .collect(),
        }),
        Err(e) => {
            tracing::error!(error = %e, "conversation list failed");
            server_error()
        }
    }
}

/// A single conversation: history + send box + live region (024 AC2).
pub async fn conversation(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
    Path(key): Path<String>,
) -> Response {
    let now = now();
    let online_secs = state.lifecycle_rules.presence_online_secs;
    // Resolve the title + load history, access-checked, depending on the key kind. DM headers also
    // carry the other party's presence (025); channels do not.
    let (title, presence, history) = if let Some(other) = parse_dm_key(&key) {
        let (title, other_activity) = match view_profile(state.accounts.as_ref(), other).await {
            Ok(p) => (p.name, p.last_activity),
            Err(eperica_application::ProfileError::NotFound) => return not_found(),
            Err(e) => {
                tracing::error!(error = %e, "dm header profile failed");
                return server_error();
            }
        };
        match open_dm(state.accounts.as_ref(), player, other, 100, now).await {
            Ok(h) => (title, Some(other_activity), h),
            Err(e) => return comms_error_response(e),
        }
    } else if let Some(channel) = ChatChannel::parse(&key) {
        let title = channel_title(&state, channel).await;
        match open_chat(
            state.accounts.as_ref(),
            state.accounts.as_ref(),
            player,
            &key,
            100,
            now,
        )
        .await
        {
            Ok(h) => (title, None, h),
            Err(e) => return comms_error_response(e),
        }
    } else {
        return not_found();
    };
    let (has_presence, online, presence_label) = match presence {
        Some(activity) => {
            let (online, label) = presence_view(activity, now, online_secs);
            (true, online, label)
        }
        None => (false, false, String::new()),
    };
    let lines = history
        .into_iter()
        .map(|m| ChatLineView {
            sender: m.sender_name,
            body: m.body,
            mine: m.sender == player,
        })
        .collect();
    page(&ConversationTemplate {
        key,
        title,
        has_presence,
        online,
        presence_label,
        lines,
    })
}

/// Display title for a channel (the alliance name, or "Global").
async fn channel_title(state: &AppState, channel: ChatChannel) -> String {
    match channel {
        ChatChannel::Global => "Global".to_owned(),
        ChatChannel::Alliance(a) => state
            .accounts
            .alliance_summary(a)
            .await
            .ok()
            .flatten()
            .map_or_else(|| "Alliance".to_owned(), |(name, _)| name),
    }
}

/// Map a comms error to a response (Forbidden → 403; otherwise 500).
fn comms_error_response(e: CommsError) -> Response {
    match e {
        CommsError::Forbidden => forbidden(),
        other => {
            tracing::error!(error = %other, "conversation failed");
            server_error()
        }
    }
}

/// Send into a conversation (024 AC1) — a DM or a channel, depending on the key.
pub async fn messages_send(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
    Form(form): Form<SendForm>,
) -> Response {
    let result = if let Some(other) = parse_dm_key(&form.conversation) {
        send_dm(
            state.accounts.as_ref(),
            state.accounts.as_ref(),
            player,
            other,
            &form.body,
            now(),
        )
        .await
        .map(|_| ())
    } else {
        send_chat(
            state.accounts.as_ref(),
            state.accounts.as_ref(),
            player,
            &form.conversation,
            &form.body,
            now(),
        )
        .await
        .map(|_| ())
    };
    let flash = result.err().map(|e| {
        tracing::warn!(error = %e, "send rejected");
        user_msg(e.to_string())
    });
    with_flash(
        Redirect::to(&format!("/messages/c/{}", form.conversation)).into_response(),
        flash,
    )
}

/// Open (or start) the DM with a player from their profile (024 AC9).
pub async fn messages_with(AuthUser(_player): AuthUser, Path(id): Path<String>) -> Response {
    match id.trim().parse::<u128>() {
        Ok(other) => {
            Redirect::to(&format!("/messages/c/{}", dm_key(PlayerId(other)))).into_response()
        }
        Err(_) => Redirect::to("/messages").into_response(),
    }
}

/// The viewer's total unread (the nav badge polls this — 024 AC4).
pub async fn messages_unread(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
) -> Response {
    let n = unread_badge(state.accounts.as_ref(), state.accounts.as_ref(), player)
        .await
        .unwrap_or(0);
    (StatusCode::OK, n.to_string()).into_response()
}

/// Live SSE stream for one conversation (024 AC6). Access-checked; emits new lines as they arrive.
pub async fn messages_stream(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
    Path(key): Path<String>,
) -> Response {
    // The broadcast filter key. For a DM, subscribe on the **pair-canonical** key derived from
    // (viewer, other): only the two parties can compute it, so a viewer can never wiretap a third party's
    // thread (the URL key `dm:<other>` is viewer-relative and NOT pair-unique). For a channel, the key is
    // the channel itself, gated by membership.
    let want = if let Some(other) = parse_dm_key(&key) {
        dm_pair_key(player, other)
    } else if let Some(channel) = ChatChannel::parse(&key) {
        if !can_access_channel(channel, viewer_alliance(&state, player).await) {
            return forbidden();
        }
        key
    } else {
        return forbidden();
    };

    let mut rx = state.chat_hub.subscribe();
    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(msg) if msg.keys.contains(&want) => {
                    let data = serde_json::json!({
                        "sender": msg.sender_name,
                        "body": msg.body,
                        "ts": msg.created_ms,
                    })
                    .to_string();
                    yield Ok::<_, std::convert::Infallible>(
                        axum::response::sse::Event::default().data(data),
                    );
                }
                Ok(_) => {}
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    };
    axum::response::sse::Sse::new(stream)
        .keep_alive(axum::response::sse::KeepAlive::default())
        .into_response()
}

/// The send form (024).
#[derive(Deserialize)]
pub struct SendForm {
    conversation: String,
    body: String,
}

/// Deep-link for a notification's referenced entity (026), or empty when there's nothing to link to.
fn notification_href(ref_kind: Option<&str>, ref_id: Option<&str>) -> String {
    match (ref_kind, ref_id) {
        (Some("report"), Some(id)) => format!("/reports/{id}"),
        (Some("dm"), Some(other)) => format!("/messages/c/dm:{other}"),
        (Some("village"), Some(coord)) => match coord.split_once('|') {
            Some((x, y)) => format!("/map?x={x}&y={y}"),
            None => String::new(),
        },
        _ => String::new(),
    }
}

/// The notifications feed (026 AC4/AC5, Player only). Renders most-recent first and marks the player's
/// notifications read on view (owner-scoped) so the bell clears.
pub async fn notifications_page(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
) -> Response {
    let repo = state.accounts.as_ref();
    let list = match list_notifications(repo, player).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!(error = %e, "notifications list failed");
            return server_error();
        }
    };
    // Viewing marks them read (026 AC5). Best-effort — a failure must not break the page.
    if let Err(e) = mark_notifications_read(repo, player, now()).await {
        tracing::error!(error = %e, "marking notifications read failed");
    }
    let notifications = list
        .into_iter()
        .map(|n| NotificationRowView {
            label: n.kind.label().to_owned(),
            href: notification_href(n.ref_kind.as_deref(), n.ref_id.as_deref()),
            body: n.body,
            read: n.read,
        })
        .collect();
    page(&NotificationsTemplate { notifications })
}

/// The player's unread notification count — the nav bell polls this (026 AC4).
pub async fn notifications_unread(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
) -> Response {
    let n = notification_unread(state.accounts.as_ref(), player)
        .await
        .unwrap_or(0);
    (StatusCode::OK, n.to_string()).into_response()
}

/// Explicit mark-all-read (026 AC5, owner-scoped).
pub async fn notifications_read(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
) -> Response {
    if let Err(e) = mark_notifications_read(state.accounts.as_ref(), player, now()).await {
        tracing::error!(error = %e, "mark-all-read failed");
        return server_error();
    }
    Redirect::to("/notifications").into_response()
}

/// Live SSE stream for the logged-in player's notification bell (026 AC6). Subscribed on the player's
/// **private** key `notif:<uuid>` — a player can only ever receive their own (no cross-player leak, P4).
pub async fn notifications_stream(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
) -> Response {
    let want = notif_key(player);
    let mut rx = state.notification_hub.subscribe();
    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(n) if n.key == want => {
                    let data = serde_json::json!({ "kind": n.kind }).to_string();
                    yield Ok::<_, std::convert::Infallible>(
                        axum::response::sse::Event::default().data(data),
                    );
                }
                Ok(_) => {}
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    };
    axum::response::sse::Sse::new(stream)
        .keep_alive(axum::response::sse::KeepAlive::default())
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::{pct_encode, user_msg};

    #[test]
    fn user_msg_hides_storage_errors_but_keeps_reasons() {
        // AC2: an internal storage/backend failure never reaches the player verbatim.
        assert_eq!(
            user_msg("storage error: connection refused".to_owned()),
            "Something went wrong — please try again."
        );
        // AC1: a use-case reason passes through, capitalized for the banner.
        assert_eq!(
            user_msg("not enough resources".to_owned()),
            "Not enough resources"
        );
        // Already-capitalized / empty messages are left intact.
        assert_eq!(
            user_msg("No player with that name.".to_owned()),
            "No player with that name."
        );
        assert_eq!(user_msg(String::new()), "");
    }

    #[test]
    fn pct_encode_escapes_for_cookie_value() {
        assert_eq!(
            pct_encode("Not enough resources"),
            "Not%20enough%20resources"
        );
        // Unreserved set is preserved; everything else (incl. multi-byte UTF-8) is escaped.
        assert_eq!(pct_encode("a-b_c.d~e"), "a-b_c.d~e");
        assert_eq!(pct_encode("—"), "%E2%80%94"); // em-dash, 3 UTF-8 bytes
    }
}
