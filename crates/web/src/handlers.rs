//! HTTP handlers for the register / login / village flow.

use crate::auth::{
    AuthUser, GameContext, MaybeAuthUser, MaybeRealUser, RealUser, WORLD_COOKIE, WorldScope,
    auth_cookie, clear_cookie, world_cookie,
};
use crate::state::AppState;
use crate::templates::{
    AcademyRow, AcademyTemplate, AchievementRowView, ActiveView, AdminAccountRow, AdminTemplate,
    AdminWorldRow, AllianceStatsTemplate, AllianceTemplate, AlliedVillageView, ArtifactRowView,
    AuditRow, BuildRow, ChatLineView, CompletedQuestView, ConversationRow, ConversationTemplate,
    CurrentQuestView, DetailTemplate, DiploRowView, ForceRow, ForumPostRow, ForumTemplate,
    ForumThreadRow, ForumThreadTemplate, GarrisonRow, HistoryPointView, ImpressumTemplate,
    IncomingView, IndexTemplate, JoinableWorldRow, JoinedWorldRow, LandingWorldRow,
    LeaderboardRowView, LeaderboardTemplate, LoginTemplate, MapCellView, MapTemplate,
    MarketTemplate, MedalRowView, MemberStatRow, MessagesTemplate, ModAccountTemplate,
    ModQueueTemplate, ModReportRow, MovementRow, NotificationRowView, NotificationsTemplate,
    OasisRow, OutgoingInviteView, PendingInviteView, PlayerStatsTemplate, PrivacyTemplate,
    ProfileTemplate, QuestsTemplate, QueueView, RallyTemplate, RallyUnitRow, RegisterTemplate,
    ReinforcementRow, ReportRow, ReportTemplate, ReportsTemplate, ResourceRibbon, RosterRowView,
    ScoutReportTemplate, ScoutResourceRow, SearchHitRow, SearchTemplate, SettingsTemplate,
    SettingsToggleRow, ShipmentRow, SitterRow, SittingTemplate, SmithyRow, SmithyTemplate,
    StyleGuideTemplate, TermsTemplate, TrainRow, TroopsTemplate, VillageStatRow, VillageSwitchRow,
    VillageTemplate, WonderStandingView, WonderTemplate, WorldsTemplate,
};
use askama::Template;
use axum::Form;
use axum::extract::{ConnectInfo, Path, Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum_extra::extract::PrivateCookieJar;
use eperica_application::{
    AccountRepository, AchievementRepository, ActiveBuild, AdminError, AdminRepository,
    AllianceLeaderboardRow, AllianceRepository, ArtifactRepository, BattleReportView, BoardScope,
    BuildRepository, CombatRepository, CommsError, ConflictMetric, ConquestRepository,
    DiplomacyCommand, ElevatedRole, ForumError, LeaderboardRow, LoginError, MedalRepository,
    MedalSubjectKind, ModerationError, ModerationRepository, MovementRepository, OasisRepository,
    PlayerHit, QuestRepository, RegisterCommand, RegisterError, RepoError, ScoutIntel,
    ScoutReportView, ScoutRepository, TradeRepository, TrainingRepository, UnitOrderKind,
    UnitRepository, Window, WonderRepository, account_signals, admin_overview,
    alliance_conflict_leaderboard, alliance_population_leaderboard, alliance_statistics,
    alliance_view, authenticate, authorize_sit, climbers_leaderboard, conflict_leaderboard,
    conversation_list, create_world as admin_create_world_uc, disband_alliance, dm_key,
    dm_pair_key, edit_bio, end_protection_if_established, evaluate_achievements, evaluate_quests,
    expel_member, file_report, found_alliance, grant_sitter, invite_player, leave_alliance,
    list_accounts as admin_list_accounts, list_forum, list_notifications_for_account, list_sitters,
    list_sitting_for, list_worlds as admin_list_worlds, load_culture, load_economy, map_viewport,
    mark_notifications_read_for_account, notif_key, notification_settings,
    notification_unread_for_account, open_chat, open_dm, open_thread, order_attack, order_build,
    order_oasis_attack, order_oasis_recall, order_oasis_reinforce, order_reinforcement,
    order_research, order_return, order_scout, order_settle, order_smithy_upgrade, order_trade,
    order_train, order_wonder_build, parse_dm_key, player_statistics, population_history,
    population_leaderboard, register, reinforcement_reports, reply, require_admin, resolve_report,
    respond_invite, review_queue, revoke_invite, revoke_sitter, sanction_account, search,
    search_accounts as admin_search_accounts, send_chat, send_dm, set_diplomacy, set_member_role,
    set_notification_pref, set_role as admin_set_role_uc, sitter_log, start_thread,
    transfer_founder, unread_badge, view_profile, viewport_coords,
};
use eperica_domain::{
    AllianceId, AllianceRight, AllianceRole, AttackMode, BuildTarget, BuildingKind, ChatChannel,
    Coordinate, DiplomacyStance, DiplomacyStatus, Economy, GameSpeed, MedalCategory, MovementKind,
    OasisBonus, PlayerId, Presence, Quadrant, QuestReward, QueueLane, ReportReason, ResearchDenied,
    ResourceAmounts, ResourceKind, RightSet, SanctionKind, ScoutTarget, TileKind, Timestamp,
    TradeKind, Tribe, UnitId, UnitRole, UnitRules, UpgradeDenied, Village, VillageId, WorldId,
    can_access_channel, can_afford, can_research, can_upgrade, current_quest, expansion_slots,
    garrison_upkeep, is_inactive, per_unit_time_secs, presence, queue_lane, regenerate_loyalty,
    scaled_time_secs,
};
use eperica_infrastructure::now;
use eperica_infrastructure::{DEFAULT_PRESET, KNOWN_PRESETS, WorldRules, known_preset};
use serde::Deserialize;

fn resource_label(kind: ResourceKind) -> &'static str {
    match kind {
        ResourceKind::Wood => "Wood",
        ResourceKind::Clay => "Clay",
        ResourceKind::Iron => "Iron",
        ResourceKind::Crop => "Crop",
    }
}

/// Lowercase resource slug for the village-plan field plot colour (069).
fn resource_slug(kind: ResourceKind) -> &'static str {
    match kind {
        ResourceKind::Wood => "wood",
        ResourceKind::Clay => "clay",
        ResourceKind::Iron => "iron",
        ResourceKind::Crop => "crop",
    }
}

/// The building's own-page leaf for the village-plan inspector "Enter" link (069); empty if it has no page.
fn building_page(kind: BuildingKind) -> &'static str {
    match kind {
        BuildingKind::Academy => "academy",
        BuildingKind::Smithy => "smithy",
        BuildingKind::Barracks => "barracks",
        BuildingKind::Stable => "stable",
        BuildingKind::Workshop => "workshop",
        BuildingKind::RallyPoint => "rally",
        BuildingKind::Marketplace => "market",
        _ => "",
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

/// A one-line description of what a building does, for the generic building page (087).
fn building_blurb(kind: BuildingKind) -> &'static str {
    match kind {
        BuildingKind::MainBuilding => {
            "The heart of the village — higher levels speed every construction."
        }
        BuildingKind::RallyPoint => "Musters your army; required to send and return troops.",
        BuildingKind::Warehouse => "Stores wood, clay and iron — each level raises the cap.",
        BuildingKind::Granary => "Stores crop — each level raises the cap.",
        BuildingKind::Marketplace => {
            "Enables trade; its level sets how many merchants you command."
        }
        BuildingKind::Embassy => "Diplomacy — level 1 to join an alliance, level 3 to found one.",
        BuildingKind::Wall => "Rings the village in defence; reduced by rams in a siege.",
        BuildingKind::Barracks => "Trains infantry.",
        BuildingKind::Academy => "Researches new unit types so they can be trained.",
        BuildingKind::Smithy => "Forges your troops' weapons and armour to greater strength.",
        BuildingKind::Stable => "Trains cavalry.",
        BuildingKind::Workshop => "Builds siege engines — rams and catapults.",
        BuildingKind::Residence => "Trains settlers and administrators; gates expansion.",
        BuildingKind::Cranny => "Hides a share of your resources from looters.",
        BuildingKind::Outpost => "Garrisons captured oases; its level sets how many you may hold.",
        BuildingKind::TownHall => "Produces culture points, which gate founding new villages.",
        BuildingKind::Palace => "Designates your capital and trains settlers/administrators.",
        BuildingKind::Treasury => "Houses a captured artifact, whose power aids your empire.",
        BuildingKind::Wonder => "The Wonder of the World — raise it to 100 to win the round.",
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

/// The selected village as a domain id (server re-validates ownership in the use-case, P4). The id rides in
/// the URL path as a hyphenated **UUID** (064), the same form as the `{world}` segment. An absent or
/// unparseable id ⇒ `None` (the capital / first-village default).
fn selected_village(village: Option<&str>) -> Option<VillageId> {
    village
        .and_then(|s| uuid::Uuid::parse_str(s.trim()).ok())
        .map(|u| VillageId(u.as_u128()))
}

/// A village id as its hyphenated-UUID path segment (064) — the `village_id` every village-coupled template
/// carries so its links/forms read `/w/{world}/village/{village}/…`.
fn village_seg(village: VillageId) -> String {
    uuid::Uuid::from_u128(village.0).to_string()
}

/// Redirect to `path`, preserving the selected village (013 AC11) so the user stays on that village.
/// A world-scoped path (056): `/w/{uuid}{rest}` for the given world, e.g.
/// `world_path(w, "/village")` → `/w/<uuid>/village`. `rest` must start with `/`.
fn world_path(world: WorldId, rest: &str) -> String {
    format!("/w/{}{rest}", uuid::Uuid::from_u128(world.0))
}

/// The world's hyphenated UUID as a string (056) — the `world` field every world-scoped template carries so
/// its links can read `/w/{{ world }}/…`.
fn world_id_str(world: WorldId) -> String {
    uuid::Uuid::from_u128(world.0).to_string()
}

/// A village-coupled path (064): `/w/{world}/village/{village}{leaf}`, where `village` is the hyphenated-UUID
/// path segment and `leaf` is the trailing route (`""` = overview, `"/academy"`, `"/barracks"`, …; must start
/// with `/` when non-empty).
fn village_path(world: WorldId, village: &str, leaf: &str) -> String {
    world_path(world, &format!("/village/{village}{leaf}"))
}

/// Redirect back to a village-coupled page (064), staying on the same village (the id is the path segment).
fn redirect_to_village_leaf(world: WorldId, village: &str, leaf: &str) -> Response {
    Redirect::to(&village_path(world, village, leaf)).into_response()
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

/// Public landing page (Visitor). Lists the open worlds so a visitor can pick one and register straight
/// into it.
pub async fn index(State(state): State<AppState>) -> Response {
    let worlds = match state.accounts.list_worlds().await {
        Ok(ws) => ws
            .into_iter()
            .filter(|w| w.won_ms.is_none())
            .map(|w| LandingWorldRow {
                id: world_id_str(w.id),
                name: w.name,
                speed_label: format!("{}× speed", w.speed),
            })
            .collect(),
        Err(e) => {
            // A landing without the worlds list is still useful — log and show the page anyway.
            tracing::error!(error = %e, "landing list_worlds failed");
            Vec::new()
        }
    };
    page(&IndexTemplate { worlds })
}

/// Resolve a `?world=<uuid>` choice to a (validated hyphenated id, name) pair — only for a real, open
/// (not-won) world the registry can serve. `(None, None)` otherwise.
async fn resolve_world_choice(
    state: &AppState,
    world: Option<&str>,
) -> (Option<String>, Option<String>) {
    let Some(raw) = world else {
        return (None, None);
    };
    let Ok(uuid) = uuid::Uuid::parse_str(raw.trim()) else {
        return (None, None);
    };
    let wid = WorldId(uuid.as_u128());
    match state.accounts.list_worlds().await {
        Ok(ws) => ws
            .into_iter()
            .find(|w| w.id == wid && w.won_ms.is_none())
            .map(|w| (Some(world_id_str(w.id)), Some(w.name)))
            .unwrap_or((None, None)),
        Err(_) => (None, None),
    }
}

/// Where to send a freshly-registered account: into the world they picked on the landing (joining it if
/// it isn't their home world), or the lobby when no/invalid world was chosen.
async fn route_after_register(
    state: &AppState,
    account: PlayerId,
    tribe_slug: &str,
    world: Option<&str>,
) -> String {
    let Some(raw) = world else {
        return "/worlds".to_owned();
    };
    let Ok(uuid) = uuid::Uuid::parse_str(raw.trim()) else {
        return "/worlds".to_owned();
    };
    let wid = WorldId(uuid.as_u128());
    // The home world's player is created by registration itself — just drop into it.
    if wid == state.world_id {
        return world_path(wid, "/village");
    }
    // Another world: it must be real + open, and the registry must run it; join with the registered tribe.
    let Some(tribe) = Tribe::from_slug(tribe_slug) else {
        return "/worlds".to_owned();
    };
    // Re-validate on the success path that creates a player — never trust the earlier resolution (`world`
    // arrives as a plain string, P4); a stale/invalid id falls back to the lobby instead of a bad join.
    match state.accounts.list_worlds().await {
        Ok(ws) if ws.iter().any(|w| w.id == wid && w.won_ms.is_none()) => {}
        _ => return "/worlds".to_owned(),
    }
    let Some((repo, _map, _speed, _radius, rules)) = state.world_registry.context_for(wid).await
    else {
        return "/worlds".to_owned();
    };
    match repo
        .create_player_in_world(account, tribe, &rules.starting_village)
        .await
    {
        Ok(_) | Err(RepoError::Duplicate) => world_path(wid, "/village"),
        Err(e) => {
            tracing::error!(error = %e, "post-register world join failed");
            "/worlds".to_owned()
        }
    }
}

/// Legal: Impressum (operator disclosure, § 5 DDG) — public, static.
pub async fn impressum() -> Response {
    page(&ImpressumTemplate)
}

/// Legal: privacy policy — public, static.
pub async fn privacy() -> Response {
    page(&PrivacyTemplate)
}

/// Legal: terms of service — public, static.
pub async fn terms() -> Response {
    page(&TermsTemplate)
}

/// Registration form (Visitor).
pub async fn register_form(
    State(state): State<AppState>,
    Query(q): Query<RegisterQuery>,
) -> Response {
    let (world, world_name) = resolve_world_choice(&state, q.world.as_deref()).await;
    page(&RegisterTemplate {
        error: None,
        world,
        world_name,
    })
}

/// `/register?world=<uuid>` — the world a landing "Enlist" link preselected.
#[derive(Deserialize)]
pub struct RegisterQuery {
    #[serde(default)]
    world: Option<String>,
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
    /// Optional world the visitor chose on the landing — drop them into it on success.
    #[serde(default)]
    world: Option<String>,
}

/// Handle registration (AC1, AC3). On success (no confirmation required) logs the user in.
pub async fn register_submit(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    ConnectInfo(peer): ConnectInfo<std::net::SocketAddr>,
    jar: PrivateCookieJar,
    Form(form): Form<RegisterForm>,
) -> Response {
    // Resolve the chosen world (validated) so it survives both an error re-render and the success redirect.
    let (world_field, world_name) = resolve_world_choice(&state, form.world.as_deref()).await;
    let tribe_slug = form.tribe.clone();
    let cmd = RegisterCommand {
        username: form.username,
        email: form.email,
        password: form.password,
        tribe: form.tribe,
    };
    // Re-render the form (on a rejected submission) keeping the preselected world.
    let reject = |error: String| {
        page(&RegisterTemplate {
            error: Some(error),
            world: world_field.clone(),
            world_name: world_name.clone(),
        })
    };
    match register(
        state.accounts.as_ref(),
        state.hasher.as_ref(),
        &state.world_rules.starting_village,
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
                // Not logged in yet, so a landing world choice is intentionally dropped here — the player
                // picks a world from the lobby after confirming their email.
                page(&LoginTemplate {
                    error: Some("Account created. Confirm your email, then log in.".to_owned()),
                })
            } else {
                let jar = jar.add(auth_cookie(user.id.0));
                let dest =
                    route_after_register(&state, user.id, &tribe_slug, world_field.as_deref())
                        .await;
                (jar, Redirect::to(&dest)).into_response()
            }
        }
        Err(RegisterError::Invalid(message)) => reject(message),
        Err(RegisterError::Taken) => reject("That username or email is already taken.".to_owned()),
        Err(RegisterError::WorldFull) => {
            reject("The world is full — no free tile to settle.".to_owned())
        }
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
            (jar, Redirect::to("/worlds")).into_response()
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

/// Redirect a bare world-coupled route (an old `/village` link, or a nav fallback when no world is in the
/// URL) to the lobby (056). The world lives in the path now; without one, the player picks a world here.
pub async fn redirect_to_lobby() -> Response {
    Redirect::to("/worlds").into_response()
}

/// Bare **public** routes default to the **home** world (058) so a logged-out visitor can read the boards
/// without picking a world. (Game routes still go to the lobby via [`redirect_to_lobby`].)
pub async fn redirect_home_leaderboard(State(state): State<AppState>) -> Response {
    Redirect::to(&world_path(state.world_id, "/leaderboard")).into_response()
}

/// Bare `/wonder` → the home world's Wonder page (058).
pub async fn redirect_home_wonder(State(state): State<AppState>) -> Response {
    Redirect::to(&world_path(state.world_id, "/wonder")).into_response()
}

/// The world lobby (045 AC2): the worlds the account plays (with the current one marked) + the running
/// worlds it can join. Login required.
pub async fn worlds_page(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    AuthUser(account): AuthUser,
) -> Response {
    let current = jar
        .get(WORLD_COOKIE)
        .and_then(|c| c.value().parse::<u128>().ok())
        .map(WorldId)
        .unwrap_or(state.world_id);
    let joined_worlds = match state.accounts.worlds_of_user(account).await {
        Ok(w) => w,
        Err(e) => {
            tracing::error!(error = %e, "worlds_of_user failed");
            return server_error();
        }
    };
    let all_worlds = match state.accounts.list_worlds().await {
        Ok(w) => w,
        Err(e) => {
            tracing::error!(error = %e, "list_worlds failed");
            return server_error();
        }
    };
    let joined_ids: std::collections::HashSet<WorldId> =
        joined_worlds.iter().map(|w| w.world).collect();
    // World metadata (name/speed/radius) by id, from the global worlds listing.
    let meta: std::collections::HashMap<WorldId, (String, f64, u32)> = all_worlds
        .iter()
        .map(|w| (w.id, (w.name.clone(), w.speed, w.radius)))
        .collect();
    let joined = joined_worlds
        .iter()
        .map(|w| {
            let (name, speed, radius) = meta
                .get(&w.world)
                .cloned()
                .unwrap_or_else(|| (String::new(), 1.0, 0));
            JoinedWorldRow {
                id: world_id_str(w.world),
                name,
                speed,
                radius,
                tribe: w.tribe.slug().to_owned(),
                is_current: w.world == current,
                is_home: w.world == state.world_id,
            }
        })
        .collect();
    // Joinable = running worlds (in the listing) that are not won and not already joined.
    let joinable = all_worlds
        .iter()
        .filter(|w| !joined_ids.contains(&w.id) && w.won_ms.is_none())
        .map(|w| JoinableWorldRow {
            id: w.id.0.to_string(),
            name: w.name.clone(),
            speed: w.speed,
            radius: w.radius,
        })
        .collect();
    page(&WorldsTemplate { joined, joinable })
}

/// The join-world form (045 AC3): the world to join + the tribe to play.
#[derive(Deserialize)]
pub struct JoinWorldForm {
    world: String,
    tribe: String,
}

/// Join a world (045 AC3): create the account's player there (042 primitive) + select it. Server-
/// authoritative — only a world the registry runs and the account has not already joined; a re-join is a
/// no-op. Drops the player into that world's village.
pub async fn join_world(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    AuthUser(account): AuthUser,
    Form(form): Form<JoinWorldForm>,
) -> Response {
    let Ok(world) = form.world.trim().parse::<u128>().map(WorldId) else {
        return Redirect::to("/worlds").into_response();
    };
    let Some(tribe) = Tribe::from_slug(form.tribe.trim()) else {
        return Redirect::to("/worlds").into_response();
    };
    // Server-authoritative (P4, AC3): the target must be a real, **not-won** world — never trust the
    // posted id beyond what the joinable list offers. A won (frozen, 021) world cannot be joined.
    match state.accounts.list_worlds().await {
        Ok(worlds) => {
            if !worlds.iter().any(|w| w.id == world && w.won_ms.is_none()) {
                return Redirect::to("/worlds").into_response();
            }
        }
        Err(e) => {
            tracing::error!(error = %e, "join world: list_worlds failed");
            return server_error();
        }
    }
    // The world must be one the registry runs — `context_for` yields its (world-scoped) repo + rules for the
    // join, so the new village uses the **selected** world's starting template (its preset), not the home's.
    let Some((repo, _map, _speed, _radius, rules)) = state.world_registry.context_for(world).await
    else {
        return Redirect::to("/worlds").into_response();
    };
    match repo
        .create_player_in_world(account, tribe, &rules.starting_village)
        .await
    {
        // Joined now, or already joined (idempotent) — drop into the world's village (056). The cookie is
        // just the non-essential "last-visited" hint; the URL is what selects the world.
        Ok(_) | Err(RepoError::Duplicate) => (
            jar.add(world_cookie(world.0)),
            Redirect::to(&world_path(world, "/village")),
        )
            .into_response(),
        Err(e) => {
            tracing::error!(error = %e, "join world failed");
            server_error()
        }
    }
}

/// The canonical village entry (064): `/w/{world}/village` (no id) → 302 to the player's capital (or first)
/// village's path, so the nav "Village" link and old bare links always land on a concrete village URL.
pub async fn village_index(ctx: GameContext) -> Response {
    match village_view_data(&ctx, None).await {
        Ok((village, _)) => {
            redirect_to_village_leaf(ctx.world_id, &village_seg(village.id), "").into_response()
        }
        Err(r) => r,
    }
}

/// The effect a field's *next* level grants (031), scaled by world speed to match displayed rates.
fn field_effect(rules: &WorldRules, speed: GameSpeed, kind: ResourceKind, level: u8) -> String {
    let econ = &rules.economy;
    let cur = econ.field_production_per_hour(kind, level, speed);
    let next = econ.field_production_per_hour(kind, level + 1, speed);
    let dpop = econ.field_population(level + 1) - econ.field_population(level);
    let mut s = format!("Production {cur} → {next}/h");
    if dpop != 0 {
        s.push_str(&format!(" · +{dpop} pop"));
    }
    s
}

/// The effect a building's *next* level grants (031) — pure rule reads across the economy / combat /
/// trade / culture / build bundles; the tribe selects the (tribe-flavoured) Wall profile.
fn building_effect(
    rules: &WorldRules,
    tribe: Option<Tribe>,
    kind: BuildingKind,
    level: u8,
) -> String {
    let (econ, build_rules, combat, merchants, culture) = (
        &rules.economy,
        &rules.build,
        &rules.combat,
        &rules.merchant,
        &rules.culture,
    );
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
        BuildingKind::Barracks | BuildingKind::Stable | BuildingKind::Workshop => Some(format!(
            "Training speed ×{:.2} → ×{:.2}",
            rules.units.training.building_factor(level),
            rules.units.training.building_factor(next)
        )),
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
}

/// Build a `BuildRow` for one field/building target: its next-level cost, orderability, the explicit
/// gate reason (072), and any in-flight countdown. Shared by the village plan and the per-building/field
/// detail + functional pages so the upgrade panel is identical everywhere. `effect` is precomputed by the
/// caller (it knows the field's `ResourceKind`); pass the field-cap/at-max override afterwards if needed.
#[allow(clippy::too_many_arguments)]
fn build_row(
    rules: &WorldRules,
    tribe: Option<Tribe>,
    amounts: ResourceAmounts,
    active: &[ActiveBuild],
    table: &'static str,
    slot: u8,
    kind: &'static str,
    res: &'static str,
    page: &'static str,
    label: String,
    level: u8,
    target: BuildTarget,
    effect: String,
) -> BuildRow {
    let build_rules = &rules.build;
    let lane_of = |t: BuildTarget| tribe.map_or(QueueLane::All, |tr| queue_lane(tr, t));
    let lane = lane_of(target);
    let busy = active.iter().any(|a| lane_of(a.target) == lane);
    let cost = build_rules.cost(target, level);
    let at_max = cost.is_none();
    let affordable = cost.is_some_and(|c| can_afford(amounts, c));
    let can_order = !busy && affordable;
    let c = cost.unwrap_or(ResourceAmounts {
        wood: 0,
        clay: 0,
        iron: 0,
        crop: 0,
    });
    // 072: the explicit reason a non-max slot can't be ordered — a busy lane outranks affordability
    // (you can't build while the lane is occupied even if you can pay), else the exact shortfall.
    let gate = if at_max || can_order {
        String::new()
    } else if busy {
        "A construction is already underway — wait for it to finish.".to_owned()
    } else {
        let short: Vec<String> = [
            ("wood", c.wood - amounts.wood),
            ("clay", c.clay - amounts.clay),
            ("iron", c.iron - amounts.iron),
            ("crop", c.crop - amounts.crop),
        ]
        .into_iter()
        .filter(|(_, missing)| *missing > 0)
        .map(|(name, missing)| format!("{missing} more {name}"))
        .collect();
        format!("Need {}.", short.join(", "))
    };
    let building_ms = active
        .iter()
        .find(|a| a.target == target)
        .map(|a| a.complete_at.0);
    BuildRow {
        table,
        slot,
        kind,
        res,
        page,
        label,
        level,
        cost_wood: c.wood,
        cost_clay: c.clay,
        cost_iron: c.iron,
        cost_crop: c.crop,
        at_max,
        can_order,
        effect: if at_max { String::new() } else { effect },
        building_ms,
        gate,
    }
}

/// The upgrade-panel `BuildRow` for a building at a given level (087) — the building-specific wrapper over
/// [`build_row`], used by every building page's aside and the generic building page.
fn building_upgrade_row(
    rules: &WorldRules,
    tribe: Option<Tribe>,
    amounts: ResourceAmounts,
    active: &[ActiveBuild],
    kind: BuildingKind,
    level: u8,
) -> BuildRow {
    let slot = building_slot(kind);
    build_row(
        rules,
        tribe,
        amounts,
        active,
        "building",
        slot,
        building_kind_id(kind),
        "",
        building_page(kind),
        building_label(kind).to_owned(),
        level,
        BuildTarget::Building { slot, kind },
        building_effect(rules, tribe, kind, level),
    )
}

/// A player's village with its live economy, switchable across all their villages (Player only —
/// AC3/AC4/AC7, 013 AC11). The `{village}` path segment (a UUID, 064) selects which to show; the use-case
/// re-validates ownership and falls back to the capital for a bad/foreign id (P4).
pub async fn village(
    ctx: GameContext,
    Path((_world, village)): Path<(String, String)>,
) -> Response {
    let player = ctx.player;
    let account = ctx.account;
    let selected = selected_village(Some(&village));
    let user = match ctx.accounts.find_user_by_id(account).await {
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
        &ctx.accounts,
        &ctx.rules.economy,
        &ctx.rules.units,
        &ctx.rules.achievements,
        player,
    )
    .await
    {
        tracing::error!(error = %e, "achievement evaluation failed");
    }

    // 018: lazily complete any onboarding quests now satisfied (server-authoritative, idempotent,
    // stage-gated). Best-effort — a failure here must not break the village view.
    if let Err(e) =
        evaluate_quests(&ctx.accounts, &ctx.rules.economy, &ctx.rules.quests, player).await
    {
        tracing::error!(error = %e, "quest evaluation failed");
    }

    // 019: this authenticated view is the activity signal (throttled), and the natural place to end
    // beginner's protection once the player is established. Best-effort.
    if let Err(e) = ctx.accounts.touch_activity(account, now()).await {
        tracing::error!(error = %e, "activity touch failed");
    }
    if let Err(e) = end_protection_if_established(
        &ctx.accounts,
        &ctx.rules.economy,
        &ctx.rules.lifecycle,
        account,
        now(),
    )
    .await
    {
        tracing::error!(error = %e, "protection threshold check failed");
    }
    // The remaining protection window, if any, for the view.
    let protected_until = match ctx.accounts.protection_of(account).await {
        Ok(p) => p,
        Err(e) => {
            tracing::error!(error = %e, "protection lookup failed");
            None
        }
    };

    // 020 AC8: the artifacts this player holds, with the holding village's coordinate.
    let held = ctx
        .accounts
        .held_by_player(player)
        .await
        .unwrap_or_default();
    let owned = ctx.accounts.villages_of(player).await.unwrap_or_default();
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
        &ctx.accounts,
        &ctx.rules.economy,
        &ctx.rules.units,
        ctx.speed,
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
    let ribbon = resource_ribbon(&economy.economy);

    // The garrison panel + total upkeep (005 AC6/AC9); names resolved via the tribe's roster.
    let roster = village.tribe.map_or(&[][..], |t| ctx.rules.units.roster(t));
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
    // (label, leaf) for each built training building; the template renders
    // `/w/{world}/village/{village}/{leaf}` (064).
    let troop_links: Vec<(&'static str, &'static str)> = [
        (BuildingKind::Barracks, "Barracks", "barracks"),
        (BuildingKind::Stable, "Stable", "stable"),
        (BuildingKind::Workshop, "Workshop", "workshop"),
    ]
    .into_iter()
    .filter(|(kind, _, _)| {
        village
            .buildings
            .iter()
            .any(|b| b.kind == *kind && b.level > 0)
    })
    .map(|(_, label, leaf)| (label, leaf))
    .collect();

    let active = match ctx.accounts.active_builds(village.id).await {
        Ok(a) => a,
        Err(e) => {
            tracing::error!(error = %e, "active build lookup failed");
            return server_error();
        }
    };
    let build_rules = &ctx.rules.build;

    // A target is orderable only if its queue lane is free — Romans get a field and a building lane,
    // other tribes one shared lane (004 AC13); `build_row` re-derives that per target. The capital may
    // raise its resource fields past the normal cap (013 AC10); the cost table runs to the capital cap,
    // so a non-capital field is gated on `field_cap`.
    let tribe = village.tribe;
    let speed = ctx.speed;
    let field_cap = build_rules.field_max_level(village.is_capital);
    let fields = village
        .fields
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let slot = u8::try_from(i).unwrap_or(0);
            let mut row = build_row(
                &ctx.rules,
                tribe,
                amounts,
                &active,
                "field",
                slot,
                "",
                resource_slug(f.kind),
                "",
                format!("{} field #{slot}", resource_label(f.kind)),
                f.level,
                BuildTarget::Field { slot },
                field_effect(&ctx.rules, speed, f.kind, f.level),
            );
            if f.level >= field_cap {
                // A non-capital field caps below the cost table's end; blank the next-level data too.
                row.at_max = true;
                row.can_order = false;
                row.effect = String::new();
                row.gate = String::new();
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
        build_row(
            &ctx.rules,
            tribe,
            amounts,
            &active,
            "building",
            slot,
            building_kind_id(kind),
            "",
            building_page(kind),
            building_label(kind).to_owned(),
            level,
            BuildTarget::Building { slot, kind },
            building_effect(&ctx.rules, tribe, kind, level),
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
    let movements_view = match ctx.accounts.active_movements(player).await {
        Ok(m) => m,
        Err(e) => {
            tracing::error!(error = %e, "movements lookup failed");
            return server_error();
        }
    };
    let here = match ctx.accounts.reinforcements_at(village.id).await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "reinforcements-here lookup failed");
            return server_error();
        }
    };
    let abroad = match ctx.accounts.reinforcements_of(player).await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "reinforcements-abroad lookup failed");
            return server_error();
        }
    };
    let unit_rules: &UnitRules = &ctx.rules.units;
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

    let trades = match ctx.accounts.active_trades(player).await {
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
    let oases: Vec<OasisRow> = match ctx.accounts.occupied_oases(village.id).await {
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
    let owned = match ctx.accounts.villages_of(player).await {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = %e, "villages lookup failed");
            return server_error();
        }
    };
    let switcher: Vec<VillageSwitchRow> = owned
        .iter()
        .map(|v| VillageSwitchRow {
            id: village_seg(v.id),
            label: format!("({}|{})", v.coordinate.x, v.coordinate.y),
            is_capital: v.is_capital,
            is_current: v.id == village.id,
        })
        .collect();

    // The player's pooled culture points + the expansion-slot gate (013 AC1/AC4/AC11).
    let culture = match load_culture(
        &ctx.accounts,
        &ctx.accounts,
        &ctx.rules.culture,
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
    let loyalty = match ctx.accounts.village_loyalty(village.id).await {
        Ok(Some((value, updated))) => regenerate_loyalty(
            value,
            (now().0 - updated.0) / 1000,
            &ctx.rules.loyalty,
            ctx.speed,
        ),
        Ok(None) => ctx.rules.loyalty.starting_loyalty,
        Err(e) => {
            tracing::error!(error = %e, "loyalty lookup failed");
            return server_error();
        }
    };

    // The round-over notice (021 AC7) — best-effort; a lookup error must not break the village view.
    let world_won = matches!(ctx.accounts.world_ended().await, Ok(Some(_)));

    page(&VillageTemplate {
        world: world_id_str(ctx.world_id),
        tribe_slug: village.tribe.map_or("", |t| t.slug()),
        username: user.username,
        world_won,
        is_wonder_site: village.is_wonder_site,
        village_id: village_seg(village.id),
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
        ribbon,
        population: eperica_domain::economy::population(
            &village.fields,
            &village.buildings,
            &ctx.rules.economy,
        ),
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

/// The viewport half-extent: the map view shows a `(2·HALF + 1)`-square grid. 091: a 13×13 window so the
/// (smaller) tiles fill the map column and more of the world is visible at a glance.
const MAP_HALF: i32 = 6;

/// Optional map-view center (defaults to the player's village). On the Rally Point, `village` also
/// selects which of the player's villages the troops are sent from (013 AC11).
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
pub async fn map(ctx: GameContext, Query(q): Query<MapQuery>) -> Response {
    let player = ctx.player;
    // The header username is the human (account-level), not the world player.
    let user = match ctx.accounts.find_user_by_id(ctx.account).await {
        Ok(Some(u)) => u,
        Ok(None) => return Redirect::to("/login").into_response(),
        Err(e) => {
            tracing::error!(error = %e, "lookup user failed");
            return server_error();
        }
    };
    let villages = match ctx.accounts.villages_of(player).await {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = %e, "villages lookup failed");
            return server_error();
        }
    };
    // The player's capital tile, distinguished on the map (013 AC9/AC11).
    let capital_coord = villages.iter().find(|v| v.is_capital).map(|v| v.coordinate);
    // The acting village for the map's "send" shortcuts — the capital, else the first village (064): the
    // Rally links carry it in the path (`/village/{acting}/rally?x=…`).
    let acting_vid = villages
        .iter()
        .find(|v| v.is_capital)
        .or_else(|| villages.first())
        .map(|v| village_seg(v.id));
    let radius = ctx.map.radius();
    // Center on the query (if given) or the player's first village, wrapped into bounds.
    let center = match (q.x, q.y) {
        (Some(x), Some(y)) => Coordinate::new(x, y).wrapped(radius),
        _ => villages
            .first()
            .map_or(Coordinate::new(0, 0), |v| v.coordinate),
    };

    let coords = viewport_coords(center, MAP_HALF, radius);
    let markers = match ctx.accounts.villages_at(&coords).await {
        Ok(m) => m,
        Err(e) => {
            tracing::error!(error = %e, "map markers lookup failed");
            return server_error();
        }
    };
    let viewport = map_viewport(ctx.map.as_ref(), center, MAP_HALF, &markers);

    // Which oases in view are occupied, and by whom (012 AC12).
    let oasis_owners: std::collections::HashMap<Coordinate, String> =
        match ctx.accounts.oasis_owners_at(&coords).await {
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
                            ctx.rules.lifecycle.inactive_after_secs,
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
                            ctx.rules.lifecycle.presence_online_secs,
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
                            href = acting_vid.as_ref().map(|vid| {
                                village_path(
                                    ctx.world_id,
                                    vid,
                                    &format!("/rally?x={}&y={}", coord.x, coord.y),
                                )
                            });
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
                        href = acting_vid.as_ref().map(|vid| {
                            village_path(
                                ctx.world_id,
                                vid,
                                &format!("/rally?x={}&y={}", coord.x, coord.y),
                            )
                        });
                    }
                    // Distance from home (toroidal, rounded) — helps judge travel time at a glance.
                    if let Some(o) = origin {
                        let d = ctx.map.distance(o, coord);
                        if d >= 0.5 {
                            label.push_str(&format!(" · {} fields away", d.round() as i64));
                        }
                    }
                    MapCellView {
                        cell_class: class,
                        glyph,
                        label,
                        href,
                        x: coord.x,
                        y: coord.y,
                    }
                })
                .collect()
        })
        .collect();

    let span = 2 * MAP_HALF + 1;
    page(&MapTemplate {
        world: world_id_str(ctx.world_id),
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
    /// 087: the village-relative leaf to return to (the page the form was on, e.g. `/smithy` or
    /// `/building/warehouse`). Validated server-side to a safe leaf (P4); falls back to the target's page.
    #[serde(default)]
    back: Option<String>,
}

/// Order an upgrade/construction for the path village, then return to it (Player only, P4). The village rides
/// in the path (064); the use-case re-validates ownership (P4).
pub async fn build_submit(
    ctx: GameContext,
    Path((_world, village)): Path<(String, String)>,
    Form(form): Form<BuildForm>,
) -> Response {
    let player = ctx.player;
    let target = match form.table.as_str() {
        "field" => BuildTarget::Field { slot: form.slot },
        "building" => match parse_building_kind(form.kind.as_deref()) {
            // Slot is derived server-side from the kind — never trusted from the client (P4), so a
            // crafted request cannot place a building in (clobber) another building's slot.
            Some(kind) => BuildTarget::Building {
                slot: building_slot(kind),
                kind,
            },
            None => return redirect_to_village_leaf(ctx.world_id, &village, ""),
        },
        _ => return redirect_to_village_leaf(ctx.world_id, &village, ""),
    };

    let flash = order_build(
        &ctx.accounts,
        &ctx.accounts,
        &ctx.accounts,
        &ctx.rules.economy,
        &ctx.rules.build,
        &ctx.rules.units,
        ctx.speed,
        now(),
        player,
        selected_village(Some(&village)),
        target,
    )
    .await
    .err()
    .map(|e| {
        tracing::warn!(error = %e, "build order rejected");
        user_msg(e.to_string())
    });
    // 087: an upgrade is ordered from the target's own page (a building or field), so return there. Prefer
    // the page the form was on (`back`) when it is a safe village-relative leaf; otherwise derive it from the
    // validated target. Either way the redirect is confined to this village's own pages (P4 — no open
    // redirect: a bad `back` is rejected, not followed).
    let leaf = form
        .back
        .filter(|b| is_safe_leaf(b))
        .unwrap_or_else(|| target_page_leaf(target));
    with_flash(
        redirect_to_village_leaf(ctx.world_id, &village, &leaf),
        flash,
    )
}

/// A `back` leaf is safe iff it is a short village-relative path: starts with `/` and contains only
/// lowercase letters, digits, `/`, `_`, `-` (so it can only ever land on a `…/village/{id}/<leaf>` page).
fn is_safe_leaf(leaf: &str) -> bool {
    leaf.starts_with('/')
        && leaf.len() <= 48
        && leaf[1..].bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'/' | b'_' | b'-')
        })
}

/// The village-relative leaf for a build target's own page (087): a building's dedicated functional page
/// (e.g. `smithy`) when it has one, else the generic `building/{kind}`; a field's `field/{slot}`.
fn target_page_leaf(target: BuildTarget) -> String {
    match target {
        BuildTarget::Field { slot } => format!("/field/{slot}"),
        BuildTarget::Building { kind, .. } => {
            let page = building_page(kind);
            if page.is_empty() {
                format!("/building/{}", building_kind_id(kind))
            } else {
                format!("/{page}")
            }
        }
    }
}

/// 087: the generic per-building page — its description, the resource ribbon, and the working upgrade
/// panel. Reached by clicking a plot on the village plan; buildings with a dedicated functional page
/// (Smithy/Academy/…) are linked there instead, but this still renders for any kind reached directly.
pub async fn building_detail(
    ctx: GameContext,
    Path((_world, village, kind_seg)): Path<(String, String, String)>,
) -> Response {
    let Some(kind) = parse_building_kind(Some(&kind_seg)) else {
        return redirect_to_village_leaf(ctx.world_id, &village, "");
    };
    let (village, economy) = match village_view_data(&ctx, selected_village(Some(&village))).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let active = ctx
        .accounts
        .active_builds(village.id)
        .await
        .unwrap_or_default();
    let level = building_level(&village, kind);
    let upgrade = building_upgrade_row(
        &ctx.rules,
        village.tribe,
        economy.amounts,
        &active,
        kind,
        level,
    );
    page(&DetailTemplate {
        world: world_id_str(ctx.world_id),
        tribe_slug: village.tribe.map_or("", |t| t.slug()),
        village_id: village_seg(village.id),
        village_label: format!("({}|{})", village.coordinate.x, village.coordinate.y),
        ribbon: resource_ribbon(&economy),
        eyebrow: "Building",
        title: building_label(kind).to_owned(),
        blurb: building_blurb(kind).to_owned(),
        icon: format!("i-{}", building_kind_id(kind)),
        upgrade,
    })
}

/// 087: the generic per-field page — production effect, the resource ribbon, and the working upgrade panel.
/// Reached by clicking a field plot on the village plan.
pub async fn field_detail(
    ctx: GameContext,
    Path((_world, village, slot)): Path<(String, String, u8)>,
) -> Response {
    let (village, economy) = match village_view_data(&ctx, selected_village(Some(&village))).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let Some(f) = village.fields.get(slot as usize) else {
        return redirect_to_village_leaf(ctx.world_id, &village_seg(village.id), "");
    };
    let active = ctx
        .accounts
        .active_builds(village.id)
        .await
        .unwrap_or_default();
    let mut upgrade = build_row(
        &ctx.rules,
        village.tribe,
        economy.amounts,
        &active,
        "field",
        slot,
        "",
        resource_slug(f.kind),
        "",
        format!("{} field #{slot}", resource_label(f.kind)),
        f.level,
        BuildTarget::Field { slot },
        field_effect(&ctx.rules, ctx.speed, f.kind, f.level),
    );
    // A non-capital field caps below the cost table's end (which runs to the capital cap), 013 AC10.
    if f.level >= ctx.rules.build.field_max_level(village.is_capital) {
        upgrade.at_max = true;
        upgrade.can_order = false;
        upgrade.effect = String::new();
        upgrade.gate = String::new();
    }
    let title = format!("{} field #{slot}", resource_label(f.kind));
    let blurb = format!(
        "Clears and works the land to raise your {} output.",
        resource_label(f.kind).to_lowercase()
    );
    let icon = format!("i-{}", resource_slug(f.kind));
    page(&DetailTemplate {
        world: world_id_str(ctx.world_id),
        tribe_slug: village.tribe.map_or("", |t| t.slug()),
        village_id: village_seg(village.id),
        village_label: format!("({}|{})", village.coordinate.x, village.coordinate.y),
        ribbon: resource_ribbon(&economy),
        eyebrow: "Resource field",
        title,
        blurb,
        icon,
        upgrade,
    })
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
/// World-scoped: the repo/speed/player **and the economy/unit rules** all come from `GameContext` (050), so
/// the economy settles under the selected world's speed (044) and its rule preset.
/// The shared building-page resource ribbon, from a settled economy (067).
fn resource_ribbon(e: &Economy) -> ResourceRibbon {
    ResourceRibbon {
        wood: e.amounts.wood,
        clay: e.amounts.clay,
        iron: e.amounts.iron,
        crop: e.amounts.crop,
        wood_rate: e.rates.wood,
        clay_rate: e.rates.clay,
        iron_rate: e.rates.iron,
        crop_rate: e.rates.crop_net,
        warehouse: e.capacities.warehouse,
        granary: e.capacities.granary,
    }
}

async fn village_view_data(
    ctx: &GameContext,
    selected: Option<VillageId>,
) -> Result<(Village, Economy), Response> {
    match load_economy(
        &ctx.accounts,
        &ctx.rules.economy,
        &ctx.rules.units,
        ctx.speed,
        now(),
        ctx.player,
        selected,
    )
    .await
    {
        // The full economy (amounts + rates + capacities) so building pages can show the resource ribbon (066).
        Ok(Some(e)) => Ok((e.village, e.economy)),
        Ok(None) => {
            tracing::error!(player = ?ctx.player, "authenticated user has no village/economy");
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
    ctx: GameContext,
    Path((_world, village)): Path<(String, String)>,
) -> Response {
    let player = ctx.player;
    let (village, economy) = match village_view_data(&ctx, selected_village(Some(&village))).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let amounts = economy.amounts;
    let Some(tribe) = village.tribe else {
        tracing::error!(?player, "village has no tribe");
        return server_error();
    };
    let (researched, orders) = match tokio::try_join!(
        ctx.accounts.researched_units(village.id),
        ctx.accounts.active_unit_orders(village.id),
    ) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(error = %e, "academy state lookup failed");
            return server_error();
        }
    };
    let unit_rules: &UnitRules = &ctx.rules.units;
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
                (Some(r.cost), scaled_time_secs(r.time_secs, ctx.speed))
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
                portrait: format!("{}_{}", tribe.slug(), spec.id.as_str()),
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

    let build_active = ctx
        .accounts
        .active_builds(village.id)
        .await
        .unwrap_or_default();
    let upgrade = building_upgrade_row(
        &ctx.rules,
        village.tribe,
        amounts,
        &build_active,
        BuildingKind::Academy,
        building_level(&village, BuildingKind::Academy),
    );
    page(&AcademyTemplate {
        world: world_id_str(ctx.world_id),
        tribe_slug: village.tribe.map_or("", |t| t.slug()),
        village_id: village_seg(village.id),
        village_label: format!("({}|{})", village.coordinate.x, village.coordinate.y),
        ribbon: resource_ribbon(&economy),
        has_academy: building_level(&village, BuildingKind::Academy) > 0,
        rows,
        active,
        upgrade,
    })
}

/// The Smithy: researched units with upgrade levels and actions (004 AC15; Player only, P4).
pub async fn smithy(ctx: GameContext, Path((_world, village)): Path<(String, String)>) -> Response {
    let player = ctx.player;
    let (village, economy) = match village_view_data(&ctx, selected_village(Some(&village))).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let amounts = economy.amounts;
    let Some(tribe) = village.tribe else {
        tracing::error!(?player, "village has no tribe");
        return server_error();
    };
    let (researched, levels, orders) = match tokio::try_join!(
        ctx.accounts.researched_units(village.id),
        ctx.accounts.unit_levels(village.id),
        ctx.accounts.active_unit_orders(village.id),
    ) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(error = %e, "smithy state lookup failed");
            return server_error();
        }
    };
    let unit_rules: &UnitRules = &ctx.rules.units;
    let smithy_lvl = building_level(&village, BuildingKind::Smithy);
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
    // The portrait of the unit at the anvil, for the aside (066).
    let active_portrait = upgrade_active.map(|o| format!("{}_{}", tribe.slug(), o.unit.as_str()));

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
                .map_or(0, |t| scaled_time_secs(t, ctx.speed));
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
                    (f64::from(base) * ctx.rules.combat.smithy_factor(lvl)).round() as u32
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
                portrait: format!("{}_{}", tribe.slug(), spec.id.as_str()),
                name: spec.name.clone(),
                role: role_label(spec.role),
                level,
                target: level + 1,
                forging: upgrade_active.is_some_and(|o| o.unit == spec.id),
                pips: (0..smithy_lvl).map(|i| i < level).collect(),
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

    let build_active = ctx
        .accounts
        .active_builds(village.id)
        .await
        .unwrap_or_default();
    let upgrade = building_upgrade_row(
        &ctx.rules,
        village.tribe,
        amounts,
        &build_active,
        BuildingKind::Smithy,
        smithy_lvl,
    );
    page(&SmithyTemplate {
        world: world_id_str(ctx.world_id),
        tribe_slug: village.tribe.map_or("", |t| t.slug()),
        village_id: village_seg(village.id),
        village_label: format!("({}|{})", village.coordinate.x, village.coordinate.y),
        has_smithy: smithy_lvl > 0,
        smithy_level: smithy_lvl,
        ribbon: resource_ribbon(&economy),
        rows,
        active,
        active_portrait,
        upgrade,
    })
}

/// Research/upgrade form fields.
#[derive(Deserialize)]
pub struct UnitForm {
    unit: String,
}

/// Order a unit research for the path village, then return to the Academy (Player only, P4).
pub async fn research_submit(
    ctx: GameContext,
    Path((_world, village)): Path<(String, String)>,
    Form(form): Form<UnitForm>,
) -> Response {
    let player = ctx.player;
    let flash = order_research(
        &ctx.accounts,
        &ctx.accounts,
        &ctx.accounts,
        &ctx.rules.economy,
        &ctx.rules.units,
        ctx.speed,
        now(),
        player,
        selected_village(Some(&village)),
        UnitId(form.unit),
    )
    .await
    .err()
    .map(|e| {
        tracing::warn!(error = %e, "research order rejected");
        user_msg(e.to_string())
    });
    with_flash(
        redirect_to_village_leaf(ctx.world_id, &village, "/academy"),
        flash,
    )
}

/// The three static training routes (064): `/village/{village}/barracks|stable|workshop`. Each fixes its
/// `BuildingKind` and shares [`troops`]; a `{building}` capture would conflict with the static `academy`/
/// `smithy`/… siblings, so they are explicit.
pub async fn troops_barracks(
    ctx: GameContext,
    Path((_world, village)): Path<(String, String)>,
) -> Response {
    troops(ctx, village, BuildingKind::Barracks).await
}

pub async fn troops_stable(
    ctx: GameContext,
    Path((_world, village)): Path<(String, String)>,
) -> Response {
    troops(ctx, village, BuildingKind::Stable).await
}

pub async fn troops_workshop(
    ctx: GameContext,
    Path((_world, village)): Path<(String, String)>,
) -> Response {
    troops(ctx, village, BuildingKind::Workshop).await
}

/// A troop building's training view: researched units it trains, the running batch (005 AC9;
/// Player only, P4). The `{village}` rides in the path (064); the building is fixed by the caller.
async fn troops(ctx: GameContext, village: String, building: BuildingKind) -> Response {
    let player = ctx.player;
    let (village, economy) = match village_view_data(&ctx, selected_village(Some(&village))).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let Some(tribe) = village.tribe else {
        tracing::error!(?player, "village has no tribe");
        return server_error();
    };
    let (researched, active) = match tokio::try_join!(
        ctx.accounts.researched_units(village.id),
        ctx.accounts.active_training(village.id),
    ) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(error = %e, "troop view lookup failed");
            return server_error();
        }
    };
    let unit_rules: &UnitRules = &ctx.rules.units;
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
                portrait: format!("{}_{}", tribe.slug(), spec.id.as_str()),
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
                    ctx.speed,
                )),
                time_secs: per_unit_time_secs(
                    spec.train_secs,
                    building_level,
                    &unit_rules.training,
                    ctx.speed,
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

    let build_active = ctx
        .accounts
        .active_builds(village.id)
        .await
        .unwrap_or_default();
    let upgrade = building_upgrade_row(
        &ctx.rules,
        village.tribe,
        economy.amounts,
        &build_active,
        building,
        building_level,
    );
    page(&TroopsTemplate {
        world: world_id_str(ctx.world_id),
        tribe_slug: village.tribe.map_or("", |t| t.slug()),
        village_id: village_seg(village.id),
        village_label: format!("({}|{})", village.coordinate.x, village.coordinate.y),
        ribbon: resource_ribbon(&economy),
        building: building_label(building),
        building_level,
        has_building: building_level > 0,
        rows,
        active: active_view,
        upgrade,
    })
}

/// Training form fields.
#[derive(Deserialize)]
pub struct TrainForm {
    unit: String,
    count: u32,
}

/// Order a training batch for the path village, then return to the building page (Player only, P4).
pub async fn train_submit(
    ctx: GameContext,
    Path((_world, village)): Path<(String, String)>,
    Form(form): Form<TrainForm>,
) -> Response {
    let player = ctx.player;
    let unit = UnitId(form.unit);
    let flash = order_train(
        &ctx.accounts,
        &ctx.accounts,
        &ctx.accounts,
        &ctx.accounts,
        &ctx.rules.economy,
        &ctx.rules.units,
        ctx.speed,
        now(),
        player,
        selected_village(Some(&village)),
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
        .find_map(|t| ctx.rules.units.unit(t, &unit))
        .map(|s| s.trained_in);
    let leaf = match building {
        Some(BuildingKind::Barracks) => "/barracks",
        Some(BuildingKind::Stable) => "/stable",
        Some(BuildingKind::Workshop) => "/workshop",
        _ => "",
    };
    with_flash(
        redirect_to_village_leaf(ctx.world_id, &village, leaf),
        flash,
    )
}

/// Order a Smithy upgrade for the player's village, then return to the Smithy (Player only, P4).
pub async fn smithy_upgrade_submit(
    ctx: GameContext,
    Path((_world, village)): Path<(String, String)>,
    Form(form): Form<UnitForm>,
) -> Response {
    let player = ctx.player;
    let flash = order_smithy_upgrade(
        &ctx.accounts,
        &ctx.accounts,
        &ctx.accounts,
        &ctx.rules.economy,
        &ctx.rules.units,
        ctx.speed,
        now(),
        player,
        selected_village(Some(&village)),
        UnitId(form.unit),
    )
    .await
    .err()
    .map(|e| {
        tracing::warn!(error = %e, "smithy upgrade rejected");
        user_msg(e.to_string())
    });
    with_flash(
        redirect_to_village_leaf(ctx.world_id, &village, "/smithy"),
        flash,
    )
}

/// The Rally Point: the garrison troops that can be sent to reinforce (007 AC7; Player only, P4).
pub async fn rally(
    ctx: GameContext,
    Path((_world, village)): Path<(String, String)>,
    Query(q): Query<MapQuery>,
) -> Response {
    let player = ctx.player;
    let (village, economy) = match village_view_data(&ctx, selected_village(Some(&village))).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let Some(tribe) = village.tribe else {
        tracing::error!(?player, "village has no tribe");
        return server_error();
    };
    let garrison = match ctx.accounts.garrison(village.id).await {
        Ok(g) => g,
        Err(e) => {
            tracing::error!(error = %e, "garrison lookup failed");
            return server_error();
        }
    };
    let roster = ctx.rules.units.roster(tribe);
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
    let target_is_oasis = target.is_some_and(|c| ctx.map.oasis_bonus_at(c).is_some());
    // The Settle order is offered only with a free expansion slot (013 AC11).
    let can_settle = match load_culture(
        &ctx.accounts,
        &ctx.accounts,
        &ctx.rules.culture,
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
    let build_active = ctx
        .accounts
        .active_builds(village.id)
        .await
        .unwrap_or_default();
    let upgrade = building_upgrade_row(
        &ctx.rules,
        village.tribe,
        economy.amounts,
        &build_active,
        BuildingKind::RallyPoint,
        building_level(&village, BuildingKind::RallyPoint),
    );
    page(&RallyTemplate {
        world: world_id_str(ctx.world_id),
        tribe_slug: village.tribe.map_or("", |t| t.slug()),
        village_id: village_seg(village.id),
        village_label: format!("({}|{})", village.coordinate.x, village.coordinate.y),
        ribbon: resource_ribbon(&economy),
        units,
        target_x: q.x,
        target_y: q.y,
        target_is_oasis,
        can_settle,
        settlers_per_village: ctx.rules.culture.settlers_per_village,
        origin_x: village.coordinate.x,
        origin_y: village.coordinate.y,
        radius: i32::try_from(ctx.radius).unwrap_or(i32::MAX),
        speed_mult: ctx.speed.multiplier(),
        upgrade,
    })
}

/// Send a reinforcement from the Rally Point, then return to the village (Player only, P4).
///
/// The composition arrives as `count_<unit-slug>` fields alongside the target `x`/`y`; counts are
/// parsed and re-validated server-side (P4) — the use-case rejects anything over the garrison.
pub async fn rally_send(
    ctx: GameContext,
    Path((_world, village)): Path<(String, String)>,
    Form(form): Form<std::collections::HashMap<String, String>>,
) -> Response {
    let player = ctx.player;
    let selected = selected_village(Some(&village));
    let x = form.get("x").and_then(|s| s.trim().parse::<i32>().ok());
    let y = form.get("y").and_then(|s| s.trim().parse::<i32>().ok());
    let (Some(x), Some(y)) = (x, y) else {
        return redirect_to_village_leaf(ctx.world_id, &village, "/rally");
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
                &ctx.accounts,
                &ctx.accounts,
                &ctx.accounts,
                &ctx.accounts,
                &ctx.rules.economy,
                &ctx.rules.units,
                &ctx.rules.culture,
                ctx.map.as_ref(),
                ctx.speed,
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
            &ctx.accounts,
            &ctx.accounts,
            &ctx.accounts,
            &ctx.rules.economy,
            &ctx.rules.units,
            ctx.map.as_ref(),
            ctx.speed,
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
            if ctx.map.oasis_bonus_at(target).is_some() {
                order_oasis_attack(
                    &ctx.accounts,
                    &ctx.accounts,
                    &ctx.accounts,
                    &ctx.rules.economy,
                    &ctx.rules.units,
                    ctx.map.as_ref(),
                    ctx.speed,
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
                    &ctx.accounts,
                    &ctx.accounts,
                    &ctx.accounts,
                    &ctx.accounts,
                    &ctx.rules.economy,
                    &ctx.rules.units,
                    ctx.map.as_ref(),
                    ctx.speed,
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
            if ctx.map.oasis_bonus_at(target).is_some() {
                order_oasis_reinforce(
                    &ctx.accounts,
                    &ctx.accounts,
                    &ctx.accounts,
                    &ctx.rules.economy,
                    &ctx.rules.units,
                    ctx.map.as_ref(),
                    ctx.speed,
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
                    &ctx.accounts,
                    &ctx.accounts,
                    &ctx.accounts,
                    &ctx.rules.economy,
                    &ctx.rules.units,
                    ctx.map.as_ref(),
                    ctx.speed,
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
    with_flash(redirect_to_village_leaf(ctx.world_id, &village, ""), flash)
}

/// Send-back form fields (the host village whose stationed troops to recall).
#[derive(Deserialize)]
pub struct RallyReturnForm {
    host: String,
}

/// Recall the player's troops stationed at a host, then return to the village (Player only, P4).
pub async fn rally_return(
    ctx: GameContext,
    Path((_world, village)): Path<(String, String)>,
    Form(form): Form<RallyReturnForm>,
) -> Response {
    let player = ctx.player;
    let Ok(host) = form.host.trim().parse::<u128>() else {
        return redirect_to_village_leaf(ctx.world_id, &village, "");
    };
    let flash = order_return(
        &ctx.accounts,
        &ctx.accounts,
        &ctx.rules.units,
        ctx.map.as_ref(),
        ctx.speed,
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
    with_flash(redirect_to_village_leaf(ctx.world_id, &village, ""), flash)
}

/// Recall form fields (the oasis tile to recall stationed troops from).
#[derive(Deserialize)]
pub struct OasisRecallForm {
    x: i32,
    y: i32,
}

/// Recall the player's troops stationed at one of their oases, then return to the village (012 AC7;
/// Player only, P4).
pub async fn oasis_recall(
    ctx: GameContext,
    Path((_world, village)): Path<(String, String)>,
    Form(form): Form<OasisRecallForm>,
) -> Response {
    let player = ctx.player;
    let target = Coordinate::new(form.x, form.y);
    let flash = order_oasis_recall(
        &ctx.accounts,
        &ctx.accounts,
        &ctx.rules.units,
        ctx.map.as_ref(),
        ctx.speed,
        now(),
        player,
        selected_village(Some(&village)),
        target,
    )
    .await
    .err()
    .map(|e| {
        tracing::warn!(error = %e, "oasis recall rejected");
        user_msg(e.to_string())
    });
    with_flash(redirect_to_village_leaf(ctx.world_id, &village, ""), flash)
}

/// The Marketplace: the merchant pool (free/total + capacity) and a send-resources form (008 AC6;
/// Player only, P4).
pub async fn market(ctx: GameContext, Path((_world, village)): Path<(String, String)>) -> Response {
    let player = ctx.player;
    let (village, economy) = match village_view_data(&ctx, selected_village(Some(&village))).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let village_id = village_seg(village.id);
    let village_label = format!("({}|{})", village.coordinate.x, village.coordinate.y);
    let ribbon = resource_ribbon(&economy);
    let Some(tribe) = village.tribe else {
        tracing::error!(?player, "village has no tribe");
        return server_error();
    };
    let level = building_level(&village, BuildingKind::Marketplace);
    let build_active = ctx
        .accounts
        .active_builds(village.id)
        .await
        .unwrap_or_default();
    if level == 0 {
        return page(&MarketTemplate {
            world: world_id_str(ctx.world_id),
            tribe_slug: village.tribe.map_or("", |t| t.slug()),
            village_id,
            village_label,
            ribbon,
            has_marketplace: false,
            capacity: 0,
            free: 0,
            total: 0,
            merchant_speed: 0,
            origin_x: 0,
            origin_y: 0,
            radius: 0,
            speed_mult: ctx.speed.multiplier(),
            upgrade: building_upgrade_row(
                &ctx.rules,
                village.tribe,
                economy.amounts,
                &build_active,
                BuildingKind::Marketplace,
                level,
            ),
        });
    }
    let committed = match ctx.accounts.committed_merchants(village.id).await {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, "committed merchants lookup failed");
            return server_error();
        }
    };
    let total = ctx.rules.merchant.merchants_total(level);
    let profile = ctx.rules.merchant.profile(tribe);
    page(&MarketTemplate {
        world: world_id_str(ctx.world_id),
        tribe_slug: village.tribe.map_or("", |t| t.slug()),
        village_id,
        village_label,
        ribbon,
        has_marketplace: true,
        capacity: profile.capacity,
        free: total.saturating_sub(committed),
        total,
        merchant_speed: profile.speed,
        origin_x: village.coordinate.x,
        origin_y: village.coordinate.y,
        radius: i32::try_from(ctx.radius).unwrap_or(i32::MAX),
        speed_mult: ctx.speed.multiplier(),
        upgrade: building_upgrade_row(
            &ctx.rules,
            village.tribe,
            economy.amounts,
            &build_active,
            BuildingKind::Marketplace,
            level,
        ),
    })
}

/// Send a resource shipment from the Marketplace, then return to the village (Player only, P4).
///
/// The amounts arrive as `amount_<resource>` fields alongside the target `x`/`y`; they are parsed
/// and re-validated server-side (P4) — the use-case rejects an over-stored or over-merchant load.
pub async fn market_send(
    ctx: GameContext,
    Path((_world, village)): Path<(String, String)>,
    Form(form): Form<std::collections::HashMap<String, String>>,
) -> Response {
    let player = ctx.player;
    let x = form.get("x").and_then(|s| s.trim().parse::<i32>().ok());
    let y = form.get("y").and_then(|s| s.trim().parse::<i32>().ok());
    let (Some(x), Some(y)) = (x, y) else {
        return redirect_to_village_leaf(ctx.world_id, &village, "/market");
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
        &ctx.accounts,
        &ctx.accounts,
        &ctx.rules.economy,
        &ctx.rules.units,
        &ctx.rules.merchant,
        ctx.map.as_ref(),
        ctx.speed,
        now(),
        player,
        selected_village(Some(&village)),
        Coordinate::new(x, y),
        bundle,
    )
    .await
    .err()
    .map(|e| {
        tracing::warn!(error = %e, "trade order rejected");
        user_msg(e.to_string())
    });
    with_flash(redirect_to_village_leaf(ctx.world_id, &village, ""), flash)
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
fn scout_report_row(world: WorldId, r: &ScoutReportView) -> ReportRow {
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
        href: world_path(world, &format!("/reports/scout/{}", r.id)),
    }
}

/// The player's reports inbox — battle reports (009) and scout reports (010), newest first (P4).
pub async fn reports(ctx: GameContext) -> Response {
    let player = ctx.player;
    let battle = match ctx.accounts.reports_for(player, 50).await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "reports lookup failed");
            return server_error();
        }
    };
    let scouts = match ctx.accounts.scout_reports_for(player, 50).await {
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
                href: world_path(ctx.world_id, &format!("/reports/{}", r.id)),
            }
        })
        .collect();
    rows.extend(scouts.iter().map(|r| scout_report_row(ctx.world_id, r)));
    // 016 AC3/AC12: battles where the player **reinforced** an ally — their own report (the owner's
    // own defenses are already above as `defender_player`). Informational rows (no separate detail).
    let defended = match reinforcement_reports(&ctx.accounts, &ctx.rules.ranking, player).await {
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
    page(&ReportsTemplate {
        world: world_id_str(ctx.world_id),
        reports: rows,
    })
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
    world: WorldId,
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
                href: world_path(world, &format!("/stats/player/{}", r.player.0)),
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
fn alliance_rows(world: WorldId, rows: Vec<AllianceLeaderboardRow>) -> Vec<LeaderboardRowView> {
    rows.into_iter()
        .enumerate()
        .map(|(i, r)| LeaderboardRowView {
            rank: i + 1,
            name: r.name,
            tag: r.tag,
            href: world_path(world, &format!("/stats/alliance/{}", r.alliance.0)),
            value: r.value,
            has_presence: false,
            online: false,
            presence_label: String::new(),
        })
        .collect()
}

/// Public leaderboards (016 AC2/AC5/AC6/AC8): population / attackers / defenders / raiders + the
/// alliance aggregates, filterable by quadrant and (for conflict boards) time window.
pub async fn leaderboard(world: WorldScope, Query(q): Query<LeaderboardQuery>) -> Response {
    let scope_key = q.scope.unwrap_or_else(|| "world".to_owned());
    let window_key = q.window.unwrap_or_else(|| "all".to_owned());
    let scope = parse_scope(&scope_key);
    let repo = &world.accounts;
    let econ = &world.rules.economy;
    let rules = &world.rules.ranking;
    let window = parse_window(&window_key, rules);
    let now_ts = now();
    let online_secs = world.rules.lifecycle.presence_online_secs;

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
                    .map(|r| player_rows(world.world_id, r, now_ts, online_secs)),
                "Attack points",
                false,
                true,
            ),
            "defenders" => (
                conflict_leaderboard(repo, rules, ConflictMetric::Defense, scope, window, now_ts)
                    .await
                    .map(|r| player_rows(world.world_id, r, now_ts, online_secs)),
                "Defense points",
                false,
                true,
            ),
            "raiders" => (
                conflict_leaderboard(repo, rules, ConflictMetric::Raided, scope, window, now_ts)
                    .await
                    .map(|r| player_rows(world.world_id, r, now_ts, online_secs)),
                "Resources looted",
                false,
                true,
            ),
            "climbers" => (
                climbers_leaderboard(repo, rules, scope)
                    .await
                    .map(|r| player_rows(world.world_id, r, now_ts, online_secs)),
                "Population gained",
                false,
                false,
            ),
            "alliances" => (
                alliance_population_leaderboard(repo, econ, rules, scope)
                    .await
                    .map(|r| alliance_rows(world.world_id, r)),
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
                .map(|r| alliance_rows(world.world_id, r)),
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
                .map(|r| alliance_rows(world.world_id, r)),
                "Defense points",
                true,
                true,
            ),
            _ => (
                population_leaderboard(repo, econ, rules, scope)
                    .await
                    .map(|r| player_rows(world.world_id, r, now_ts, online_secs)),
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
        world: world_id_str(world.world_id),
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
pub async fn wonder(world: WorldScope) -> Response {
    let repo = &world.accounts;
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
        world: world_id_str(world.world_id),
        winner,
        max_level: eperica_domain::MAX_WONDER_LEVEL,
        standings,
    })
}

/// Order one level of Wonder construction on a controlled site (021 AC4) — the only path that builds a
/// Wonder; gating (site control + alliance holds a plan + level < 100) is server-side.
pub async fn wonder_build_submit(ctx: GameContext, Form(form): Form<WonderBuildForm>) -> Response {
    let player = ctx.player;
    let flash = order_wonder_build(
        &ctx.accounts,
        &ctx.accounts,
        &ctx.accounts,
        &ctx.accounts,
        &ctx.accounts,
        &ctx.rules.economy,
        &ctx.rules.build,
        &ctx.rules.units,
        ctx.speed,
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
    // Return to the site village if the form named one (064), else the canonical entry → capital.
    let back = match form.village.as_deref().map(str::trim) {
        Some(v) if !v.is_empty() => redirect_to_village_leaf(ctx.world_id, v, ""),
        _ => Redirect::to(&world_path(ctx.world_id, "/village")).into_response(),
    };
    with_flash(back, flash)
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
                    name: w.name,
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
        default_artifact_days: state.artifact_release_offset_secs / 86_400,
        default_wonder_days: state.wonder_release_offset_secs / 86_400,
        presets: KNOWN_PRESETS.iter().map(|p| (*p).to_owned()).collect(),
        default_preset: DEFAULT_PRESET.to_owned(),
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
    /// End-game release schedule in **days** (047) — omitted ⇒ the operator's env default.
    #[serde(default)]
    artifact_days: Option<i64>,
    #[serde(default)]
    wonder_days: Option<i64>,
    /// The rule preset the world plays under (052) — omitted ⇒ `classic` (the default).
    #[serde(default)]
    preset: Option<String>,
    /// The world's display name (056) — shown to players in the lobby/nav.
    #[serde(default)]
    name: String,
}

/// Create a new world from the admin console and start it running live (041 AC1/AC2). Admin-gated on the
/// real human; the new world's scheduler is started through the registry — no restart.
pub async fn admin_world_submit(
    State(state): State<AppState>,
    RealUser(player): RealUser,
    Form(form): Form<CreateWorldForm>,
) -> Response {
    // Resolve the end-game schedule: the form's per-world override (days → seconds), or the operator's
    // env-configured default when a field is omitted (047). Bounds (`0 < artifact < wonder`) are enforced
    // server-side in the use case.
    const SECS_PER_DAY: i64 = 86_400;
    let artifact_offset = form
        .artifact_days
        .map_or(state.artifact_release_offset_secs, |d| {
            d.saturating_mul(SECS_PER_DAY)
        });
    let wonder_offset = form
        .wonder_days
        .map_or(state.wonder_release_offset_secs, |d| {
            d.saturating_mul(SECS_PER_DAY)
        });
    // The chosen preset (052), defaulting to `classic`. Validate against the server-authoritative allow-list
    // here (P4) — an unknown name is rejected before any row is written.
    let preset = form.preset.as_deref().unwrap_or(DEFAULT_PRESET);
    if !known_preset(preset) {
        return with_flash(
            Redirect::to("/admin").into_response(),
            Some("Unknown rule preset.".to_owned()),
        );
    }
    match admin_create_world_uc(
        state.accounts.as_ref(),
        state.accounts.as_ref(),
        player,
        form.speed,
        form.radius,
        artifact_offset,
        wonder_offset,
        preset,
        form.name.trim(),
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
        return Redirect::to("/worlds").into_response();
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
    with_flash(Redirect::to("/worlds").into_response(), flash)
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
pub async fn search_page(world: WorldScope, Query(sq): Query<SearchQuery>) -> Response {
    let query = sq.q.unwrap_or_default();
    let trimmed = query.trim().to_owned();
    if trimmed.is_empty() {
        return page(&SearchTemplate {
            world: world_id_str(world.world_id),
            query,
            searched: false,
            players: Vec::new(),
            alliances: Vec::new(),
            coordinate_href: None,
            coordinate_label: String::new(),
        });
    }
    let results = match search(&world.accounts, &world.accounts, &trimmed).await {
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
            href: world_path(world.world_id, &format!("/stats/player/{}", p.player.0)),
            label: p.name,
        })
        .collect();
    let alliances = results
        .alliances
        .into_iter()
        .map(|a| SearchHitRow {
            href: world_path(world.world_id, &format!("/stats/alliance/{}", a.alliance.0)),
            label: format!("{} [{}]", a.name, a.tag),
        })
        .collect();
    let (coordinate_href, coordinate_label) = match results.coordinate {
        Some(c) => (
            Some(world_path(
                world.world_id,
                &format!("/map?x={}&y={}", c.x, c.y),
            )),
            format!("({}|{})", c.x, c.y),
        ),
        None => (None, String::new()),
    };
    page(&SearchTemplate {
        world: world_id_str(world.world_id),
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
    world: WorldScope,
    // Under `/w/{world}/…` the route captures both `world` and `id`; the world is read by the extractor
    // (056), so take it as a 2-tuple and use only the id.
    axum::extract::Path((_world, id)): axum::extract::Path<(String, String)>,
) -> Response {
    let Ok(pid) = id.parse::<u128>() else {
        return not_found();
    };
    let repo = &world.accounts;
    let s = match player_statistics(repo, &world.rules.economy, PlayerId(pid)).await {
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
        world.rules.lifecycle.presence_online_secs,
    );
    let mut achievements: Vec<AchievementRowView> = held
        .iter()
        .map(|a| AchievementRowView {
            label: achievement_label(&a.0).to_owned(),
        })
        .collect();
    achievements.sort_by(|a, b| a.label.cmp(&b.label));
    page(&PlayerStatsTemplate {
        world: world_id_str(world.world_id),
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
    MaybeRealUser(me): MaybeRealUser,
) -> Response {
    // Visitor-safe (055): a non-redirecting extractor, so a logged-out poller gets an empty `200` — never a
    // redirect to the login HTML, which the banner JS would otherwise render as raw markup.
    let name = match me {
        Some(me) => sit_owner_name(&state, &jar, me).await.unwrap_or_default(),
        None => String::new(),
    };
    (StatusCode::OK, name).into_response()
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
            (jar, Redirect::to("/worlds")).into_response()
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
pub async fn quests_page(ctx: GameContext) -> Response {
    let player = ctx.player;
    // Lazily complete anything now satisfied — best-effort, must not break the page.
    if let Err(e) =
        evaluate_quests(&ctx.accounts, &ctx.rules.economy, &ctx.rules.quests, player).await
    {
        tracing::error!(error = %e, "quest evaluation failed");
    }
    let repo = &ctx.accounts;
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
    // Capital (else first) village — only used for the nav back-link (064: the hyphenated-UUID path segment).
    let village_id = villages
        .iter()
        .find(|v| v.is_capital)
        .or_else(|| villages.first())
        .map(|v| village_seg(v.id))
        .unwrap_or_default();
    let chain = &ctx.rules.quests;
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
        world: world_id_str(ctx.world_id),
        village_id,
        current,
        completed: done,
    })
}

/// Public alliance statistics page (016 AC10).
pub async fn alliance_stats_page(
    world: WorldScope,
    // Under `/w/{world}/…` the route captures both `world` and `id`; the world is read by the extractor
    // (056), so take it as a 2-tuple and use only the id.
    axum::extract::Path((_world, id)): axum::extract::Path<(String, String)>,
) -> Response {
    let Ok(aid) = id.parse::<u128>() else {
        return not_found();
    };
    let repo = &world.accounts;
    let s = match alliance_statistics(repo, &world.rules.economy, AllianceId(aid)).await {
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
        world: world_id_str(world.world_id),
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
                    href: world_path(world.world_id, &format!("/stats/player/{}", player.0)),
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
    ctx: GameContext,
    // Under `/w/{world}/…` the route captures both `world` and `id`; the world is read by the extractor
    // (056), so take it as a 2-tuple and use only the id.
    axum::extract::Path((_world, id)): axum::extract::Path<(String, String)>,
) -> Response {
    let player = ctx.player;
    let Ok(id) = id.parse::<u128>() else {
        return Redirect::to(&world_path(ctx.world_id, "/reports")).into_response();
    };
    let r = match ctx.accounts.scout_report(id, player).await {
        Ok(Some(r)) => r,
        Ok(None) => return Redirect::to(&world_path(ctx.world_id, "/reports")).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "scout report lookup failed");
            return server_error();
        }
    };
    let unit_rules = &ctx.rules.units;
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
        world: world_id_str(ctx.world_id),
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
    ctx: GameContext,
    // Under `/w/{world}/…` the route captures both `world` and `id`; the world is read by the extractor
    // (056), so take it as a 2-tuple and use only the id.
    axum::extract::Path((_world, id)): axum::extract::Path<(String, String)>,
) -> Response {
    let player = ctx.player;
    let Ok(id) = id.parse::<u128>() else {
        return Redirect::to(&world_path(ctx.world_id, "/reports")).into_response();
    };
    let report = match ctx.accounts.report(id, player).await {
        Ok(Some(r)) => r,
        Ok(None) => return Redirect::to(&world_path(ctx.world_id, "/reports")).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "report lookup failed");
            return server_error();
        }
    };
    let unit_rules = &ctx.rules.units;
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
        world: world_id_str(ctx.world_id),
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
pub async fn alliance(ctx: GameContext) -> Response {
    let player = ctx.player;
    let repo = &ctx.accounts;
    let rules = &ctx.rules.alliance;
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
                world: world_id_str(ctx.world_id),
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
                world: world_id_str(ctx.world_id),
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

pub async fn alliance_found(ctx: GameContext, Form(form): Form<FoundForm>) -> Response {
    let player = ctx.player;
    let flash = found_alliance(
        &ctx.accounts,
        &ctx.rules.alliance,
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
    with_flash(
        Redirect::to(&world_path(ctx.world_id, "/alliance")).into_response(),
        flash,
    )
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
    ctx: GameContext,
    Form(form): Form<UsernameForm>,
) -> Response {
    let player = ctx.player;
    let flash = match resolve_player(&state, &form.username).await {
        Some(invitee) => invite_player(&ctx.accounts, player, invitee)
            .await
            .err()
            .map(|e| {
                tracing::warn!(error = %e, "invite rejected");
                user_msg(e.to_string())
            }),
        None => Some("No player with that name.".to_owned()),
    };
    with_flash(
        Redirect::to(&world_path(ctx.world_id, "/alliance")).into_response(),
        flash,
    )
}

pub async fn alliance_revoke(
    State(state): State<AppState>,
    ctx: GameContext,
    Form(form): Form<UsernameForm>,
) -> Response {
    let player = ctx.player;
    let flash = match resolve_player(&state, &form.username).await {
        Some(invitee) => revoke_invite(&ctx.accounts, player, invitee)
            .await
            .err()
            .map(|e| {
                tracing::warn!(error = %e, "revoke rejected");
                user_msg(e.to_string())
            }),
        None => Some("No player with that name.".to_owned()),
    };
    with_flash(
        Redirect::to(&world_path(ctx.world_id, "/alliance")).into_response(),
        flash,
    )
}

#[derive(Deserialize)]
pub struct RespondForm {
    alliance: String,
    accept: bool,
}

pub async fn alliance_respond(ctx: GameContext, Form(form): Form<RespondForm>) -> Response {
    let player = ctx.player;
    let flash = match form.alliance.parse::<u128>() {
        Ok(id) => respond_invite(
            &ctx.accounts,
            &ctx.rules.alliance,
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
    with_flash(
        Redirect::to(&world_path(ctx.world_id, "/alliance")).into_response(),
        flash,
    )
}

pub async fn alliance_leave(ctx: GameContext) -> Response {
    let player = ctx.player;
    let flash = leave_alliance(&ctx.accounts, player).await.err().map(|e| {
        tracing::warn!(error = %e, "leave rejected");
        user_msg(e.to_string())
    });
    with_flash(
        Redirect::to(&world_path(ctx.world_id, "/alliance")).into_response(),
        flash,
    )
}

pub async fn alliance_disband(ctx: GameContext) -> Response {
    let player = ctx.player;
    let flash = disband_alliance(&ctx.accounts, player)
        .await
        .err()
        .map(|e| {
            tracing::warn!(error = %e, "disband rejected");
            user_msg(e.to_string())
        });
    with_flash(
        Redirect::to(&world_path(ctx.world_id, "/alliance")).into_response(),
        flash,
    )
}

#[derive(Deserialize)]
pub struct TargetForm {
    target: String,
}

pub async fn alliance_expel(ctx: GameContext, Form(form): Form<TargetForm>) -> Response {
    let player = ctx.player;
    let flash = match form.target.parse::<u128>() {
        Ok(id) => expel_member(&ctx.accounts, player, PlayerId(id))
            .await
            .err()
            .map(|e| {
                tracing::warn!(error = %e, "expel rejected");
                user_msg(e.to_string())
            }),
        Err(_) => None,
    };
    with_flash(
        Redirect::to(&world_path(ctx.world_id, "/alliance")).into_response(),
        flash,
    )
}

pub async fn alliance_transfer(ctx: GameContext, Form(form): Form<TargetForm>) -> Response {
    let player = ctx.player;
    let flash = match form.target.parse::<u128>() {
        Ok(id) => transfer_founder(&ctx.accounts, player, PlayerId(id))
            .await
            .err()
            .map(|e| {
                tracing::warn!(error = %e, "transfer rejected");
                user_msg(e.to_string())
            }),
        Err(_) => None,
    };
    with_flash(
        Redirect::to(&world_path(ctx.world_id, "/alliance")).into_response(),
        flash,
    )
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

pub async fn alliance_role(ctx: GameContext, Form(form): Form<RoleForm>) -> Response {
    let player = ctx.player;
    let Ok(id) = form.target.parse::<u128>() else {
        return Redirect::to(&world_path(ctx.world_id, "/alliance")).into_response();
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
    let flash = set_member_role(&ctx.accounts, player, PlayerId(id), role, rights)
        .await
        .err()
        .map(|e| {
            tracing::warn!(error = %e, "role change rejected");
            user_msg(e.to_string())
        });
    with_flash(
        Redirect::to(&world_path(ctx.world_id, "/alliance")).into_response(),
        flash,
    )
}

#[derive(Deserialize)]
pub struct DiplomacyForm {
    other: String,
    command: String,
}

pub async fn alliance_diplomacy(ctx: GameContext, Form(form): Form<DiplomacyForm>) -> Response {
    let player = ctx.player;
    let Ok(id) = form.other.parse::<u128>() else {
        return Redirect::to(&world_path(ctx.world_id, "/alliance")).into_response();
    };
    let command = match form.command.as_str() {
        "declare_war" => DiplomacyCommand::DeclareWar,
        "propose_confederation" => DiplomacyCommand::ProposeConfederation,
        "accept_confederation" => DiplomacyCommand::AcceptConfederation,
        "cancel" => DiplomacyCommand::Cancel,
        _ => return Redirect::to(&world_path(ctx.world_id, "/alliance")).into_response(),
    };
    let flash = set_diplomacy(
        &ctx.accounts,
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
    with_flash(
        Redirect::to(&world_path(ctx.world_id, "/alliance")).into_response(),
        flash,
    )
}

// ---- Alliance forum (027) ----

/// The alliance forum thread list (027 AC1, members only). Shows the announcement checkbox only when the
/// viewer holds the `Announce` right.
pub async fn forum_page(ctx: GameContext) -> Response {
    let player = ctx.player;
    let repo = &ctx.accounts;
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
        world: world_id_str(ctx.world_id),
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
pub async fn forum_new(ctx: GameContext, Form(form): Form<NewThreadForm>) -> Response {
    let player = ctx.player;
    let announcement = form.announcement.as_deref() == Some("1");
    match start_thread(
        &ctx.accounts,
        player,
        &form.title,
        &form.body,
        announcement,
        now(),
    )
    .await
    {
        Ok(id) => Redirect::to(&world_path(ctx.world_id, &format!("/alliance/forum/{id}")))
            .into_response(),
        Err(ForumError::NotAMember | ForumError::MissingRight) => forbidden(),
        Err(_) => Redirect::to(&world_path(ctx.world_id, "/alliance/forum")).into_response(),
    }
}

/// A single forum thread + its posts (027 AC1, members of the owning alliance only).
pub async fn forum_thread_page(
    ctx: GameContext,
    Path((_world, id)): Path<(String, String)>,
) -> Response {
    let player = ctx.player;
    let Ok(tid) = id.parse::<u128>() else {
        return not_found();
    };
    match open_thread(&ctx.accounts, player, tid).await {
        Ok((head, posts)) => page(&ForumThreadTemplate {
            world: world_id_str(ctx.world_id),
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
    ctx: GameContext,
    Path((_world, id)): Path<(String, String)>,
    Form(form): Form<ForumReplyForm>,
) -> Response {
    let player = ctx.player;
    let Ok(tid) = id.parse::<u128>() else {
        return not_found();
    };
    match reply(&ctx.accounts, player, tid, &form.body, now()).await {
        Ok(_) => Redirect::to(&world_path(ctx.world_id, &format!("/alliance/forum/{id}")))
            .into_response(),
        Err(ForumError::NotAMember) => forbidden(),
        Err(ForumError::NotFound) => not_found(),
        Err(_) => Redirect::to(&world_path(ctx.world_id, &format!("/alliance/forum/{id}")))
            .into_response(),
    }
}

// ---------------------------------------------------------------------------------------------------
// Communication: conversations (DMs + chat channels) — 024.
// ---------------------------------------------------------------------------------------------------

/// The account's conversations list, aggregated across **all** its worlds (024 AC3 / 060 AC1). One inbox on
/// every page; each row carries its world so it deep-links into `/w/{world}/messages/c/{key}`. `account` is
/// the `users` id; each world contributes its DM threads + global + alliance channels via the per-world repo.
pub async fn messages(State(state): State<AppState>, AuthUser(account): AuthUser) -> Response {
    let now_ts = now();
    let worlds = match state.accounts.worlds_of_user(account).await {
        Ok(w) => w,
        Err(e) => {
            tracing::error!(error = %e, "worlds_of_user failed");
            return server_error();
        }
    };
    // (world-uuid string, that world's online-window, the conversation) — merged then newest-first sorted.
    let mut rows: Vec<(String, i64, eperica_application::ConversationSummary)> = Vec::new();
    for pw in worlds {
        let Some((repo, rules)) = state.world_registry.comms_context_for(pw.world).await else {
            continue;
        };
        match conversation_list(&repo, &repo, account, pw.player).await {
            Ok(list) => {
                let world_str = uuid::Uuid::from_u128(pw.world.0).to_string();
                let online_secs = rules.lifecycle.presence_online_secs;
                rows.extend(
                    list.into_iter()
                        .map(|c| (world_str.clone(), online_secs, c)),
                );
            }
            Err(e) => tracing::error!(error = %e, "conversation list failed"),
        }
    }
    rows.sort_by_key(|(_, _, c)| std::cmp::Reverse(c.last_ms));
    let conversations = rows
        .into_iter()
        .map(|(world, online_secs, c)| {
            // DM rows carry the other party's activity; channels do not.
            let (has_presence, online, presence_label) = match c.other_last_activity {
                Some(ms) => {
                    let (online, label) = presence_view(Timestamp(ms), now_ts, online_secs);
                    (true, online, label)
                }
                None => (false, false, String::new()),
            };
            ConversationRow {
                world,
                key: c.key,
                title: c.title,
                last_body: c.last_body,
                unread: c.unread,
                has_presence,
                online,
                presence_label,
            }
        })
        .collect();
    page(&MessagesTemplate { conversations })
}

/// A single conversation **in its world** (024 AC2 / 060 AC3): history + send box + live region. Nested under
/// `/w/{world}` so `GameContext` resolves the per-world repo (`ctx.accounts`), the account (`ctx.account`, the
/// `users` id for DMs/channels), and the per-world player (`ctx.player`, for alliance access). `key` is the
/// conversation key. Operating, sending, streaming, and the read watermark all stay in this world (AC4).
pub async fn conversation(
    ctx: GameContext,
    Path((_world, key)): Path<(String, String)>,
) -> Response {
    let now = now();
    let online_secs = ctx.rules.lifecycle.presence_online_secs;
    // Resolve the title + load history, access-checked, depending on the key kind. DM headers also
    // carry the other party's presence (025); channels do not.
    let (title, presence, history) = if let Some(other) = parse_dm_key(&key) {
        let (title, other_activity) = match view_profile(&ctx.accounts, other).await {
            Ok(p) => (p.name, p.last_activity),
            Err(eperica_application::ProfileError::NotFound) => return not_found(),
            Err(e) => {
                tracing::error!(error = %e, "dm header profile failed");
                return server_error();
            }
        };
        match open_dm(&ctx.accounts, ctx.account, other, 100, now).await {
            Ok(h) => (title, Some(other_activity), h),
            Err(e) => return comms_error_response(e),
        }
    } else if let Some(channel) = ChatChannel::parse(&key) {
        let title = channel_title(&ctx.accounts, channel).await;
        match open_chat(
            &ctx.accounts,
            &ctx.accounts,
            ctx.account,
            ctx.player,
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
            // `mine` compares against the account (chat/DM authorship is by `users` id).
            mine: m.sender == ctx.account,
        })
        .collect();
    page(&ConversationTemplate {
        world: world_id_str(ctx.world_id),
        key,
        title,
        has_presence,
        online,
        presence_label,
        lines,
    })
}

/// Display title for a channel (the alliance name, or "Global"), resolved in `repo`'s world.
async fn channel_title(
    repo: &eperica_infrastructure::PgAccountRepository,
    channel: ChatChannel,
) -> String {
    use eperica_infrastructure::application::AllianceRepository;
    match channel {
        ChatChannel::Global => "Global".to_owned(),
        ChatChannel::Alliance(a) => repo
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

/// Send into a conversation **in its world** (024 AC1 / 060 AC3) — a DM or a channel, depending on the key.
/// Nested under `/w/{world}`: `GameContext` gives the per-world repo + account/player identities.
pub async fn messages_send(ctx: GameContext, Form(form): Form<SendForm>) -> Response {
    let result = if let Some(other) = parse_dm_key(&form.conversation) {
        send_dm(
            &ctx.accounts,
            &ctx.accounts,
            ctx.account,
            other,
            &form.body,
            now(),
        )
        .await
        .map(|_| ())
    } else {
        send_chat(
            &ctx.accounts,
            &ctx.accounts,
            ctx.account,
            ctx.player,
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
        Redirect::to(&world_path(
            ctx.world_id,
            &format!("/messages/c/{}", form.conversation),
        ))
        .into_response(),
        flash,
    )
}

/// Open (or start) the DM with a player from their profile, in this world (024 AC9 / 060). Nested under
/// `/w/{world}`.
pub async fn messages_with(
    ctx: GameContext,
    Path((_world, id)): Path<(String, String)>,
) -> Response {
    match id.trim().parse::<u128>() {
        Ok(other) => Redirect::to(&world_path(
            ctx.world_id,
            &format!("/messages/c/{}", dm_key(PlayerId(other))),
        ))
        .into_response(),
        // A malformed id → back to the account-level inbox (there is no `/w/{world}/messages`).
        Err(_) => Redirect::to("/messages").into_response(),
    }
}

/// The account's total unread across **all** its worlds (the nav badge polls this — 024 AC4 / 060 AC2).
pub async fn messages_unread(
    State(state): State<AppState>,
    MaybeAuthUser(player): MaybeAuthUser,
) -> Response {
    // Visitor-safe (055): a logged-out poller gets `"0"`, not a redirect to the login HTML.
    let n = match player {
        Some(account) => {
            let mut total = 0i64;
            if let Ok(worlds) = state.accounts.worlds_of_user(account).await {
                for pw in worlds {
                    if let Some((repo, _rules)) =
                        state.world_registry.comms_context_for(pw.world).await
                    {
                        total += unread_badge(&repo, &repo, account, pw.player)
                            .await
                            .unwrap_or(0);
                    }
                }
            }
            total
        }
        None => 0,
    };
    (StatusCode::OK, n.to_string()).into_response()
}

/// Live SSE stream for one conversation in its world (024 AC6 / 060 AC3). Nested under `/w/{world}`;
/// access-checked; emits new lines as they arrive.
pub async fn messages_stream(
    State(state): State<AppState>,
    ctx: GameContext,
    Path((_world, key)): Path<(String, String)>,
) -> Response {
    use eperica_infrastructure::application::AllianceRepository;
    // The broadcast filter key, **world-scoped** (060): `w:<world>:<conv-key>`, matching the world-prefixed
    // key the notify side emits — so a live line never crosses worlds (e.g. world B's `global` post does not
    // reach a world-A `global` stream). For a DM, the conv-key is the **pair-canonical** key derived from
    // (account, other): only the two parties can compute it, so a viewer can never wiretap a third party's
    // thread (the URL key `dm:<other>` is viewer-relative and NOT pair-unique). For a channel, the conv-key
    // is the channel itself, gated by this world's membership (the per-world player).
    let world = world_id_str(ctx.world_id);
    let want = if let Some(other) = parse_dm_key(&key) {
        format!("w:{world}:{}", dm_pair_key(ctx.account, other))
    } else if let Some(channel) = ChatChannel::parse(&key) {
        let alliance = ctx
            .accounts
            .alliance_of(ctx.player)
            .await
            .ok()
            .flatten()
            .map(|m| m.alliance);
        if !can_access_channel(channel, alliance) {
            return forbidden();
        }
        format!("w:{world}:{key}")
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

/// Deep-link for a notification's referenced entity (026), or empty when there's nothing to link to. Every
/// target is prefixed with **the notification's own world** (059/060) — the account feed aggregates across
/// all the account's worlds, so each row links into where it belongs. `world` is the row's hyphenated world
/// UUID. The `dm` target is the world-scoped conversation (060: conversations live under `/w/{world}`).
fn notification_href(world: &str, ref_kind: Option<&str>, ref_id: Option<&str>) -> String {
    match (ref_kind, ref_id) {
        (Some("report"), Some(id)) => format!("/w/{world}/reports/{id}"),
        (Some("dm"), Some(other)) => format!("/w/{world}/messages/c/dm:{other}"),
        (Some("village"), Some(coord)) => match coord.split_once('|') {
            Some((x, y)) => format!("/w/{world}/map?x={x}&y={y}"),
            None => String::new(),
        },
        _ => String::new(),
    }
}

/// The notifications feed (026 AC4/AC5; 059 AC1/AC3, Player only). Aggregates the account's notifications
/// across **all** its worlds, most-recent first, and marks them all read on view (account-scoped) so the bell
/// clears. `player` is the account id (= `user_id`); each row deep-links into its own world.
pub async fn notifications_page(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
) -> Response {
    let repo = state.accounts.as_ref();
    let list = match list_notifications_for_account(repo, player).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!(error = %e, "notifications list failed");
            return server_error();
        }
    };
    // Viewing marks them read across all worlds (026 AC5 / 059 AC3). Best-effort — a failure must not break
    // the page.
    if let Err(e) = mark_notifications_read_for_account(repo, player, now()).await {
        tracing::error!(error = %e, "marking notifications read failed");
    }
    let notifications = list
        .into_iter()
        .map(|n| NotificationRowView {
            label: n.kind.label().to_owned(),
            href: notification_href(&n.world, n.ref_kind.as_deref(), n.ref_id.as_deref()),
            body: n.body,
            read: n.read,
        })
        .collect();
    page(&NotificationsTemplate { notifications })
}

/// The account's unread notification count across all its worlds — the nav bell polls this (026 AC4 / 059 AC2).
pub async fn notifications_unread(
    State(state): State<AppState>,
    MaybeAuthUser(player): MaybeAuthUser,
) -> Response {
    // Visitor-safe (055): a logged-out poller gets `"0"`, not a redirect to the login HTML.
    let n = match player {
        Some(player) => notification_unread_for_account(state.accounts.as_ref(), player)
            .await
            .unwrap_or(0),
        None => 0,
    };
    (StatusCode::OK, n.to_string()).into_response()
}

/// Explicit mark-all-read across all the account's worlds (026 AC5 / 059 AC3, account-scoped).
pub async fn notifications_read(
    State(state): State<AppState>,
    AuthUser(player): AuthUser,
) -> Response {
    if let Err(e) =
        mark_notifications_read_for_account(state.accounts.as_ref(), player, now()).await
    {
        tracing::error!(error = %e, "mark-all-read failed");
        return server_error();
    }
    Redirect::to("/notifications").into_response()
}

/// Live SSE stream for the logged-in player's notification bell (026 AC6). Subscribed on the player's
/// **private** key `notif:<uuid>` — a player can only ever receive their own (no cross-player leak, P4).
pub async fn notifications_stream(
    State(state): State<AppState>,
    MaybeAuthUser(player): MaybeAuthUser,
) -> Response {
    // Visitor-safe (055): the base template opens this EventSource on every page. A logged-out caller gets
    // `204 No Content` — the SSE "do not reconnect" signal — never a `text/html` login redirect (which the
    // browser logs as a MIME-type EventSource error).
    let Some(player) = player else {
        return StatusCode::NO_CONTENT.into_response();
    };
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
