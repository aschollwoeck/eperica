//! Performance & scale test support (023): a reusable, bulk-SQL world seeder shared by the CI scale
//! tests and the `eperica-perf` tool, so the in-CI guard and the on-demand pass never drift.
//!
//! Seeding is **not** game logic — it inserts rows directly (bypassing the domain) to build a large world
//! cheaply for measurement. It is idempotent (`ON CONFLICT DO NOTHING`), so it is safe to re-run / top up.

use eperica_domain::WorldId;
use sqlx::PgPool;
use uuid::Uuid;

/// A summary of what a seed call produced / found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeedSummary {
    /// Total non-NPC `perf_*` players now in the world.
    pub players: i64,
    /// Total villages now in the world.
    pub villages: i64,
}

/// Bulk-seed `players` perf accounts into `world_id` — each a confirmed, non-NPC user with one village
/// (on a distinct tile in a compact square near the origin), a resources row, 18 level-1 fields, and a
/// Main Building. One set-based statement; idempotent via `ON CONFLICT DO NOTHING` (re-runs top up).
///
/// Returns a [`SeedSummary`] of the world's resulting perf-player + village counts.
///
/// Re-running with the **same** `players` tops up cleanly (idempotent). Re-running with a **different**
/// `players` on the same database changes the placement `width`, so a new player's tile can collide with an
/// earlier run's village and be skipped (leaving that user without a village) — for a clean run, use a
/// fresh database. The returned counts always reflect the real state, so measurements stay honest.
///
/// # Errors
/// Returns [`sqlx::Error`] on a storage failure.
pub async fn seed_world(
    pool: &PgPool,
    world_id: WorldId,
    players: u32,
) -> Result<SeedSummary, sqlx::Error> {
    let world = Uuid::from_u128(world_id.0);
    // Compact square so a realistic map viewport overlaps many villages; width ≥ √N keeps tiles distinct.
    let width = ((players as f64).sqrt().ceil() as i32 + 1).max(1);

    sqlx::query(
        "WITH ins_users AS ( \
             INSERT INTO users (id, username, email, password_hash, email_confirmed, tribe) \
             SELECT gen_random_uuid(), 'perf_' || g, 'perf_' || g || '@perf.local', '!', true, 'romans' \
             FROM generate_series(1, $2) g \
             ON CONFLICT (username) DO NOTHING \
             RETURNING id, (split_part(username, '_', 2))::int AS g \
         ), \
         ins_villages AS ( \
             INSERT INTO villages (id, world_id, owner_id, x, y, tribe) \
             SELECT gen_random_uuid(), $1, u.id, ((u.g - 1) % $3), ((u.g - 1) / $3), 'romans' \
             FROM ins_users u \
             ON CONFLICT (world_id, x, y) DO NOTHING \
             RETURNING id \
         ), \
         ins_res AS ( \
             INSERT INTO village_resources (village_id, wood, clay, iron, crop, updated_at) \
             SELECT id, 1000, 1000, 1000, 1000, now() FROM ins_villages \
             RETURNING village_id \
         ), \
         ins_fields AS ( \
             INSERT INTO village_fields (village_id, slot, resource_type, level) \
             SELECT v.id, s, 'wood', 1 FROM ins_villages v CROSS JOIN generate_series(0, 17) s \
             RETURNING village_id \
         ) \
         INSERT INTO village_buildings (village_id, slot, building_type, level) \
         SELECT id, 0, 'main_building', 3 FROM ins_villages",
    )
    .bind(world)
    .bind(i64::from(players))
    .bind(width)
    .execute(pool)
    .await?;

    // Refresh planner statistics after the bulk insert so measurements reflect a real (autovacuumed)
    // database — without this the planner mis-estimates row counts and picks poor plans (023).
    sqlx::query("ANALYZE users, villages, village_fields, village_buildings")
        .execute(pool)
        .await?;

    let players: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM users WHERE username LIKE 'perf\\_%' AND is_npc = false",
    )
    .fetch_one(pool)
    .await?;
    let villages: i64 = sqlx::query_scalar("SELECT count(*) FROM villages WHERE world_id = $1")
        .bind(world)
        .fetch_one(pool)
        .await?;
    Ok(SeedSummary { players, villages })
}

/// The compact square's side length used by [`seed_world`] for `players` — so callers (e.g. a map-viewport
/// measurement) can build a viewport that overlaps the seeded block.
pub fn seed_block_width(players: u32) -> i32 {
    ((players as f64).sqrt().ceil() as i32 + 1).max(1)
}

/// Bulk-insert `n` due `Heartbeat` events (due 1s ago) for the scheduler-throughput measurement (023).
///
/// # Errors
/// Returns [`sqlx::Error`] on a storage failure.
pub async fn seed_heartbeats(pool: &PgPool, n: u32) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO scheduled_events (id, world_id, kind, due_at, status) \
         SELECT gen_random_uuid(), (SELECT id FROM worlds LIMIT 1), 'heartbeat', \
                now() - interval '1 second', 'pending' \
         FROM generate_series(1, $1) g",
    )
    .bind(i64::from(n))
    .execute(pool)
    .await?;
    Ok(())
}
