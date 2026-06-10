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

#[derive(Template)]
#[template(path = "village.html")]
pub struct VillageTemplate {
    /// Owner's username.
    pub username: String,
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
}
