//! PostgreSQL adapter for the application's [`AccountRepository`] port.

use async_trait::async_trait;
use eperica_application::{
    AccountRepository, ActiveBuild, BuildRepository, DueBuild, NewBuildOrder, NewUser, RepoError,
    UserRecord,
};
use eperica_domain::{
    BuildTarget, BuildingKind, BuildingSlot, Coordinate, PlayerId, ResourceAmounts, ResourceField,
    ResourceKind, StartingVillage, Timestamp, Tribe, Village, VillageId, WorldId,
    coordinates_within,
};
use sqlx::{Acquire, PgPool, Row, postgres::PgRow};
use uuid::Uuid;

/// SQLx-backed account repository bound to a single world.
#[derive(Debug, Clone)]
pub struct PgAccountRepository {
    pool: PgPool,
    world_id: WorldId,
    radius: u32,
    starting_amounts: ResourceAmounts,
}

impl PgAccountRepository {
    /// Create a repository for `world_id` (map `radius` for placement; `starting_amounts` seeded into
    /// each new village's resources).
    pub fn new(
        pool: PgPool,
        world_id: WorldId,
        radius: u32,
        starting_amounts: ResourceAmounts,
    ) -> Self {
        Self {
            pool,
            world_id,
            radius,
            starting_amounts,
        }
    }

    /// Reset build orders stuck in `processing` (e.g. left by a crash) back to `pending` so they are
    /// reprocessed. `apply_build` is idempotent (it sets an absolute level), so this is safe.
    pub async fn requeue_orphaned_builds(&self) -> Result<u64, RepoError> {
        let result =
            sqlx::query("UPDATE build_orders SET status = 'pending' WHERE status = 'processing'")
                .execute(&self.pool)
                .await
                .map_err(backend)?;
        Ok(result.rows_affected())
    }
}

fn backend(e: sqlx::Error) -> RepoError {
    RepoError::Backend(e.to_string())
}

fn is_unique_violation(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(db) if db.code().as_deref() == Some("23505"))
}

fn resource_str(kind: ResourceKind) -> &'static str {
    match kind {
        ResourceKind::Wood => "wood",
        ResourceKind::Clay => "clay",
        ResourceKind::Iron => "iron",
        ResourceKind::Crop => "crop",
    }
}

fn building_str(kind: BuildingKind) -> &'static str {
    match kind {
        BuildingKind::MainBuilding => "main_building",
        BuildingKind::RallyPoint => "rally_point",
        BuildingKind::Warehouse => "warehouse",
        BuildingKind::Granary => "granary",
    }
}

fn parse_resource(s: &str) -> Result<ResourceKind, RepoError> {
    match s {
        "wood" => Ok(ResourceKind::Wood),
        "clay" => Ok(ResourceKind::Clay),
        "iron" => Ok(ResourceKind::Iron),
        "crop" => Ok(ResourceKind::Crop),
        other => Err(RepoError::Backend(format!(
            "unknown resource_type: {other}"
        ))),
    }
}

fn parse_building(s: &str) -> Result<BuildingKind, RepoError> {
    match s {
        "main_building" => Ok(BuildingKind::MainBuilding),
        "rally_point" => Ok(BuildingKind::RallyPoint),
        "warehouse" => Ok(BuildingKind::Warehouse),
        "granary" => Ok(BuildingKind::Granary),
        other => Err(RepoError::Backend(format!(
            "unknown building_type: {other}"
        ))),
    }
}

fn parse_tribe(s: Option<String>) -> Result<Option<Tribe>, RepoError> {
    match s.as_deref() {
        None => Ok(None),
        Some("romans") => Ok(Some(Tribe::Romans)),
        Some("teutons") => Ok(Some(Tribe::Teutons)),
        Some("gauls") => Ok(Some(Tribe::Gauls)),
        Some(other) => Err(RepoError::Backend(format!("unknown tribe: {other}"))),
    }
}

fn row_to_user(r: &PgRow) -> Result<UserRecord, RepoError> {
    let id: Uuid = r.try_get("id").map_err(backend)?;
    Ok(UserRecord {
        id: PlayerId(id.as_u128()),
        username: r.try_get("username").map_err(backend)?,
        email: r.try_get("email").map_err(backend)?,
        password_hash: r.try_get("password_hash").map_err(backend)?,
        email_confirmed: r.try_get("email_confirmed").map_err(backend)?,
    })
}

#[async_trait]
impl AccountRepository for PgAccountRepository {
    async fn create_account(
        &self,
        user: NewUser,
        template: &StartingVillage,
    ) -> Result<UserRecord, RepoError> {
        let mut tx = self.pool.begin().await.map_err(backend)?;

        let user_id = Uuid::new_v4();
        let insert_user = sqlx::query(
            "INSERT INTO users (id, username, email, password_hash, email_confirmed) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(user_id)
        .bind(&user.username)
        .bind(&user.email)
        .bind(&user.password_hash)
        .bind(user.email_confirmed)
        .execute(&mut *tx)
        .await;
        if let Err(e) = insert_user {
            return Err(if is_unique_violation(&e) {
                RepoError::Duplicate
            } else {
                backend(e)
            });
        }

        let owner = PlayerId(user_id.as_u128());
        let world_uuid = Uuid::from_u128(self.world_id.0);

        // Place the village on the first free in-bounds tile. Each attempt is a SAVEPOINT so a
        // coordinate clash rolls back just that insert (not the whole transaction).
        let mut placed = false;
        for coord in coordinates_within(self.radius) {
            let village_uuid = Uuid::new_v4();
            let village = Village::found(VillageId(village_uuid.as_u128()), owner, coord, template);

            let mut sp = tx.begin().await.map_err(backend)?;
            let insert_village = sqlx::query(
                "INSERT INTO villages (id, world_id, owner_id, x, y, tribe) \
                 VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(village_uuid)
            .bind(world_uuid)
            .bind(user_id)
            .bind(coord.x)
            .bind(coord.y)
            .bind(Option::<String>::None)
            .execute(&mut *sp)
            .await;

            match insert_village {
                Ok(_) => {
                    for (slot, f) in village.fields.iter().enumerate() {
                        sqlx::query(
                            "INSERT INTO village_fields (village_id, slot, resource_type, level) \
                             VALUES ($1, $2, $3, $4)",
                        )
                        .bind(village_uuid)
                        .bind(slot as i16)
                        .bind(resource_str(f.kind))
                        .bind(i16::from(f.level))
                        .execute(&mut *sp)
                        .await
                        .map_err(backend)?;
                    }
                    for (slot, b) in village.buildings.iter().enumerate() {
                        sqlx::query(
                            "INSERT INTO village_buildings (village_id, slot, building_type, level) \
                             VALUES ($1, $2, $3, $4)",
                        )
                        .bind(village_uuid)
                        .bind(slot as i16)
                        .bind(building_str(b.kind))
                        .bind(i16::from(b.level))
                        .execute(&mut *sp)
                        .await
                        .map_err(backend)?;
                    }
                    sqlx::query(
                        "INSERT INTO village_resources (village_id, wood, clay, iron, crop, updated_at) \
                         VALUES ($1, $2, $3, $4, $5, now())",
                    )
                    .bind(village_uuid)
                    .bind(self.starting_amounts.wood)
                    .bind(self.starting_amounts.clay)
                    .bind(self.starting_amounts.iron)
                    .bind(self.starting_amounts.crop)
                    .execute(&mut *sp)
                    .await
                    .map_err(backend)?;

                    sp.commit().await.map_err(backend)?;
                    placed = true;
                    break;
                }
                Err(e) if is_unique_violation(&e) => {
                    sp.rollback().await.map_err(backend)?;
                }
                Err(e) => return Err(backend(e)),
            }
        }

        if !placed {
            return Err(RepoError::WorldFull);
        }

        tx.commit().await.map_err(backend)?;
        Ok(UserRecord {
            id: owner,
            username: user.username,
            email: user.email,
            password_hash: user.password_hash,
            email_confirmed: user.email_confirmed,
        })
    }

    async fn find_user_by_username(&self, username: &str) -> Result<Option<UserRecord>, RepoError> {
        let row = sqlx::query(
            "SELECT id, username, email, password_hash, email_confirmed FROM users WHERE username = $1",
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await
        .map_err(backend)?;
        row.as_ref().map(row_to_user).transpose()
    }

    async fn find_user_by_id(&self, id: PlayerId) -> Result<Option<UserRecord>, RepoError> {
        let row = sqlx::query(
            "SELECT id, username, email, password_hash, email_confirmed FROM users WHERE id = $1",
        )
        .bind(Uuid::from_u128(id.0))
        .fetch_optional(&self.pool)
        .await
        .map_err(backend)?;
        row.as_ref().map(row_to_user).transpose()
    }

    async fn villages_of(&self, owner: PlayerId) -> Result<Vec<Village>, RepoError> {
        let owner_uuid = Uuid::from_u128(owner.0);
        let village_rows = sqlx::query(
            "SELECT id, x, y, tribe FROM villages WHERE owner_id = $1 ORDER BY created_at, id",
        )
        .bind(owner_uuid)
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;

        let mut villages = Vec::with_capacity(village_rows.len());
        for r in &village_rows {
            let vid: Uuid = r.try_get("id").map_err(backend)?;
            let x: i32 = r.try_get("x").map_err(backend)?;
            let y: i32 = r.try_get("y").map_err(backend)?;
            let tribe_raw: Option<String> = r.try_get("tribe").map_err(backend)?;

            let field_rows = sqlx::query(
                "SELECT resource_type, level FROM village_fields WHERE village_id = $1 ORDER BY slot",
            )
            .bind(vid)
            .fetch_all(&self.pool)
            .await
            .map_err(backend)?;
            let mut fields = Vec::with_capacity(field_rows.len());
            for fr in &field_rows {
                let kind =
                    parse_resource(&fr.try_get::<String, _>("resource_type").map_err(backend)?)?;
                let level: i16 = fr.try_get("level").map_err(backend)?;
                fields.push(ResourceField {
                    kind,
                    level: u8::try_from(level).unwrap_or(0),
                });
            }

            let building_rows = sqlx::query(
                "SELECT building_type, level FROM village_buildings WHERE village_id = $1 ORDER BY slot",
            )
            .bind(vid)
            .fetch_all(&self.pool)
            .await
            .map_err(backend)?;
            let mut buildings = Vec::with_capacity(building_rows.len());
            for br in &building_rows {
                let kind =
                    parse_building(&br.try_get::<String, _>("building_type").map_err(backend)?)?;
                let level: i16 = br.try_get("level").map_err(backend)?;
                buildings.push(BuildingSlot {
                    kind,
                    level: u8::try_from(level).unwrap_or(0),
                });
            }

            villages.push(Village {
                id: VillageId(vid.as_u128()),
                owner,
                coordinate: Coordinate::new(x, y),
                tribe: parse_tribe(tribe_raw)?,
                fields,
                buildings,
            });
        }
        Ok(villages)
    }

    async fn stored_resources(
        &self,
        village: VillageId,
    ) -> Result<Option<(ResourceAmounts, Timestamp)>, RepoError> {
        let row = sqlx::query(
            "SELECT wood, clay, iron, crop, (EXTRACT(EPOCH FROM updated_at) * 1000)::bigint AS updated_ms \
             FROM village_resources WHERE village_id = $1",
        )
        .bind(Uuid::from_u128(village.0))
        .fetch_optional(&self.pool)
        .await
        .map_err(backend)?;

        let Some(r) = row else { return Ok(None) };
        let amounts = ResourceAmounts {
            wood: r.try_get("wood").map_err(backend)?,
            clay: r.try_get("clay").map_err(backend)?,
            iron: r.try_get("iron").map_err(backend)?,
            crop: r.try_get("crop").map_err(backend)?,
        };
        let updated_ms: i64 = r.try_get("updated_ms").map_err(backend)?;
        Ok(Some((amounts, Timestamp(updated_ms))))
    }
}

fn target_columns(target: BuildTarget) -> (&'static str, i16, Option<&'static str>) {
    match target {
        BuildTarget::Field { slot } => ("field", i16::from(slot), None),
        BuildTarget::Building { slot, kind } => {
            ("building", i16::from(slot), Some(building_str(kind)))
        }
    }
}

fn parse_target(
    table: &str,
    slot: i16,
    building_type: Option<String>,
) -> Result<BuildTarget, RepoError> {
    let slot = u8::try_from(slot).unwrap_or(0);
    match table {
        "field" => Ok(BuildTarget::Field { slot }),
        "building" => {
            let bt = building_type.ok_or_else(|| {
                RepoError::Backend("building target missing building_type".into())
            })?;
            Ok(BuildTarget::Building {
                slot,
                kind: parse_building(&bt)?,
            })
        }
        other => Err(RepoError::Backend(format!("unknown target_table: {other}"))),
    }
}

#[async_trait]
impl BuildRepository for PgAccountRepository {
    async fn start_build(
        &self,
        village: VillageId,
        settled: ResourceAmounts,
        now: Timestamp,
        order: NewBuildOrder,
    ) -> Result<(), RepoError> {
        let (table, slot, building_type) = target_columns(order.target);
        let vid = Uuid::from_u128(village.0);
        let mut tx = self.pool.begin().await.map_err(backend)?;

        sqlx::query(
            "UPDATE village_resources SET wood=$1, clay=$2, iron=$3, crop=$4, \
             updated_at = to_timestamp($5::double precision / 1000.0) WHERE village_id=$6",
        )
        .bind(settled.wood)
        .bind(settled.clay)
        .bind(settled.iron)
        .bind(settled.crop)
        .bind(now.0 as f64)
        .bind(vid)
        .execute(&mut *tx)
        .await
        .map_err(backend)?;

        let insert = sqlx::query(
            "INSERT INTO build_orders \
             (id, village_id, target_table, slot, building_type, target_level, complete_at, status) \
             VALUES ($1, $2, $3, $4, $5, $6, to_timestamp($7::double precision / 1000.0), 'pending')",
        )
        .bind(Uuid::new_v4())
        .bind(vid)
        .bind(table)
        .bind(slot)
        .bind(building_type)
        .bind(i16::from(order.target_level))
        .bind(order.complete_at.0 as f64)
        .execute(&mut *tx)
        .await;
        if let Err(e) = insert {
            return Err(if is_unique_violation(&e) {
                RepoError::Duplicate
            } else {
                backend(e)
            });
        }

        tx.commit().await.map_err(backend)?;
        Ok(())
    }

    async fn active_build(&self, village: VillageId) -> Result<Option<ActiveBuild>, RepoError> {
        let row = sqlx::query(
            "SELECT target_table, slot, building_type, target_level, \
             (EXTRACT(EPOCH FROM complete_at) * 1000)::bigint AS complete_ms \
             FROM build_orders WHERE village_id = $1 AND status = 'pending' LIMIT 1",
        )
        .bind(Uuid::from_u128(village.0))
        .fetch_optional(&self.pool)
        .await
        .map_err(backend)?;

        let Some(r) = row else { return Ok(None) };
        let table: String = r.try_get("target_table").map_err(backend)?;
        let slot: i16 = r.try_get("slot").map_err(backend)?;
        let building_type: Option<String> = r.try_get("building_type").map_err(backend)?;
        let target_level: i16 = r.try_get("target_level").map_err(backend)?;
        let complete_ms: i64 = r.try_get("complete_ms").map_err(backend)?;
        Ok(Some(ActiveBuild {
            target: parse_target(&table, slot, building_type)?,
            target_level: u8::try_from(target_level).unwrap_or(0),
            complete_at: Timestamp(complete_ms),
        }))
    }

    async fn claim_due_builds(
        &self,
        now: Timestamp,
        limit: i64,
    ) -> Result<Vec<DueBuild>, RepoError> {
        let rows = sqlx::query(
            "UPDATE build_orders SET status = 'processing' WHERE id IN ( \
                 SELECT id FROM build_orders \
                 WHERE status = 'pending' AND complete_at <= to_timestamp($1::double precision / 1000.0) \
                 ORDER BY complete_at, id LIMIT $2 FOR UPDATE SKIP LOCKED \
             ) RETURNING id, village_id, target_table, slot, building_type, target_level",
        )
        .bind(now.0 as f64)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;

        let mut out = Vec::with_capacity(rows.len());
        for r in &rows {
            let id: Uuid = r.try_get("id").map_err(backend)?;
            let village: Uuid = r.try_get("village_id").map_err(backend)?;
            let table: String = r.try_get("target_table").map_err(backend)?;
            let slot: i16 = r.try_get("slot").map_err(backend)?;
            let building_type: Option<String> = r.try_get("building_type").map_err(backend)?;
            let target_level: i16 = r.try_get("target_level").map_err(backend)?;
            out.push(DueBuild {
                id: id.as_u128(),
                village: VillageId(village.as_u128()),
                target: parse_target(&table, slot, building_type)?,
                target_level: u8::try_from(target_level).unwrap_or(0),
            });
        }
        Ok(out)
    }

    async fn apply_build(&self, due: DueBuild) -> Result<(), RepoError> {
        let level = i16::from(due.target_level);
        let vid = Uuid::from_u128(due.village.0);
        let mut tx = self.pool.begin().await.map_err(backend)?;

        match due.target {
            BuildTarget::Field { slot } => {
                sqlx::query(
                    "UPDATE village_fields SET level = $1 WHERE village_id = $2 AND slot = $3",
                )
                .bind(level)
                .bind(vid)
                .bind(i16::from(slot))
                .execute(&mut *tx)
                .await
                .map_err(backend)?;
            }
            BuildTarget::Building { slot, kind } => {
                sqlx::query(
                    "INSERT INTO village_buildings (village_id, slot, building_type, level) \
                     VALUES ($1, $2, $3, $4) \
                     ON CONFLICT (village_id, slot) DO UPDATE \
                     SET level = EXCLUDED.level, building_type = EXCLUDED.building_type",
                )
                .bind(vid)
                .bind(i16::from(slot))
                .bind(building_str(kind))
                .bind(level)
                .execute(&mut *tx)
                .await
                .map_err(backend)?;
            }
        }

        sqlx::query("UPDATE build_orders SET status = 'done' WHERE id = $1")
            .bind(Uuid::from_u128(due.id))
            .execute(&mut *tx)
            .await
            .map_err(backend)?;

        tx.commit().await.map_err(backend)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eperica_domain::{GameSpeed, WorldConfig};

    #[tokio::test]
    async fn create_account_persists_user_and_one_village() {
        let _ = dotenvy::dotenv();
        let Ok(url) = std::env::var("DATABASE_URL") else {
            eprintln!("skipping create_account test: DATABASE_URL not set");
            return;
        };
        let pool = crate::create_pool(&url).await.expect("connect");
        crate::run_migrations(&pool).await.expect("migrate");

        let config = WorldConfig::new(GameSpeed::new(1.0).unwrap(), 50);
        let world_id = crate::world::ensure_world(&pool, &config)
            .await
            .expect("ensure world");
        let rules = crate::economy_rules().expect("economy rules");
        let repo = PgAccountRepository::new(
            pool.clone(),
            world_id,
            config.radius,
            rules.starting_amounts,
        );
        let template = crate::starting_village().expect("template");

        let uname = format!("user_{}", Uuid::new_v4().simple());
        let new_user = NewUser {
            username: uname.clone(),
            email: format!("{uname}@example.com"),
            password_hash: "hash".to_owned(),
            email_confirmed: true,
        };

        let user = repo
            .create_account(new_user, &template)
            .await
            .expect("create account");

        // AC3/AC4: exactly one village with 18 fields and the core buildings.
        let villages = repo.villages_of(user.id).await.expect("villages");
        assert_eq!(villages.len(), 1);
        assert_eq!(villages[0].fields.len(), 18);
        assert!(villages[0].buildings.len() >= 2);
        assert_eq!(villages[0].owner, user.id);

        // T4: starting resources were seeded and are readable.
        let stored = repo
            .stored_resources(villages[0].id)
            .await
            .expect("stored resources");
        assert!(stored.is_some());

        // AC1: duplicate username rejected.
        let dup = NewUser {
            username: uname.clone(),
            email: format!("{uname}-2@example.com"),
            password_hash: "hash".to_owned(),
            email_confirmed: true,
        };
        assert!(matches!(
            repo.create_account(dup, &template).await,
            Err(RepoError::Duplicate)
        ));

        // Lookup works.
        assert!(repo.find_user_by_username(&uname).await.unwrap().is_some());
    }

    /// Regression for the migration-boundary bug: a village that predates `village_resources`
    /// (no resources row) must be repairable by the backfill. We reproduce the legacy state by
    /// deleting the seeded row, then apply the same backfill as migration 0003.
    #[tokio::test]
    async fn backfill_repairs_legacy_village_without_resources() {
        let _ = dotenvy::dotenv();
        let Ok(url) = std::env::var("DATABASE_URL") else {
            eprintln!("skipping backfill test: DATABASE_URL not set");
            return;
        };
        let pool = crate::create_pool(&url).await.expect("connect");
        crate::run_migrations(&pool).await.expect("migrate");

        let config = WorldConfig::new(GameSpeed::new(1.0).unwrap(), 50);
        let world_id = crate::world::ensure_world(&pool, &config)
            .await
            .expect("ensure world");
        let rules = crate::economy_rules().expect("economy rules");
        let repo = PgAccountRepository::new(
            pool.clone(),
            world_id,
            config.radius,
            rules.starting_amounts,
        );

        let uname = format!("legacy_{}", Uuid::new_v4().simple());
        let user = repo
            .create_account(
                NewUser {
                    username: uname.clone(),
                    email: format!("{uname}@example.com"),
                    password_hash: "hash".to_owned(),
                    email_confirmed: true,
                },
                &crate::starting_village().unwrap(),
            )
            .await
            .expect("create account");
        let village_id = repo.villages_of(user.id).await.unwrap()[0].id;

        // Reproduce the legacy state: a village with no resources row.
        sqlx::query("DELETE FROM village_resources WHERE village_id = $1")
            .bind(Uuid::from_u128(village_id.0))
            .execute(&pool)
            .await
            .unwrap();
        assert!(repo.stored_resources(village_id).await.unwrap().is_none());

        // Apply the backfill (same statement as migration 0003) and confirm it is repaired.
        sqlx::query(
            "INSERT INTO village_resources (village_id, wood, clay, iron, crop, updated_at) \
             SELECT id, 750, 750, 750, 750, now() FROM villages \
             ON CONFLICT (village_id) DO NOTHING",
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(repo.stored_resources(village_id).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn build_order_lifecycle() {
        let _ = dotenvy::dotenv();
        let Ok(url) = std::env::var("DATABASE_URL") else {
            eprintln!("skipping build lifecycle test: DATABASE_URL not set");
            return;
        };
        let pool = crate::create_pool(&url).await.expect("connect");
        crate::run_migrations(&pool).await.expect("migrate");
        let config = WorldConfig::new(GameSpeed::new(1.0).unwrap(), 50);
        let world_id = crate::world::ensure_world(&pool, &config)
            .await
            .expect("ensure world");
        let rules = crate::economy_rules().expect("economy rules");
        let repo = PgAccountRepository::new(
            pool.clone(),
            world_id,
            config.radius,
            rules.starting_amounts,
        );

        let uname = format!("build_{}", Uuid::new_v4().simple());
        let user = repo
            .create_account(
                NewUser {
                    username: uname.clone(),
                    email: format!("{uname}@example.com"),
                    password_hash: "h".to_owned(),
                    email_confirmed: true,
                },
                &crate::starting_village().unwrap(),
            )
            .await
            .expect("create account");
        let village_id = repo.villages_of(user.id).await.unwrap()[0].id;

        let now = Timestamp(1_700_000_000_000);
        let order = NewBuildOrder {
            target: BuildTarget::Field { slot: 0 },
            target_level: 1,
            complete_at: Timestamp(now.0 + 1000),
        };
        let settled = ResourceAmounts {
            wood: 700,
            clay: 700,
            iron: 700,
            crop: 700,
        };

        // AC1: starting a build settles resources + creates the order.
        repo.start_build(village_id, settled, now, order)
            .await
            .expect("start build");
        let active = repo
            .active_build(village_id)
            .await
            .unwrap()
            .expect("active");
        assert_eq!(active.target, BuildTarget::Field { slot: 0 });
        assert_eq!(
            repo.stored_resources(village_id)
                .await
                .unwrap()
                .unwrap()
                .0
                .wood,
            700
        );

        // AC3: a second order is rejected (one active order, DB-enforced).
        assert!(matches!(
            repo.start_build(village_id, settled, now, order).await,
            Err(RepoError::Duplicate)
        ));

        // AC5: claim the due order and apply it; the field gains a level. Pending orders are persisted
        // (a fresh processor over the same DB would claim them), so this survives a restart.
        let due = repo
            .claim_due_builds(Timestamp(now.0 + 2000), 10)
            .await
            .unwrap();
        assert_eq!(due.len(), 1);
        repo.apply_build(due[0]).await.expect("apply build");
        let fields = repo.villages_of(user.id).await.unwrap()[0].fields.clone();
        assert_eq!(fields[0].level, 1);
        assert!(
            repo.claim_due_builds(Timestamp(now.0 + 2000), 10)
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn process_due_builds_applies_due_orders() {
        let _ = dotenvy::dotenv();
        let Ok(url) = std::env::var("DATABASE_URL") else {
            eprintln!("skipping process_due_builds test: DATABASE_URL not set");
            return;
        };
        let pool = crate::create_pool(&url).await.expect("connect");
        crate::run_migrations(&pool).await.expect("migrate");
        let config = WorldConfig::new(GameSpeed::new(1.0).unwrap(), 50);
        let world_id = crate::world::ensure_world(&pool, &config)
            .await
            .expect("ensure world");
        let rules = crate::economy_rules().expect("economy rules");
        let repo = PgAccountRepository::new(
            pool.clone(),
            world_id,
            config.radius,
            rules.starting_amounts,
        );

        let uname = format!("proc_{}", Uuid::new_v4().simple());
        let user = repo
            .create_account(
                NewUser {
                    username: uname.clone(),
                    email: format!("{uname}@example.com"),
                    password_hash: "h".to_owned(),
                    email_confirmed: true,
                },
                &crate::starting_village().unwrap(),
            )
            .await
            .expect("create account");
        let village_id = repo.villages_of(user.id).await.unwrap()[0].id;

        let now = Timestamp(2_000_000_000_000);
        repo.start_build(
            village_id,
            ResourceAmounts {
                wood: 700,
                clay: 700,
                iron: 700,
                crop: 700,
            },
            Timestamp(now.0 - 10_000),
            NewBuildOrder {
                target: BuildTarget::Field { slot: 1 },
                target_level: 1,
                complete_at: Timestamp(now.0 - 1000), // already due at `now`
            },
        )
        .await
        .expect("start build");

        // T6/AC5: the scheduler's use-case claims and applies due orders. `claim_due_builds` is
        // DB-global, and parallel tests may have their own due orders, so assert *this* village's
        // outcome (its field reached level 1) rather than a global processed count.
        eperica_application::process_due_builds(&repo, now, 1000)
            .await
            .expect("process due builds");
        let fields = repo.villages_of(user.id).await.unwrap()[0].fields.clone();
        assert_eq!(fields[1].level, 1);
    }

    /// AC5 (building path): constructing a new building in an empty center slot exercises the
    /// `apply_build` Building arm — the `INSERT ... ON CONFLICT` upsert taking its INSERT branch.
    /// The starting village has only Main Building (slot 0) + Rally Point (slot 1), so building a
    /// Warehouse at slot 2 creates a brand-new row (vs. the Field path, which only ever UPDATEs).
    #[tokio::test]
    async fn build_constructs_new_building_in_empty_slot() {
        let _ = dotenvy::dotenv();
        let Ok(url) = std::env::var("DATABASE_URL") else {
            eprintln!("skipping building construction test: DATABASE_URL not set");
            return;
        };
        let pool = crate::create_pool(&url).await.expect("connect");
        crate::run_migrations(&pool).await.expect("migrate");
        let config = WorldConfig::new(GameSpeed::new(1.0).unwrap(), 50);
        let world_id = crate::world::ensure_world(&pool, &config)
            .await
            .expect("ensure world");
        let rules = crate::economy_rules().expect("economy rules");
        let repo = PgAccountRepository::new(
            pool.clone(),
            world_id,
            config.radius,
            rules.starting_amounts,
        );

        let uname = format!("warehouse_{}", Uuid::new_v4().simple());
        let user = repo
            .create_account(
                NewUser {
                    username: uname.clone(),
                    email: format!("{uname}@example.com"),
                    password_hash: "h".to_owned(),
                    email_confirmed: true,
                },
                &crate::starting_village().unwrap(),
            )
            .await
            .expect("create account");
        let village_id = repo.villages_of(user.id).await.unwrap()[0].id;

        // Precondition: the empty slot has no Warehouse yet.
        assert!(
            repo.villages_of(user.id).await.unwrap()[0]
                .buildings
                .iter()
                .all(|b| b.kind != BuildingKind::Warehouse),
            "starting village should not have a Warehouse"
        );

        let now = Timestamp(2_100_000_000_000);
        repo.start_build(
            village_id,
            ResourceAmounts {
                wood: 700,
                clay: 700,
                iron: 700,
                crop: 700,
            },
            Timestamp(now.0 - 10_000),
            NewBuildOrder {
                target: BuildTarget::Building {
                    slot: 2,
                    kind: BuildingKind::Warehouse,
                },
                target_level: 1,
                complete_at: Timestamp(now.0 - 1000), // already due at `now`
            },
        )
        .await
        .expect("start build");

        // Claim + apply the due build; the empty slot now holds a level-1 Warehouse.
        let due = repo.claim_due_builds(now, 1000).await.unwrap();
        let mine = due
            .iter()
            .find(|d| d.village == village_id)
            .copied()
            .expect("this village's build is due");
        repo.apply_build(mine).await.expect("apply build");

        let warehouse = repo.villages_of(user.id).await.unwrap()[0]
            .buildings
            .iter()
            .find(|b| b.kind == BuildingKind::Warehouse)
            .copied()
            .expect("Warehouse was constructed");
        assert_eq!(warehouse.level, 1);
    }
}
