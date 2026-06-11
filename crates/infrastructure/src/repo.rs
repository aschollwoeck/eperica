//! PostgreSQL adapter for the application's [`AccountRepository`] port.

use async_trait::async_trait;
use eperica_application::{
    AccountRepository, ActiveBuild, ActiveTraining, ActiveUnitOrder, BuildRepository, DueBuild,
    DueTraining, DueUnitOrder, NewBuildOrder, NewTrainingOrder, NewUnitOrder, NewUser, RepoError,
    StarvationRepository, TrainingRepository, UnitOrderKind, UnitRepository, UserRecord,
};
use eperica_domain::{
    BuildTarget, BuildingKind, BuildingSlot, Coordinate, PlayerId, QueueLane, ResourceAmounts,
    ResourceField, ResourceKind, StartingVillage, Timestamp, Tribe, UnitCounts, UnitId, Village,
    VillageId, WorldId, coordinates_within,
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

    /// Reset unit orders stuck in `processing` back to `pending` (crash recovery).
    /// `apply_unit_order` is idempotent, so reprocessing is safe.
    pub async fn requeue_orphaned_unit_orders(&self) -> Result<u64, RepoError> {
        let result =
            sqlx::query("UPDATE unit_orders SET status = 'pending' WHERE status = 'processing'")
                .execute(&self.pool)
                .await
                .map_err(backend)?;
        Ok(result.rows_affected())
    }

    /// Reset training batches stuck in `processing` back to `active` (crash recovery). Safe:
    /// `apply_training` moves garrison and progress in one transaction, so a re-claim recomputes
    /// completions from the unchanged `count_done` (AC5).
    pub async fn requeue_orphaned_training(&self) -> Result<u64, RepoError> {
        let result =
            sqlx::query("UPDATE training_orders SET status = 'active' WHERE status = 'processing'")
                .execute(&self.pool)
                .await
                .map_err(backend)?;
        Ok(result.rows_affected())
    }

    /// Reset starvation checks stuck in `processing` back to `pending` (crash recovery). Safe:
    /// the handler re-validates from live state at fire time (AC7).
    pub async fn requeue_orphaned_starvation(&self) -> Result<u64, RepoError> {
        let result = sqlx::query(
            "UPDATE starvation_checks SET status = 'pending' WHERE status = 'processing'",
        )
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
        BuildingKind::Barracks => "barracks",
        BuildingKind::Academy => "academy",
        BuildingKind::Smithy => "smithy",
        BuildingKind::Stable => "stable",
        BuildingKind::Workshop => "workshop",
        BuildingKind::Residence => "residence",
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
        "barracks" => Ok(BuildingKind::Barracks),
        "academy" => Ok(BuildingKind::Academy),
        "smithy" => Ok(BuildingKind::Smithy),
        "stable" => Ok(BuildingKind::Stable),
        "workshop" => Ok(BuildingKind::Workshop),
        "residence" => Ok(BuildingKind::Residence),
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
    let tribe_str: String = r.try_get("tribe").map_err(backend)?;
    let tribe = Tribe::from_slug(&tribe_str)
        .ok_or_else(|| RepoError::Backend(format!("unknown tribe: {tribe_str}")))?;
    Ok(UserRecord {
        id: PlayerId(id.as_u128()),
        username: r.try_get("username").map_err(backend)?,
        email: r.try_get("email").map_err(backend)?,
        password_hash: r.try_get("password_hash").map_err(backend)?,
        email_confirmed: r.try_get("email_confirmed").map_err(backend)?,
        tribe,
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
            "INSERT INTO users (id, username, email, password_hash, email_confirmed, tribe) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(user_id)
        .bind(&user.username)
        .bind(&user.email)
        .bind(&user.password_hash)
        .bind(user.email_confirmed)
        .bind(user.tribe.slug())
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
            let village = Village::found(
                VillageId(village_uuid.as_u128()),
                owner,
                coord,
                user.tribe,
                template,
            );

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
            .bind(user.tribe.slug())
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
            tribe: user.tribe,
        })
    }

    async fn find_user_by_username(&self, username: &str) -> Result<Option<UserRecord>, RepoError> {
        let row = sqlx::query(
            "SELECT id, username, email, password_hash, email_confirmed, tribe FROM users WHERE username = $1",
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await
        .map_err(backend)?;
        row.as_ref().map(row_to_user).transpose()
    }

    async fn find_user_by_id(&self, id: PlayerId) -> Result<Option<UserRecord>, RepoError> {
        let row = sqlx::query(
            "SELECT id, username, email, password_hash, email_confirmed, tribe FROM users WHERE id = $1",
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

    async fn village_by_id(&self, village: VillageId) -> Result<Option<Village>, RepoError> {
        let vid = Uuid::from_u128(village.0);
        let Some(r) = sqlx::query("SELECT owner_id, x, y, tribe FROM villages WHERE id = $1")
            .bind(vid)
            .fetch_optional(&self.pool)
            .await
            .map_err(backend)?
        else {
            return Ok(None);
        };
        let owner: Uuid = r.try_get("owner_id").map_err(backend)?;
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
            let kind = parse_resource(&fr.try_get::<String, _>("resource_type").map_err(backend)?)?;
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
            let kind = parse_building(&br.try_get::<String, _>("building_type").map_err(backend)?)?;
            let level: i16 = br.try_get("level").map_err(backend)?;
            buildings.push(BuildingSlot {
                kind,
                level: u8::try_from(level).unwrap_or(0),
            });
        }

        Ok(Some(Village {
            id: village,
            owner: PlayerId(owner.as_u128()),
            coordinate: Coordinate::new(x, y),
            tribe: parse_tribe(tribe_raw)?,
            fields,
            buildings,
        }))
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

    async fn garrison(&self, village: VillageId) -> Result<UnitCounts, RepoError> {
        let rows = sqlx::query(
            "SELECT unit_id, count FROM village_units WHERE village_id = $1 ORDER BY unit_id",
        )
        .bind(Uuid::from_u128(village.0))
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;
        rows.iter()
            .map(|r| {
                let unit: String = r.try_get("unit_id").map_err(backend)?;
                let count: i32 = r.try_get("count").map_err(backend)?;
                Ok((UnitId(unit), u32::try_from(count).unwrap_or(0)))
            })
            .collect()
    }
}

fn lane_str(lane: QueueLane) -> &'static str {
    match lane {
        QueueLane::All => "all",
        QueueLane::Field => "field",
        QueueLane::Building => "building",
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
        settled_from: Timestamp,
        now: Timestamp,
        order: NewBuildOrder,
    ) -> Result<(), RepoError> {
        let (table, slot, building_type) = target_columns(order.target);
        let vid = Uuid::from_u128(village.0);
        let mut tx = self.pool.begin().await.map_err(backend)?;

        // Optimistic settle: only applies if the row is still at the snapshot the caller computed
        // `settled` from — a concurrent order on another queue cannot have its debit overwritten
        // (P2/P4). The comparison uses the same ms expression `stored_resources` reads.
        let updated = sqlx::query(
            "UPDATE village_resources SET wood=$1, clay=$2, iron=$3, crop=$4, \
             updated_at = to_timestamp($5::double precision / 1000.0) \
             WHERE village_id=$6 \
               AND (EXTRACT(EPOCH FROM updated_at) * 1000)::bigint = $7",
        )
        .bind(settled.wood)
        .bind(settled.clay)
        .bind(settled.iron)
        .bind(settled.crop)
        .bind(now.0 as f64)
        .bind(vid)
        .bind(settled_from.0)
        .execute(&mut *tx)
        .await
        .map_err(backend)?;
        if updated.rows_affected() == 0 {
            // Also covers a missing resources row — callers just read it, so that is unreachable.
            return Err(RepoError::Conflict);
        }

        let insert = sqlx::query(
            "INSERT INTO build_orders \
             (id, village_id, target_table, slot, building_type, target_level, complete_at, status, lane) \
             VALUES ($1, $2, $3, $4, $5, $6, to_timestamp($7::double precision / 1000.0), 'pending', $8)",
        )
        .bind(Uuid::new_v4())
        .bind(vid)
        .bind(table)
        .bind(slot)
        .bind(building_type)
        .bind(i16::from(order.target_level))
        .bind(order.complete_at.0 as f64)
        .bind(lane_str(order.lane))
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

    async fn active_builds(&self, village: VillageId) -> Result<Vec<ActiveBuild>, RepoError> {
        let rows = sqlx::query(
            "SELECT target_table, slot, building_type, target_level, \
             (EXTRACT(EPOCH FROM complete_at) * 1000)::bigint AS complete_ms \
             FROM build_orders WHERE village_id = $1 AND status = 'pending' \
             ORDER BY complete_at, id",
        )
        .bind(Uuid::from_u128(village.0))
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;

        let mut out = Vec::with_capacity(rows.len());
        for r in &rows {
            let table: String = r.try_get("target_table").map_err(backend)?;
            let slot: i16 = r.try_get("slot").map_err(backend)?;
            let building_type: Option<String> = r.try_get("building_type").map_err(backend)?;
            let target_level: i16 = r.try_get("target_level").map_err(backend)?;
            let complete_ms: i64 = r.try_get("complete_ms").map_err(backend)?;
            out.push(ActiveBuild {
                target: parse_target(&table, slot, building_type)?,
                target_level: u8::try_from(target_level).unwrap_or(0),
                complete_at: Timestamp(complete_ms),
            });
        }
        Ok(out)
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

fn unit_order_kind_str(kind: UnitOrderKind) -> &'static str {
    match kind {
        UnitOrderKind::Research => "research",
        UnitOrderKind::SmithyUpgrade => "smithy",
    }
}

fn parse_unit_order_kind(s: &str) -> Result<UnitOrderKind, RepoError> {
    match s {
        "research" => Ok(UnitOrderKind::Research),
        "smithy" => Ok(UnitOrderKind::SmithyUpgrade),
        other => Err(RepoError::Backend(format!(
            "unknown unit order kind: {other}"
        ))),
    }
}

#[async_trait]
impl UnitRepository for PgAccountRepository {
    async fn start_unit_order(
        &self,
        village: VillageId,
        settled: ResourceAmounts,
        settled_from: Timestamp,
        now: Timestamp,
        order: NewUnitOrder,
    ) -> Result<(), RepoError> {
        let vid = Uuid::from_u128(village.0);
        let mut tx = self.pool.begin().await.map_err(backend)?;

        // Optimistic settle — see `start_build`: a stale snapshot must not overwrite a concurrent
        // debit from another queue (P2/P4).
        let updated = sqlx::query(
            "UPDATE village_resources SET wood=$1, clay=$2, iron=$3, crop=$4, \
             updated_at = to_timestamp($5::double precision / 1000.0) \
             WHERE village_id=$6 \
               AND (EXTRACT(EPOCH FROM updated_at) * 1000)::bigint = $7",
        )
        .bind(settled.wood)
        .bind(settled.clay)
        .bind(settled.iron)
        .bind(settled.crop)
        .bind(now.0 as f64)
        .bind(vid)
        .bind(settled_from.0)
        .execute(&mut *tx)
        .await
        .map_err(backend)?;
        if updated.rows_affected() == 0 {
            // Also covers a missing resources row — callers just read it, so that is unreachable.
            return Err(RepoError::Conflict);
        }

        let insert = sqlx::query(
            "INSERT INTO unit_orders (id, village_id, kind, unit_id, target_level, complete_at, status) \
             VALUES ($1, $2, $3, $4, $5, to_timestamp($6::double precision / 1000.0), 'pending')",
        )
        .bind(Uuid::new_v4())
        .bind(vid)
        .bind(unit_order_kind_str(order.kind))
        .bind(order.unit.as_str())
        .bind(order.target_level.map(i16::from))
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

    async fn active_unit_orders(
        &self,
        village: VillageId,
    ) -> Result<Vec<ActiveUnitOrder>, RepoError> {
        let rows = sqlx::query(
            "SELECT kind, unit_id, target_level, \
             (EXTRACT(EPOCH FROM complete_at) * 1000)::bigint AS complete_ms \
             FROM unit_orders WHERE village_id = $1 AND status = 'pending'",
        )
        .bind(Uuid::from_u128(village.0))
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;

        let mut out = Vec::with_capacity(rows.len());
        for r in &rows {
            let kind: String = r.try_get("kind").map_err(backend)?;
            let unit: String = r.try_get("unit_id").map_err(backend)?;
            let target_level: Option<i16> = r.try_get("target_level").map_err(backend)?;
            let complete_ms: i64 = r.try_get("complete_ms").map_err(backend)?;
            out.push(ActiveUnitOrder {
                kind: parse_unit_order_kind(&kind)?,
                unit: UnitId(unit),
                target_level: target_level.map(|l| u8::try_from(l).unwrap_or(0)),
                complete_at: Timestamp(complete_ms),
            });
        }
        Ok(out)
    }

    async fn researched_units(&self, village: VillageId) -> Result<Vec<UnitId>, RepoError> {
        let rows = sqlx::query("SELECT unit_id FROM village_research WHERE village_id = $1")
            .bind(Uuid::from_u128(village.0))
            .fetch_all(&self.pool)
            .await
            .map_err(backend)?;
        rows.iter()
            .map(|r| Ok(UnitId(r.try_get("unit_id").map_err(backend)?)))
            .collect()
    }

    async fn unit_levels(&self, village: VillageId) -> Result<Vec<(UnitId, u8)>, RepoError> {
        let rows =
            sqlx::query("SELECT unit_id, level FROM village_unit_levels WHERE village_id = $1")
                .bind(Uuid::from_u128(village.0))
                .fetch_all(&self.pool)
                .await
                .map_err(backend)?;
        rows.iter()
            .map(|r| {
                let unit: String = r.try_get("unit_id").map_err(backend)?;
                let level: i16 = r.try_get("level").map_err(backend)?;
                Ok((UnitId(unit), u8::try_from(level).unwrap_or(0)))
            })
            .collect()
    }

    async fn claim_due_unit_orders(
        &self,
        now: Timestamp,
        limit: i64,
    ) -> Result<Vec<DueUnitOrder>, RepoError> {
        let rows = sqlx::query(
            "UPDATE unit_orders SET status = 'processing' WHERE id IN ( \
                 SELECT id FROM unit_orders \
                 WHERE status = 'pending' AND complete_at <= to_timestamp($1::double precision / 1000.0) \
                 ORDER BY complete_at, id LIMIT $2 FOR UPDATE SKIP LOCKED \
             ) RETURNING id, village_id, kind, unit_id, target_level",
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
            let kind: String = r.try_get("kind").map_err(backend)?;
            let unit: String = r.try_get("unit_id").map_err(backend)?;
            let target_level: Option<i16> = r.try_get("target_level").map_err(backend)?;
            out.push(DueUnitOrder {
                id: id.as_u128(),
                village: VillageId(village.as_u128()),
                kind: parse_unit_order_kind(&kind)?,
                unit: UnitId(unit),
                target_level: target_level.map(|l| u8::try_from(l).unwrap_or(0)),
            });
        }
        Ok(out)
    }

    async fn apply_unit_order(&self, due: DueUnitOrder) -> Result<(), RepoError> {
        let vid = Uuid::from_u128(due.village.0);
        let mut tx = self.pool.begin().await.map_err(backend)?;

        match due.kind {
            UnitOrderKind::Research => {
                // Idempotent: re-applying an already-researched unit is a no-op (AC8).
                sqlx::query(
                    "INSERT INTO village_research (village_id, unit_id) VALUES ($1, $2) \
                     ON CONFLICT (village_id, unit_id) DO NOTHING",
                )
                .bind(vid)
                .bind(due.unit.as_str())
                .execute(&mut *tx)
                .await
                .map_err(backend)?;
            }
            UnitOrderKind::SmithyUpgrade => {
                // Idempotent: sets the absolute target level (AC12).
                let level = i16::from(due.target_level.unwrap_or(0));
                sqlx::query(
                    "INSERT INTO village_unit_levels (village_id, unit_id, level) \
                     VALUES ($1, $2, $3) \
                     ON CONFLICT (village_id, unit_id) DO UPDATE SET level = EXCLUDED.level",
                )
                .bind(vid)
                .bind(due.unit.as_str())
                .bind(level)
                .execute(&mut *tx)
                .await
                .map_err(backend)?;
            }
        }

        sqlx::query("UPDATE unit_orders SET status = 'done' WHERE id = $1")
            .bind(Uuid::from_u128(due.id))
            .execute(&mut *tx)
            .await
            .map_err(backend)?;

        tx.commit().await.map_err(backend)?;
        Ok(())
    }
}

#[async_trait]
impl TrainingRepository for PgAccountRepository {
    async fn start_training(
        &self,
        village: VillageId,
        settled: ResourceAmounts,
        settled_from: Timestamp,
        now: Timestamp,
        order: NewTrainingOrder,
    ) -> Result<(), RepoError> {
        let vid = Uuid::from_u128(village.0);
        let mut tx = self.pool.begin().await.map_err(backend)?;

        // Optimistic settle — see `start_build`: a stale snapshot must not overwrite a concurrent
        // debit from another queue (P2/P4).
        let updated = sqlx::query(
            "UPDATE village_resources SET wood=$1, clay=$2, iron=$3, crop=$4, \
             updated_at = to_timestamp($5::double precision / 1000.0) \
             WHERE village_id=$6 \
               AND (EXTRACT(EPOCH FROM updated_at) * 1000)::bigint = $7",
        )
        .bind(settled.wood)
        .bind(settled.clay)
        .bind(settled.iron)
        .bind(settled.crop)
        .bind(now.0 as f64)
        .bind(vid)
        .bind(settled_from.0)
        .execute(&mut *tx)
        .await
        .map_err(backend)?;
        if updated.rows_affected() == 0 {
            // Also covers a missing resources row — callers just read it, so that is unreachable.
            return Err(RepoError::Conflict);
        }

        let next_ms = now.0 + order.per_unit_secs.saturating_mul(1000);
        let insert = sqlx::query(
            "INSERT INTO training_orders \
             (id, village_id, building, unit_id, count_total, per_unit_secs, started_at, \
              next_complete_at, status) \
             VALUES ($1, $2, $3, $4, $5, $6, to_timestamp($7::double precision / 1000.0), \
                     to_timestamp($8::double precision / 1000.0), 'active')",
        )
        .bind(Uuid::new_v4())
        .bind(vid)
        .bind(building_str(order.building))
        .bind(order.unit.as_str())
        .bind(i32::try_from(order.count).unwrap_or(i32::MAX))
        .bind(order.per_unit_secs)
        .bind(now.0 as f64)
        .bind(next_ms as f64)
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

    async fn active_training(&self, village: VillageId) -> Result<Vec<ActiveTraining>, RepoError> {
        let rows = sqlx::query(
            "SELECT building, unit_id, count_total, count_done, per_unit_secs, \
             (EXTRACT(EPOCH FROM next_complete_at) * 1000)::bigint AS next_ms \
             FROM training_orders \
             WHERE village_id = $1 AND status IN ('active', 'processing') \
             ORDER BY building",
        )
        .bind(Uuid::from_u128(village.0))
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;

        let mut out = Vec::with_capacity(rows.len());
        for r in &rows {
            let building: String = r.try_get("building").map_err(backend)?;
            let unit: String = r.try_get("unit_id").map_err(backend)?;
            let count_total: i32 = r.try_get("count_total").map_err(backend)?;
            let count_done: i32 = r.try_get("count_done").map_err(backend)?;
            let per_unit_secs: i64 = r.try_get("per_unit_secs").map_err(backend)?;
            let next_ms: i64 = r.try_get("next_ms").map_err(backend)?;
            out.push(ActiveTraining {
                building: parse_building(&building)?,
                unit: UnitId(unit),
                count_total: u32::try_from(count_total).unwrap_or(0),
                count_done: u32::try_from(count_done).unwrap_or(0),
                per_unit_secs,
                next_complete_at: Timestamp(next_ms),
            });
        }
        Ok(out)
    }

    async fn claim_due_training(
        &self,
        now: Timestamp,
        limit: i64,
    ) -> Result<Vec<DueTraining>, RepoError> {
        let rows = sqlx::query(
            "UPDATE training_orders SET status = 'processing' WHERE id IN ( \
                 SELECT id FROM training_orders \
                 WHERE status = 'active' AND next_complete_at <= to_timestamp($1::double precision / 1000.0) \
                 ORDER BY next_complete_at, id LIMIT $2 FOR UPDATE SKIP LOCKED \
             ) RETURNING id, village_id, unit_id, count_total, count_done, per_unit_secs, \
                         (EXTRACT(EPOCH FROM started_at) * 1000)::bigint AS started_ms",
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
            let unit: String = r.try_get("unit_id").map_err(backend)?;
            let count_total: i32 = r.try_get("count_total").map_err(backend)?;
            let count_done: i32 = r.try_get("count_done").map_err(backend)?;
            let per_unit_secs: i64 = r.try_get("per_unit_secs").map_err(backend)?;
            let started_ms: i64 = r.try_get("started_ms").map_err(backend)?;
            out.push(DueTraining {
                id: id.as_u128(),
                village: VillageId(village.as_u128()),
                unit: UnitId(unit),
                count_total: u32::try_from(count_total).unwrap_or(0),
                count_done: u32::try_from(count_done).unwrap_or(0),
                per_unit_secs,
                started_at: Timestamp(started_ms),
            });
        }
        Ok(out)
    }

    async fn apply_training(&self, due: &DueTraining, completed: u32) -> Result<(), RepoError> {
        let vid = Uuid::from_u128(due.village.0);
        let mut tx = self.pool.begin().await.map_err(backend)?;

        if completed > 0 {
            sqlx::query(
                "INSERT INTO village_units (village_id, unit_id, count) VALUES ($1, $2, $3) \
                 ON CONFLICT (village_id, unit_id) \
                 DO UPDATE SET count = village_units.count + EXCLUDED.count",
            )
            .bind(vid)
            .bind(due.unit.as_str())
            .bind(i32::try_from(completed).unwrap_or(i32::MAX))
            .execute(&mut *tx)
            .await
            .map_err(backend)?;
        }

        let new_done = due
            .count_done
            .saturating_add(completed)
            .min(due.count_total);
        if new_done >= due.count_total {
            sqlx::query(
                "UPDATE training_orders SET count_done = $1, status = 'done' WHERE id = $2",
            )
            .bind(i32::try_from(new_done).unwrap_or(i32::MAX))
            .bind(Uuid::from_u128(due.id))
            .execute(&mut *tx)
            .await
            .map_err(backend)?;
        } else {
            // The (done+1)-th unit of the batch completes next.
            let next_ms = due.started_at.0
                + due
                    .per_unit_secs
                    .saturating_mul(i64::from(new_done) + 1)
                    .saturating_mul(1000);
            sqlx::query(
                "UPDATE training_orders SET count_done = $1, status = 'active', \
                 next_complete_at = to_timestamp($2::double precision / 1000.0) WHERE id = $3",
            )
            .bind(i32::try_from(new_done).unwrap_or(i32::MAX))
            .bind(next_ms as f64)
            .bind(Uuid::from_u128(due.id))
            .execute(&mut *tx)
            .await
            .map_err(backend)?;
        }

        tx.commit().await.map_err(backend)?;
        Ok(())
    }
}

#[async_trait]
impl StarvationRepository for PgAccountRepository {
    async fn schedule_starvation_check(
        &self,
        village: VillageId,
        due_at: Timestamp,
    ) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT INTO starvation_checks (village_id, due_at, status) \
             VALUES ($1, to_timestamp($2::double precision / 1000.0), 'pending') \
             ON CONFLICT (village_id) DO UPDATE \
             SET due_at = EXCLUDED.due_at, status = 'pending'",
        )
        .bind(Uuid::from_u128(village.0))
        .bind(due_at.0 as f64)
        .execute(&self.pool)
        .await
        .map_err(backend)?;
        Ok(())
    }

    async fn cancel_starvation_check(&self, village: VillageId) -> Result<(), RepoError> {
        sqlx::query("DELETE FROM starvation_checks WHERE village_id = $1")
            .bind(Uuid::from_u128(village.0))
            .execute(&self.pool)
            .await
            .map_err(backend)?;
        Ok(())
    }

    async fn claim_due_starvation(
        &self,
        now: Timestamp,
        limit: i64,
    ) -> Result<Vec<VillageId>, RepoError> {
        let rows = sqlx::query(
            "UPDATE starvation_checks SET status = 'processing' WHERE village_id IN ( \
                 SELECT village_id FROM starvation_checks \
                 WHERE status = 'pending' AND due_at <= to_timestamp($1::double precision / 1000.0) \
                 ORDER BY due_at, village_id LIMIT $2 FOR UPDATE SKIP LOCKED \
             ) RETURNING village_id",
        )
        .bind(now.0 as f64)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;
        rows.iter()
            .map(|r| {
                let id: Uuid = r.try_get("village_id").map_err(backend)?;
                Ok(VillageId(id.as_u128()))
            })
            .collect()
    }

    async fn apply_starvation(
        &self,
        village: VillageId,
        settled: ResourceAmounts,
        settled_from: Timestamp,
        now: Timestamp,
        survivors: &UnitCounts,
    ) -> Result<(), RepoError> {
        let vid = Uuid::from_u128(village.0);
        let mut tx = self.pool.begin().await.map_err(backend)?;

        // Optimistic settle — see `start_build`; on Conflict the claimed check stays `processing`
        // and is retried after an orphan requeue (AC7 exactly-once still holds).
        let updated = sqlx::query(
            "UPDATE village_resources SET wood=$1, clay=$2, iron=$3, crop=$4, \
             updated_at = to_timestamp($5::double precision / 1000.0) \
             WHERE village_id=$6 \
               AND (EXTRACT(EPOCH FROM updated_at) * 1000)::bigint = $7",
        )
        .bind(settled.wood)
        .bind(settled.clay)
        .bind(settled.iron)
        .bind(settled.crop)
        .bind(now.0 as f64)
        .bind(vid)
        .bind(settled_from.0)
        .execute(&mut *tx)
        .await
        .map_err(backend)?;
        if updated.rows_affected() == 0 {
            return Err(RepoError::Conflict);
        }

        sqlx::query("DELETE FROM village_units WHERE village_id = $1")
            .bind(vid)
            .execute(&mut *tx)
            .await
            .map_err(backend)?;
        for (unit, count) in survivors.iter().filter(|(_, c)| *c > 0) {
            sqlx::query(
                "INSERT INTO village_units (village_id, unit_id, count) VALUES ($1, $2, $3)",
            )
            .bind(vid)
            .bind(unit.as_str())
            .bind(i32::try_from(*count).unwrap_or(i32::MAX))
            .execute(&mut *tx)
            .await
            .map_err(backend)?;
        }

        sqlx::query("UPDATE starvation_checks SET status = 'done' WHERE village_id = $1")
            .bind(vid)
            .execute(&mut *tx)
            .await
            .map_err(backend)?;

        tx.commit().await.map_err(backend)?;
        Ok(())
    }

    async fn resolve_starvation_check(
        &self,
        village: VillageId,
        reschedule_at: Option<Timestamp>,
    ) -> Result<(), RepoError> {
        match reschedule_at {
            Some(due_at) => {
                sqlx::query(
                    "UPDATE starvation_checks \
                     SET due_at = to_timestamp($1::double precision / 1000.0), status = 'pending' \
                     WHERE village_id = $2",
                )
                .bind(due_at.0 as f64)
                .bind(Uuid::from_u128(village.0))
                .execute(&self.pool)
                .await
                .map_err(backend)?;
            }
            None => {
                sqlx::query("UPDATE starvation_checks SET status = 'done' WHERE village_id = $1")
                    .bind(Uuid::from_u128(village.0))
                    .execute(&self.pool)
                    .await
                    .map_err(backend)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eperica_domain::{GameSpeed, WorldConfig};

    /// The resources row's last-settled time — the snapshot orders must be computed from.
    async fn snapshot(repo: &PgAccountRepository, village: VillageId) -> Timestamp {
        repo.stored_resources(village).await.unwrap().unwrap().1
    }

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
            tribe: Tribe::Gauls,
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

        // 004 AC1: the chosen tribe is stored on the account and stamped on the village.
        assert_eq!(user.tribe, Tribe::Gauls);
        assert_eq!(villages[0].tribe, Some(Tribe::Gauls));

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
            tribe: Tribe::Gauls,
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
                    tribe: Tribe::Gauls,
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

    /// 004 AC3 migration-boundary guard: a village row that predates the tribe column being
    /// populated (tribe NULL — the pre-004 state; the column stays nullable) must be repaired by
    /// the 0005 backfill, which copies the owner's tribe.
    ///
    /// The *users* half of the backfill cannot be reproduced post-hoc (the column is NOT NULL
    /// after 0005); it is guaranteed by the migration itself — its `SET NOT NULL` aborts if any
    /// row were left without a tribe — so only the villages half needs a data-level test.
    #[tokio::test]
    async fn tribe_backfill_repairs_pre_004_village() {
        let _ = dotenvy::dotenv();
        let Ok(url) = std::env::var("DATABASE_URL") else {
            eprintln!("skipping tribe backfill test: DATABASE_URL not set");
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

        let uname = format!("pretribe_{}", Uuid::new_v4().simple());
        let user = repo
            .create_account(
                NewUser {
                    username: uname.clone(),
                    email: format!("{uname}@example.com"),
                    password_hash: "hash".to_owned(),
                    email_confirmed: true,
                    tribe: Tribe::Gauls,
                },
                &crate::starting_village().unwrap(),
            )
            .await
            .expect("create account");
        let village_id = repo.villages_of(user.id).await.unwrap()[0].id;

        // Reproduce the pre-004 state: the village has no tribe yet.
        sqlx::query("UPDATE villages SET tribe = NULL WHERE id = $1")
            .bind(Uuid::from_u128(village_id.0))
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(repo.villages_of(user.id).await.unwrap()[0].tribe, None);

        // Apply the backfill (same statement as migration 0005): tribe copied from the owner.
        sqlx::query(
            "UPDATE villages v SET tribe = u.tribe FROM users u \
             WHERE v.owner_id = u.id AND v.tribe IS NULL",
        )
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(
            repo.villages_of(user.id).await.unwrap()[0].tribe,
            Some(Tribe::Gauls)
        );
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
                    tribe: Tribe::Gauls,
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
            lane: QueueLane::All,
        };
        let settled = ResourceAmounts {
            wood: 700,
            clay: 700,
            iron: 700,
            crop: 700,
        };

        // AC1: starting a build settles resources + creates the order.
        let snap = snapshot(&repo, village_id).await;
        repo.start_build(village_id, settled, snap, now, order)
            .await
            .expect("start build");
        let active = repo.active_builds(village_id).await.unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].target, BuildTarget::Field { slot: 0 });
        assert_eq!(
            repo.stored_resources(village_id)
                .await
                .unwrap()
                .unwrap()
                .0
                .wood,
            700
        );

        // AC3: a second order is rejected (one active order, DB-enforced). The first settle moved
        // the snapshot to `now`, so a fresh caller computes from there.
        assert!(matches!(
            repo.start_build(village_id, settled, now, now, order).await,
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

    /// 004 AC13: a Roman village holds one field and one building order concurrently (separate
    /// lanes), but never two of the same lane; a non-Roman village is limited to one in total
    /// (single 'all' lane) — both DB-enforced under races by the partial unique index.
    #[tokio::test]
    async fn roman_lanes_allow_field_and_building_in_parallel() {
        let _ = dotenvy::dotenv();
        let Ok(url) = std::env::var("DATABASE_URL") else {
            eprintln!("skipping lane test: DATABASE_URL not set");
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

        let now = Timestamp(1_700_000_000_000);
        let settled = ResourceAmounts {
            wood: 500,
            clay: 500,
            iron: 500,
            crop: 500,
        };
        // Due far beyond any other test's global claim window (the largest synthetic "now" used
        // by parallel tests is 2.1e12), so they can never claim these pending orders away.
        let order = |target, lane| NewBuildOrder {
            target,
            target_level: 1,
            complete_at: Timestamp(now.0 + 1_000_000_000_000),
            lane,
        };
        let field = BuildTarget::Field { slot: 0 };
        let building = BuildTarget::Building {
            slot: 2,
            kind: BuildingKind::Warehouse,
        };

        // Roman village: a field order and a building order coexist.
        let uname = format!("lane_r_{}", Uuid::new_v4().simple());
        let roman = repo
            .create_account(
                NewUser {
                    username: uname.clone(),
                    email: format!("{uname}@example.com"),
                    password_hash: "h".to_owned(),
                    email_confirmed: true,
                    tribe: Tribe::Romans,
                },
                &crate::starting_village().unwrap(),
            )
            .await
            .expect("create roman");
        let rv = repo.villages_of(roman.id).await.unwrap()[0].id;
        let snap = snapshot(&repo, rv).await;
        repo.start_build(rv, settled, snap, now, order(field, QueueLane::Field))
            .await
            .expect("field lane");
        repo.start_build(rv, settled, now, now, order(building, QueueLane::Building))
            .await
            .expect("building lane runs in parallel");
        assert_eq!(repo.active_builds(rv).await.unwrap().len(), 2);
        // A second order in an occupied lane is rejected.
        assert!(matches!(
            repo.start_build(
                rv,
                settled,
                now,
                now,
                order(BuildTarget::Field { slot: 1 }, QueueLane::Field)
            )
            .await,
            Err(RepoError::Duplicate)
        ));

        // Non-Roman village: any second order is rejected (single 'all' lane).
        let uname = format!("lane_g_{}", Uuid::new_v4().simple());
        let gaul = repo
            .create_account(
                NewUser {
                    username: uname.clone(),
                    email: format!("{uname}@example.com"),
                    password_hash: "h".to_owned(),
                    email_confirmed: true,
                    tribe: Tribe::Gauls,
                },
                &crate::starting_village().unwrap(),
            )
            .await
            .expect("create gaul");
        let gv = repo.villages_of(gaul.id).await.unwrap()[0].id;
        let snap = snapshot(&repo, gv).await;
        repo.start_build(gv, settled, snap, now, order(field, QueueLane::All))
            .await
            .expect("first order");
        assert!(matches!(
            repo.start_build(gv, settled, now, now, order(building, QueueLane::All))
                .await,
            Err(RepoError::Duplicate)
        ));
    }

    /// 004 AC6/AC8/AC10/AC12: unit-order lifecycle — one active order per kind (DB-enforced),
    /// settle+debit on start, apply-exactly-once (idempotent), pending orders survive a restart
    /// (orphan requeue reproduces crash recovery).
    #[tokio::test]
    async fn unit_order_lifecycle() {
        let _ = dotenvy::dotenv();
        let Ok(url) = std::env::var("DATABASE_URL") else {
            eprintln!("skipping unit order test: DATABASE_URL not set");
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

        let uname = format!("unit_{}", Uuid::new_v4().simple());
        let user = repo
            .create_account(
                NewUser {
                    username: uname.clone(),
                    email: format!("{uname}@example.com"),
                    password_hash: "h".to_owned(),
                    email_confirmed: true,
                    tribe: Tribe::Gauls,
                },
                &crate::starting_village().unwrap(),
            )
            .await
            .expect("create account");
        let village_id = repo.villages_of(user.id).await.unwrap()[0].id;

        let now = Timestamp(1_700_000_000_000);
        let settled = ResourceAmounts {
            wood: 600,
            clay: 600,
            iron: 600,
            crop: 600,
        };
        let research = NewUnitOrder {
            kind: UnitOrderKind::Research,
            unit: UnitId("swordsman".into()),
            target_level: None,
            complete_at: Timestamp(now.0 + 1000),
        };

        // AC6: starting a research settles resources and creates the order.
        let snap = snapshot(&repo, village_id).await;
        repo.start_unit_order(village_id, settled, snap, now, research.clone())
            .await
            .expect("start research");

        // The race the optimistic settle exists for: a caller that computed from the now-stale
        // snapshot must conflict instead of overwriting the research debit (P2/P4).
        let stale = NewUnitOrder {
            kind: UnitOrderKind::SmithyUpgrade,
            unit: UnitId("phalanx".into()),
            target_level: Some(1),
            complete_at: Timestamp(now.0 + 1500),
        };
        assert!(matches!(
            repo.start_unit_order(
                village_id,
                ResourceAmounts {
                    wood: 450,
                    clay: 450,
                    iron: 450,
                    crop: 450,
                },
                snap,
                now,
                stale,
            )
            .await,
            Err(RepoError::Conflict)
        ));
        // The research debit survived the conflicting attempt.
        assert_eq!(
            repo.stored_resources(village_id).await.unwrap().unwrap().0,
            settled
        );
        assert_eq!(
            repo.stored_resources(village_id)
                .await
                .unwrap()
                .unwrap()
                .0
                .wood,
            600
        );

        // A second research is rejected (one per kind, DB-enforced under races)...
        assert!(matches!(
            repo.start_unit_order(village_id, settled, now, now, research.clone())
                .await,
            Err(RepoError::Duplicate)
        ));
        // ...but a Smithy upgrade (computed from the fresh snapshot) runs concurrently...
        let upgrade = NewUnitOrder {
            kind: UnitOrderKind::SmithyUpgrade,
            unit: UnitId("phalanx".into()),
            target_level: Some(1),
            complete_at: Timestamp(now.0 + 1500),
        };
        repo.start_unit_order(village_id, settled, now, now, upgrade.clone())
            .await
            .expect("start upgrade");
        // ...and a second upgrade is rejected too.
        assert!(matches!(
            repo.start_unit_order(village_id, settled, now, now, upgrade)
                .await,
            Err(RepoError::Duplicate)
        ));
        assert_eq!(repo.active_unit_orders(village_id).await.unwrap().len(), 2);

        // AC8/AC12: claim both due orders and apply them — exactly once, idempotently.
        let due = repo
            .claim_due_unit_orders(Timestamp(now.0 + 2000), 10)
            .await
            .unwrap();
        let mine: Vec<_> = due
            .into_iter()
            .filter(|d| d.village == village_id)
            .collect();
        assert_eq!(mine.len(), 2);
        for d in &mine {
            repo.apply_unit_order(d.clone()).await.expect("apply");
            repo.apply_unit_order(d.clone())
                .await
                .expect("re-apply is a no-op");
        }
        let researched = repo.researched_units(village_id).await.unwrap();
        assert_eq!(researched, vec![UnitId("swordsman".into())]);
        let levels = repo.unit_levels(village_id).await.unwrap();
        assert_eq!(levels, vec![(UnitId("phalanx".into()), 1)]);
        assert!(
            repo.claim_due_unit_orders(Timestamp(now.0 + 2000), 10)
                .await
                .unwrap()
                .is_empty()
        );

        // Crash recovery: a claimed-but-unapplied order is requeued and claimable again (AC8).
        repo.start_unit_order(
            village_id,
            settled,
            now,
            now,
            NewUnitOrder {
                kind: UnitOrderKind::Research,
                unit: UnitId("druidrider".into()),
                target_level: None,
                complete_at: Timestamp(now.0 + 1000),
            },
        )
        .await
        .expect("second research after first completed");
        let claimed = repo
            .claim_due_unit_orders(Timestamp(now.0 + 2000), 10)
            .await
            .unwrap();
        assert!(claimed.iter().any(|d| d.village == village_id));
        // "Crash" before applying: requeue orphans, then a fresh claim sees it again.
        assert!(repo.requeue_orphaned_unit_orders().await.unwrap() >= 1);
        let reclaimed = repo
            .claim_due_unit_orders(Timestamp(now.0 + 2000), 10)
            .await
            .unwrap();
        let mine: Vec<_> = reclaimed
            .into_iter()
            .filter(|d| d.village == village_id)
            .collect();
        assert_eq!(mine.len(), 1);
        repo.apply_unit_order(mine[0].clone()).await.expect("apply");
        assert_eq!(repo.researched_units(village_id).await.unwrap().len(), 2);
    }

    /// 005 AC2/AC5: training-batch lifecycle — settle+debit on start, one batch per building
    /// (DB-enforced), partial completions delivered exactly (k units after k × perUnit), crash
    /// recovery via orphan requeue, and no unit lost or duplicated.
    #[tokio::test]
    async fn training_batch_lifecycle() {
        let _ = dotenvy::dotenv();
        let Ok(url) = std::env::var("DATABASE_URL") else {
            eprintln!("skipping training test: DATABASE_URL not set");
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

        let uname = format!("train_{}", Uuid::new_v4().simple());
        let user = repo
            .create_account(
                NewUser {
                    username: uname.clone(),
                    email: format!("{uname}@example.com"),
                    password_hash: "h".to_owned(),
                    email_confirmed: true,
                    tribe: Tribe::Gauls,
                },
                &crate::starting_village().unwrap(),
            )
            .await
            .expect("create account");
        let village_id = repo.villages_of(user.id).await.unwrap()[0].id;

        let now = Timestamp(1_700_000_000_000);
        let settled = ResourceAmounts {
            wood: 400,
            clay: 400,
            iron: 400,
            crop: 400,
        };
        let order = NewTrainingOrder {
            building: BuildingKind::Barracks,
            unit: UnitId("phalanx".into()),
            count: 3,
            per_unit_secs: 100,
        };

        // AC2: starting a batch settles + debits and creates the order.
        let snap = snapshot(&repo, village_id).await;
        repo.start_training(village_id, settled, snap, now, order.clone())
            .await
            .expect("start training");
        assert_eq!(
            repo.stored_resources(village_id).await.unwrap().unwrap().0,
            settled
        );
        // A second batch at the same building is rejected (DB-enforced)...
        assert!(matches!(
            repo.start_training(village_id, settled, now, now, order.clone())
                .await,
            Err(RepoError::Duplicate)
        ));
        // ...but another building's queue is free.
        repo.start_training(
            village_id,
            settled,
            now,
            now,
            NewTrainingOrder {
                building: BuildingKind::Stable,
                unit: UnitId("pathfinder".into()),
                count: 1,
                per_unit_secs: 5_000,
            },
        )
        .await
        .expect("stable batch runs in parallel");
        assert_eq!(repo.active_training(village_id).await.unwrap().len(), 2);

        // AC5: at started + 2.5 × perUnit, exactly 2 units are due and delivered.
        let claim_at = Timestamp(now.0 + 250 * 1000);
        let due = repo.claim_due_training(claim_at, 10).await.unwrap();
        let mine: Vec<_> = due
            .into_iter()
            .filter(|d| d.village == village_id)
            .collect();
        assert_eq!(mine.len(), 1, "only the barracks batch is due");
        let d = &mine[0];
        let elapsed = (claim_at.0 - d.started_at.0) / 1000;
        let k = u32::try_from(elapsed / d.per_unit_secs).unwrap() - d.count_done;
        assert_eq!(k, 2);
        repo.apply_training(d, k).await.expect("apply");
        assert_eq!(
            repo.garrison(village_id).await.unwrap(),
            vec![(UnitId("phalanx".into()), 2)]
        );
        // Nothing more due at the same instant (next completion is at 3 × perUnit).
        assert!(
            repo.claim_due_training(claim_at, 10)
                .await
                .unwrap()
                .iter()
                .all(|d| d.village != village_id)
        );

        // Crash recovery: claim the final unit, "crash" before applying, requeue, re-claim — the
        // recomputed completion count is unchanged, so nothing is lost or duplicated.
        let final_at = Timestamp(now.0 + 320 * 1000);
        let due = repo.claim_due_training(final_at, 10).await.unwrap();
        assert!(due.iter().any(|d| d.village == village_id));
        assert!(repo.requeue_orphaned_training().await.unwrap() >= 1);
        let due = repo.claim_due_training(final_at, 10).await.unwrap();
        let d = due.iter().find(|d| d.village == village_id).expect("due");
        let elapsed = (final_at.0 - d.started_at.0) / 1000;
        let k = u32::try_from(elapsed / d.per_unit_secs).unwrap() - d.count_done;
        assert_eq!(k, 1);
        repo.apply_training(d, k).await.expect("apply final");
        assert_eq!(
            repo.garrison(village_id).await.unwrap(),
            vec![(UnitId("phalanx".into()), 3)]
        );
        // The finished batch never claims again; the building's queue is free for a new batch.
        assert!(
            repo.claim_due_training(Timestamp(final_at.0 + 1_000_000), 10)
                .await
                .unwrap()
                .iter()
                .all(|d| d.village != village_id)
        );
        // settled_from = `now`: the last settle (batch start) stamped the resources row then.
        repo.start_training(village_id, settled, now, final_at, order)
            .await
            .expect("queue free after completion");
    }

    /// 005 AC7 (persistence side): the depletion check is claimable once, the cull replaces the
    /// garrison and settles in one snapshot-guarded transaction, a stale snapshot conflicts
    /// without side effects, and resolve can reschedule or finish a claimed check.
    #[tokio::test]
    async fn starvation_check_lifecycle() {
        let _ = dotenvy::dotenv();
        let Ok(url) = std::env::var("DATABASE_URL") else {
            eprintln!("skipping starvation test: DATABASE_URL not set");
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

        let uname = format!("starve_{}", Uuid::new_v4().simple());
        let user = repo
            .create_account(
                NewUser {
                    username: uname.clone(),
                    email: format!("{uname}@example.com"),
                    password_hash: "h".to_owned(),
                    email_confirmed: true,
                    tribe: Tribe::Gauls,
                },
                &crate::starting_village().unwrap(),
            )
            .await
            .expect("create account");
        let village_id = repo.villages_of(user.id).await.unwrap()[0].id;

        // Seed a garrison directly (training delivery is covered by the training test).
        sqlx::query(
            "INSERT INTO village_units (village_id, unit_id, count) VALUES ($1, 'phalanx', 5)",
        )
        .bind(Uuid::from_u128(village_id.0))
        .execute(&pool)
        .await
        .unwrap();

        let now = Timestamp(1_700_000_000_000);
        let starved = ResourceAmounts {
            wood: 750,
            clay: 750,
            iron: 750,
            crop: 0,
        };

        // A due check is claimed exactly once.
        repo.schedule_starvation_check(village_id, Timestamp(now.0 - 1000))
            .await
            .expect("schedule");
        let due = repo.claim_due_starvation(now, 10).await.unwrap();
        assert!(due.contains(&village_id));
        assert!(
            !repo
                .claim_due_starvation(now, 10)
                .await
                .unwrap()
                .contains(&village_id)
        );

        // A stale snapshot conflicts and leaves garrison + resources untouched.
        assert!(matches!(
            repo.apply_starvation(
                village_id,
                starved,
                Timestamp(123),
                now,
                &vec![(UnitId("phalanx".into()), 2)],
            )
            .await,
            Err(RepoError::Conflict)
        ));
        assert_eq!(
            repo.garrison(village_id).await.unwrap(),
            vec![(UnitId("phalanx".into()), 5)]
        );

        // The cull applies once: settle + survivors + check done, in one transaction.
        let snap = snapshot(&repo, village_id).await;
        repo.apply_starvation(
            village_id,
            starved,
            snap,
            now,
            &vec![(UnitId("phalanx".into()), 2)],
        )
        .await
        .expect("apply starvation");
        assert_eq!(
            repo.garrison(village_id).await.unwrap(),
            vec![(UnitId("phalanx".into()), 2)]
        );
        assert_eq!(
            repo.stored_resources(village_id).await.unwrap().unwrap().0,
            starved
        );

        // A claimed check can be rescheduled (recovered village) or finished.
        repo.schedule_starvation_check(village_id, Timestamp(now.0 - 1000))
            .await
            .unwrap();
        let due = repo.claim_due_starvation(now, 10).await.unwrap();
        assert!(due.contains(&village_id));
        repo.resolve_starvation_check(village_id, Some(Timestamp(now.0 + 3_600_000)))
            .await
            .unwrap();
        assert!(
            !repo
                .claim_due_starvation(now, 10)
                .await
                .unwrap()
                .contains(&village_id)
        );
        let due = repo
            .claim_due_starvation(Timestamp(now.0 + 3_600_001), 10)
            .await
            .unwrap();
        assert!(due.contains(&village_id));
        repo.resolve_starvation_check(village_id, None)
            .await
            .unwrap();

        // Cancel removes a pending check entirely (AC8 path).
        repo.schedule_starvation_check(village_id, Timestamp(now.0 - 1000))
            .await
            .unwrap();
        repo.cancel_starvation_check(village_id).await.unwrap();
        assert!(
            !repo
                .claim_due_starvation(now, 10)
                .await
                .unwrap()
                .contains(&village_id)
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
                    tribe: Tribe::Gauls,
                },
                &crate::starting_village().unwrap(),
            )
            .await
            .expect("create account");
        let village_id = repo.villages_of(user.id).await.unwrap()[0].id;

        let now = Timestamp(2_000_000_000_000);
        let snap = snapshot(&repo, village_id).await;
        repo.start_build(
            village_id,
            ResourceAmounts {
                wood: 700,
                clay: 700,
                iron: 700,
                crop: 700,
            },
            snap,
            Timestamp(now.0 - 10_000),
            NewBuildOrder {
                target: BuildTarget::Field { slot: 1 },
                target_level: 1,
                complete_at: Timestamp(now.0 - 1000), // already due at `now`
                lane: QueueLane::All,
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
                    tribe: Tribe::Gauls,
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
        let snap = snapshot(&repo, village_id).await;
        repo.start_build(
            village_id,
            ResourceAmounts {
                wood: 700,
                clay: 700,
                iron: 700,
                crop: 700,
            },
            snap,
            Timestamp(now.0 - 10_000),
            NewBuildOrder {
                target: BuildTarget::Building {
                    slot: 2,
                    kind: BuildingKind::Warehouse,
                },
                target_level: 1,
                complete_at: Timestamp(now.0 - 1000), // already due at `now`
                lane: QueueLane::All,
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
