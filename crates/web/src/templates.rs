//! Askama page templates (see specs/ui-style-guide.md for the design system).

use askama::Template;

#[derive(Template)]
#[template(path = "index.html")]
pub struct IndexTemplate;

#[derive(Template)]
#[template(path = "register.html")]
pub struct RegisterTemplate {
    /// An error message to show above the form, if any.
    pub error: Option<String>,
}

#[derive(Template)]
#[template(path = "login.html")]
pub struct LoginTemplate {
    /// An error or notice message to show above the form, if any.
    pub error: Option<String>,
}

#[derive(Template)]
#[template(path = "styleguide.html")]
pub struct StyleGuideTemplate;

#[cfg(test)]
mod tests {
    use super::*;

    fn village(crop_rate: i64) -> VillageTemplate {
        VillageTemplate {
            username: "player".to_owned(),
            village_id: "1".to_owned(),
            is_capital: false,
            villages: Vec::new(),
            cp: 0,
            cp_rate: 4,
            slots_used: 1,
            slots_allowed: 1,
            next_threshold: Some(200),
            has_free_slot: false,
            tribe: "Gauls",
            x: 0,
            y: 0,
            wood: 1,
            clay: 1,
            iron: 1,
            crop: 1,
            wood_rate: 1,
            clay_rate: 1,
            iron_rate: 1,
            crop_rate,
            warehouse: 800,
            granary: 800,
            active: Vec::new(),
            has_academy: false,
            has_smithy: false,
            troop_links: Vec::new(),
            garrison: Vec::new(),
            garrison_upkeep: 0,
            movements: Vec::new(),
            reinforcements_here: Vec::new(),
            reinforcements_abroad: Vec::new(),
            shipments: Vec::new(),
            oases: Vec::new(),
            fields: Vec::new(),
            buildings: Vec::new(),
        }
    }

    // AC7: crop production is flagged when net <= 0, and not when positive.
    #[test]
    fn crop_warning_shown_only_when_net_nonpositive() {
        assert!(village(-5).render().unwrap().contains("starving"));
        assert!(village(0).render().unwrap().contains("starving"));
        assert!(!village(5).render().unwrap().contains("starving"));
    }
}

/// A row in the build UI: a field or building, its level, next-level cost, and orderability.
pub struct BuildRow {
    /// `"field"` or `"building"` (the POST `table` value).
    pub table: &'static str,
    /// Slot number (the POST `slot` value).
    pub slot: u8,
    /// Building kind id for the POST `kind` value (empty for fields).
    pub kind: &'static str,
    /// Display label.
    pub label: String,
    /// Current level (0 = not built, for constructable buildings).
    pub level: u8,
    pub cost_wood: i64,
    pub cost_clay: i64,
    pub cost_iron: i64,
    pub cost_crop: i64,
    /// At max level (no further upgrade).
    pub at_max: bool,
    /// Whether an order can be placed now (affordable, not maxed, none active).
    pub can_order: bool,
}

/// An active build/research/upgrade order, for display + countdown.
pub struct ActiveView {
    /// What is building.
    pub label: String,
    /// The level it reaches.
    pub target_level: u8,
    /// Completion time (Unix-ms UTC), for the client-side countdown.
    pub complete_ms: i64,
}

/// An in-progress order on a unit page (research or upgrade), for display + countdown.
pub struct QueueView {
    /// What is in progress (e.g. "Researching Swordsman").
    pub label: String,
    /// Completion time (Unix-ms UTC), for the client-side countdown.
    pub complete_ms: i64,
}

/// One unit type in the Academy view (004 AC15).
pub struct AcademyRow {
    /// Unit slug for the POST `unit` value.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Role label (Infantry/Cavalry/…).
    pub role: &'static str,
    pub attack: u32,
    pub def_inf: u32,
    pub def_cav: u32,
    pub speed: u32,
    pub carry: u32,
    pub upkeep: u32,
    /// Already researched (incl. tier-1).
    pub researched: bool,
    /// The Research action is offered now (requirements met, affordable, queue free).
    pub can_order: bool,
    /// Why the action is unavailable (requirements text or "insufficient resources"); empty if
    /// researched or orderable.
    pub gate: String,
    pub cost_wood: i64,
    pub cost_clay: i64,
    pub cost_iron: i64,
    pub cost_crop: i64,
    /// Research duration at the current world speed, formatted `h:mm:ss`.
    pub time: String,
}

#[derive(Template)]
#[template(path = "academy.html")]
pub struct AcademyTemplate {
    /// The village this page acts on (carried into the research form + back link, 013 AC11).
    pub village_id: String,
    /// Whether the village has an Academy (otherwise the page only explains the requirement).
    pub has_academy: bool,
    /// The tribe's roster.
    pub rows: Vec<AcademyRow>,
    /// The research in progress, if any.
    pub active: Option<QueueView>,
}

/// One researched unit type in the Smithy view (004 AC15).
pub struct SmithyRow {
    /// Unit slug for the POST `unit` value.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Current upgrade level.
    pub level: u8,
    /// The Upgrade action is offered now.
    pub can_order: bool,
    /// Why the action is unavailable (cap reached, smithy level, insufficient resources); empty
    /// when orderable.
    pub gate: String,
    pub cost_wood: i64,
    pub cost_clay: i64,
    pub cost_iron: i64,
    pub cost_crop: i64,
    /// Upgrade duration at the current world speed, formatted `h:mm:ss`.
    pub time: String,
}

#[derive(Template)]
#[template(path = "smithy.html")]
pub struct SmithyTemplate {
    /// The village this page acts on (carried into the upgrade form + back link, 013 AC11).
    pub village_id: String,
    /// Whether the village has a Smithy (otherwise the page only explains the requirement).
    pub has_smithy: bool,
    /// The Smithy's building level (caps unit levels).
    pub smithy_level: u8,
    /// Researched units with their upgrade state.
    pub rows: Vec<SmithyRow>,
    /// The upgrade in progress, if any.
    pub active: Option<QueueView>,
}

/// One trainable unit row in a troop-building view (005 AC9).
pub struct TrainRow {
    /// Unit slug for the POST `unit` value.
    pub id: String,
    /// Display name.
    pub name: String,
    pub attack: u32,
    pub def_inf: u32,
    pub def_cav: u32,
    pub upkeep: u32,
    /// Per-unit cost.
    pub cost_wood: i64,
    pub cost_clay: i64,
    pub cost_iron: i64,
    pub cost_crop: i64,
    /// Per-unit training time at the current building level and world speed, `h:mm:ss`.
    pub time: String,
    /// The Train form is offered (no batch running at this building).
    pub can_order: bool,
    /// Why the action is unavailable; empty when orderable.
    pub gate: String,
}

#[derive(Template)]
#[template(path = "troops.html")]
pub struct TroopsTemplate {
    /// The village this page acts on (carried into the train form + back link, 013 AC11).
    pub village_id: String,
    /// "Barracks" / "Stable" / "Workshop".
    pub building: &'static str,
    /// Whether the village has this building (otherwise the page explains the requirement).
    pub has_building: bool,
    /// Researched units this building trains.
    pub rows: Vec<TrainRow>,
    /// The running batch, if any.
    pub active: Option<QueueView>,
}

/// One garrison line on the village page (005 AC9).
pub struct GarrisonRow {
    /// Display name.
    pub name: String,
    /// Units stationed.
    pub count: u32,
    /// Crop/hour this line consumes.
    pub upkeep: i64,
}

/// One rendered cell of the map view (006 AC7).
pub struct MapCellView {
    /// The full BEM class list (terrain + optional village/self modifiers).
    pub cell_class: String,
    /// The cell's glyph (terrain mark, or `★` for a village).
    pub glyph: &'static str,
    /// The hover label: full tile description, coordinate, and owner if occupied.
    pub label: String,
    /// A target link for actionable tiles (an oasis → the Rally Point pre-filled with the tile);
    /// `None` for plain terrain (012 AC12).
    pub href: Option<String>,
}

#[derive(Template)]
#[template(path = "map.html")]
pub struct MapTemplate {
    /// The center coordinate the view is built around.
    pub center_x: i32,
    pub center_y: i32,
    /// Map radius, for display.
    pub radius: i32,
    /// Recenter targets (one axis shifted by a full screen).
    pub north_y: i32,
    pub south_y: i32,
    pub east_x: i32,
    pub west_x: i32,
    /// The grid: rows north→south, each west→east.
    pub rows: Vec<Vec<MapCellView>>,
}

/// A trainable garrison unit offered for sending on the Rally Point page (007 AC7).
pub struct RallyUnitRow {
    /// Unit slug (the `count_<id>` form field).
    pub id: String,
    /// Display name.
    pub name: String,
    /// How many are in the garrison (the input's max).
    pub available: u32,
}

#[derive(Template)]
#[template(path = "rally.html")]
pub struct RallyTemplate {
    /// The id of the village these troops are sent from (carried into the form, AC11).
    pub village_id: String,
    /// The garrison units that can be sent (empty hides the form).
    pub units: Vec<RallyUnitRow>,
    /// Pre-filled target tile from a map link (012 AC12), if any.
    pub target_x: Option<i32>,
    pub target_y: Option<i32>,
    /// Whether the pre-filled target is an oasis (hints attack/reinforce over the village modes).
    pub target_is_oasis: bool,
    /// Whether the player has a free expansion slot — offers the **Settle** order (013 AC11).
    pub can_settle: bool,
    /// Settlers a founding consumes (shown in the settle hint).
    pub settlers_per_village: u32,
}

/// An in-flight movement line on the village page (007 AC7).
pub struct MovementRow {
    /// "Reinforcement to (x|y)" / "Returning to (x|y)".
    pub label: String,
    /// Composition summary, e.g. "4 Phalanx, 2 Swordsman".
    pub troops: String,
    /// Arrival time (Unix-ms UTC) for the live countdown.
    pub arrive_ms: i64,
}

/// An occupied-oasis line on the village page (012 AC12): its tile, the bonus it grants, and a
/// recall action.
pub struct OasisRow {
    /// The oasis tile x.
    pub x: i32,
    /// The oasis tile y.
    pub y: i32,
    /// The bonus it grants, e.g. "Oasis +25% wood".
    pub bonus: String,
}

/// A stationed-reinforcement line (here or abroad) on the village page (007 AC7).
pub struct ReinforcementRow {
    /// The counterparty owner's name.
    pub owner: String,
    /// The counterparty village's coordinate, e.g. "(3|4)".
    pub coord: String,
    /// Composition summary.
    pub troops: String,
    /// The host village id (for the send-back action); empty for "stationed here".
    pub host_id: String,
}

/// One line in the reports inbox (009 AC8 / 010 AC12) — a battle or a scout report.
pub struct ReportRow {
    /// When it happened (Unix-ms UTC) for the relative display.
    pub when_ms: i64,
    /// A one-line summary, e.g. "Raid on bob (3|4)" or "Scouted bob (3|4)".
    pub headline: String,
    /// The outcome from this player's perspective ("Victory" / "Intel gathered" / …).
    pub outcome: String,
    /// The detail link for this report (battle vs scout route).
    pub href: String,
}

#[derive(Template)]
#[template(path = "reports.html")]
pub struct ReportsTemplate {
    /// The player's reports, newest first (empty shows a notice).
    pub reports: Vec<ReportRow>,
}

/// One unit line in a battle report's force table (009 AC8): sent/defending and lost.
pub struct ForceRow {
    pub name: String,
    pub count: u32,
    pub lost: u32,
}

#[derive(Template)]
#[template(path = "report.html")]
pub struct ReportTemplate {
    /// "Attack" or "Raid".
    pub kind: &'static str,
    /// The framed summary, e.g. "You raided bob (3|4)".
    pub headline: String,
    /// The outcome from this player's perspective.
    pub outcome: String,
    /// Luck as a signed percentage (e.g. +12).
    pub luck_pct: i64,
    /// Morale as a percentage (≤ 100).
    pub morale_pct: i64,
    pub wall_before: u8,
    pub wall_after: u8,
    pub attacker_name: String,
    pub attacker_rows: Vec<ForceRow>,
    pub defender_name: String,
    pub defender_rows: Vec<ForceRow>,
    /// For the defender of a combined attack: "The enemy also scouted your defenses." (010 AC8).
    pub scouted_note: Option<String>,
    /// Resources looted, formatted (011); `None` when nothing was taken.
    pub loot: Option<String>,
    /// The razed building, e.g. "Warehouse 3 → 1" (011); `None` when none.
    pub razed: Option<String>,
}

/// One revealed resource line in a scout report's intel (010 AC9).
pub struct ScoutResourceRow {
    pub name: String,
    pub amount: i64,
}

#[derive(Template)]
#[template(path = "scout_report.html")]
pub struct ScoutReportTemplate {
    /// The framed summary, e.g. "You scouted bob (3|4)" or "alice (1|2) scouted your village".
    pub headline: String,
    /// A one-line outcome (scouts sent/lost for the scouter; scouts destroyed for the target).
    pub summary: String,
    /// Whether the viewer is the scouter (sees intel) vs the detected target (notification only).
    pub is_scouter: bool,
    /// What was spied on ("Resources" / "Defenses").
    pub target_type: &'static str,
    /// Which intel block to render: "resources", "defenses", or "none".
    pub intel_kind: &'static str,
    /// Revealed resources (when `intel_kind == "resources"`).
    pub resources: Vec<ScoutResourceRow>,
    /// Revealed stationed troops (when `intel_kind == "defenses"`).
    pub troops: Vec<ForceRow>,
    /// The revealed Wall level (when `intel_kind == "defenses"`).
    pub wall_level: u8,
}

/// A garrison-independent Marketplace view (008 AC6): merchant pool + per-tribe capacity.
#[derive(Template)]
#[template(path = "market.html")]
pub struct MarketTemplate {
    /// The village this page acts on (carried into the send form + back link, 013 AC11).
    pub village_id: String,
    /// Whether the village has a Marketplace (otherwise the page only explains the requirement).
    pub has_marketplace: bool,
    /// Total resources one of this tribe's merchants carries.
    pub capacity: u32,
    /// Merchants available to dispatch now.
    pub free: u32,
    /// Merchants the Marketplace provides at its current level.
    pub total: u32,
}

/// An in-flight shipment line on the village page (008 AC6).
pub struct ShipmentRow {
    /// "Shipment to (x|y)" / "Merchants returning from (x|y)".
    pub label: String,
    /// Contents summary, e.g. "300 wood, 50 clay" ("—" for an empty return).
    pub contents: String,
    /// Merchants committed to this leg.
    pub merchants: u32,
    /// Arrival time (Unix-ms UTC) for the live countdown.
    pub arrive_ms: i64,
}

/// One entry in the village switcher (013 AC11): an owned village, the page links to it via
/// `?village=<id>`.
pub struct VillageSwitchRow {
    /// The village id (the `?village=` selector value).
    pub id: String,
    /// Display label, e.g. "(3|4)".
    pub label: String,
    /// Whether this is the player's capital (badged).
    pub is_capital: bool,
    /// Whether this is the village currently shown.
    pub is_current: bool,
}

#[derive(Template)]
#[template(path = "village.html")]
pub struct VillageTemplate {
    /// Owner's username.
    pub username: String,
    /// The shown village's id (carried into action forms + nav links so they target it, AC11).
    pub village_id: String,
    /// Whether the shown village is the player's capital (badged; raises its field cap, AC9/AC10).
    pub is_capital: bool,
    /// Every owned village, for the switcher (more than one ⇒ the switcher renders).
    pub villages: Vec<VillageSwitchRow>,
    /// Pooled culture points settled to now (013 AC1).
    pub cp: i64,
    /// The player's live CP/hour.
    pub cp_rate: i64,
    /// Villages currently held.
    pub slots_used: u32,
    /// Villages the player may hold (the slot gate, AC4).
    pub slots_allowed: u32,
    /// CP the next village requires, or `None` when the threshold table is exhausted.
    pub next_threshold: Option<i64>,
    /// Whether a free expansion slot is available (used < allowed) — enables the settle hint.
    pub has_free_slot: bool,
    /// The village's tribe display name (004).
    pub tribe: &'static str,
    /// Village x coordinate.
    pub x: i32,
    /// Village y coordinate.
    pub y: i32,
    /// Current stored amounts.
    pub wood: i64,
    pub clay: i64,
    pub iron: i64,
    pub crop: i64,
    /// Hourly production (crop is net of upkeep, may be negative).
    pub wood_rate: i64,
    pub clay_rate: i64,
    pub iron_rate: i64,
    pub crop_rate: i64,
    /// Storage capacities.
    pub warehouse: i64,
    pub granary: i64,
    /// The active build orders — at most one per lane (two for Romans, 004 AC13).
    pub active: Vec<ActiveView>,
    /// Whether the village has an Academy (shows the link).
    pub has_academy: bool,
    /// Whether the village has a Smithy (shows the link).
    pub has_smithy: bool,
    /// Built troop buildings, as (label, href) links (005 AC9).
    pub troop_links: Vec<(&'static str, &'static str)>,
    /// The standing garrison (005 AC9); empty hides the panel.
    pub garrison: Vec<GarrisonRow>,
    /// The garrison's total crop upkeep per hour.
    pub garrison_upkeep: i64,
    /// In-flight movements the player owns (007).
    pub movements: Vec<MovementRow>,
    /// Reinforcements stationed at this village (others helping the player, 007).
    pub reinforcements_here: Vec<ReinforcementRow>,
    /// The player's troops stationed abroad, each with a send-back action (007).
    pub reinforcements_abroad: Vec<ReinforcementRow>,
    /// The player's in-flight shipments (008); empty hides the panel.
    pub shipments: Vec<ShipmentRow>,
    /// The oases this village holds, with their bonus + a recall action (012 AC12); empty hides it.
    pub oases: Vec<OasisRow>,
    /// Resource-field build rows.
    pub fields: Vec<BuildRow>,
    /// Building build rows.
    pub buildings: Vec<BuildRow>,
}
