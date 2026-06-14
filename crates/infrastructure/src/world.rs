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
    /// The artifact-release instant (Unix-ms UTC), or `None` if not scheduled (020, GDD §13.2).
    pub artifact_release_at: Option<Timestamp>,
}

/// Ensure a world exists and return its id and seed. The first call inserts it from `config` with a
/// deterministic per-world seed derived from its id; later calls return the existing row.
///
/// # Errors
/// Returns [`sqlx::Error`] on a storage failure.
pub async fn ensure_world(pool: &PgPool, config: &WorldConfig) -> Result<World, sqlx::Error> {
    // Default: artifacts release 90 days after creation (overridden in production via config, and in
    // release tests via `ensure_world_with_release`).
    ensure_world_with_release(pool, config, 90 * 24 * 60 * 60).await
}

/// Like [`ensure_world`] but with an explicit artifact-release offset (seconds after creation, 020).
///
/// # Errors
/// Returns [`sqlx::Error`] on a storage failure.
pub async fn ensure_world_with_release(
    pool: &PgPool,
    config: &WorldConfig,
    artifact_release_offset_secs: i64,
) -> Result<World, sqlx::Error> {
    const RELEASE_MS: &str =
        "(EXTRACT(EPOCH FROM artifact_release_at) * 1000)::bigint AS release_ms";
    if let Some(row) = sqlx::query(&format!(
        "SELECT id, seed, (EXTRACT(EPOCH FROM created_at) * 1000)::bigint AS created_ms, {RELEASE_MS} \
         FROM worlds LIMIT 1"
    ))
    .fetch_optional(pool)
    .await?
    {
        let id: Uuid = row.try_get("id")?;
        let seed: i64 = row.try_get("seed")?;
        let created_ms: i64 = row.try_get("created_ms")?;
        let release_ms: Option<i64> = row.try_get("release_ms")?;
        return Ok(World {
            id: WorldId(id.as_u128()),
            seed,
            created_at: Timestamp(created_ms),
            artifact_release_at: release_ms.map(Timestamp),
        });
    }

    let id = Uuid::new_v4();
    // The seed is derived from the world id (same rule as the 0009 backfill) so it is deterministic
    // and distinct per world without needing an RNG dependency. The artifact release is offset from
    // creation by config (020, GDD §13.2).
    let row = sqlx::query(&format!(
        "INSERT INTO worlds (id, speed, radius, seed, artifact_release_at) \
         VALUES ($1, $2, $3, hashtextextended($1::text, 0), \
                 now() + make_interval(secs => $4::double precision)) \
         RETURNING seed, (EXTRACT(EPOCH FROM created_at) * 1000)::bigint AS created_ms, {RELEASE_MS}"
    ))
    .bind(id)
    .bind(config.speed.multiplier())
    .bind(i32::try_from(config.radius).unwrap_or(i32::MAX))
    .bind(artifact_release_offset_secs as f64)
    .fetch_one(pool)
    .await?;
    let seed: i64 = row.try_get("seed")?;
    let created_ms: i64 = row.try_get("created_ms")?;
    let release_ms: Option<i64> = row.try_get("release_ms")?;
    Ok(World {
        id: WorldId(id.as_u128()),
        seed,
        created_at: Timestamp(created_ms),
        artifact_release_at: release_ms.map(Timestamp),
    })
}
