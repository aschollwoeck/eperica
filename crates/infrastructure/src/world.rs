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
    /// The Wonder-release instant (Unix-ms UTC), or `None` if not scheduled (021, after the artifact date).
    pub wonder_release_at: Option<Timestamp>,
}

const SELECT_COLS: &str = "id, seed, (EXTRACT(EPOCH FROM created_at) * 1000)::bigint AS created_ms, \
    (EXTRACT(EPOCH FROM artifact_release_at) * 1000)::bigint AS artifact_ms, \
    (EXTRACT(EPOCH FROM wonder_release_at) * 1000)::bigint AS wonder_ms";

fn world_from_row(row: &sqlx::postgres::PgRow) -> Result<World, sqlx::Error> {
    let id: Uuid = row.try_get("id")?;
    Ok(World {
        id: WorldId(id.as_u128()),
        seed: row.try_get("seed")?,
        created_at: Timestamp(row.try_get("created_ms")?),
        artifact_release_at: row.try_get::<Option<i64>, _>("artifact_ms")?.map(Timestamp),
        wonder_release_at: row.try_get::<Option<i64>, _>("wonder_ms")?.map(Timestamp),
    })
}

/// Ensure a world exists and return it. The first call inserts it from `config` with a deterministic
/// per-world seed derived from its id; later calls return the existing row.
///
/// # Errors
/// Returns [`sqlx::Error`] on a storage failure.
pub async fn ensure_world(pool: &PgPool, config: &WorldConfig) -> Result<World, sqlx::Error> {
    // Defaults: artifacts at 90 days, the Wonder at 120 days (overridden in production via config, and in
    // release tests via `ensure_world_with_release`).
    ensure_world_with_release(pool, config, 90 * 24 * 60 * 60, 120 * 24 * 60 * 60).await
}

/// Like [`ensure_world`] but with explicit artifact-release (020) and Wonder-release (021) offsets
/// (seconds after creation).
///
/// # Errors
/// Returns [`sqlx::Error`] on a storage failure.
pub async fn ensure_world_with_release(
    pool: &PgPool,
    config: &WorldConfig,
    artifact_release_offset_secs: i64,
    wonder_release_offset_secs: i64,
) -> Result<World, sqlx::Error> {
    if let Some(row) = sqlx::query(&format!("SELECT {SELECT_COLS} FROM worlds LIMIT 1"))
        .fetch_optional(pool)
        .await?
    {
        return world_from_row(&row);
    }

    let id = Uuid::new_v4();
    // The seed is derived from the world id (same rule as the 0009 backfill) so it is deterministic and
    // distinct per world without an RNG dependency. The end-game release dates are config offsets from
    // creation (020/021, GDD §13.2).
    let row = sqlx::query(&format!(
        "INSERT INTO worlds (id, speed, radius, seed, artifact_release_at, wonder_release_at) \
         VALUES ($1, $2, $3, hashtextextended($1::text, 0), \
                 now() + make_interval(secs => $4::double precision), \
                 now() + make_interval(secs => $5::double precision)) \
         RETURNING {SELECT_COLS}"
    ))
    .bind(id)
    .bind(config.speed.multiplier())
    .bind(i32::try_from(config.radius).unwrap_or(i32::MAX))
    .bind(artifact_release_offset_secs as f64)
    .bind(wonder_release_offset_secs as f64)
    .fetch_one(pool)
    .await?;
    world_from_row(&row)
}
