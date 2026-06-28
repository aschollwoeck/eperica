//! Askama page templates (see specs/ui-style-guide.md for the design system).

use askama::Template;

#[derive(Template)]
#[template(path = "index.html")]
pub struct IndexTemplate {
    /// Open worlds shown on the landing — a visitor clicks one to register straight into it.
    pub worlds: Vec<LandingWorldRow>,
}

/// One open world on the landing page.
pub struct LandingWorldRow {
    /// Hyphenated world UUID — used in `/register?world={id}` and the `/w/{id}` routes.
    pub id: String,
    pub name: String,
    /// Human speed label, e.g. "3× speed".
    pub speed_label: String,
}

#[derive(Template)]
#[template(path = "impressum.html")]
pub struct ImpressumTemplate;

#[derive(Template)]
#[template(path = "privacy.html")]
pub struct PrivacyTemplate;

#[derive(Template)]
#[template(path = "terms.html")]
pub struct TermsTemplate;

#[derive(Template)]
#[template(path = "register.html")]
pub struct RegisterTemplate {
    /// An error message to show above the form, if any.
    pub error: Option<String>,
    /// Preselected world (from a landing "Enlist" link) — carried as a hidden field so registration drops
    /// the new account straight into it. `world_name` is shown in the heading.
    pub world: Option<String>,
    pub world_name: Option<String>,
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

    // 100: the storage bar width is `amount/capacity` as a clamped percentage — what `_ribbon.html` renders
    // into `style="width: …%"`. (The bug it replaced: an inline `<i>` ignored its width and never filled.)
    #[test]
    fn ribbon_pct_drives_the_storage_bar() {
        let rb = |wood: i64, cap: i64| ResourceRibbon {
            wood,
            clay: cap / 2,
            iron: 0,
            crop: cap,
            wood_rate: 0,
            clay_rate: 0,
            iron_rate: 0,
            crop_rate: 0,
            warehouse: cap,
            granary: cap,
        };
        let r = rb(3000, 12000);
        assert_eq!(r.wood_pct(), 25); // 3000 / 12000
        assert_eq!(r.clay_pct(), 50); // half full
        assert_eq!(r.iron_pct(), 0); // empty
        assert_eq!(r.crop_pct(), 100); // full
        assert_eq!(rb(99_999, 12000).wood_pct(), 100); // over cap clamps to 100
        assert_eq!(rb(500, 0).wood_pct(), 0); // unknown cap → 0 (no divide-by-zero)
    }

    fn village(crop_rate: i64) -> VillageTemplate {
        VillageTemplate {
            world: "00000000-0000-0000-0000-000000000000".to_owned(),
            tribe_slug: "gauls",
            username: "player".to_owned(),
            world_won: false,
            is_wonder_site: false,
            village_id: "1".to_owned(),
            is_capital: false,
            loyalty: 100,
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
            ribbon: ResourceRibbon {
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
            },
            population: 42,
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
            plots: Vec::new(),
            protection: None,
            artifacts: Vec::new(),
        }
    }

    // AC7: crop production is flagged when net <= 0, and not when positive.
    #[test]
    fn crop_warning_shown_only_when_net_nonpositive() {
        assert!(village(-5).render().unwrap().contains("starving"));
        assert!(village(0).render().unwrap().contains("starving"));
        assert!(!village(5).render().unwrap().contains("starving"));
    }

    // 065: a building page emits the village's tribe-specific plate (`<tribe>_<slug>.webp`) layered over the
    // neutral plate; an empty tribe slug emits no tribe layer (the neutral plate is the sole fallback). The
    // Academy stands in for the building pages (the village page itself now uses the 069 fortress plan, not
    // the building-bg mechanism).
    fn dummy_upgrade() -> BuildRow {
        BuildRow {
            table: "building",
            slot: 0,
            kind: "academy",
            res: "",
            page: "academy",
            label: "Academy".into(),
            level: 3,
            cost_wood: 100,
            cost_clay: 100,
            cost_iron: 100,
            cost_crop: 50,
            at_max: false,
            can_order: true,
            cost_gated: false,
            effect: "Units in the roster".into(),
            building_ms: None,
            gate: String::new(),
        }
    }

    fn academy_tpl(tribe_slug: &'static str) -> AcademyTemplate {
        AcademyTemplate {
            world: "w".into(),
            tribe_slug,
            village_id: "v".into(),
            village_label: "(0|0)".into(),
            ribbon: ResourceRibbon {
                wood: 0,
                clay: 0,
                iron: 0,
                crop: 0,
                wood_rate: 0,
                clay_rate: 0,
                iron_rate: 0,
                crop_rate: 0,
                warehouse: 0,
                granary: 0,
            },
            has_academy: true,
            rows: Vec::new(),
            active: None,
            upgrade: dummy_upgrade(),
        }
    }

    #[test]
    fn building_bg_layers_the_tribe_plate_when_tribe_is_known() {
        let html = academy_tpl("gauls").render().unwrap();
        assert!(html.contains("--building-img: url('/static/buildings/academy.webp')"));
        assert!(
            html.contains("--building-img-tribe: url('/static/buildings/gauls_academy.webp')"),
            "the tribe plate is layered for a known tribe"
        );
    }

    #[test]
    fn building_bg_omits_the_tribe_plate_when_tribe_is_unknown() {
        let html = academy_tpl("").render().unwrap();
        assert!(html.contains("--building-img: url('/static/buildings/academy.webp')"));
        assert!(
            !html.contains("--building-img-tribe"),
            "no tribe layer (nor a stray `_academy.webp`) when the tribe is unknown"
        );
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
    /// Resource slug for a field plot (`wood`/`clay`/`iron`/`crop`); empty for buildings (069 plan colour).
    pub res: &'static str,
    /// The building's own-page leaf for the inspector "Enter" link (e.g. `academy`); empty if none (069).
    pub page: &'static str,
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
    /// 109: disabled *only* because resources are short (not maxed, not a busy lane) — the button then
    /// carries its cost as `data-cost-*` so the client re-enables it as resources tick up.
    pub cost_gated: bool,
    /// What the next level grants (e.g. "Production 30 → 42/h · +2 pop"); empty at max level.
    pub effect: String,
    /// If this slot is under construction, its completion time (Unix-ms) for the plan's on-plot countdown
    /// (069); `None` otherwise.
    pub building_ms: Option<i64>,
    /// When the slot can't be ordered (and isn't at max), the explicit reason — a busy queue lane or the
    /// exact resource shortfall (072); empty when orderable or at max.
    pub gate: String,
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

/// The resource ribbon shown on every in-village building page (067): current amounts, hourly production
/// rates and storage caps. `crop_rate` is net of upkeep and may be negative. Rendered by `_ribbon.html`.
pub struct ResourceRibbon {
    pub wood: i64,
    pub clay: i64,
    pub iron: i64,
    pub crop: i64,
    pub wood_rate: i64,
    pub clay_rate: i64,
    pub iron_rate: i64,
    pub crop_rate: i64,
    pub warehouse: i64,
    pub granary: i64,
}

impl ResourceRibbon {
    /// `amt/cap` as a 0–100 percentage for the storage bar — server-rendered so the bar reflects fullness
    /// without/before JS (the 070 live counter then keeps it moving). `0` when the cap is unknown.
    fn pct(amt: i64, cap: i64) -> u8 {
        if cap <= 0 {
            0
        } else {
            (amt.max(0).saturating_mul(100) / cap).clamp(0, 100) as u8
        }
    }
    pub fn wood_pct(&self) -> u8 {
        Self::pct(self.wood, self.warehouse)
    }
    pub fn clay_pct(&self) -> u8 {
        Self::pct(self.clay, self.warehouse)
    }
    pub fn iron_pct(&self) -> u8 {
        Self::pct(self.iron, self.warehouse)
    }
    pub fn crop_pct(&self) -> u8 {
        Self::pct(self.crop, self.granary)
    }
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
    /// Tribe-prefixed portrait slug (`<tribe>_<id>`) for the roster thumbnail (067).
    pub portrait: String,
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
    /// 109: requirements met & queue free but unaffordable — the button renders disabled with `data-cost-*`
    /// so the client re-enables it as resources tick up.
    pub cost_gated: bool,
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
    /// The selected world's UUID (056) — world-coupled links read `/w/{{ world }}/…`.
    pub world: String,
    /// The village's tribe slug for the tribe-specific background plate (065); empty ⇒ neutral plate.
    pub tribe_slug: &'static str,
    /// The village this page acts on (carried into the research form + back link, 013 AC11).
    pub village_id: String,
    /// The acting village's coordinate label, shown in the hero eyebrow (067).
    pub village_label: String,
    /// The shared resource ribbon (067).
    pub ribbon: ResourceRibbon,
    /// Whether the village has an Academy (otherwise the page only explains the requirement).
    pub has_academy: bool,
    /// The tribe's roster.
    pub rows: Vec<AcademyRow>,
    /// The research in progress, if any.
    pub active: Option<QueueView>,
    /// The Academy's own build/upgrade panel (087), shown in the aside.
    pub upgrade: BuildRow,
}

/// One researched unit type in the Smithy view (004 AC15).
pub struct SmithyRow {
    /// Unit slug for the POST `unit` value.
    pub id: String,
    /// Tribe-prefixed portrait slug (`<tribe>_<id>`) for the roster thumbnail (066).
    pub portrait: String,
    /// Display name.
    pub name: String,
    /// Role label (Infantry/Cavalry/…), 066.
    pub role: &'static str,
    /// Current upgrade level.
    pub level: u8,
    /// The level this upgrade forges to (`level + 1`), 066.
    pub target: u8,
    /// This unit is the one currently at the anvil (066).
    pub forging: bool,
    /// Pip track to the Smithy's cap (066): one entry per forgeable level, `true` where already forged.
    pub pips: Vec<bool>,
    /// The Upgrade action is offered now.
    pub can_order: bool,
    /// 109: forgeable & queue free but unaffordable — the button renders disabled with `data-cost-*` so the
    /// client re-enables it as resources tick up.
    pub cost_gated: bool,
    /// Why the action is unavailable (cap reached, smithy level, insufficient resources); empty
    /// when orderable.
    pub gate: String,
    pub cost_wood: i64,
    pub cost_clay: i64,
    pub cost_iron: i64,
    pub cost_crop: i64,
    /// Upgrade duration at the current world speed, formatted `h:mm:ss`.
    pub time: String,
    /// The stat gain the next level grants (031); empty at max level.
    pub effect: String,
}

#[derive(Template)]
#[template(path = "smithy.html")]
pub struct SmithyTemplate {
    /// The selected world's UUID (056) — world-coupled links read `/w/{{ world }}/…`.
    pub world: String,
    /// The village's tribe slug for the tribe-specific background plate (065); empty ⇒ neutral plate.
    pub tribe_slug: &'static str,
    /// The village this page acts on (carried into the upgrade form + back link, 013 AC11).
    pub village_id: String,
    /// The acting village's coordinate label, shown in the hero eyebrow (066).
    pub village_label: String,
    /// Whether the village has a Smithy (otherwise the page only explains the requirement).
    pub has_smithy: bool,
    /// The Smithy's building level (caps unit levels; the pip-track length, 066).
    pub smithy_level: u8,
    /// The shared resource ribbon (067).
    pub ribbon: ResourceRibbon,
    /// Researched units with their upgrade state.
    pub rows: Vec<SmithyRow>,
    /// The upgrade in progress, if any.
    pub active: Option<QueueView>,
    /// The portrait slug of the unit at the anvil (066), for the aside; `None` when idle.
    pub active_portrait: Option<String>,
    /// The Smithy's own build/upgrade panel (087), shown in the aside.
    pub upgrade: BuildRow,
}

/// One trainable unit row in a troop-building view (005 AC9).
pub struct TrainRow {
    /// Unit slug for the POST `unit` value.
    pub id: String,
    /// Tribe-prefixed portrait slug (`<tribe>_<id>`, e.g. `romans_legionnaire`); the page resolves
    /// `/static/units/<portrait>.webp` as a thumbnail, falling back to a placeholder when absent (063).
    pub portrait: String,
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
    /// Per-unit training time in seconds (031 — for the live batch-total JS).
    pub time_secs: i64,
    /// The Train form is offered (no batch running at this building).
    pub can_order: bool,
    /// Why the action is unavailable; empty when orderable.
    pub gate: String,
}

#[derive(Template)]
#[template(path = "troops.html")]
pub struct TroopsTemplate {
    /// The selected world's UUID (056) — world-coupled links read `/w/{{ world }}/…`.
    pub world: String,
    /// The village's tribe slug for the tribe-specific background plate (065); empty ⇒ neutral plate.
    pub tribe_slug: &'static str,
    /// The village this page acts on (carried into the train form + back link, 013 AC11).
    pub village_id: String,
    /// The acting village's coordinate label, shown in the hero eyebrow (067).
    pub village_label: String,
    /// The shared resource ribbon (067).
    pub ribbon: ResourceRibbon,
    /// "Barracks" / "Stable" / "Workshop".
    pub building: &'static str,
    /// The training building's level, shown in the hero (067).
    pub building_level: u8,
    /// Whether the village has this building (otherwise the page explains the requirement).
    pub has_building: bool,
    /// Researched units this building trains.
    pub rows: Vec<TrainRow>,
    /// The running batch, if any.
    pub active: Option<QueueView>,
    /// This training building's own build/upgrade panel (087), shown in the aside.
    pub upgrade: BuildRow,
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

/// One rendered cell of the map view (006 AC7). `Serialize` so the `/map/tiles` JSON endpoint (093) streams
/// these to the draggable client.
#[derive(serde::Serialize)]
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
    /// 096: a "Send merchant" link (the Marketplace pre-filled with the tile) — `Some` for any village tile,
    /// `None` otherwise (you can only ship resources to a village).
    pub market_href: Option<String>,
    /// 104: the tile is a free valley you can settle on — the `href` is the Rally Point for a Settle order,
    /// so the inspector labels its send button "Send settlers" instead of "Send troops".
    pub settle: bool,
    /// The tile's coordinate, for the inspector's "center here" link + send-troops shortcut (074).
    pub x: i32,
    pub y: i32,
}

#[derive(Template)]
#[template(path = "map.html")]
pub struct MapTemplate {
    /// The selected world's UUID (056) — world-coupled links read `/w/{{ world }}/…`.
    pub world: String,
    /// 107: the acting village segment — the map's links/tile-fetch read `/village/{{ village }}/map…`.
    pub village: String,
    /// The center coordinate the view is built around.
    pub center_x: i32,
    pub center_y: i32,
    /// The player's home coordinate (095) — the "recentre on home" target.
    pub home_x: i32,
    pub home_y: i32,
    /// Map radius, for display.
    pub radius: i32,
    /// Column count of the initial grid (093) — sets the layer's `grid-template-columns`.
    pub cols: i32,
    /// The initial grid (093): rows north→south, each west→east. The draggable client re-fetches via
    /// `/map/tiles`; this server-rendered grid is the no-JS / first-paint fallback.
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
    /// Per-unit stats (031 — for the live carry/power/ETA preview as the army is selected).
    pub speed: u32,
    pub carry: u32,
    pub attack: u32,
    pub def_inf: u32,
    pub def_cav: u32,
    /// 097: this unit is a scout / a catapult — drives which order-specific fields the form reveals.
    pub is_scout: bool,
    pub is_catapult: bool,
}

#[derive(Template)]
#[template(path = "rally.html")]
pub struct RallyTemplate {
    /// The selected world's UUID (056) — world-coupled links read `/w/{{ world }}/…`.
    pub world: String,
    /// The village's tribe slug for the tribe-specific background plate (065); empty ⇒ neutral plate.
    pub tribe_slug: &'static str,
    /// The id of the village these troops are sent from (carried into the form, AC11).
    pub village_id: String,
    /// The acting village's coordinate label, shown in the hero eyebrow (067).
    pub village_label: String,
    /// The shared resource ribbon (067).
    pub ribbon: ResourceRibbon,
    /// The garrison units that can be sent (empty hides the form).
    pub units: Vec<RallyUnitRow>,
    /// Pre-filled target tile from a map link (012 AC12), if any.
    pub target_x: Option<i32>,
    pub target_y: Option<i32>,
    /// 106: the order pre-selected in the form (`raid`/`attack`/`reinforce`/`scout`/`settle`) — from the map
    /// link's `mode`, else the `raid` default.
    pub mode: &'static str,
    /// Whether the pre-filled target is an oasis (hints attack/reinforce over the village modes).
    pub target_is_oasis: bool,
    /// Whether the player has a free expansion slot — offers the **Settle** order (013 AC11).
    pub can_settle: bool,
    /// Settlers a founding consumes (shown in the settle hint).
    pub settlers_per_village: u32,
    /// Origin coordinates + world radius + speed (031 — for the client-side travel-time/ETA preview,
    /// matching the domain's toroidal distance × world speed).
    pub origin_x: i32,
    pub origin_y: i32,
    pub radius: i32,
    pub speed_mult: f64,
    /// The Rally Point's own build/upgrade panel (087), shown in the aside.
    pub upgrade: BuildRow,
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
    /// The selected world's UUID (056) — world-coupled links read `/w/{{ world }}/…`.
    pub world: String,
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
    /// The selected world's UUID (056) — world-coupled links read `/w/{{ world }}/…`.
    pub world: String,
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
    /// 098: for the attacker of a won oasis raid — why no loot came back (oases hold no resources, 012).
    pub oasis_note: Option<String>,
    /// The razed building, e.g. "Warehouse 3 → 1" (011); `None` when none.
    pub razed: Option<String>,
    /// The loyalty change from an administrator strike, e.g. "60 → 30" (014); `None` when none.
    pub loyalty: Option<String>,
    /// Whether the village changed hands (014 AC10).
    pub conquered: bool,
}

/// One revealed resource line in a scout report's intel (010 AC9).
pub struct ScoutResourceRow {
    pub name: String,
    pub amount: i64,
}

#[derive(Template)]
#[template(path = "scout_report.html")]
pub struct ScoutReportTemplate {
    /// The selected world's UUID (056) — world-coupled links read `/w/{{ world }}/…`.
    pub world: String,
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
    /// The selected world's UUID (056) — world-coupled links read `/w/{{ world }}/…`.
    pub world: String,
    /// The village's tribe slug for the tribe-specific background plate (065); empty ⇒ neutral plate.
    pub tribe_slug: &'static str,
    /// The village this page acts on (carried into the send form + back link, 013 AC11).
    pub village_id: String,
    /// The acting village's coordinate label, shown in the hero eyebrow (067).
    pub village_label: String,
    /// The shared resource ribbon (067).
    pub ribbon: ResourceRibbon,
    /// Whether the village has a Marketplace (otherwise the page only explains the requirement).
    pub has_marketplace: bool,
    /// Total resources one of this tribe's merchants carries.
    pub capacity: u32,
    /// Merchants available to dispatch now.
    pub free: u32,
    /// Merchants the Marketplace provides at its current level.
    pub total: u32,
    /// Merchant map speed (fields/h) + origin/radius/world-speed (031 — for the live merchants-needed +
    /// round-trip-time preview, matching the domain's toroidal distance × world speed).
    pub merchant_speed: u32,
    pub origin_x: i32,
    pub origin_y: i32,
    pub radius: i32,
    pub speed_mult: f64,
    /// Pre-filled target tile from a map "Send merchant" link (096), if any.
    pub target_x: Option<i32>,
    pub target_y: Option<i32>,
    /// The Marketplace's own build/upgrade panel (087), shown in the aside.
    pub upgrade: BuildRow,
}

/// The generic per-building / per-field detail page (087): a hero, a one-line description, the resource
/// ribbon, and the working upgrade panel. Serves the buildings that lack a dedicated functional page and
/// every resource field, so the village plan can be a pure overview that links here.
#[derive(Template)]
#[template(path = "detail.html")]
pub struct DetailTemplate {
    /// The selected world's UUID (056) — world-coupled links read `/w/{{ world }}/…`.
    pub world: String,
    /// The village's tribe slug for the tribe-specific background plate (065); empty ⇒ neutral plate.
    pub tribe_slug: &'static str,
    /// The village this page acts on (carried into the upgrade form + back link).
    pub village_id: String,
    /// The acting village's coordinate label, shown in the hero eyebrow.
    pub village_label: String,
    /// The shared resource ribbon (067).
    pub ribbon: ResourceRibbon,
    /// Hero eyebrow: "Building" or "Resource field".
    pub eyebrow: &'static str,
    /// Page title (the building name, or "Wood field #3").
    pub title: String,
    /// One-line description of what this building/field does.
    pub blurb: String,
    /// SVG symbol id for the hero crest (e.g. `i-warehouse` or `i-wood`).
    pub icon: String,
    /// The build/upgrade panel data.
    pub upgrade: BuildRow,
    /// 110: this slot can be demolished now (built, not the Main Building, Main Building high enough) —
    /// the page offers a Demolish action that frees the slot.
    pub can_demolish: bool,
    /// 110: the centre slot the Demolish action targets.
    pub demolish_slot: u8,
}

/// 110: the build menu for an **empty** centre slot — the kinds the player may construct there now, each
/// with its cost and (un)affordability. A reserved slot offers only its kind.
#[derive(Template)]
#[template(path = "build_menu.html")]
pub struct BuildMenuTemplate {
    pub world: String,
    pub tribe_slug: &'static str,
    pub village_id: String,
    pub village_label: String,
    pub ribbon: ResourceRibbon,
    /// The empty centre slot being built on.
    pub slot: u8,
    /// Whether this is a reserved slot (a single fixed option, e.g. the Wall).
    pub reserved: bool,
    /// The buildable kinds — each a build row carrying its cost, gate, and affordability.
    pub options: Vec<BuildRow>,
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

/// One entry in the village switcher (013 AC11): an owned village, the page links to it at
/// `/w/{world}/village/{id}` (064 — the id is the path segment).
pub struct VillageSwitchRow {
    /// The village id as its hyphenated-UUID path segment (064).
    pub id: String,
    /// Display label, e.g. "(3|4)".
    pub label: String,
    /// Whether this is the player's capital (badged).
    pub is_capital: bool,
    /// Whether this is the village currently shown.
    pub is_current: bool,
}

/// 110: one centre slot on the village plan — a built building or an empty build spot. Positioned by
/// `slot` (the plan renders all `VILLAGE_BUILDING_SLOTS` of them at fixed positions).
pub struct PlotView {
    /// The centre slot (0..VILLAGE_BUILDING_SLOTS) — drives the fixed plan position.
    pub slot: u8,
    /// Building kind id for the icon (`#i-<kind>`); a reserved empty slot shows its kind's icon, a
    /// general empty slot shows none (the "+" affordance).
    pub kind: &'static str,
    /// Display name: the building, the reserved kind ("Wall"), or "Empty".
    pub label: String,
    /// Building level; 0 when empty.
    pub level: u8,
    /// Whether a building occupies the slot.
    pub occupied: bool,
    /// Whether this is a reserved special slot (Rally Point / Wall) — styled distinctly.
    pub reserved: bool,
    /// Completion time (Unix-ms) when the slot is under construction; `None` otherwise.
    pub building_ms: Option<i64>,
    /// Where the plot links: a built functional building's page, else the slot page (upgrade/menu).
    pub href: String,
}

#[derive(Template)]
#[template(path = "village.html")]
pub struct VillageTemplate {
    /// The selected world's UUID (056) — world-coupled links read `/w/{{ world }}/…`.
    pub world: String,
    /// The village's tribe slug for the tribe-specific background plate (065); empty ⇒ neutral plate.
    pub tribe_slug: &'static str,
    /// Owner's username.
    pub username: String,
    /// Whether the world has been won (021 AC7) — shows a victory notice + freeze warning.
    pub world_won: bool,
    /// Whether the shown village is a conquered Wonder site (021) — offers the Wonder-build action.
    pub is_wonder_site: bool,
    /// The shown village's id (carried into action forms + nav links so they target it, AC11).
    pub village_id: String,
    /// Whether the shown village is the player's capital (badged; raises its field cap, AC9/AC10).
    pub is_capital: bool,
    /// The shown village's loyalty, regenerated to now (014); a capital can never be conquered.
    pub loyalty: i64,
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
    /// The shared resource ribbon (069).
    pub ribbon: ResourceRibbon,
    /// Total village population (field + building population), shown in the command header (069).
    pub population: i64,
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
    /// 110: the village centre as a fixed list of slot plots (built buildings + empty spots).
    pub plots: Vec<PlotView>,
    /// Beginner's-protection notice (019 AC9): a human summary of the remaining window, or `None`
    /// once protection has ended.
    pub protection: Option<String>,
    /// Artifacts this player holds (020 AC8) — type/scope/effect + holding-village coordinate.
    pub artifacts: Vec<ArtifactRowView>,
}

/// One held artifact on the village view (020 AC8).
pub struct ArtifactRowView {
    /// A human label: "Speed (large) — ×2.0".
    pub label: String,
    /// The holding village's coordinate, e.g. "(12|−4)".
    pub holder: String,
}

// ---------------------------------------------------------------- alliances (015)

/// A pending invitation shown to the invited player (AC3/AC11).
pub struct PendingInviteView {
    /// The inviting alliance's id (string — the form value).
    pub alliance_id: String,
    /// Its name.
    pub name: String,
    /// Its tag.
    pub tag: String,
}

/// One roster row (AC8/AC11).
pub struct RosterRowView {
    /// The member's id (string — the management form value).
    pub player_id: String,
    /// Their login name.
    pub name: String,
    /// Their role (Founder / Leader / Member).
    pub role: &'static str,
    /// A comma-separated rights summary (empty for none).
    pub rights: String,
    /// Whether this row is the viewer.
    pub is_self: bool,
}

/// One diplomacy row (AC7/AC11).
pub struct DiploRowView {
    /// The other alliance's id (string — the form value).
    pub other_id: String,
    /// Its name + tag.
    pub other: String,
    /// A human label for the stance (e.g. "War", "Confederation", "Confederation (proposed by them)").
    pub label: String,
    /// Whether the viewer's alliance can accept a pending proposal from the other side.
    pub can_accept: bool,
}

/// An allied village in the shared list (AC8).
pub struct AlliedVillageView {
    /// The owner's name.
    pub owner: String,
    pub x: i32,
    pub y: i32,
}

/// An incoming hostile movement against an allied village (AC9).
pub struct IncomingView {
    pub x: i32,
    pub y: i32,
    /// Arrival instant (ms) — rendered client-side.
    pub arrive_ms: i64,
}

/// A pending invitation the alliance has sent (the management view, AC11).
pub struct OutgoingInviteView {
    /// The invited player's name (the revoke form value).
    pub invitee_name: String,
}

#[derive(Template)]
#[template(path = "alliance.html")]
pub struct AllianceTemplate {
    /// The selected world's UUID (056) — world-coupled links read `/w/{{ world }}/…`.
    pub world: String,
    /// Whether the viewer is in an alliance (selects which half of the page renders).
    pub in_alliance: bool,
    // --- when NOT in an alliance ---
    /// Whether the viewer's Embassy is high enough to found (shows the form).
    pub can_found: bool,
    /// The viewer's highest Embassy level.
    pub embassy_level: u8,
    /// The Embassy level required to found.
    pub found_level: u8,
    /// The Embassy level required to join.
    pub join_level: u8,
    /// Pending invitations addressed to the viewer.
    pub pending: Vec<PendingInviteView>,
    // --- when in an alliance ---
    pub name: String,
    pub tag: String,
    pub my_role: &'static str,
    pub is_founder: bool,
    pub can_invite: bool,
    pub can_diplomacy: bool,
    pub can_expel: bool,
    pub can_manage: bool,
    pub roster: Vec<RosterRowView>,
    pub diplomacy: Vec<DiploRowView>,
    pub allied_villages: Vec<AlliedVillageView>,
    pub incoming: Vec<IncomingView>,
    pub outgoing_invites: Vec<OutgoingInviteView>,
}

// ---------------------------------------------------------------- 016: ranking & statistics

/// One row of a leaderboard (016): rank, the entity's name (+ tag for alliances), its stat-page
/// link, and the metric value.
pub struct LeaderboardRowView {
    pub rank: usize,
    pub name: String,
    /// The alliance tag (empty for player rows).
    pub tag: String,
    /// Link to the entity's statistics page.
    pub href: String,
    pub value: i64,
    /// 025 presence: whether to render an indicator at all (player rows yes, alliance rows no),
    /// the online flag, and a human label ("online" / "last seen …").
    pub has_presence: bool,
    pub online: bool,
    pub presence_label: String,
}

#[derive(Template)]
#[template(path = "leaderboard.html")]
pub struct LeaderboardTemplate {
    /// The selected world's UUID (056) — world-coupled links read `/w/{{ world }}/…`.
    pub world: String,
    /// The selected category key (e.g. "population", "attackers", "alliances").
    pub category: String,
    /// All category options as (key, label).
    pub categories: Vec<(&'static str, &'static str)>,
    /// The selected scope key ("world" / "ne" / "nw" / "sw" / "se").
    pub scope: String,
    pub scopes: Vec<(&'static str, &'static str)>,
    /// The selected window key ("all" / "7d" / "30d"); options are built from config (P7).
    pub window: String,
    pub windows: Vec<(String, String)>,
    /// Whether the selected category ranks alliances (shows the tag column).
    pub is_alliance: bool,
    /// Whether the selected category is windowed (shows the window selector).
    pub windowed: bool,
    /// The metric column header (e.g. "Population", "Attack points").
    pub value_label: &'static str,
    pub rows: Vec<LeaderboardRowView>,
}

/// One village line on a player's statistics page (016 AC9): its tile and population.
pub struct VillageStatRow {
    pub x: i32,
    pub y: i32,
    pub population: i64,
}

/// A medal earned by a player or alliance (017 AC5): its category, rank, and the week it was won.
pub struct MedalRowView {
    pub category: String,
    pub rank: i64,
    pub period: i64,
}

/// An achievement a player holds (017 AC8): a human-readable label.
pub struct AchievementRowView {
    pub label: String,
}

/// One population-over-time point on a player's stat page (017 AC11).
pub struct HistoryPointView {
    pub period: i64,
    pub population: i64,
}

#[derive(Template)]
#[template(path = "profile.html")]
pub struct ProfileTemplate {
    /// The owner's current bio (editable).
    pub bio: String,
}

#[derive(Template)]
#[template(path = "player_stats.html")]
pub struct PlayerStatsTemplate {
    /// The selected world's UUID (056) — world-coupled links read `/w/{{ world }}/…`.
    pub world: String,
    /// The viewed player's id (for the report action — 022 AC2).
    pub subject_id: String,
    pub name: String,
    /// The player's profile bio (025; empty if unset).
    pub bio: String,
    /// Presence indicator (025): online flag + a human label.
    pub online: bool,
    pub presence_label: String,
    pub population: i64,
    pub attack_points: i64,
    pub defense_points: i64,
    pub loot_total: i64,
    pub villages: Vec<VillageStatRow>,
    pub medals: Vec<MedalRowView>,
    pub achievements: Vec<AchievementRowView>,
    pub history: Vec<HistoryPointView>,
}

/// One member line on an alliance's statistics page (016 AC10).
pub struct MemberStatRow {
    pub name: String,
    pub href: String,
    pub population: i64,
    pub attack_points: i64,
    pub defense_points: i64,
}

#[derive(Template)]
#[template(path = "alliance_stats.html")]
pub struct AllianceStatsTemplate {
    /// The selected world's UUID (056) — world-coupled links read `/w/{{ world }}/…`.
    pub world: String,
    pub name: String,
    pub tag: String,
    pub population: i64,
    pub attack_points: i64,
    pub defense_points: i64,
    pub members: Vec<MemberStatRow>,
    pub medals: Vec<MedalRowView>,
}

/// The player's current (next-to-complete) onboarding quest (018 AC8): what to do + its reward.
pub struct CurrentQuestView {
    pub description: String,
    pub reward: String,
}

/// One completed onboarding quest on the quests page (018 AC8).
pub struct CompletedQuestView {
    pub description: String,
}

#[derive(Template)]
#[template(path = "quests.html")]
pub struct QuestsTemplate {
    /// The selected world's UUID (056) — world-coupled links read `/w/{{ world }}/…`.
    pub world: String,
    /// The player's village id (for the nav back-links). The first/capital village.
    pub village_id: String,
    /// The current quest, or `None` when the whole chain is done (the tapered-off state).
    pub current: Option<CurrentQuestView>,
    /// Quests already completed, in chain order.
    pub completed: Vec<CompletedQuestView>,
}

/// One alliance's row on the Wonder race board (021 AC9).
pub struct WonderStandingView {
    /// 1-based rank by Wonder level.
    pub rank: usize,
    /// Alliance name.
    pub name: String,
    /// Alliance tag.
    pub tag: String,
    /// Its highest Wonder level (0..=100).
    pub level: u8,
}

#[derive(Template)]
#[template(path = "wonder.html")]
pub struct WonderTemplate {
    /// The selected world's UUID (056) — world-coupled links read `/w/{{ world }}/…`.
    pub world: String,
    /// The winning alliance `(name, tag)` once the round is won; `None` while it is ongoing (021 AC6).
    pub winner: Option<(String, String)>,
    /// The level a Wonder must reach to win (100).
    pub max_level: u8,
    /// The race standings, highest Wonder first.
    pub standings: Vec<WonderStandingView>,
}

/// One open report on the moderator review queue page (022 AC3/AC9).
pub struct ModReportRow {
    /// The report id (the resolve target).
    pub id: String,
    /// Reporter + subject display names.
    pub reporter_name: String,
    pub subject_id: String,
    pub subject_name: String,
    /// Reason + note.
    pub reason: String,
    pub note: String,
}

#[derive(Template)]
#[template(path = "mod_queue.html")]
pub struct ModQueueTemplate {
    /// Open reports, oldest first.
    pub reports: Vec<ModReportRow>,
}

#[derive(Template)]
#[template(path = "mod_account.html")]
pub struct ModAccountTemplate {
    /// The inspected account.
    pub subject_id: String,
    pub username: String,
    /// Current sanction status.
    pub banned: bool,
    pub suspended: bool,
    /// Detection signals (022 AC7).
    pub ip_association_count: u32,
    pub shared_ip_flagged: bool,
    pub peak_action_count: u32,
    pub inhuman_action_rate: bool,
}

/// One world row in the admin console's worlds table (041 AC3).
pub struct AdminWorldRow {
    pub id: String,
    pub name: String,
    pub speed: f64,
    pub radius: u32,
    pub created_ms: i64,
    pub won: bool,
    /// The home world (the one the web serves) — labelled in the table.
    pub is_home: bool,
}

/// One account row in the admin console listing (036 AC3).
pub struct AdminAccountRow {
    pub id: String,
    pub username: String,
    pub is_moderator: bool,
    pub is_admin: bool,
    pub abandoned: bool,
    /// Whether this row is the viewing admin (hides the self-demote-admin control, AC3).
    pub is_self: bool,
}

#[derive(Template)]
#[template(path = "admin.html")]
pub struct AdminTemplate {
    // World/server overview (036 AC4) — read-only.
    pub speed: f64,
    pub radius: u32,
    pub seed: i64,
    pub created_ms: i64,
    pub artifact_release_ms: Option<i64>,
    pub wonder_release_ms: Option<i64>,
    pub won_ms: Option<i64>,
    pub accounts: i64,
    pub villages: i64,
    pub pending_events: i64,
    // World management (041): every world the registry runs + the create-form bound.
    pub worlds: Vec<AdminWorldRow>,
    pub max_radius: u32,
    /// 047: the operator's env-default end-game schedule (days), prefilled into the create-world form.
    pub default_artifact_days: i64,
    pub default_wonder_days: i64,
    /// 052: the rule presets an admin may pick for a new world (the server-authoritative allow-list), and
    /// the default selection.
    pub presets: Vec<String>,
    pub default_preset: String,
    // Account role administration (036 AC3).
    /// The current search query (echoed into the box); empty for the default recent listing.
    pub query: String,
    /// Whether a (non-empty) account search was run — distinguishes "recent accounts" from "results".
    pub searched: bool,
    pub rows: Vec<AdminAccountRow>,
}

/// A row in the conversations list (024 AC3 / 060: aggregated across worlds).
pub struct ConversationRow {
    /// The conversation's world (hyphenated UUID, 060) — its link is `/w/{world}/messages/c/{key}`.
    pub world: String,
    /// Conversation key (`dm:<uuid>` / `global` / `alliance:<id>`).
    pub key: String,
    /// Display title (other player's name, or channel name).
    pub title: String,
    /// Last-message preview (empty if none yet).
    pub last_body: String,
    /// Unread count for the viewer.
    pub unread: i64,
    /// 025 presence (DM rows only): whether to show an indicator, the online flag + a human label.
    pub has_presence: bool,
    pub online: bool,
    pub presence_label: String,
}

#[derive(Template)]
#[template(path = "messages.html")]
pub struct MessagesTemplate {
    /// Conversations, newest-activity first.
    pub conversations: Vec<ConversationRow>,
}

/// One row in the notifications feed (026 AC4).
pub struct NotificationRowView {
    /// Kind label ("Incoming attack" / "Battle report" / "New message").
    pub label: String,
    /// A short detail line (may be empty).
    pub body: String,
    /// Deep-link to the referenced entity (empty if none).
    pub href: String,
    /// Whether it was already read (for styling).
    pub read: bool,
}

#[derive(Template)]
#[template(path = "notifications.html")]
pub struct NotificationsTemplate {
    /// Notifications, most-recent first.
    pub notifications: Vec<NotificationRowView>,
}

/// A player row on the account-sitting page (030) — a sitter you authorised, or an owner you sit for.
pub struct SitterRow {
    pub id: String,
    pub name: String,
}

/// One entry in the sitter-action audit log (030).
pub struct AuditRow {
    pub sitter: String,
    pub action: String,
    /// A coarse "how long ago" label.
    pub when: String,
}

#[derive(Template)]
#[template(path = "sitting.html")]
pub struct SittingTemplate {
    /// Sitters the player has authorised for their own account.
    pub my_sitters: Vec<SitterRow>,
    /// Owners who authorised the player to sit for them.
    pub sitting_for: Vec<SitterRow>,
    /// The player's own audit log (what their sitters did).
    pub audit: Vec<AuditRow>,
    /// If the player is currently sitting someone, that owner's name.
    pub currently_sitting: Option<String>,
}

/// One notification-kind toggle on the settings page (029).
pub struct SettingsToggleRow {
    /// The kind's stable token (the checkbox `name`).
    pub token: String,
    /// Human label.
    pub label: String,
    /// Whether the kind is currently enabled (checkbox checked).
    pub enabled: bool,
}

#[derive(Template)]
#[template(path = "settings.html")]
pub struct SettingsTemplate {
    pub notifications: Vec<SettingsToggleRow>,
}

/// A player or alliance hit on the search page (028).
pub struct SearchHitRow {
    pub href: String,
    pub label: String,
}

#[derive(Template)]
#[template(path = "search.html")]
pub struct SearchTemplate {
    /// The selected world's UUID (056) — world-coupled links read `/w/{{ world }}/…`.
    pub world: String,
    /// The submitted query (echoed into the box).
    pub query: String,
    /// Whether a (non-empty) search was actually run — distinguishes the prompt from "no results".
    pub searched: bool,
    pub players: Vec<SearchHitRow>,
    pub alliances: Vec<SearchHitRow>,
    /// A "go to (x|y)" map link when the query parsed as a coordinate.
    pub coordinate_href: Option<String>,
    pub coordinate_label: String,
}

/// One row in the alliance forum thread list (027 AC1).
pub struct ForumThreadRow {
    pub id: String,
    pub title: String,
    pub author: String,
    pub announcement: bool,
    pub post_count: i64,
}

#[derive(Template)]
#[template(path = "forum.html")]
pub struct ForumTemplate {
    /// The selected world's UUID (056) — world-coupled links read `/w/{{ world }}/…`.
    pub world: String,
    /// Threads, most-recent activity first.
    pub threads: Vec<ForumThreadRow>,
    /// Whether the viewer may start an announcement (holds the `Announce` right).
    pub can_announce: bool,
}

/// One post in a forum thread (027 AC1).
pub struct ForumPostRow {
    pub author: String,
    pub body: String,
}

#[derive(Template)]
#[template(path = "forum_thread.html")]
pub struct ForumThreadTemplate {
    /// The selected world's UUID (056) — world-coupled links read `/w/{{ world }}/…`.
    pub world: String,
    /// The thread id (for the reply form action).
    pub thread_id: String,
    pub title: String,
    /// A locked (announcement) thread hides the reply form.
    pub locked: bool,
    pub posts: Vec<ForumPostRow>,
}

/// One rendered line in a conversation (024 AC2).
pub struct ChatLineView {
    /// Sender display name.
    pub sender: String,
    /// Body.
    pub body: String,
    /// Whether the viewer sent it (for alignment/styling).
    pub mine: bool,
}

#[derive(Template)]
#[template(path = "conversation.html")]
pub struct ConversationTemplate {
    /// The conversation's world (hyphenated UUID, 060) — the send/stream/back links are `/w/{world}/…`.
    pub world: String,
    /// The conversation key (used by the send form + SSE stream URL).
    pub key: String,
    /// Display title.
    pub title: String,
    /// 025 presence (DM headers only): whether to show an indicator, the online flag + a human label.
    pub has_presence: bool,
    pub online: bool,
    pub presence_label: String,
    /// History (oldest→newest).
    pub lines: Vec<ChatLineView>,
}

/// The world lobby (045): the worlds the account plays + the worlds it can join.
#[derive(Template)]
#[template(path = "worlds.html")]
pub struct WorldsTemplate {
    /// Worlds the account already has a player in (with the current one marked).
    pub joined: Vec<JoinedWorldRow>,
    /// Running worlds the account has not joined yet.
    pub joinable: Vec<JoinableWorldRow>,
}

/// A world the account plays in (045 AC2).
pub struct JoinedWorldRow {
    pub id: String,
    pub name: String,
    pub speed: f64,
    pub radius: u32,
    /// The account's tribe in this world.
    pub tribe: String,
    /// The currently-selected world (per the `world` cookie).
    pub is_current: bool,
    /// The home world (the one the web serves by default).
    pub is_home: bool,
}

/// A world the account can join (045 AC3).
pub struct JoinableWorldRow {
    pub id: String,
    pub name: String,
    pub speed: f64,
    pub radius: u32,
}
