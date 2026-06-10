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
    /// Count of wood fields.
    pub wood: usize,
    /// Count of clay fields.
    pub clay: usize,
    /// Count of iron fields.
    pub iron: usize,
    /// Count of crop fields.
    pub crop: usize,
    /// Building descriptions.
    pub buildings: Vec<String>,
}
