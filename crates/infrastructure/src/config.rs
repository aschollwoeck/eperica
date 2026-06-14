//! Application configuration loaded from the environment (operator-set).
//!
//! The world's [`GameSpeed`] and map radius come from configuration (P7); nothing time-dependent is
//! hardcoded. A `.env` file is loaded if present (convenient for development).

use eperica_domain::{GameSpeed, WorldConfig};
use std::env;

/// Errors that can occur while loading configuration.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// A required environment variable was not set.
    #[error("missing required environment variable: {0}")]
    Missing(&'static str),
    /// An environment variable held a value that could not be parsed/validated.
    #[error("invalid value for {0}: {1}")]
    Invalid(&'static str, String),
}

/// Runtime configuration for the application: the database connection and the world settings.
#[derive(Debug, Clone)]
pub struct AppConfig {
    /// PostgreSQL connection string.
    pub database_url: String,
    /// The world's static configuration (speed, radius).
    pub world: WorldConfig,
    /// Seconds after world creation when artifacts are released (020, GDD §13.2). Default 90 days.
    pub artifact_release_offset_secs: i64,
}

impl AppConfig {
    /// Load configuration from the environment (loading `.env` first if present).
    ///
    /// Defaults: `WORLD_SPEED=1`, `WORLD_RADIUS=200`. `DATABASE_URL` is required.
    ///
    /// # Errors
    /// Returns [`ConfigError`] if `DATABASE_URL` is missing or a value fails to parse/validate.
    pub fn from_env() -> Result<Self, ConfigError> {
        let _ = dotenvy::dotenv();

        let database_url =
            env::var("DATABASE_URL").map_err(|_| ConfigError::Missing("DATABASE_URL"))?;

        let speed_raw = env::var("WORLD_SPEED").unwrap_or_else(|_| "1".to_owned());
        let speed_val: f64 = speed_raw
            .parse()
            .map_err(|_| ConfigError::Invalid("WORLD_SPEED", speed_raw.clone()))?;
        let speed = GameSpeed::new(speed_val)
            .map_err(|_| ConfigError::Invalid("WORLD_SPEED", speed_raw))?;

        let radius_raw = env::var("WORLD_RADIUS").unwrap_or_else(|_| "200".to_owned());
        let radius: u32 = radius_raw
            .parse()
            .map_err(|_| ConfigError::Invalid("WORLD_RADIUS", radius_raw))?;

        let release_raw =
            env::var("ARTIFACT_RELEASE_DELAY_SECS").unwrap_or_else(|_| "7776000".to_owned());
        let artifact_release_offset_secs: i64 = release_raw
            .parse()
            .map_err(|_| ConfigError::Invalid("ARTIFACT_RELEASE_DELAY_SECS", release_raw))?;

        Ok(Self {
            database_url,
            world: WorldConfig::new(speed, radius),
            artifact_release_offset_secs,
        })
    }
}
