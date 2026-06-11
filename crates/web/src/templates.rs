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

#[derive(Template)]
#[template(path = "village.html")]
pub struct VillageTemplate {
    /// Owner's username.
    pub username: String,
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
    /// Resource-field build rows.
    pub fields: Vec<BuildRow>,
    /// Building build rows.
    pub buildings: Vec<BuildRow>,
}
