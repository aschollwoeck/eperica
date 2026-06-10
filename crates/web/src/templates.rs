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
