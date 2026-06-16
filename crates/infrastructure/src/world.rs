//! World bootstrap — ensures the (single, for now) world row exists and carries its map seed.

use eperica_domain::{Timestamp, WorldConfig, WorldId};
use sqlx::{PgPool, Row};
use uuid::Uuid;

/// A world's identity, its persisted map seed (006), and its creation instant (the real-time anchor
/// for the weekly medal settlement, 017).
#[derive(Debug, Clone)]
pub struct World {
    /// Stable identity.
    pub id: WorldId,
    /// The world's speed multiplier (P7) — each world may run at its own pace (040).
    pub speed: f64,
    /// The map radius (006) — each world has its own size (040).
    pub radius: u32,
    /// The map-generation seed (P6).
    pub seed: i64,
    /// When the world was created (Unix-ms UTC) — anchors the weekly medal periods (017).
    pub created_at: Timestamp,
    /// The artifact-release instant (Unix-ms UTC), or `None` if not scheduled (020, GDD §13.2).
    pub artifact_release_at: Option<Timestamp>,
    /// The Wonder-release instant (Unix-ms UTC), or `None` if not scheduled (021, after the artifact date).
    pub wonder_release_at: Option<Timestamp>,
    /// The named rule preset this world plays under (049) — `'classic'` by default; the registry resolves
    /// it to a [`crate::WorldRules`] bundle (050).
    pub rule_preset: String,
}

const SELECT_COLS: &str = "id, speed, radius, seed, rule_preset, \
    (EXTRACT(EPOCH FROM created_at) * 1000)::bigint AS created_ms, \
    (EXTRACT(EPOCH FROM artifact_release_at) * 1000)::bigint AS artifact_ms, \
    (EXTRACT(EPOCH FROM wonder_release_at) * 1000)::bigint AS wonder_ms";

fn world_from_row(row: &sqlx::postgres::PgRow) -> Result<World, sqlx::Error> {
    let id: Uuid = row.try_get("id")?;
    Ok(World {
        id: WorldId(id.as_u128()),
        speed: row.try_get("speed")?,
        radius: u32::try_from(row.try_get::<i32, _>("radius")?).unwrap_or(0),
        seed: row.try_get("seed")?,
        created_at: Timestamp(row.try_get("created_ms")?),
        artifact_release_at: row.try_get::<Option<i64>, _>("artifact_ms")?.map(Timestamp),
        wonder_release_at: row.try_get::<Option<i64>, _>("wonder_ms")?.map(Timestamp),
        rule_preset: row.try_get("rule_preset")?,
    })
}

/// Load every world (040) — the registry runtime spawns a scheduler per row. Ordered by creation so the
/// home world (created first) is stable.
///
/// # Errors
/// Returns [`sqlx::Error`] on a storage failure.
pub async fn all_worlds(pool: &PgPool) -> Result<Vec<World>, sqlx::Error> {
    let rows = sqlx::query(&format!(
        "SELECT {SELECT_COLS} FROM worlds ORDER BY created_at, id"
    ))
    .fetch_all(pool)
    .await?;
    rows.iter().map(world_from_row).collect()
}

/// Load one world by id (040/041) — the registry uses this to start a freshly-created world.
///
/// # Errors
/// Returns [`sqlx::Error`] on a storage failure.
pub async fn world_by_id(pool: &PgPool, id: WorldId) -> Result<Option<World>, sqlx::Error> {
    let row = sqlx::query(&format!("SELECT {SELECT_COLS} FROM worlds WHERE id = $1"))
        .bind(Uuid::from_u128(id.0))
        .fetch_optional(pool)
        .await?;
    row.as_ref().map(world_from_row).transpose()
}

/// Create a **new** world (041) — unlike [`ensure_world`] this always inserts a fresh row (a new round),
/// with a deterministic per-world seed derived from its id and the given end-game release offsets.
/// Returns the new world.
///
/// # Errors
/// Returns [`sqlx::Error`] on a storage failure.
pub async fn create_world(
    pool: &PgPool,
    config: &WorldConfig,
    artifact_release_offset_secs: i64,
    wonder_release_offset_secs: i64,
    rule_preset: &str,
) -> Result<World, sqlx::Error> {
    let id = Uuid::new_v4();
    let row = sqlx::query(&format!(
        "INSERT INTO worlds \
           (id, speed, radius, seed, artifact_release_at, wonder_release_at, rule_preset) \
         VALUES ($1, $2, $3, hashtextextended($1::text, 0), \
                 now() + make_interval(secs => $4::double precision), \
                 now() + make_interval(secs => $5::double precision), $6) \
         RETURNING {SELECT_COLS}"
    ))
    .bind(id)
    .bind(config.speed.multiplier())
    .bind(i32::try_from(config.radius).unwrap_or(i32::MAX))
    .bind(artifact_release_offset_secs as f64)
    .bind(wonder_release_offset_secs as f64)
    .bind(rule_preset)
    .fetch_one(pool)
    .await?;
    world_from_row(&row)
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
    // The **home** world is the oldest (the original, env-configured one) — pinned deterministically so
    // it cannot flip between restarts once more worlds exist (040; the home world runs on the env config).
    if let Some(row) = sqlx::query(&format!(
        "SELECT {SELECT_COLS} FROM worlds ORDER BY created_at, id LIMIT 1"
    ))
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
