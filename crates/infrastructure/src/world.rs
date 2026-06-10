//! World bootstrap — ensures the (single, for slice 001) world row exists.

use eperica_domain::{WorldConfig, WorldId};
use sqlx::{PgPool, Row};
use uuid::Uuid;

/// Ensure a world exists and return its id. Slice 001 runs a single world; the first call inserts it
/// from `config`, later calls return the existing one.
///
/// # Errors
/// Returns [`sqlx::Error`] on a storage failure.
pub async fn ensure_world(pool: &PgPool, config: &WorldConfig) -> Result<WorldId, sqlx::Error> {
    if let Some(row) = sqlx::query("SELECT id FROM worlds LIMIT 1")
        .fetch_optional(pool)
        .await?
    {
        let id: Uuid = row.try_get("id")?;
        return Ok(WorldId(id.as_u128()));
    }

    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO worlds (id, speed, radius) VALUES ($1, $2, $3)")
        .bind(id)
        .bind(config.speed.multiplier())
        .bind(i32::try_from(config.radius).unwrap_or(i32::MAX))
        .execute(pool)
        .await?;
    Ok(WorldId(id.as_u128()))
}
