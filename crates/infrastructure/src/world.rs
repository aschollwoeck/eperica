//! World bootstrap — ensures the (single, for now) world row exists and carries its map seed.

use eperica_domain::{WorldConfig, WorldId};
use sqlx::{PgPool, Row};
use uuid::Uuid;

/// A world's identity and its persisted map seed (006). The terrain is a pure function of `seed`.
#[derive(Debug, Clone, Copy)]
pub struct World {
    /// Stable identity.
    pub id: WorldId,
    /// The map-generation seed (P6).
    pub seed: i64,
}

/// Ensure a world exists and return its id and seed. The first call inserts it from `config` with a
/// deterministic per-world seed derived from its id; later calls return the existing row.
///
/// # Errors
/// Returns [`sqlx::Error`] on a storage failure.
pub async fn ensure_world(pool: &PgPool, config: &WorldConfig) -> Result<World, sqlx::Error> {
    if let Some(row) = sqlx::query("SELECT id, seed FROM worlds LIMIT 1")
        .fetch_optional(pool)
        .await?
    {
        let id: Uuid = row.try_get("id")?;
        let seed: i64 = row.try_get("seed")?;
        return Ok(World {
            id: WorldId(id.as_u128()),
            seed,
        });
    }

    let id = Uuid::new_v4();
    // The seed is derived from the world id (same rule as the 0009 backfill) so it is deterministic
    // and distinct per world without needing an RNG dependency.
    let row = sqlx::query(
        "INSERT INTO worlds (id, speed, radius, seed) \
         VALUES ($1, $2, $3, hashtextextended($1::text, 0)) RETURNING seed",
    )
    .bind(id)
    .bind(config.speed.multiplier())
    .bind(i32::try_from(config.radius).unwrap_or(i32::MAX))
    .fetch_one(pool)
    .await?;
    let seed: i64 = row.try_get("seed")?;
    Ok(World {
        id: WorldId(id.as_u128()),
        seed,
    })
}
