//! World bootstrap — ensures the (single, for now) world row exists and carries its map seed.

use eperica_domain::{Timestamp, WorldConfig, WorldId};
use sqlx::{PgPool, Row};
use uuid::Uuid;

/// A world's identity, its persisted map seed (006), and its creation instant (the real-time anchor
/// for the weekly medal settlement, 017).
#[derive(Debug, Clone, Copy)]
pub struct World {
    /// Stable identity.
    pub id: WorldId,
    /// The map-generation seed (P6).
    pub seed: i64,
    /// When the world was created (Unix-ms UTC) — anchors the weekly medal periods (017).
    pub created_at: Timestamp,
}

/// Ensure a world exists and return its id and seed. The first call inserts it from `config` with a
/// deterministic per-world seed derived from its id; later calls return the existing row.
///
/// # Errors
/// Returns [`sqlx::Error`] on a storage failure.
pub async fn ensure_world(pool: &PgPool, config: &WorldConfig) -> Result<World, sqlx::Error> {
    if let Some(row) =
        sqlx::query("SELECT id, seed, (EXTRACT(EPOCH FROM created_at) * 1000)::bigint AS created_ms FROM worlds LIMIT 1")
            .fetch_optional(pool)
            .await?
    {
        let id: Uuid = row.try_get("id")?;
        let seed: i64 = row.try_get("seed")?;
        let created_ms: i64 = row.try_get("created_ms")?;
        return Ok(World {
            id: WorldId(id.as_u128()),
            seed,
            created_at: Timestamp(created_ms),
        });
    }

    let id = Uuid::new_v4();
    // The seed is derived from the world id (same rule as the 0009 backfill) so it is deterministic
    // and distinct per world without needing an RNG dependency.
    let row = sqlx::query(
        "INSERT INTO worlds (id, speed, radius, seed) \
         VALUES ($1, $2, $3, hashtextextended($1::text, 0)) \
         RETURNING seed, (EXTRACT(EPOCH FROM created_at) * 1000)::bigint AS created_ms",
    )
    .bind(id)
    .bind(config.speed.multiplier())
    .bind(i32::try_from(config.radius).unwrap_or(i32::MAX))
    .fetch_one(pool)
    .await?;
    let seed: i64 = row.try_get("seed")?;
    let created_ms: i64 = row.try_get("created_ms")?;
    Ok(World {
        id: WorldId(id.as_u128()),
        seed,
        created_at: Timestamp(created_ms),
    })
}
