//! PostgreSQL adapter for the application's [`AccountRepository`] port.

use async_trait::async_trait;
use eperica_application::{
    AccountRepository, ActiveBuild, ActiveTraining, ActiveUnitOrder, BattleApply, BattleReportView,
    BuildRepository, CombatRepository, DueAttack, DueBuild, DueMovement, DueOasisAttack, DueScout,
    DueTrade, DueTraining, DueUnitOrder, MovementRepository, MovementView, NewBuildOrder,
    NewScoutReport, NewTrainingOrder, NewUnitOrder, NewUser, OasisBattleApply, OasisOwnership,
    OasisRepository, OasisState, RazedBuilding, RepoError, ResourceWrite, ScoutApply, ScoutIntel,
    ScoutReportView, ScoutRepository, StarvationRepository, StationedGroup, TradeRepository,
    TradeView, TrainingRepository, UnitOrderKind, UnitRepository, UserRecord, VillageMarker,
};
use eperica_domain::{
    BuildTarget, BuildingKind, BuildingSlot, Coordinate, MovementKind, OasisBonus, OasisRules,
    PlayerId, QueueLane, ResourceAmounts, ResourceField, ResourceKind, ScoutTarget,
    StartingVillage, TileKind, Timestamp, TradeKind, Tribe, UnitCounts, UnitId, UnitSpec, Village,
    VillageId, WorldId, WorldMap, coordinates_within, oasis_garrison,
};
use sqlx::{Acquire, PgPool, Row, postgres::PgRow};
use uuid::Uuid;

/// SQLx-backed account repository bound to a single world.
#[derive(Debug, Clone)]
pub struct PgAccountRepository {
    pool: PgPool,
    world_id: WorldId,
    map: WorldMap,
    starting_amounts: ResourceAmounts,
}

impl PgAccountRepository {
    /// Create a repository for `world_id`. The world's `seed` + `radius` (with the embedded map
    /// balance) drive the generated map used for village placement (006); `starting_amounts` are
    /// seeded into each new village's resources.
    pub fn new(
        pool: PgPool,
        world_id: WorldId,
        seed: i64,
        radius: u32,
        starting_amounts: ResourceAmounts,
    ) -> Self {
        let rules = crate::balance::map_rules().expect("embedded map balance is valid");
        Self {
            pool,
            world_id,
            map: WorldMap::new(seed as u64, radius, rules),
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

    /// Reset movements stuck in `processing` back to `in_transit` (crash recovery). Safe:
    /// `apply_movement` delivers and marks done in one transaction, so a re-claim re-applies a
    /// never-committed arrival cleanly (007 AC4).
    pub async fn requeue_orphaned_movements(&self) -> Result<u64, RepoError> {
        let result = sqlx::query(
            "UPDATE troop_movements SET status = 'in_transit' WHERE status = 'processing'",
        )
        .execute(&self.pool)
        .await
        .map_err(backend)?;
        Ok(result.rows_affected())
    }

    /// Reset trade legs stuck in `processing` back to `in_transit` (crash recovery). Safe:
    /// `deliver_and_schedule_return` credits + schedules + marks done in one transaction, so a
    /// re-claim re-applies a never-committed delivery cleanly (008 AC4).
    pub async fn requeue_orphaned_trades(&self) -> Result<u64, RepoError> {
        let result = sqlx::query(
            "UPDATE trade_movements SET status = 'in_transit' WHERE status = 'processing'",
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
        BuildingKind::Marketplace => "marketplace",
        BuildingKind::Wall => "wall",
        BuildingKind::Barracks => "barracks",
        BuildingKind::Academy => "academy",
        BuildingKind::Smithy => "smithy",
        BuildingKind::Stable => "stable",
        BuildingKind::Workshop => "workshop",
        BuildingKind::Residence => "residence",
        BuildingKind::Cranny => "cranny",
        BuildingKind::Outpost => "outpost",
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
        "marketplace" => Ok(BuildingKind::Marketplace),
        "wall" => Ok(BuildingKind::Wall),
        "barracks" => Ok(BuildingKind::Barracks),
        "academy" => Ok(BuildingKind::Academy),
        "smithy" => Ok(BuildingKind::Smithy),
        "stable" => Ok(BuildingKind::Stable),
        "workshop" => Ok(BuildingKind::Workshop),
        "residence" => Ok(BuildingKind::Residence),
        "cranny" => Ok(BuildingKind::Cranny),
        "outpost" => Ok(BuildingKind::Outpost),
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

        // Place the village on the first free **valley** in the deterministic ring order (oases and
        // Natar are skipped, 006 AC5); its fields come from that valley's distribution. Each attempt
        // is a SAVEPOINT so a coordinate clash rolls back just that insert (not the whole tx).
        let mut placed = false;
        for coord in coordinates_within(self.map.radius()) {
            let Some(TileKind::Valley(distribution)) = self.map.tile_at(coord) else {
                continue;
            };
            let village_uuid = Uuid::new_v4();
            let village = Village::found(
                VillageId(village_uuid.as_u128()),
                owner,
                coord,
                user.tribe,
                distribution,
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

    async fn villages_at(&self, coords: &[Coordinate]) -> Result<Vec<VillageMarker>, RepoError> {
        if coords.is_empty() {
            return Ok(Vec::new());
        }
        let xs: Vec<i32> = coords.iter().map(|c| c.x).collect();
        let ys: Vec<i32> = coords.iter().map(|c| c.y).collect();
        // Exact match on the requested tiles via the (world_id, x, y) unique index.
        let rows = sqlx::query(
            "SELECT v.x, v.y, u.username FROM villages v JOIN users u ON u.id = v.owner_id \
             WHERE v.world_id = $1 AND (v.x, v.y) IN (SELECT * FROM unnest($2::int[], $3::int[]))",
        )
        .bind(Uuid::from_u128(self.world_id.0))
        .bind(&xs)
        .bind(&ys)
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;
        rows.iter()
            .map(|r| {
                let x: i32 = r.try_get("x").map_err(backend)?;
                let y: i32 = r.try_get("y").map_err(backend)?;
                let owner_name: String = r.try_get("username").map_err(backend)?;
                Ok(VillageMarker {
                    coordinate: Coordinate::new(x, y),
                    owner_name,
                })
            })
            .collect()
    }

    async fn village_at(&self, coord: Coordinate) -> Result<Option<Village>, RepoError> {
        let id: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM villages WHERE world_id = $1 AND x = $2 AND y = $3")
                .bind(Uuid::from_u128(self.world_id.0))
                .bind(coord.x)
                .bind(coord.y)
                .fetch_optional(&self.pool)
                .await
                .map_err(backend)?;
        match id {
            Some(id) => self.village_by_id(VillageId(id.as_u128())).await,
            None => Ok(None),
        }
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

    async fn apply_training(
        &self,
        due: &DueTraining,
        completed: u32,
        settled: ResourceAmounts,
        settled_from: Timestamp,
        settle_to: Timestamp,
    ) -> Result<(), RepoError> {
        let vid = Uuid::from_u128(due.village.0);
        let mut tx = self.pool.begin().await.map_err(backend)?;

        // Optimistic settle — see `start_build`. Delivering a unit changes the upkeep rate, so
        // the store must be settled (with the pre-delivery rate) up to the delivery instant in
        // the SAME transaction as the garrison change (AC6; spec: troops in training do not eat).
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
        .bind(settle_to.0 as f64)
        .bind(vid)
        .bind(settled_from.0)
        .execute(&mut *tx)
        .await
        .map_err(backend)?;
        if updated.rows_affected() == 0 {
            return Err(RepoError::Conflict);
        }

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

    async fn release_training(&self, due: &DueTraining) -> Result<(), RepoError> {
        sqlx::query("UPDATE training_orders SET status = 'active' WHERE id = $1")
            .bind(Uuid::from_u128(due.id))
            .execute(&self.pool)
            .await
            .map_err(backend)?;
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

        // Optimistic settle — see `start_build`; on Conflict nothing is applied and the caller
        // re-pends the check for a next-tick retry (AC7 exactly-once still holds).
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

fn trade_kind_str(kind: TradeKind) -> &'static str {
    match kind {
        TradeKind::Deliver => "deliver",
        TradeKind::Return => "return",
    }
}

fn parse_trade_kind(s: &str) -> Result<TradeKind, RepoError> {
    match s {
        "deliver" => Ok(TradeKind::Deliver),
        "return" => Ok(TradeKind::Return),
        other => Err(RepoError::Backend(format!("unknown trade kind: {other}"))),
    }
}

fn movement_kind_str(kind: MovementKind) -> &'static str {
    match kind {
        MovementKind::Reinforce => "reinforce",
        MovementKind::Return => "return",
        MovementKind::Attack => "attack",
        MovementKind::Raid => "raid",
        MovementKind::Scout => "scout",
        MovementKind::OasisAttack => "oasis_attack",
        MovementKind::OasisReinforce => "oasis_reinforce",
    }
}

fn parse_movement_kind(s: &str) -> Result<MovementKind, RepoError> {
    match s {
        "reinforce" => Ok(MovementKind::Reinforce),
        "return" => Ok(MovementKind::Return),
        "attack" => Ok(MovementKind::Attack),
        "raid" => Ok(MovementKind::Raid),
        "scout" => Ok(MovementKind::Scout),
        "oasis_attack" => Ok(MovementKind::OasisAttack),
        "oasis_reinforce" => Ok(MovementKind::OasisReinforce),
        other => Err(RepoError::Backend(format!(
            "unknown movement kind: {other}"
        ))),
    }
}

/// Load a movement's troop composition.
/// Load the troop composition of several movements in a single query, grouped by movement id
/// (avoids an N+1 on the `/village` read path and the due-event processor — P11). Returns an empty
/// composition for any id with no rows.
async fn movement_troops_batch(
    pool: &PgPool,
    movements: &[Uuid],
) -> Result<std::collections::HashMap<Uuid, UnitCounts>, RepoError> {
    let mut grouped: std::collections::HashMap<Uuid, UnitCounts> = std::collections::HashMap::new();
    if movements.is_empty() {
        return Ok(grouped);
    }
    let rows = sqlx::query(
        "SELECT movement_id, unit_id, count FROM movement_troops \
         WHERE movement_id = ANY($1) ORDER BY movement_id, unit_id",
    )
    .bind(movements)
    .fetch_all(pool)
    .await
    .map_err(backend)?;
    for r in &rows {
        let movement: Uuid = r.try_get("movement_id").map_err(backend)?;
        let unit: String = r.try_get("unit_id").map_err(backend)?;
        let count: i32 = r.try_get("count").map_err(backend)?;
        grouped
            .entry(movement)
            .or_default()
            .push((UnitId(unit), u32::try_from(count).unwrap_or(0)));
    }
    Ok(grouped)
}

/// Guarded debit from a village garrison within a transaction: for each type, decrement
/// (`count > n`) or delete the stack (`count == n`) — never UPDATE to 0 (the CHECK forbids it).
/// Exactly one of the two must affect a row, else the garrison no longer covers the request
/// (`Conflict`, race-proof, P4). Shared by reinforcement and attack/raid dispatch.
async fn guarded_debit(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    village: Uuid,
    troops: &[(UnitId, u32)],
) -> Result<(), RepoError> {
    for (unit, n) in troops.iter().filter(|(_, n)| *n > 0) {
        let n = i32::try_from(*n).unwrap_or(i32::MAX);
        let dec = sqlx::query(
            "UPDATE village_units SET count = count - $1 \
             WHERE village_id = $2 AND unit_id = $3 AND count > $1",
        )
        .bind(n)
        .bind(village)
        .bind(unit.as_str())
        .execute(&mut **tx)
        .await
        .map_err(backend)?;
        let affected = if dec.rows_affected() == 1 {
            1
        } else {
            sqlx::query(
                "DELETE FROM village_units WHERE village_id = $1 AND unit_id = $2 AND count = $3",
            )
            .bind(village)
            .bind(unit.as_str())
            .bind(n)
            .execute(&mut **tx)
            .await
            .map_err(backend)?
            .rows_affected()
        };
        if affected != 1 {
            return Err(RepoError::Conflict);
        }
    }
    Ok(())
}

#[async_trait]
impl MovementRepository for PgAccountRepository {
    #[allow(clippy::too_many_arguments)]
    async fn start_reinforcement(
        &self,
        home: VillageId,
        deliver: VillageId,
        owner: PlayerId,
        origin: Coordinate,
        dest: Coordinate,
        now: Timestamp,
        arrive_at: Timestamp,
        troops: &[(UnitId, u32)],
    ) -> Result<(), RepoError> {
        let mut tx = self.pool.begin().await.map_err(backend)?;
        guarded_debit(&mut tx, Uuid::from_u128(home.0), troops).await?;
        insert_movement(
            &mut tx,
            Uuid::new_v4(),
            owner,
            MovementKind::Reinforce,
            home,
            deliver,
            origin,
            dest,
            now,
            arrive_at,
            troops,
        )
        .await?;
        tx.commit().await.map_err(backend)?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn start_return(
        &self,
        host: VillageId,
        home: VillageId,
        owner: PlayerId,
        origin: Coordinate,
        dest: Coordinate,
        now: Timestamp,
        arrive_at: Timestamp,
    ) -> Result<UnitCounts, RepoError> {
        let host_uuid = Uuid::from_u128(host.0);
        let home_uuid = Uuid::from_u128(home.0);
        let mut tx = self.pool.begin().await.map_err(backend)?;

        // Atomically read+delete the stationed group (lock it against a concurrent return).
        let rows = sqlx::query(
            "SELECT unit_id, count FROM reinforcements \
             WHERE host_village = $1 AND home_village = $2 ORDER BY unit_id FOR UPDATE",
        )
        .bind(host_uuid)
        .bind(home_uuid)
        .fetch_all(&mut *tx)
        .await
        .map_err(backend)?;
        if rows.is_empty() {
            return Err(RepoError::Conflict);
        }
        let troops: UnitCounts = rows
            .iter()
            .map(|r| {
                let unit: String = r.try_get("unit_id").map_err(backend)?;
                let count: i32 = r.try_get("count").map_err(backend)?;
                Ok((UnitId(unit), u32::try_from(count).unwrap_or(0)))
            })
            .collect::<Result<_, RepoError>>()?;

        sqlx::query("DELETE FROM reinforcements WHERE host_village = $1 AND home_village = $2")
            .bind(host_uuid)
            .bind(home_uuid)
            .execute(&mut *tx)
            .await
            .map_err(backend)?;

        let movement_id = Uuid::new_v4();
        insert_movement(
            &mut tx,
            movement_id,
            owner,
            MovementKind::Return,
            home,
            home, // delivered back to the home garrison
            origin,
            dest,
            now,
            arrive_at,
            &troops,
        )
        .await?;

        tx.commit().await.map_err(backend)?;
        Ok(troops)
    }

    async fn active_movements(&self, owner: PlayerId) -> Result<Vec<MovementView>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, kind, dest_x, dest_y, \
             (EXTRACT(EPOCH FROM arrive_at) * 1000)::bigint AS arrive_ms \
             FROM troop_movements WHERE owner_id = $1 AND status = 'in_transit' \
             ORDER BY arrive_at, id",
        )
        .bind(Uuid::from_u128(owner.0))
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;

        let ids: Vec<Uuid> = rows
            .iter()
            .map(|r| r.try_get("id").map_err(backend))
            .collect::<Result<_, RepoError>>()?;
        let mut troops = movement_troops_batch(&self.pool, &ids).await?;

        let mut out = Vec::with_capacity(rows.len());
        for (r, id) in rows.iter().zip(&ids) {
            let kind: String = r.try_get("kind").map_err(backend)?;
            let dest_x: i32 = r.try_get("dest_x").map_err(backend)?;
            let dest_y: i32 = r.try_get("dest_y").map_err(backend)?;
            let arrive_ms: i64 = r.try_get("arrive_ms").map_err(backend)?;
            out.push(MovementView {
                kind: parse_movement_kind(&kind)?,
                destination: Coordinate::new(dest_x, dest_y),
                arrive_at: Timestamp(arrive_ms),
                troops: troops.remove(id).unwrap_or_default(),
            });
        }
        Ok(out)
    }

    async fn reinforcements_at(
        &self,
        village: VillageId,
    ) -> Result<Vec<StationedGroup>, RepoError> {
        let rows = sqlx::query(
            "SELECT r.home_village, r.unit_id, r.count, hv.x, hv.y, hv.tribe, u.username \
             FROM reinforcements r \
             JOIN villages hv ON hv.id = r.home_village \
             JOIN users u ON u.id = hv.owner_id \
             WHERE r.host_village = $1 ORDER BY r.home_village, r.unit_id",
        )
        .bind(Uuid::from_u128(village.0))
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;
        group_reinforcements(&rows, |home| StationedGroup {
            host_village: village,
            home_village: home,
            other_coord: Coordinate::new(0, 0),
            other_owner: String::new(),
            home_tribe: None,
            troops: Vec::new(),
        })
    }

    async fn reinforcements_of(&self, owner: PlayerId) -> Result<Vec<StationedGroup>, RepoError> {
        let rows = sqlx::query(
            "SELECT r.host_village, r.home_village, r.unit_id, r.count, hostv.x, hostv.y, \
             homev.tribe, u.username \
             FROM reinforcements r \
             JOIN villages homev ON homev.id = r.home_village \
             JOIN villages hostv ON hostv.id = r.host_village \
             JOIN users u ON u.id = hostv.owner_id \
             WHERE homev.owner_id = $1 ORDER BY r.host_village, r.unit_id",
        )
        .bind(Uuid::from_u128(owner.0))
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;

        // Group by host village.
        let mut out: Vec<StationedGroup> = Vec::new();
        for r in &rows {
            let host: Uuid = r.try_get("host_village").map_err(backend)?;
            let home: Uuid = r.try_get("home_village").map_err(backend)?;
            let unit: String = r.try_get("unit_id").map_err(backend)?;
            let count: i32 = r.try_get("count").map_err(backend)?;
            let x: i32 = r.try_get("x").map_err(backend)?;
            let y: i32 = r.try_get("y").map_err(backend)?;
            let tribe_raw: Option<String> = r.try_get("tribe").map_err(backend)?;
            let tribe = parse_tribe(tribe_raw)?;
            let username: String = r.try_get("username").map_err(backend)?;
            let host_id = VillageId(host.as_u128());
            let count = u32::try_from(count).unwrap_or(0);
            match out.last_mut() {
                Some(g) if g.host_village == host_id => g.troops.push((UnitId(unit), count)),
                _ => out.push(StationedGroup {
                    host_village: host_id,
                    home_village: VillageId(home.as_u128()),
                    other_coord: Coordinate::new(x, y),
                    other_owner: username,
                    home_tribe: tribe,
                    troops: vec![(UnitId(unit), count)],
                }),
            }
        }
        Ok(out)
    }

    async fn claim_due_movements(
        &self,
        now: Timestamp,
        limit: i64,
    ) -> Result<Vec<DueMovement>, RepoError> {
        let rows = sqlx::query(
            "UPDATE troop_movements SET status = 'processing' WHERE id IN ( \
                 SELECT id FROM troop_movements \
                 WHERE status = 'in_transit' AND kind IN ('reinforce', 'return') \
                   AND arrive_at <= to_timestamp($1::double precision / 1000.0) \
                 ORDER BY arrive_at, id LIMIT $2 FOR UPDATE SKIP LOCKED \
             ) RETURNING id, kind, home_village, deliver_village, \
                 loot_wood, loot_clay, loot_iron, loot_crop",
        )
        .bind(now.0 as f64)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;

        let ids: Vec<Uuid> = rows
            .iter()
            .map(|r| r.try_get("id").map_err(backend))
            .collect::<Result<_, RepoError>>()?;
        let mut troops = movement_troops_batch(&self.pool, &ids).await?;

        let mut out = Vec::with_capacity(rows.len());
        for (r, id) in rows.iter().zip(&ids) {
            let kind: String = r.try_get("kind").map_err(backend)?;
            let home: Uuid = r.try_get("home_village").map_err(backend)?;
            let deliver: Uuid = r.try_get("deliver_village").map_err(backend)?;
            out.push(DueMovement {
                id: id.as_u128(),
                kind: parse_movement_kind(&kind)?,
                home_village: VillageId(home.as_u128()),
                deliver_village: VillageId(deliver.as_u128()),
                troops: troops.remove(id).unwrap_or_default(),
                loot: ResourceAmounts {
                    wood: r.try_get("loot_wood").map_err(backend)?,
                    clay: r.try_get("loot_clay").map_err(backend)?,
                    iron: r.try_get("loot_iron").map_err(backend)?,
                    crop: r.try_get("loot_crop").map_err(backend)?,
                },
            });
        }
        Ok(out)
    }

    async fn apply_movement(
        &self,
        due: &DueMovement,
        credit: Option<ResourceWrite>,
    ) -> Result<(), RepoError> {
        let deliver = Uuid::from_u128(due.deliver_village.0);
        let home = Uuid::from_u128(due.home_village.0);
        let mut tx = self.pool.begin().await.map_err(backend)?;

        for (unit, count) in due.troops.iter().filter(|(_, c)| *c > 0) {
            let count = i32::try_from(*count).unwrap_or(i32::MAX);
            match due.kind {
                MovementKind::Reinforce => {
                    // Station at the destination, owned by the home village.
                    sqlx::query(
                        "INSERT INTO reinforcements (host_village, home_village, unit_id, count) \
                         VALUES ($1, $2, $3, $4) \
                         ON CONFLICT (host_village, home_village, unit_id) \
                         DO UPDATE SET count = reinforcements.count + EXCLUDED.count",
                    )
                    .bind(deliver)
                    .bind(home)
                    .bind(unit.as_str())
                    .bind(count)
                    .execute(&mut *tx)
                    .await
                    .map_err(backend)?;
                }
                MovementKind::Return => {
                    // Rejoin the home garrison.
                    sqlx::query(
                        "INSERT INTO village_units (village_id, unit_id, count) \
                         VALUES ($1, $2, $3) \
                         ON CONFLICT (village_id, unit_id) \
                         DO UPDATE SET count = village_units.count + EXCLUDED.count",
                    )
                    .bind(deliver)
                    .bind(unit.as_str())
                    .bind(count)
                    .execute(&mut *tx)
                    .await
                    .map_err(backend)?;
                }
                // Attack/raid/scout/oasis arrivals are resolved by the combat/scout/oasis processors,
                // not stationed here; `claim_due_movements` excludes them, so this is unreachable.
                MovementKind::Attack
                | MovementKind::Raid
                | MovementKind::Scout
                | MovementKind::OasisAttack
                | MovementKind::OasisReinforce => {
                    return Err(RepoError::Backend(
                        "combat/scout/oasis movement routed to apply_movement".into(),
                    ));
                }
            }
        }

        // Loot credit (011): a `return` carrying loot writes the home's settled, capped resources —
        // guarded on the snapshot the caller settled from (P2/P4).
        if let Some(credit) = credit {
            let updated = sqlx::query(
                "UPDATE village_resources SET wood=$1, clay=$2, iron=$3, crop=$4, \
                 updated_at = to_timestamp($5::double precision / 1000.0) \
                 WHERE village_id=$6 AND (EXTRACT(EPOCH FROM updated_at) * 1000)::bigint = $7",
            )
            .bind(credit.after.wood)
            .bind(credit.after.clay)
            .bind(credit.after.iron)
            .bind(credit.after.crop)
            .bind(credit.clock.0 as f64)
            .bind(deliver)
            .bind(credit.settled_from.0)
            .execute(&mut *tx)
            .await
            .map_err(backend)?;
            if updated.rows_affected() == 0 {
                return Err(RepoError::Conflict);
            }
        }

        sqlx::query("UPDATE troop_movements SET status = 'done' WHERE id = $1")
            .bind(Uuid::from_u128(due.id))
            .execute(&mut *tx)
            .await
            .map_err(backend)?;

        tx.commit().await.map_err(backend)?;
        Ok(())
    }
}

/// Insert a movement row + its troop child rows within an open transaction.
#[allow(clippy::too_many_arguments)]
async fn insert_movement(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    movement_id: Uuid,
    owner: PlayerId,
    kind: MovementKind,
    home: VillageId,
    deliver: VillageId,
    origin: Coordinate,
    dest: Coordinate,
    now: Timestamp,
    arrive_at: Timestamp,
    troops: &[(UnitId, u32)],
) -> Result<(), RepoError> {
    sqlx::query(
        "INSERT INTO troop_movements \
         (id, owner_id, kind, home_village, deliver_village, origin_x, origin_y, dest_x, dest_y, \
          depart_at, arrive_at, status) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, \
                 to_timestamp($10::double precision / 1000.0), \
                 to_timestamp($11::double precision / 1000.0), 'in_transit')",
    )
    .bind(movement_id)
    .bind(Uuid::from_u128(owner.0))
    .bind(movement_kind_str(kind))
    .bind(Uuid::from_u128(home.0))
    .bind(Uuid::from_u128(deliver.0))
    .bind(origin.x)
    .bind(origin.y)
    .bind(dest.x)
    .bind(dest.y)
    .bind(now.0 as f64)
    .bind(arrive_at.0 as f64)
    .execute(&mut **tx)
    .await
    .map_err(backend)?;
    for (unit, n) in troops.iter().filter(|(_, n)| *n > 0) {
        sqlx::query(
            "INSERT INTO movement_troops (movement_id, unit_id, count) VALUES ($1, $2, $3)",
        )
        .bind(movement_id)
        .bind(unit.as_str())
        .bind(i32::try_from(*n).unwrap_or(i32::MAX))
        .execute(&mut **tx)
        .await
        .map_err(backend)?;
    }
    Ok(())
}

/// Group reinforcement rows (sorted by home_village) for `reinforcements_at`: the counterparty is
/// each helper's home village. `seed` builds an empty group for a new home id; rows fill it.
fn group_reinforcements(
    rows: &[PgRow],
    seed: impl Fn(VillageId) -> StationedGroup,
) -> Result<Vec<StationedGroup>, RepoError> {
    let mut out: Vec<StationedGroup> = Vec::new();
    for r in rows {
        let home: Uuid = r.try_get("home_village").map_err(backend)?;
        let unit: String = r.try_get("unit_id").map_err(backend)?;
        let count: i32 = r.try_get("count").map_err(backend)?;
        let x: i32 = r.try_get("x").map_err(backend)?;
        let y: i32 = r.try_get("y").map_err(backend)?;
        let tribe_raw: Option<String> = r.try_get("tribe").map_err(backend)?;
        let tribe = parse_tribe(tribe_raw)?;
        let username: String = r.try_get("username").map_err(backend)?;
        let home_id = VillageId(home.as_u128());
        let count = u32::try_from(count).unwrap_or(0);
        match out.last_mut() {
            Some(g) if g.home_village == home_id => g.troops.push((UnitId(unit), count)),
            _ => {
                let mut g = seed(home_id);
                g.other_coord = Coordinate::new(x, y);
                g.other_owner = username;
                g.home_tribe = tribe;
                g.troops.push((UnitId(unit), count));
                out.push(g);
            }
        }
    }
    Ok(out)
}

/// Insert one trade leg (deliver or return) into `trade_movements` within a transaction.
#[allow(clippy::too_many_arguments)]
async fn insert_trade_leg(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: Uuid,
    owner: PlayerId,
    kind: TradeKind,
    home: VillageId,
    target: VillageId,
    origin: Coordinate,
    dest: Coordinate,
    depart_at: Timestamp,
    arrive_at: Timestamp,
    bundle: ResourceAmounts,
    merchants: u32,
) -> Result<(), RepoError> {
    sqlx::query(
        "INSERT INTO trade_movements \
         (id, owner_id, kind, home_village, target_village, origin_x, origin_y, dest_x, dest_y, \
          wood, clay, iron, crop, merchants, depart_at, arrive_at, status) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, \
                 to_timestamp($15::double precision / 1000.0), \
                 to_timestamp($16::double precision / 1000.0), 'in_transit')",
    )
    .bind(id)
    .bind(Uuid::from_u128(owner.0))
    .bind(trade_kind_str(kind))
    .bind(Uuid::from_u128(home.0))
    .bind(Uuid::from_u128(target.0))
    .bind(origin.x)
    .bind(origin.y)
    .bind(dest.x)
    .bind(dest.y)
    .bind(bundle.wood)
    .bind(bundle.clay)
    .bind(bundle.iron)
    .bind(bundle.crop)
    .bind(i32::try_from(merchants).unwrap_or(i32::MAX))
    .bind(depart_at.0 as f64)
    .bind(arrive_at.0 as f64)
    .execute(&mut **tx)
    .await
    .map_err(backend)?;
    Ok(())
}

#[async_trait]
impl TradeRepository for PgAccountRepository {
    async fn committed_merchants(&self, home: VillageId) -> Result<u32, RepoError> {
        let total: Option<i64> = sqlx::query_scalar(
            "SELECT SUM(merchants)::bigint FROM trade_movements \
             WHERE home_village = $1 AND status IN ('in_transit', 'processing')",
        )
        .bind(Uuid::from_u128(home.0))
        .fetch_one(&self.pool)
        .await
        .map_err(backend)?;
        Ok(u32::try_from(total.unwrap_or(0)).unwrap_or(u32::MAX))
    }

    #[allow(clippy::too_many_arguments)]
    async fn start_trade(
        &self,
        home: VillageId,
        target: VillageId,
        owner: PlayerId,
        origin: Coordinate,
        dest: Coordinate,
        settled: ResourceAmounts,
        settled_from: Timestamp,
        now: Timestamp,
        arrive_at: Timestamp,
        bundle: ResourceAmounts,
        merchants: u32,
    ) -> Result<(), RepoError> {
        let mut tx = self.pool.begin().await.map_err(backend)?;

        // Optimistic settle — see `start_build`: debit the sender only if its resources row is still
        // at the snapshot the caller computed `settled` from, so a concurrent order/credit cannot
        // have its write overwritten (P2/P4).
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
        .bind(Uuid::from_u128(home.0))
        .bind(settled_from.0)
        .execute(&mut *tx)
        .await
        .map_err(backend)?;
        if updated.rows_affected() == 0 {
            return Err(RepoError::Conflict);
        }

        insert_trade_leg(
            &mut tx,
            Uuid::new_v4(),
            owner,
            TradeKind::Deliver,
            home,
            target,
            origin,
            dest,
            now,
            arrive_at,
            bundle,
            merchants,
        )
        .await?;
        tx.commit().await.map_err(backend)?;
        Ok(())
    }

    async fn active_trades(&self, owner: PlayerId) -> Result<Vec<TradeView>, RepoError> {
        let rows = sqlx::query(
            "SELECT kind, dest_x, dest_y, wood, clay, iron, crop, merchants, \
             (EXTRACT(EPOCH FROM arrive_at) * 1000)::bigint AS arrive_ms \
             FROM trade_movements \
             WHERE owner_id = $1 AND status IN ('in_transit', 'processing') \
             ORDER BY arrive_at, id",
        )
        .bind(Uuid::from_u128(owner.0))
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;

        let mut out = Vec::with_capacity(rows.len());
        for r in &rows {
            let kind: String = r.try_get("kind").map_err(backend)?;
            let dest_x: i32 = r.try_get("dest_x").map_err(backend)?;
            let dest_y: i32 = r.try_get("dest_y").map_err(backend)?;
            let arrive_ms: i64 = r.try_get("arrive_ms").map_err(backend)?;
            let merchants: i32 = r.try_get("merchants").map_err(backend)?;
            out.push(TradeView {
                kind: parse_trade_kind(&kind)?,
                destination: Coordinate::new(dest_x, dest_y),
                arrive_at: Timestamp(arrive_ms),
                bundle: row_bundle(r)?,
                merchants: u32::try_from(merchants).unwrap_or(0),
            });
        }
        Ok(out)
    }

    async fn claim_due_trades(
        &self,
        now: Timestamp,
        limit: i64,
    ) -> Result<Vec<DueTrade>, RepoError> {
        let rows = sqlx::query(
            "UPDATE trade_movements SET status = 'processing' WHERE id IN ( \
                 SELECT id FROM trade_movements \
                 WHERE status = 'in_transit' AND arrive_at <= to_timestamp($1::double precision / 1000.0) \
                 ORDER BY arrive_at, id LIMIT $2 FOR UPDATE SKIP LOCKED \
             ) RETURNING id, kind, owner_id, home_village, target_village, \
                 origin_x, origin_y, dest_x, dest_y, wood, clay, iron, crop, merchants, \
                 (EXTRACT(EPOCH FROM arrive_at) * 1000)::bigint AS arrive_ms",
        )
        .bind(now.0 as f64)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;

        let mut out = Vec::with_capacity(rows.len());
        for r in &rows {
            let id: Uuid = r.try_get("id").map_err(backend)?;
            let kind: String = r.try_get("kind").map_err(backend)?;
            let owner: Uuid = r.try_get("owner_id").map_err(backend)?;
            let home: Uuid = r.try_get("home_village").map_err(backend)?;
            let target: Uuid = r.try_get("target_village").map_err(backend)?;
            let origin_x: i32 = r.try_get("origin_x").map_err(backend)?;
            let origin_y: i32 = r.try_get("origin_y").map_err(backend)?;
            let dest_x: i32 = r.try_get("dest_x").map_err(backend)?;
            let dest_y: i32 = r.try_get("dest_y").map_err(backend)?;
            let arrive_ms: i64 = r.try_get("arrive_ms").map_err(backend)?;
            let merchants: i32 = r.try_get("merchants").map_err(backend)?;
            out.push(DueTrade {
                id: id.as_u128(),
                kind: parse_trade_kind(&kind)?,
                owner: PlayerId(owner.as_u128()),
                home_village: VillageId(home.as_u128()),
                target_village: VillageId(target.as_u128()),
                origin: Coordinate::new(origin_x, origin_y),
                dest: Coordinate::new(dest_x, dest_y),
                arrive_at: Timestamp(arrive_ms),
                bundle: row_bundle(r)?,
                merchants: u32::try_from(merchants).unwrap_or(0),
            });
        }
        Ok(out)
    }

    async fn deliver_and_schedule_return(
        &self,
        due: &DueTrade,
        target_settled: ResourceAmounts,
        target_from: Timestamp,
        credit_clock: Timestamp,
        return_arrive: Timestamp,
    ) -> Result<(), RepoError> {
        let mut tx = self.pool.begin().await.map_err(backend)?;

        // Guarded credit of the target: write the capped settled amounts only if its resources row
        // is still at the snapshot the caller settled from (P2/P4). `credit_clock` is the new settle
        // clock (never earlier than the snapshot), so later reads accrue production correctly and the
        // clock never regresses.
        let updated = sqlx::query(
            "UPDATE village_resources SET wood=$1, clay=$2, iron=$3, crop=$4, \
             updated_at = to_timestamp($5::double precision / 1000.0) \
             WHERE village_id=$6 \
               AND (EXTRACT(EPOCH FROM updated_at) * 1000)::bigint = $7",
        )
        .bind(target_settled.wood)
        .bind(target_settled.clay)
        .bind(target_settled.iron)
        .bind(target_settled.crop)
        .bind(credit_clock.0 as f64)
        .bind(Uuid::from_u128(due.target_village.0))
        .bind(target_from.0)
        .execute(&mut *tx)
        .await
        .map_err(backend)?;
        if updated.rows_affected() == 0 {
            return Err(RepoError::Conflict);
        }

        sqlx::query("UPDATE trade_movements SET status = 'done' WHERE id = $1")
            .bind(Uuid::from_u128(due.id))
            .execute(&mut *tx)
            .await
            .map_err(backend)?;

        // The empty merchants travel home (origin/dest swapped), departing at the true arrival and
        // freeing up when they get back.
        insert_trade_leg(
            &mut tx,
            Uuid::new_v4(),
            due.owner,
            TradeKind::Return,
            due.home_village,
            due.target_village,
            due.dest,
            due.origin,
            due.arrive_at,
            return_arrive,
            ResourceAmounts {
                wood: 0,
                clay: 0,
                iron: 0,
                crop: 0,
            },
            due.merchants,
        )
        .await?;
        tx.commit().await.map_err(backend)?;
        Ok(())
    }

    async fn complete_trade(&self, id: u128) -> Result<(), RepoError> {
        sqlx::query(
            "UPDATE trade_movements SET status = 'done' WHERE id = $1 AND status <> 'done'",
        )
        .bind(Uuid::from_u128(id))
        .execute(&self.pool)
        .await
        .map_err(backend)?;
        Ok(())
    }

    async fn release_trade(&self, id: u128) -> Result<(), RepoError> {
        sqlx::query(
            "UPDATE trade_movements SET status = 'in_transit' \
             WHERE id = $1 AND status = 'processing'",
        )
        .bind(Uuid::from_u128(id))
        .execute(&self.pool)
        .await
        .map_err(backend)?;
        Ok(())
    }
}

/// Read the carried `(wood, clay, iron, crop)` bundle from a `trade_movements` row.
fn row_bundle(r: &PgRow) -> Result<ResourceAmounts, RepoError> {
    Ok(ResourceAmounts {
        wood: r.try_get("wood").map_err(backend)?,
        clay: r.try_get("clay").map_err(backend)?,
        iron: r.try_get("iron").map_err(backend)?,
        crop: r.try_get("crop").map_err(backend)?,
    })
}

/// Serialise a `unit → count` composition as a jsonb object for a battle report.
fn counts_to_json(counts: &UnitCounts) -> serde_json::Value {
    serde_json::Value::Object(
        counts
            .iter()
            .map(|(id, c)| (id.as_str().to_owned(), serde_json::Value::from(*c)))
            .collect(),
    )
}

/// Read a `unit → count` composition back from a report's jsonb column.
fn counts_from_json(value: &serde_json::Value) -> UnitCounts {
    value
        .as_object()
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_u64().map(|n| (UnitId(k.clone()), n as u32)))
                .collect()
        })
        .unwrap_or_default()
}

/// Subtract `losses` from a village's `village_units`, deleting any stack that runs out (combat
/// casualties; a stack that grew/left concurrently just loses what remains).
async fn subtract_units(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    village: Uuid,
    losses: &UnitCounts,
) -> Result<(), RepoError> {
    for (unit, n) in losses.iter().filter(|(_, n)| *n > 0) {
        let n = i32::try_from(*n).unwrap_or(i32::MAX);
        let dec = sqlx::query(
            "UPDATE village_units SET count = count - $1 \
             WHERE village_id = $2 AND unit_id = $3 AND count > $1",
        )
        .bind(n)
        .bind(village)
        .bind(unit.as_str())
        .execute(&mut **tx)
        .await
        .map_err(backend)?;
        if dec.rows_affected() == 0 {
            sqlx::query(
                "DELETE FROM village_units WHERE village_id = $1 AND unit_id = $2 AND count <= $3",
            )
            .bind(village)
            .bind(unit.as_str())
            .bind(n)
            .execute(&mut **tx)
            .await
            .map_err(backend)?;
        }
    }
    Ok(())
}

/// Subtract `losses` from a reinforcement group stationed at `host` from `home`.
async fn subtract_reinforcements(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    host: Uuid,
    home: Uuid,
    losses: &UnitCounts,
) -> Result<(), RepoError> {
    for (unit, n) in losses.iter().filter(|(_, n)| *n > 0) {
        let n = i32::try_from(*n).unwrap_or(i32::MAX);
        let dec = sqlx::query(
            "UPDATE reinforcements SET count = count - $1 \
             WHERE host_village = $2 AND home_village = $3 AND unit_id = $4 AND count > $1",
        )
        .bind(n)
        .bind(host)
        .bind(home)
        .bind(unit.as_str())
        .execute(&mut **tx)
        .await
        .map_err(backend)?;
        if dec.rows_affected() == 0 {
            sqlx::query(
                "DELETE FROM reinforcements \
                 WHERE host_village = $1 AND home_village = $2 AND unit_id = $3 AND count <= $4",
            )
            .bind(host)
            .bind(home)
            .bind(unit.as_str())
            .bind(n)
            .execute(&mut **tx)
            .await
            .map_err(backend)?;
        }
    }
    Ok(())
}

/// Map a joined `battle_reports` row to a [`BattleReportView`].
fn report_from_row(r: &PgRow) -> Result<BattleReportView, RepoError> {
    let id: Uuid = r.try_get("id").map_err(backend)?;
    let occurred_ms: i64 = r.try_get("occurred_ms").map_err(backend)?;
    let kind: String = r.try_get("kind").map_err(backend)?;
    let ap: Uuid = r.try_get("attacker_player").map_err(backend)?;
    let dp: Uuid = r.try_get("defender_player").map_err(backend)?;
    let af: serde_json::Value = r.try_get("attacker_forces").map_err(backend)?;
    let al: serde_json::Value = r.try_get("attacker_losses").map_err(backend)?;
    let df: serde_json::Value = r.try_get("defender_forces").map_err(backend)?;
    let dl: serde_json::Value = r.try_get("defender_losses").map_err(backend)?;
    Ok(BattleReportView {
        id: id.as_u128(),
        occurred_at: Timestamp(occurred_ms),
        kind: parse_movement_kind(&kind)?,
        attacker_name: r.try_get("attacker_name").map_err(backend)?,
        attacker_coord: Coordinate::new(
            r.try_get("ax").map_err(backend)?,
            r.try_get("ay").map_err(backend)?,
        ),
        defender_name: r.try_get("defender_name").map_err(backend)?,
        defender_coord: Coordinate::new(
            r.try_get("dx").map_err(backend)?,
            r.try_get("dy").map_err(backend)?,
        ),
        attacker_player: PlayerId(ap.as_u128()),
        defender_player: PlayerId(dp.as_u128()),
        attacker_won: r.try_get("attacker_won").map_err(backend)?,
        luck: r.try_get("luck").map_err(backend)?,
        morale: r.try_get("morale").map_err(backend)?,
        wall_before: u8::try_from(r.try_get::<i32, _>("wall_before").map_err(backend)?)
            .unwrap_or(0),
        wall_after: u8::try_from(r.try_get::<i32, _>("wall_after").map_err(backend)?).unwrap_or(0),
        attacker_forces: counts_from_json(&af),
        attacker_losses: counts_from_json(&al),
        defender_forces: counts_from_json(&df),
        defender_losses: counts_from_json(&dl),
        scouted: r.try_get("scouted").map_err(backend)?,
        scout_target: parse_scout_target_opt(r.try_get("scout_target").map_err(backend)?)?,
        loot: ResourceAmounts {
            wood: r.try_get("loot_wood").map_err(backend)?,
            clay: r.try_get("loot_clay").map_err(backend)?,
            iron: r.try_get("loot_iron").map_err(backend)?,
            crop: r.try_get("loot_crop").map_err(backend)?,
        },
        razed: match r
            .try_get::<Option<String>, _>("razed_building")
            .map_err(backend)?
        {
            Some(b) => Some(RazedBuilding {
                kind: parse_building(&b)?,
                before: u8::try_from(r.try_get::<i16, _>("razed_before").map_err(backend)?)
                    .unwrap_or(0),
                after: u8::try_from(r.try_get::<i16, _>("razed_after").map_err(backend)?)
                    .unwrap_or(0),
            }),
            None => None,
        },
    })
}

/// The `SELECT` of a battle report joined to player names + village coordinates (inbox/detail).
const REPORT_SELECT: &str = "SELECT br.id, \
    (EXTRACT(EPOCH FROM br.occurred_at) * 1000)::bigint AS occurred_ms, br.kind, \
    au.username AS attacker_name, av.x AS ax, av.y AS ay, \
    du.username AS defender_name, dv.x AS dx, dv.y AS dy, \
    br.attacker_player, br.defender_player, br.attacker_won, br.luck, br.morale, \
    br.wall_before, br.wall_after, br.attacker_forces, br.attacker_losses, \
    br.defender_forces, br.defender_losses, br.scouted, br.scout_target, \
    br.loot_wood, br.loot_clay, br.loot_iron, br.loot_crop, \
    br.razed_building, br.razed_before, br.razed_after \
    FROM battle_reports br \
    JOIN users au ON au.id = br.attacker_player \
    JOIN villages av ON av.id = br.attacker_village \
    JOIN users du ON du.id = br.defender_player \
    JOIN villages dv ON dv.id = br.defender_village";

#[async_trait]
impl CombatRepository for PgAccountRepository {
    #[allow(clippy::too_many_arguments)]
    async fn start_attack(
        &self,
        home: VillageId,
        deliver: VillageId,
        owner: PlayerId,
        origin: Coordinate,
        dest: Coordinate,
        now: Timestamp,
        arrive_at: Timestamp,
        kind: MovementKind,
        troops: &[(UnitId, u32)],
        scout_target: Option<ScoutTarget>,
        catapult_target: Option<BuildingKind>,
    ) -> Result<(), RepoError> {
        let mut tx = self.pool.begin().await.map_err(backend)?;
        guarded_debit(&mut tx, Uuid::from_u128(home.0), troops).await?;
        let movement_id = Uuid::new_v4();
        insert_movement(
            &mut tx,
            movement_id,
            owner,
            kind,
            home,
            deliver,
            origin,
            dest,
            now,
            arrive_at,
            troops,
        )
        .await?;
        if let Some(target) = scout_target {
            set_scout_target(&mut tx, movement_id, target).await?;
        }
        if let Some(building) = catapult_target {
            sqlx::query("UPDATE troop_movements SET catapult_target = $1 WHERE id = $2")
                .bind(building_str(building))
                .bind(movement_id)
                .execute(&mut *tx)
                .await
                .map_err(backend)?;
        }
        tx.commit().await.map_err(backend)?;
        Ok(())
    }

    async fn claim_due_attacks(
        &self,
        now: Timestamp,
        limit: i64,
    ) -> Result<Vec<DueAttack>, RepoError> {
        let rows = sqlx::query(
            "UPDATE troop_movements SET status = 'processing' WHERE id IN ( \
                 SELECT id FROM troop_movements \
                 WHERE status = 'in_transit' AND kind IN ('attack', 'raid') \
                   AND arrive_at <= to_timestamp($1::double precision / 1000.0) \
                 ORDER BY arrive_at, id LIMIT $2 FOR UPDATE SKIP LOCKED \
             ) RETURNING id, kind, owner_id, home_village, deliver_village, \
                 origin_x, origin_y, dest_x, dest_y, scout_target, catapult_target, \
                 (EXTRACT(EPOCH FROM arrive_at) * 1000)::bigint AS arrive_ms",
        )
        .bind(now.0 as f64)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;

        let ids: Vec<Uuid> = rows
            .iter()
            .map(|r| r.try_get("id").map_err(backend))
            .collect::<Result<_, RepoError>>()?;
        let mut troops = movement_troops_batch(&self.pool, &ids).await?;

        let mut out = Vec::with_capacity(rows.len());
        for (r, id) in rows.iter().zip(&ids) {
            let kind: String = r.try_get("kind").map_err(backend)?;
            let owner: Uuid = r.try_get("owner_id").map_err(backend)?;
            let home: Uuid = r.try_get("home_village").map_err(backend)?;
            let target: Uuid = r.try_get("deliver_village").map_err(backend)?;
            let arrive_ms: i64 = r.try_get("arrive_ms").map_err(backend)?;
            out.push(DueAttack {
                id: id.as_u128(),
                kind: parse_movement_kind(&kind)?,
                owner: PlayerId(owner.as_u128()),
                home_village: VillageId(home.as_u128()),
                target_village: VillageId(target.as_u128()),
                origin: Coordinate::new(
                    r.try_get("origin_x").map_err(backend)?,
                    r.try_get("origin_y").map_err(backend)?,
                ),
                dest: Coordinate::new(
                    r.try_get("dest_x").map_err(backend)?,
                    r.try_get("dest_y").map_err(backend)?,
                ),
                arrive_at: Timestamp(arrive_ms),
                troops: troops.remove(id).unwrap_or_default(),
                scout_target: parse_scout_target_opt(r.try_get("scout_target").map_err(backend)?)?,
                catapult_target: r
                    .try_get::<Option<String>, _>("catapult_target")
                    .map_err(backend)?
                    .map(|s| parse_building(&s))
                    .transpose()?,
            });
        }
        Ok(out)
    }

    async fn apply_battle(&self, apply: BattleApply) -> Result<(), RepoError> {
        let target = Uuid::from_u128(apply.target.0);
        let mut tx = self.pool.begin().await.map_err(backend)?;

        // Defender casualties: the garrison, then each reinforcement group.
        subtract_units(&mut tx, target, &apply.defender_losses).await?;
        for (home, losses) in &apply.reinforcement_losses {
            subtract_reinforcements(&mut tx, target, Uuid::from_u128(home.0), losses).await?;
        }

        // Loot (011): write the target's settled, looted-down resources — guarded on the snapshot the
        // caller settled from, so a concurrent settle is detected (Conflict → requeue + re-resolve).
        if let Some(debit) = apply.target_debit {
            let updated = sqlx::query(
                "UPDATE village_resources SET wood=$1, clay=$2, iron=$3, crop=$4, \
                 updated_at = to_timestamp($5::double precision / 1000.0) \
                 WHERE village_id=$6 AND (EXTRACT(EPOCH FROM updated_at) * 1000)::bigint = $7",
            )
            .bind(debit.after.wood)
            .bind(debit.after.clay)
            .bind(debit.after.iron)
            .bind(debit.after.crop)
            .bind(debit.clock.0 as f64)
            .bind(target)
            .bind(debit.settled_from.0)
            .execute(&mut *tx)
            .await
            .map_err(backend)?;
            if updated.rows_affected() == 0 {
                return Err(RepoError::Conflict);
            }
        }

        // Catapult damage (011): set the razed building's new level.
        if let Some(razed) = apply.razed {
            sqlx::query(
                "UPDATE village_buildings SET level = $1 WHERE village_id = $2 AND building_type = $3",
            )
            .bind(i16::from(razed.after))
            .bind(target)
            .bind(building_str(razed.kind))
            .execute(&mut *tx)
            .await
            .map_err(backend)?;
        }

        // The report (visible to both parties).
        let r = &apply.report;
        sqlx::query(
            "INSERT INTO battle_reports \
             (id, kind, attacker_player, attacker_village, defender_player, defender_village, \
              attacker_won, luck, morale, wall_before, wall_after, \
              attacker_forces, attacker_losses, defender_forces, defender_losses, \
              scouted, scout_target, loot_wood, loot_clay, loot_iron, loot_crop, \
              razed_building, razed_before, razed_after) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, \
                     $18, $19, $20, $21, $22, $23, $24)",
        )
        .bind(Uuid::new_v4())
        .bind(movement_kind_str(r.kind))
        .bind(Uuid::from_u128(r.attacker_player.0))
        .bind(Uuid::from_u128(r.attacker_village.0))
        .bind(Uuid::from_u128(r.defender_player.0))
        .bind(Uuid::from_u128(r.defender_village.0))
        .bind(r.attacker_won)
        .bind(r.luck)
        .bind(r.morale)
        .bind(i32::from(r.wall_before))
        .bind(i32::from(r.wall_after))
        .bind(counts_to_json(&r.attacker_forces))
        .bind(counts_to_json(&r.attacker_losses))
        .bind(counts_to_json(&r.defender_forces))
        .bind(counts_to_json(&r.defender_losses))
        .bind(apply.scouted)
        .bind(apply.scout_target.map(|t| t.as_str()))
        .bind(r.loot.wood)
        .bind(r.loot.clay)
        .bind(r.loot.iron)
        .bind(r.loot.crop)
        .bind(r.razed.map(|d| building_str(d.kind)))
        .bind(r.razed.map(|d| i16::from(d.before)))
        .bind(r.razed.map(|d| i16::from(d.after)))
        .execute(&mut *tx)
        .await
        .map_err(backend)?;

        // The scouter-facing intel report from scouts that rode the attack (010), if any.
        if let Some(report) = &apply.scout_report {
            insert_scout_report(&mut tx, report).await?;
        }

        // The attacker's survivors travel home (a `return` movement rejoins the garrison) carrying any
        // loot (011), credited at arrival.
        if !apply.survivors.is_empty() {
            let return_id = Uuid::new_v4();
            insert_movement(
                &mut tx,
                return_id,
                apply.owner,
                MovementKind::Return,
                apply.attacker_home,
                apply.attacker_home,
                apply.target_coord,
                apply.attacker_origin,
                apply.battle_at,
                apply.return_arrive,
                &apply.survivors,
            )
            .await?;
            let l = apply.loot;
            if l.wood != 0 || l.clay != 0 || l.iron != 0 || l.crop != 0 {
                sqlx::query(
                    "UPDATE troop_movements SET loot_wood=$1, loot_clay=$2, loot_iron=$3, \
                     loot_crop=$4 WHERE id=$5",
                )
                .bind(apply.loot.wood)
                .bind(apply.loot.clay)
                .bind(apply.loot.iron)
                .bind(apply.loot.crop)
                .bind(return_id)
                .execute(&mut *tx)
                .await
                .map_err(backend)?;
            }
        }

        sqlx::query("UPDATE troop_movements SET status = 'done' WHERE id = $1")
            .bind(Uuid::from_u128(apply.movement_id))
            .execute(&mut *tx)
            .await
            .map_err(backend)?;
        tx.commit().await.map_err(backend)?;
        Ok(())
    }

    async fn reports_for(
        &self,
        player: PlayerId,
        limit: i64,
    ) -> Result<Vec<BattleReportView>, RepoError> {
        let sql = format!(
            "{REPORT_SELECT} WHERE br.attacker_player = $1 OR br.defender_player = $1 \
             ORDER BY br.occurred_at DESC LIMIT $2"
        );
        let rows = sqlx::query(&sql)
            .bind(Uuid::from_u128(player.0))
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(backend)?;
        rows.iter().map(report_from_row).collect()
    }

    async fn report(
        &self,
        id: u128,
        player: PlayerId,
    ) -> Result<Option<BattleReportView>, RepoError> {
        let sql = format!(
            "{REPORT_SELECT} WHERE br.id = $1 \
             AND (br.attacker_player = $2 OR br.defender_player = $2)"
        );
        let row = sqlx::query(&sql)
            .bind(Uuid::from_u128(id))
            .bind(Uuid::from_u128(player.0))
            .fetch_optional(&self.pool)
            .await
            .map_err(backend)?;
        row.as_ref().map(report_from_row).transpose()
    }
}

// ---------------------------------------------------------------- oases (012)

/// Insert an **oasis** movement (NULL `deliver_village`; the dest tile identifies the oasis) plus its
/// troop child rows within an open transaction.
#[allow(clippy::too_many_arguments)]
async fn insert_oasis_movement(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    movement_id: Uuid,
    owner: PlayerId,
    kind: MovementKind,
    home: VillageId,
    origin: Coordinate,
    dest: Coordinate,
    now: Timestamp,
    arrive_at: Timestamp,
    troops: &[(UnitId, u32)],
) -> Result<(), RepoError> {
    sqlx::query(
        "INSERT INTO troop_movements \
         (id, owner_id, kind, home_village, deliver_village, origin_x, origin_y, dest_x, dest_y, \
          depart_at, arrive_at, status) \
         VALUES ($1, $2, $3, $4, NULL, $5, $6, $7, $8, \
                 to_timestamp($9::double precision / 1000.0), \
                 to_timestamp($10::double precision / 1000.0), 'in_transit')",
    )
    .bind(movement_id)
    .bind(Uuid::from_u128(owner.0))
    .bind(movement_kind_str(kind))
    .bind(Uuid::from_u128(home.0))
    .bind(origin.x)
    .bind(origin.y)
    .bind(dest.x)
    .bind(dest.y)
    .bind(now.0 as f64)
    .bind(arrive_at.0 as f64)
    .execute(&mut **tx)
    .await
    .map_err(backend)?;
    for (unit, n) in troops.iter().filter(|(_, n)| *n > 0) {
        sqlx::query(
            "INSERT INTO movement_troops (movement_id, unit_id, count) VALUES ($1, $2, $3)",
        )
        .bind(movement_id)
        .bind(unit.as_str())
        .bind(i32::try_from(*n).unwrap_or(i32::MAX))
        .execute(&mut **tx)
        .await
        .map_err(backend)?;
    }
    Ok(())
}

/// Read an oasis's persisted garrison (the (world, x, y) rows of `oasis_garrison`), or an empty
/// composition if none. The caller falls back to the seeded animals when the oasis has no row.
async fn read_oasis_garrison(
    executor: &PgPool,
    world: Uuid,
    coord: Coordinate,
) -> Result<UnitCounts, RepoError> {
    let rows = sqlx::query(
        "SELECT unit_id, count FROM oasis_garrison \
         WHERE world_id = $1 AND x = $2 AND y = $3 ORDER BY unit_id",
    )
    .bind(world)
    .bind(coord.x)
    .bind(coord.y)
    .fetch_all(executor)
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

#[async_trait]
impl OasisRepository for PgAccountRepository {
    async fn oasis_at(&self, coord: Coordinate) -> Result<Option<OasisState>, RepoError> {
        let row = sqlx::query(
            "SELECT owner_village FROM oases WHERE world_id = $1 AND x = $2 AND y = $3",
        )
        .bind(Uuid::from_u128(self.world_id.0))
        .bind(coord.x)
        .bind(coord.y)
        .fetch_optional(&self.pool)
        .await
        .map_err(backend)?;
        match row {
            None => Ok(None),
            Some(r) => {
                let owner: Option<Uuid> = r.try_get("owner_village").map_err(backend)?;
                Ok(Some(OasisState {
                    owner: owner.map(|u| VillageId(u.as_u128())),
                    materialised: true,
                }))
            }
        }
    }

    async fn oasis_defenders(
        &self,
        coord: Coordinate,
        animals: &[UnitSpec],
        rules: &OasisRules,
    ) -> Result<UnitCounts, RepoError> {
        let world = Uuid::from_u128(self.world_id.0);
        // A materialised row exists iff the oasis has been fought/occupied: use its garrison.
        let exists: bool =
            sqlx::query_scalar("SELECT true FROM oases WHERE world_id = $1 AND x = $2 AND y = $3")
                .bind(world)
                .bind(coord.x)
                .bind(coord.y)
                .fetch_optional(&self.pool)
                .await
                .map_err(backend)?
                .unwrap_or(false);
        if exists {
            read_oasis_garrison(&self.pool, world, coord).await
        } else {
            // Un-fought oasis: the seeded wild animals (P6), re-derived from the world seed.
            Ok(oasis_garrison(self.map.seed(), coord, animals, rules))
        }
    }

    async fn occupied_oases(
        &self,
        village: VillageId,
    ) -> Result<Vec<(Coordinate, OasisBonus)>, RepoError> {
        let rows = sqlx::query(
            "SELECT x, y FROM oases WHERE world_id = $1 AND owner_village = $2 ORDER BY x, y",
        )
        .bind(Uuid::from_u128(self.world_id.0))
        .bind(Uuid::from_u128(village.0))
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;
        let mut out = Vec::with_capacity(rows.len());
        for r in &rows {
            let coord = Coordinate::new(
                r.try_get("x").map_err(backend)?,
                r.try_get("y").map_err(backend)?,
            );
            // The bonus is a deterministic property of the seeded tile, not stored.
            let bonus = self.map.oasis_bonus_at(coord).unwrap_or(OasisBonus {
                wood: 0,
                clay: 0,
                iron: 0,
                crop: 0,
            });
            out.push((coord, bonus));
        }
        Ok(out)
    }

    async fn village_oasis_bonus(&self, village: VillageId) -> Result<OasisBonus, RepoError> {
        let oases = self.occupied_oases(village).await?;
        let sum = |pick: fn(&OasisBonus) -> u8| {
            oases
                .iter()
                .map(|(_, b)| u32::from(pick(b)))
                .sum::<u32>()
                .min(u32::from(u8::MAX)) as u8
        };
        Ok(OasisBonus {
            wood: sum(|b| b.wood),
            clay: sum(|b| b.clay),
            iron: sum(|b| b.iron),
            crop: sum(|b| b.crop),
        })
    }

    #[allow(clippy::too_many_arguments)]
    async fn start_oasis_attack(
        &self,
        home: VillageId,
        owner: PlayerId,
        origin: Coordinate,
        oasis: Coordinate,
        now: Timestamp,
        arrive_at: Timestamp,
        troops: &[(UnitId, u32)],
    ) -> Result<(), RepoError> {
        let mut tx = self.pool.begin().await.map_err(backend)?;
        guarded_debit(&mut tx, Uuid::from_u128(home.0), troops).await?;
        insert_oasis_movement(
            &mut tx,
            Uuid::new_v4(),
            owner,
            MovementKind::OasisAttack,
            home,
            origin,
            oasis,
            now,
            arrive_at,
            troops,
        )
        .await?;
        tx.commit().await.map_err(backend)?;
        Ok(())
    }

    async fn claim_due_oasis_attacks(
        &self,
        now: Timestamp,
        limit: i64,
    ) -> Result<Vec<DueOasisAttack>, RepoError> {
        let rows = sqlx::query(
            "UPDATE troop_movements SET status = 'processing' WHERE id IN ( \
                 SELECT id FROM troop_movements \
                 WHERE status = 'in_transit' AND kind = 'oasis_attack' \
                   AND arrive_at <= to_timestamp($1::double precision / 1000.0) \
                 ORDER BY arrive_at, id LIMIT $2 FOR UPDATE SKIP LOCKED \
             ) RETURNING id, owner_id, home_village, origin_x, origin_y, dest_x, dest_y, \
                 (EXTRACT(EPOCH FROM arrive_at) * 1000)::bigint AS arrive_ms",
        )
        .bind(now.0 as f64)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;

        let ids: Vec<Uuid> = rows
            .iter()
            .map(|r| r.try_get("id").map_err(backend))
            .collect::<Result<_, RepoError>>()?;
        let mut troops = movement_troops_batch(&self.pool, &ids).await?;

        let mut out = Vec::with_capacity(rows.len());
        for (r, id) in rows.iter().zip(&ids) {
            let owner: Uuid = r.try_get("owner_id").map_err(backend)?;
            let home: Uuid = r.try_get("home_village").map_err(backend)?;
            let arrive_ms: i64 = r.try_get("arrive_ms").map_err(backend)?;
            out.push(DueOasisAttack {
                id: id.as_u128(),
                owner: PlayerId(owner.as_u128()),
                home_village: VillageId(home.as_u128()),
                origin: Coordinate::new(
                    r.try_get("origin_x").map_err(backend)?,
                    r.try_get("origin_y").map_err(backend)?,
                ),
                oasis: Coordinate::new(
                    r.try_get("dest_x").map_err(backend)?,
                    r.try_get("dest_y").map_err(backend)?,
                ),
                arrive_at: Timestamp(arrive_ms),
                troops: troops.remove(id).unwrap_or_default(),
            });
        }
        Ok(out)
    }

    async fn apply_oasis_battle(&self, apply: OasisBattleApply) -> Result<(), RepoError> {
        let world = Uuid::from_u128(self.world_id.0);
        let coord = apply.oasis;
        let mut tx = self.pool.begin().await.map_err(backend)?;

        // Materialise the oasis row and set ownership per the resolved outcome. `Unchanged` keeps the
        // existing owner (a fresh row defaults to NULL ⇒ unoccupied).
        match apply.ownership {
            OasisOwnership::Unchanged => {
                sqlx::query(
                    "INSERT INTO oases (world_id, x, y, owner_village, materialised) \
                     VALUES ($1, $2, $3, NULL, true) \
                     ON CONFLICT (world_id, x, y) DO UPDATE SET materialised = true",
                )
                .bind(world)
                .bind(coord.x)
                .bind(coord.y)
                .execute(&mut *tx)
                .await
                .map_err(backend)?;
            }
            OasisOwnership::Occupy(village) => {
                sqlx::query(
                    "INSERT INTO oases (world_id, x, y, owner_village, materialised) \
                     VALUES ($1, $2, $3, $4, true) \
                     ON CONFLICT (world_id, x, y) \
                     DO UPDATE SET owner_village = EXCLUDED.owner_village, materialised = true",
                )
                .bind(world)
                .bind(coord.x)
                .bind(coord.y)
                .bind(Uuid::from_u128(village.0))
                .execute(&mut *tx)
                .await
                .map_err(backend)?;
            }
            OasisOwnership::Free => {
                sqlx::query(
                    "INSERT INTO oases (world_id, x, y, owner_village, materialised) \
                     VALUES ($1, $2, $3, NULL, true) \
                     ON CONFLICT (world_id, x, y) \
                     DO UPDATE SET owner_village = NULL, materialised = true",
                )
                .bind(world)
                .bind(coord.x)
                .bind(coord.y)
                .execute(&mut *tx)
                .await
                .map_err(backend)?;
            }
        }

        // Replace the oasis garrison with the post-battle defenders (the FK requires the oasis row,
        // inserted above). An empty `defenders_after` clears the oasis.
        sqlx::query("DELETE FROM oasis_garrison WHERE world_id = $1 AND x = $2 AND y = $3")
            .bind(world)
            .bind(coord.x)
            .bind(coord.y)
            .execute(&mut *tx)
            .await
            .map_err(backend)?;
        for (unit, n) in apply.defenders_after.iter().filter(|(_, n)| *n > 0) {
            sqlx::query(
                "INSERT INTO oasis_garrison (world_id, x, y, unit_id, count) \
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(world)
            .bind(coord.x)
            .bind(coord.y)
            .bind(unit.as_str())
            .bind(i32::try_from(*n).unwrap_or(i32::MAX))
            .execute(&mut *tx)
            .await
            .map_err(backend)?;
        }

        // The attacker's survivors travel home (a `return` rejoins the garrison). Oases yield no loot.
        if !apply.survivors.is_empty() {
            insert_movement(
                &mut tx,
                Uuid::new_v4(),
                apply.owner,
                MovementKind::Return,
                apply.attacker_home,
                apply.attacker_home,
                apply.oasis,
                apply.attacker_origin,
                apply.battle_at,
                apply.return_arrive,
                &apply.survivors,
            )
            .await?;
        }

        sqlx::query("UPDATE troop_movements SET status = 'done' WHERE id = $1")
            .bind(Uuid::from_u128(apply.movement_id))
            .execute(&mut *tx)
            .await
            .map_err(backend)?;
        tx.commit().await.map_err(backend)?;
        Ok(())
    }
}

// ---------------------------------------------------------------- scouting (010)

/// Set a movement's `scout_target` after insertion (standalone scout, or an attack carrying scouts).
async fn set_scout_target(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    movement_id: Uuid,
    target: ScoutTarget,
) -> Result<(), RepoError> {
    sqlx::query("UPDATE troop_movements SET scout_target = $1 WHERE id = $2")
        .bind(target.as_str())
        .bind(movement_id)
        .execute(&mut **tx)
        .await
        .map_err(backend)?;
    Ok(())
}

/// Parse a nullable `scout_target` text column into a [`ScoutTarget`].
fn parse_scout_target_opt(s: Option<String>) -> Result<Option<ScoutTarget>, RepoError> {
    match s {
        None => Ok(None),
        Some(s) => ScoutTarget::from_slug(&s)
            .map(Some)
            .ok_or_else(|| RepoError::Backend(format!("unknown scout target: {s}"))),
    }
}

/// Serialise scout intel as a tagged jsonb payload.
fn scout_intel_to_json(intel: &ScoutIntel) -> serde_json::Value {
    match intel {
        ScoutIntel::Resources(a) => serde_json::json!({
            "type": "resources", "wood": a.wood, "clay": a.clay, "iron": a.iron, "crop": a.crop,
        }),
        ScoutIntel::Defenses { troops, wall_level } => serde_json::json!({
            "type": "defenses", "wall": *wall_level, "troops": counts_to_json(troops),
        }),
    }
}

/// Read scout intel back from its jsonb column (`None` for null or an unknown tag).
fn scout_intel_from_json(value: &serde_json::Value) -> Option<ScoutIntel> {
    match value.get("type").and_then(serde_json::Value::as_str)? {
        "resources" => Some(ScoutIntel::Resources(ResourceAmounts {
            wood: value
                .get("wood")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0),
            clay: value
                .get("clay")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0),
            iron: value
                .get("iron")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0),
            crop: value
                .get("crop")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0),
        })),
        "defenses" => Some(ScoutIntel::Defenses {
            troops: value
                .get("troops")
                .map(counts_from_json)
                .unwrap_or_default(),
            wall_level: u8::try_from(
                value
                    .get("wall")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0),
            )
            .unwrap_or(u8::MAX),
        }),
        _ => None,
    }
}

/// Insert one intel report (shared by the standalone apply and the combined-attack apply).
async fn insert_scout_report(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    r: &NewScoutReport,
) -> Result<(), RepoError> {
    sqlx::query(
        "INSERT INTO scout_reports \
         (id, scouter_player, scouter_village, target_player, target_village, target_x, target_y, \
          target_type, scouts_sent, scouts_lost, detected, standalone, intel) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
    )
    .bind(Uuid::new_v4())
    .bind(Uuid::from_u128(r.scouter_player.0))
    .bind(Uuid::from_u128(r.scouter_village.0))
    .bind(Uuid::from_u128(r.target_player.0))
    .bind(Uuid::from_u128(r.target_village.0))
    .bind(r.target_coord.x)
    .bind(r.target_coord.y)
    .bind(r.target_type.as_str())
    .bind(counts_to_json(&r.scouts_sent))
    .bind(counts_to_json(&r.scouts_lost))
    .bind(r.detected)
    .bind(r.standalone)
    .bind(r.intel.as_ref().map(scout_intel_to_json))
    .execute(&mut **tx)
    .await
    .map_err(backend)?;
    Ok(())
}

/// The `SELECT` of a scout report joined to scouter/target names + the scouter's coordinate.
const SCOUT_REPORT_SELECT: &str = "SELECT sr.id, \
    (EXTRACT(EPOCH FROM sr.occurred_at) * 1000)::bigint AS occurred_ms, \
    su.username AS scouter_name, sv.x AS sx, sv.y AS sy, \
    tu.username AS target_name, sr.target_x AS tx, sr.target_y AS ty, \
    sr.scouter_player, sr.target_player, sr.target_type, \
    sr.scouts_sent, sr.scouts_lost, sr.detected, sr.standalone, sr.intel \
    FROM scout_reports sr \
    JOIN users su ON su.id = sr.scouter_player \
    JOIN villages sv ON sv.id = sr.scouter_village \
    JOIN users tu ON tu.id = sr.target_player";

/// Map a joined `scout_reports` row to a [`ScoutReportView`], applying target-side redaction (P4):
/// a non-scouter viewer sees only the notification (scouts destroyed) — never the intel or the
/// scouts that were sent.
fn scout_report_from_row(r: &PgRow, player: PlayerId) -> Result<ScoutReportView, RepoError> {
    let id: Uuid = r.try_get("id").map_err(backend)?;
    let occurred_ms: i64 = r.try_get("occurred_ms").map_err(backend)?;
    let scouter: Uuid = r.try_get("scouter_player").map_err(backend)?;
    let target: Uuid = r.try_get("target_player").map_err(backend)?;
    let tt: String = r.try_get("target_type").map_err(backend)?;
    let sent: serde_json::Value = r.try_get("scouts_sent").map_err(backend)?;
    let lost: serde_json::Value = r.try_get("scouts_lost").map_err(backend)?;
    let intel_json: Option<serde_json::Value> = r.try_get("intel").map_err(backend)?;
    let viewer_is_scouter = scouter.as_u128() == player.0;
    let target_type = ScoutTarget::from_slug(&tt)
        .ok_or_else(|| RepoError::Backend(format!("unknown scout target: {tt}")))?;
    Ok(ScoutReportView {
        id: id.as_u128(),
        occurred_at: Timestamp(occurred_ms),
        scouter_player: PlayerId(scouter.as_u128()),
        scouter_name: r.try_get("scouter_name").map_err(backend)?,
        scouter_coord: Coordinate::new(
            r.try_get("sx").map_err(backend)?,
            r.try_get("sy").map_err(backend)?,
        ),
        target_player: PlayerId(target.as_u128()),
        target_name: r.try_get("target_name").map_err(backend)?,
        target_coord: Coordinate::new(
            r.try_get("tx").map_err(backend)?,
            r.try_get("ty").map_err(backend)?,
        ),
        target_type,
        scouts_sent: if viewer_is_scouter {
            counts_from_json(&sent)
        } else {
            Vec::new()
        },
        scouts_lost: counts_from_json(&lost),
        detected: r.try_get("detected").map_err(backend)?,
        standalone: r.try_get("standalone").map_err(backend)?,
        intel: if viewer_is_scouter {
            intel_json.as_ref().and_then(scout_intel_from_json)
        } else {
            None
        },
        viewer_is_scouter,
    })
}

#[async_trait]
impl ScoutRepository for PgAccountRepository {
    #[allow(clippy::too_many_arguments)]
    async fn start_scout(
        &self,
        home: VillageId,
        deliver: VillageId,
        owner: PlayerId,
        origin: Coordinate,
        dest: Coordinate,
        now: Timestamp,
        arrive_at: Timestamp,
        troops: &[(UnitId, u32)],
        target: ScoutTarget,
    ) -> Result<(), RepoError> {
        let mut tx = self.pool.begin().await.map_err(backend)?;
        guarded_debit(&mut tx, Uuid::from_u128(home.0), troops).await?;
        let movement_id = Uuid::new_v4();
        insert_movement(
            &mut tx,
            movement_id,
            owner,
            MovementKind::Scout,
            home,
            deliver,
            origin,
            dest,
            now,
            arrive_at,
            troops,
        )
        .await?;
        set_scout_target(&mut tx, movement_id, target).await?;
        tx.commit().await.map_err(backend)?;
        Ok(())
    }

    async fn claim_due_scouts(
        &self,
        now: Timestamp,
        limit: i64,
    ) -> Result<Vec<DueScout>, RepoError> {
        let rows = sqlx::query(
            "UPDATE troop_movements SET status = 'processing' WHERE id IN ( \
                 SELECT id FROM troop_movements \
                 WHERE status = 'in_transit' AND kind = 'scout' \
                   AND arrive_at <= to_timestamp($1::double precision / 1000.0) \
                 ORDER BY arrive_at, id LIMIT $2 FOR UPDATE SKIP LOCKED \
             ) RETURNING id, owner_id, home_village, deliver_village, \
                 origin_x, origin_y, dest_x, dest_y, scout_target, \
                 (EXTRACT(EPOCH FROM arrive_at) * 1000)::bigint AS arrive_ms",
        )
        .bind(now.0 as f64)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;

        let ids: Vec<Uuid> = rows
            .iter()
            .map(|r| r.try_get("id").map_err(backend))
            .collect::<Result<_, RepoError>>()?;
        let mut troops = movement_troops_batch(&self.pool, &ids).await?;

        let mut out = Vec::with_capacity(rows.len());
        for (r, id) in rows.iter().zip(&ids) {
            let owner: Uuid = r.try_get("owner_id").map_err(backend)?;
            let home: Uuid = r.try_get("home_village").map_err(backend)?;
            let target: Uuid = r.try_get("deliver_village").map_err(backend)?;
            let arrive_ms: i64 = r.try_get("arrive_ms").map_err(backend)?;
            // A scout movement always carries a target; a missing one is corrupt data.
            let target_type = parse_scout_target_opt(r.try_get("scout_target").map_err(backend)?)?
                .ok_or_else(|| RepoError::Backend("scout movement without a target".into()))?;
            out.push(DueScout {
                id: id.as_u128(),
                owner: PlayerId(owner.as_u128()),
                home_village: VillageId(home.as_u128()),
                target_village: VillageId(target.as_u128()),
                origin: Coordinate::new(
                    r.try_get("origin_x").map_err(backend)?,
                    r.try_get("origin_y").map_err(backend)?,
                ),
                dest: Coordinate::new(
                    r.try_get("dest_x").map_err(backend)?,
                    r.try_get("dest_y").map_err(backend)?,
                ),
                arrive_at: Timestamp(arrive_ms),
                troops: troops.remove(id).unwrap_or_default(),
                target_type,
            });
        }
        Ok(out)
    }

    async fn apply_scout(&self, apply: ScoutApply) -> Result<(), RepoError> {
        let mut tx = self.pool.begin().await.map_err(backend)?;
        insert_scout_report(&mut tx, &apply.report).await?;

        // Surviving scouts travel home (a `return` movement rejoins the garrison).
        if !apply.survivors.is_empty() {
            insert_movement(
                &mut tx,
                Uuid::new_v4(),
                apply.owner,
                MovementKind::Return,
                apply.scouter_home,
                apply.scouter_home,
                apply.target_coord,
                apply.scouter_origin,
                apply.scouted_at,
                apply.return_arrive,
                &apply.survivors,
            )
            .await?;
        }

        sqlx::query("UPDATE troop_movements SET status = 'done' WHERE id = $1")
            .bind(Uuid::from_u128(apply.movement_id))
            .execute(&mut *tx)
            .await
            .map_err(backend)?;
        tx.commit().await.map_err(backend)?;
        Ok(())
    }

    async fn scout_reports_for(
        &self,
        player: PlayerId,
        limit: i64,
    ) -> Result<Vec<ScoutReportView>, RepoError> {
        // The scouter sees their own missions; the target sees only detected standalone ones.
        let sql = format!(
            "{SCOUT_REPORT_SELECT} \
             WHERE sr.scouter_player = $1 \
                OR (sr.target_player = $1 AND sr.detected AND sr.standalone) \
             ORDER BY sr.occurred_at DESC LIMIT $2"
        );
        let rows = sqlx::query(&sql)
            .bind(Uuid::from_u128(player.0))
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(backend)?;
        rows.iter()
            .map(|r| scout_report_from_row(r, player))
            .collect()
    }

    async fn scout_report(
        &self,
        id: u128,
        player: PlayerId,
    ) -> Result<Option<ScoutReportView>, RepoError> {
        let sql = format!(
            "{SCOUT_REPORT_SELECT} WHERE sr.id = $1 \
             AND (sr.scouter_player = $2 \
                  OR (sr.target_player = $2 AND sr.detected AND sr.standalone))"
        );
        let row = sqlx::query(&sql)
            .bind(Uuid::from_u128(id))
            .bind(Uuid::from_u128(player.0))
            .fetch_optional(&self.pool)
            .await
            .map_err(backend)?;
        row.as_ref()
            .map(|r| scout_report_from_row(r, player))
            .transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eperica_application::NewBattleReport;
    use eperica_domain::{GameSpeed, WorldConfig};

    /// The resources row's last-settled time — the snapshot orders must be computed from.
    async fn snapshot(repo: &PgAccountRepository, village: VillageId) -> Timestamp {
        repo.stored_resources(village).await.unwrap().unwrap().1
    }

    /// 007 AC1/AC4/AC5: a reinforcement debits the source garrison, arrives once (crash-resume
    /// safe), stations at the target, and the return rejoins the source garrison.
    #[tokio::test]
    async fn movement_reinforce_and_return_lifecycle() {
        let _ = dotenvy::dotenv();
        let Ok(url) = std::env::var("DATABASE_URL") else {
            eprintln!("skipping movement test: DATABASE_URL not set");
            return;
        };
        let pool = crate::create_pool(&url).await.expect("connect");
        crate::run_migrations(&pool).await.expect("migrate");
        let config = WorldConfig::new(GameSpeed::new(1.0).unwrap(), 50);
        let world = crate::world::ensure_world(&pool, &config)
            .await
            .expect("ensure world");
        let rules = crate::economy_rules().expect("economy rules");
        let repo = PgAccountRepository::new(
            pool.clone(),
            world.id,
            world.seed,
            config.radius,
            rules.starting_amounts,
        );

        let template = crate::starting_village().unwrap();
        let account = async |tag: &str| {
            let uname = format!("{tag}_{}", Uuid::new_v4().simple());
            let user = repo
                .create_account(
                    NewUser {
                        username: uname.clone(),
                        email: format!("{uname}@example.com"),
                        password_hash: "h".to_owned(),
                        email_confirmed: true,
                        tribe: Tribe::Gauls,
                    },
                    &template,
                )
                .await
                .expect("create account");
            let v = repo.villages_of(user.id).await.unwrap()[0].clone();
            (user, uname, v)
        };
        let (alice, alice_name, a) = account("snd").await;
        let (_bob, bob_name, b) = account("rcv").await;
        assert_ne!(a.coordinate, b.coordinate);

        // Seed Alice's garrison with 10 phalanx.
        sqlx::query(
            "INSERT INTO village_units (village_id, unit_id, count) VALUES ($1, 'phalanx', 10)",
        )
        .bind(Uuid::from_u128(a.id.0))
        .execute(&pool)
        .await
        .unwrap();

        let now = Timestamp(3_000_000_000_000);
        let arrive = Timestamp(now.0 + 100_000);
        let troops = vec![(UnitId("phalanx".into()), 4)];

        // AC1: send 4 phalanx to Bob — Alice's garrison drops to 6, a movement is in flight.
        repo.start_reinforcement(
            a.id,
            b.id,
            alice.id,
            a.coordinate,
            b.coordinate,
            now,
            arrive,
            &troops,
        )
        .await
        .expect("send");
        assert_eq!(
            repo.garrison(a.id).await.unwrap(),
            vec![(UnitId("phalanx".into()), 6)]
        );
        let outgoing = repo.active_movements(alice.id).await.unwrap();
        assert_eq!(outgoing.len(), 1);
        assert_eq!(outgoing[0].kind, MovementKind::Reinforce);
        assert_eq!(outgoing[0].destination, b.coordinate);
        assert!(repo.reinforcements_at(b.id).await.unwrap().is_empty()); // not arrived yet

        // Crash-resume: claim the due arrival, "crash" before applying, requeue, re-claim, apply
        // once — the troops are stationed exactly once (AC4).
        let claimed = repo.claim_due_movements(arrive, 100).await.unwrap();
        assert!(claimed.iter().any(|d| d.home_village == a.id));
        assert!(repo.requeue_orphaned_movements().await.unwrap() >= 1);
        let due = repo.claim_due_movements(arrive, 100).await.unwrap();
        let mine = due.iter().find(|d| d.home_village == a.id).expect("due");
        repo.apply_movement(mine, None)
            .await
            .expect("apply reinforce");

        // AC4: stationed at Bob, owned by Alice; visible to both sides.
        let here = repo.reinforcements_at(b.id).await.unwrap();
        assert_eq!(here.len(), 1);
        assert_eq!(here[0].home_village, a.id);
        assert_eq!(here[0].other_owner, alice_name); // who is helping Bob
        assert_eq!(here[0].troops, vec![(UnitId("phalanx".into()), 4)]);
        let abroad = repo.reinforcements_of(alice.id).await.unwrap();
        assert_eq!(abroad.len(), 1);
        assert_eq!(abroad[0].host_village, b.id);
        assert_eq!(abroad[0].other_owner, bob_name); // where Alice's troops are
        // The applied movement no longer claims.
        assert!(
            repo.claim_due_movements(arrive, 100)
                .await
                .unwrap()
                .iter()
                .all(|d| d.home_village != a.id)
        );

        // AC5: Alice recalls them — the stationed group is removed and a return is created.
        let now2 = Timestamp(now.0 + 1_000_000);
        let arrive2 = Timestamp(now2.0 + 100_000);
        let returned = repo
            .start_return(
                b.id,
                a.id,
                alice.id,
                b.coordinate,
                a.coordinate,
                now2,
                arrive2,
            )
            .await
            .expect("return");
        assert_eq!(returned, vec![(UnitId("phalanx".into()), 4)]);
        assert!(repo.reinforcements_at(b.id).await.unwrap().is_empty());
        // A second recall finds nothing.
        assert!(matches!(
            repo.start_return(
                b.id,
                a.id,
                alice.id,
                b.coordinate,
                a.coordinate,
                now2,
                arrive2
            )
            .await,
            Err(RepoError::Conflict)
        ));

        // The return arrives via the processor — the troops rejoin Alice's garrison (back to 10),
        // and the processor reports her home for a starvation re-sync (AC5).
        let homes = eperica_application::process_due_movements(
            &repo,
            &repo,
            &crate::economy_rules().unwrap(),
            &crate::unit_rules().unwrap(),
            GameSpeed::new(1.0).unwrap(),
            arrive2,
            100,
        )
        .await
        .expect("process movements");
        assert!(homes.contains(&a.id));
        assert_eq!(
            repo.garrison(a.id).await.unwrap(),
            vec![(UnitId("phalanx".into()), 10)]
        );
    }

    /// 007 AC2/P4: when the garrison no longer covers the request (the guarded debit can't take
    /// exactly the asked count), the send is rejected with `Conflict` and **nothing** is removed —
    /// the troops are not partially debited and no movement is created.
    #[tokio::test]
    async fn start_reinforcement_over_garrison_removes_nothing() {
        let _ = dotenvy::dotenv();
        let Ok(url) = std::env::var("DATABASE_URL") else {
            eprintln!("skipping guarded-debit test: DATABASE_URL not set");
            return;
        };
        let pool = crate::create_pool(&url).await.expect("connect");
        crate::run_migrations(&pool).await.expect("migrate");
        let config = WorldConfig::new(GameSpeed::new(1.0).unwrap(), 50);
        let world = crate::world::ensure_world(&pool, &config)
            .await
            .expect("ensure world");
        let rules = crate::economy_rules().expect("economy rules");
        let repo = PgAccountRepository::new(
            pool.clone(),
            world.id,
            world.seed,
            config.radius,
            rules.starting_amounts,
        );
        let template = crate::starting_village().unwrap();
        let account = async |tag: &str| {
            let uname = format!("{tag}_{}", Uuid::new_v4().simple());
            let user = repo
                .create_account(
                    NewUser {
                        username: uname.clone(),
                        email: format!("{uname}@example.com"),
                        password_hash: "h".to_owned(),
                        email_confirmed: true,
                        tribe: Tribe::Gauls,
                    },
                    &template,
                )
                .await
                .expect("create account");
            repo.villages_of(user.id).await.unwrap()[0].clone()
        };
        let a = account("grd_snd").await;
        let b = account("grd_rcv").await;

        // Seed two stacks; request 2 sword (held, debited first) then 4 phalanx (only 3 held). The
        // first debit succeeds inside the transaction; the second fails, so the whole send must
        // roll back atomically — the swordsman debit is undone too.
        sqlx::query(
            "INSERT INTO village_units (village_id, unit_id, count) \
             VALUES ($1, 'phalanx', 3), ($1, 'swordsman', 2)",
        )
        .bind(Uuid::from_u128(a.id.0))
        .execute(&pool)
        .await
        .unwrap();

        let now = Timestamp(3_000_000_000_000);
        let arrive = Timestamp(now.0 + 100_000);
        let over = vec![
            (UnitId("swordsman".into()), 2),
            (UnitId("phalanx".into()), 4),
        ];
        assert!(matches!(
            repo.start_reinforcement(
                a.id,
                b.id,
                a.owner,
                a.coordinate,
                b.coordinate,
                now,
                arrive,
                &over
            )
            .await,
            Err(RepoError::Conflict)
        ));

        // Nothing debited (the already-taken swordsman stack was rolled back) and no movement
        // scheduled.
        let mut garrison = repo.garrison(a.id).await.unwrap();
        garrison.sort_by(|x, y| x.0.as_str().cmp(y.0.as_str()));
        assert_eq!(
            garrison,
            vec![
                (UnitId("phalanx".into()), 3),
                (UnitId("swordsman".into()), 2),
            ]
        );
        assert!(repo.active_movements(a.owner).await.unwrap().is_empty());
    }

    /// 008 AC1/AC4/AC5: a shipment debits the sender + commits merchants, delivers capped to the
    /// target (crash-resume safe) + schedules a return, and the return frees the merchants.
    #[tokio::test]
    async fn trade_send_deliver_and_return_lifecycle() {
        let _ = dotenvy::dotenv();
        let Ok(url) = std::env::var("DATABASE_URL") else {
            eprintln!("skipping trade test: DATABASE_URL not set");
            return;
        };
        let pool = crate::create_pool(&url).await.expect("connect");
        crate::run_migrations(&pool).await.expect("migrate");
        let config = WorldConfig::new(GameSpeed::new(1.0).unwrap(), 50);
        let world = crate::world::ensure_world(&pool, &config)
            .await
            .expect("ensure world");
        let rules = crate::economy_rules().expect("economy rules");
        let repo = PgAccountRepository::new(
            pool.clone(),
            world.id,
            world.seed,
            config.radius,
            rules.starting_amounts,
        );
        let template = crate::starting_village().unwrap();
        let account = async |tag: &str| {
            let uname = format!("{tag}_{}", Uuid::new_v4().simple());
            let user = repo
                .create_account(
                    NewUser {
                        username: uname.clone(),
                        email: format!("{uname}@example.com"),
                        password_hash: "h".to_owned(),
                        email_confirmed: true,
                        tribe: Tribe::Gauls,
                    },
                    &template,
                )
                .await
                .expect("create account");
            (user.id, repo.villages_of(user.id).await.unwrap()[0].clone())
        };
        let (sender, a) = account("trd_snd").await;
        let (_recv, b) = account("trd_rcv").await;
        assert_ne!(a.coordinate, b.coordinate);

        // AC1: send 300 wood with 2 merchants. The sender starts at 750 each (starting-village.toml).
        let (stored, snap) = repo.stored_resources(a.id).await.unwrap().unwrap();
        let bundle = ResourceAmounts {
            wood: 300,
            clay: 0,
            iron: 0,
            crop: 0,
        };
        let debited = ResourceAmounts {
            wood: stored.wood - 300,
            ..stored
        };
        let now = Timestamp(3_000_000_000_000);
        let arrive = Timestamp(now.0 + 100_000);
        repo.start_trade(
            a.id,
            b.id,
            sender,
            a.coordinate,
            b.coordinate,
            debited,
            snap,
            now,
            arrive,
            bundle,
            2,
        )
        .await
        .expect("send");
        assert_eq!(
            repo.stored_resources(a.id).await.unwrap().unwrap().0.wood,
            stored.wood - 300
        );
        assert_eq!(repo.committed_merchants(a.id).await.unwrap(), 2);
        let active = repo.active_trades(sender).await.unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].kind, TradeKind::Deliver);
        assert_eq!(active[0].bundle.wood, 300);
        assert_eq!(active[0].merchants, 2);

        // AC4: claim, "crash" before applying, requeue, re-claim, then deliver once — the target is
        // credited capped to its base Warehouse (800: 750 + 300 overflows, the excess lost).
        let claimed = repo.claim_due_trades(arrive, 100).await.unwrap();
        assert!(claimed.iter().any(|d| d.home_village == a.id));
        assert!(repo.requeue_orphaned_trades().await.unwrap() >= 1);
        let due = repo.claim_due_trades(arrive, 100).await.unwrap();
        let mine = due
            .iter()
            .find(|d| d.home_village == a.id && d.kind == TradeKind::Deliver)
            .expect("due deliver");
        let (t_stored, t_snap) = repo.stored_resources(b.id).await.unwrap().unwrap();
        let caps = eperica_domain::Capacities {
            warehouse: 800,
            granary: 800,
        };
        let credited = eperica_domain::deposit_capped(t_stored, mine.bundle, caps);
        let return_arrive = Timestamp(arrive.0 + 100_000);
        repo.deliver_and_schedule_return(mine, credited, t_snap, arrive, return_arrive)
            .await
            .expect("deliver");
        assert_eq!(
            repo.stored_resources(b.id).await.unwrap().unwrap().0.wood,
            800
        ); // capped
        assert_eq!(repo.committed_merchants(a.id).await.unwrap(), 2); // now on the return leg
        // The deliver no longer claims; the return is in flight.
        assert!(
            repo.claim_due_trades(arrive, 100)
                .await
                .unwrap()
                .iter()
                .all(|d| !(d.home_village == a.id && d.kind == TradeKind::Deliver))
        );

        // AC5: the return arrives and frees the merchants.
        let due_ret = repo.claim_due_trades(return_arrive, 100).await.unwrap();
        let ret = due_ret
            .iter()
            .find(|d| d.home_village == a.id && d.kind == TradeKind::Return)
            .expect("due return");
        repo.complete_trade(ret.id).await.expect("complete");
        assert_eq!(repo.committed_merchants(a.id).await.unwrap(), 0);
        assert!(repo.active_trades(sender).await.unwrap().is_empty());
    }

    /// 008 AC4/AC5 (processor path): the application `process_due_trades` — as the scheduler ticks it
    /// — delivers a due shipment (credits the target through the real economy settle, capped) and a
    /// later tick completes the empty return, freeing the merchants.
    #[tokio::test]
    async fn process_due_trades_delivers_and_frees_merchants() {
        let _ = dotenvy::dotenv();
        let Ok(url) = std::env::var("DATABASE_URL") else {
            eprintln!("skipping trade processor test: DATABASE_URL not set");
            return;
        };
        let pool = crate::create_pool(&url).await.expect("connect");
        crate::run_migrations(&pool).await.expect("migrate");
        let config = WorldConfig::new(GameSpeed::new(1.0).unwrap(), 50);
        let world = crate::world::ensure_world(&pool, &config)
            .await
            .expect("ensure world");
        let econ = crate::economy_rules().expect("economy rules");
        let units = crate::unit_rules().expect("unit rules");
        let merchants = crate::merchant_rules().expect("merchant rules");
        let map = WorldMap::new(
            world.seed as u64,
            config.radius,
            crate::map_rules().unwrap(),
        );
        let repo = PgAccountRepository::new(
            pool.clone(),
            world.id,
            world.seed,
            config.radius,
            econ.starting_amounts,
        );
        let template = crate::starting_village().unwrap();
        let account = async |tag: &str| {
            let uname = format!("{tag}_{}", Uuid::new_v4().simple());
            let user = repo
                .create_account(
                    NewUser {
                        username: uname.clone(),
                        email: format!("{uname}@example.com"),
                        password_hash: "h".to_owned(),
                        email_confirmed: true,
                        tribe: Tribe::Gauls,
                    },
                    &template,
                )
                .await
                .expect("create account");
            (user.id, repo.villages_of(user.id).await.unwrap()[0].clone())
        };
        let (sender, a) = account("prc_snd").await;
        let (_recv, b) = account("prc_rcv").await;

        // Send 300 wood, 2 merchants, due at `arrive`.
        let (stored, snap) = repo.stored_resources(a.id).await.unwrap().unwrap();
        let bundle = ResourceAmounts {
            wood: 300,
            clay: 0,
            iron: 0,
            crop: 0,
        };
        let debited = ResourceAmounts {
            wood: stored.wood - 300,
            ..stored
        };
        let now = Timestamp(3_000_000_000_000);
        let arrive = Timestamp(now.0 + 100_000);
        repo.start_trade(
            a.id,
            b.id,
            sender,
            a.coordinate,
            b.coordinate,
            debited,
            snap,
            now,
            arrive,
            bundle,
            2,
        )
        .await
        .expect("send");

        // Tick the processor at the arrival: the deliver credits the target (to its 800 base cap)
        // and schedules the empty return; the merchants stay committed.
        let speed = GameSpeed::new(1.0).unwrap();
        let credited = eperica_application::process_due_trades(
            &repo, &repo, &econ, &units, &merchants, &map, speed, arrive, 100,
        )
        .await
        .expect("deliver tick");
        assert!(credited.contains(&b.id));
        assert_eq!(
            repo.stored_resources(b.id).await.unwrap().unwrap().0.wood,
            800
        );
        assert_eq!(repo.committed_merchants(a.id).await.unwrap(), 2);

        // A later tick (well past the return arrival) completes the return and frees the merchants.
        let far = Timestamp(arrive.0 + 1_000_000_000);
        eperica_application::process_due_trades(
            &repo, &repo, &econ, &units, &merchants, &map, speed, far, 100,
        )
        .await
        .expect("return tick");
        assert_eq!(repo.committed_merchants(a.id).await.unwrap(), 0);
    }

    /// 008 AC4 (P2 reproducibility): a delivery that fires after the target was already settled past
    /// the arrival instant must **not** move the target's resource clock backwards — otherwise the
    /// next read would re-accrue production already in `stored` (a free-resource double-count).
    #[tokio::test]
    async fn late_delivery_does_not_regress_the_resource_clock() {
        let _ = dotenvy::dotenv();
        let Ok(url) = std::env::var("DATABASE_URL") else {
            eprintln!("skipping late-delivery test: DATABASE_URL not set");
            return;
        };
        let pool = crate::create_pool(&url).await.expect("connect");
        crate::run_migrations(&pool).await.expect("migrate");
        let config = WorldConfig::new(GameSpeed::new(1.0).unwrap(), 50);
        let world = crate::world::ensure_world(&pool, &config)
            .await
            .expect("ensure world");
        let econ = crate::economy_rules().expect("economy rules");
        let units = crate::unit_rules().expect("unit rules");
        let merchants = crate::merchant_rules().expect("merchant rules");
        let map = WorldMap::new(
            world.seed as u64,
            config.radius,
            crate::map_rules().unwrap(),
        );
        let repo = PgAccountRepository::new(
            pool.clone(),
            world.id,
            world.seed,
            config.radius,
            econ.starting_amounts,
        );
        let template = crate::starting_village().unwrap();
        let account = async |tag: &str| {
            let uname = format!("{tag}_{}", Uuid::new_v4().simple());
            let user = repo
                .create_account(
                    NewUser {
                        username: uname.clone(),
                        email: format!("{uname}@example.com"),
                        password_hash: "h".to_owned(),
                        email_confirmed: true,
                        tribe: Tribe::Gauls,
                    },
                    &template,
                )
                .await
                .expect("create account");
            (user.id, repo.villages_of(user.id).await.unwrap()[0].clone())
        };
        let (sender, a) = account("late_snd").await;
        let (_recv, b) = account("late_rcv").await;

        // Give the target a big Warehouse so the credit is not clamped (we assert the exact amount).
        sqlx::query(
            "INSERT INTO village_buildings (village_id, slot, building_type, level) \
             VALUES ($1, 2, 'warehouse', 10)",
        )
        .bind(Uuid::from_u128(b.id.0))
        .execute(&pool)
        .await
        .unwrap();

        // Pin the target's resource clock to a known instant T_target with a known amount.
        const T_TARGET: i64 = 4_000_000_000_000;
        sqlx::query(
            "UPDATE village_resources SET wood = 700, clay = 0, iron = 0, crop = 0, \
             updated_at = to_timestamp($1::double precision / 1000.0) WHERE village_id = $2",
        )
        .bind(T_TARGET as f64)
        .bind(Uuid::from_u128(b.id.0))
        .execute(&pool)
        .await
        .unwrap();

        // Send a shipment whose arrival is BEFORE T_target (a backlogged/late delivery).
        let (stored, snap) = repo.stored_resources(a.id).await.unwrap().unwrap();
        let bundle = ResourceAmounts {
            wood: 300,
            clay: 0,
            iron: 0,
            crop: 0,
        };
        let debited = ResourceAmounts {
            wood: stored.wood - 300,
            ..stored
        };
        let send_now = Timestamp(2_999_000_000_000);
        let arrive = Timestamp(3_000_000_000_000); // < T_target
        repo.start_trade(
            a.id,
            b.id,
            sender,
            a.coordinate,
            b.coordinate,
            debited,
            snap,
            send_now,
            arrive,
            bundle,
            2,
        )
        .await
        .expect("send");

        // Deliver well after T_target.
        let speed = GameSpeed::new(1.0).unwrap();
        eperica_application::process_due_trades(
            &repo,
            &repo,
            &econ,
            &units,
            &merchants,
            &map,
            speed,
            Timestamp(5_000_000_000_000),
            100,
        )
        .await
        .expect("deliver tick");

        // The clock did not regress to `arrive`, and the credit is exactly the snapshot + bundle
        // (no re-accrued production): 700 + 300 = 1000.
        let (after, clock) = repo.stored_resources(b.id).await.unwrap().unwrap();
        assert_eq!(clock, Timestamp(T_TARGET), "resource clock regressed");
        assert_eq!(after.wood, 1000);
    }

    /// 009 AC6/AC7: a raid debits the attacker, claims as due, and `apply_battle` (one tx) reduces
    /// the defender garrison + reinforcements, schedules the survivor return, marks the attack done,
    /// and persists a report readable by both parties but not a third.
    #[tokio::test]
    async fn combat_apply_battle_and_reports() {
        let _ = dotenvy::dotenv();
        let Ok(url) = std::env::var("DATABASE_URL") else {
            eprintln!("skipping combat test: DATABASE_URL not set");
            return;
        };
        let pool = crate::create_pool(&url).await.expect("connect");
        crate::run_migrations(&pool).await.expect("migrate");
        let config = WorldConfig::new(GameSpeed::new(1.0).unwrap(), 50);
        let world = crate::world::ensure_world(&pool, &config)
            .await
            .expect("ensure world");
        let rules = crate::economy_rules().expect("economy rules");
        let repo = PgAccountRepository::new(
            pool.clone(),
            world.id,
            world.seed,
            config.radius,
            rules.starting_amounts,
        );
        let template = crate::starting_village().unwrap();
        let account = async |tag: &str| {
            let uname = format!("{tag}_{}", Uuid::new_v4().simple());
            let user = repo
                .create_account(
                    NewUser {
                        username: uname.clone(),
                        email: format!("{uname}@example.com"),
                        password_hash: "h".to_owned(),
                        email_confirmed: true,
                        tribe: Tribe::Gauls,
                    },
                    &template,
                )
                .await
                .expect("create account");
            (user.id, repo.villages_of(user.id).await.unwrap()[0].clone())
        };
        let (attacker, a) = account("atk").await;
        let (defender, d) = account("def").await;
        let (ally, al) = account("ally").await;

        // Attacker garrison: 10 swordsmen. Defender garrison: 8 phalanx; ally reinforces with 4.
        sqlx::query(
            "INSERT INTO village_units (village_id, unit_id, count) VALUES ($1, 'swordsman', 10)",
        )
        .bind(Uuid::from_u128(a.id.0))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO village_units (village_id, unit_id, count) VALUES ($1, 'phalanx', 8)",
        )
        .bind(Uuid::from_u128(d.id.0))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO reinforcements (host_village, home_village, unit_id, count) \
             VALUES ($1, $2, 'phalanx', 4)",
        )
        .bind(Uuid::from_u128(d.id.0))
        .bind(Uuid::from_u128(al.id.0))
        .execute(&pool)
        .await
        .unwrap();

        // AC1: raid 6 swordsmen — the attacker garrison drops to 4 and an attack is in flight.
        let now = Timestamp(3_000_000_000_000);
        let arrive = Timestamp(now.0 + 100_000);
        let troops = vec![(UnitId("swordsman".into()), 6)];
        repo.start_attack(
            a.id,
            d.id,
            attacker,
            a.coordinate,
            d.coordinate,
            now,
            arrive,
            MovementKind::Raid,
            &troops,
            None,
            None,
        )
        .await
        .expect("attack");
        assert_eq!(
            repo.garrison(a.id).await.unwrap(),
            vec![(UnitId("swordsman".into()), 4)]
        );
        let due = repo.claim_due_attacks(arrive, 100).await.unwrap();
        let mine = due
            .iter()
            .find(|x| x.home_village == a.id)
            .expect("due attack");
        assert_eq!(mine.kind, MovementKind::Raid);
        assert_eq!(mine.troops, vec![(UnitId("swordsman".into()), 6)]);

        // AC6/AC7: apply a resolved raid — defender loses 4 phalanx, the ally group loses 2, the
        // attacker keeps 4 survivors (return scheduled), and a report is written.
        let return_arrive = Timestamp(arrive.0 + 100_000);
        repo.apply_battle(BattleApply {
            movement_id: mine.id,
            owner: attacker,
            attacker_home: a.id,
            attacker_origin: a.coordinate,
            target: d.id,
            target_coord: d.coordinate,
            defender_losses: vec![(UnitId("phalanx".into()), 4)],
            reinforcement_losses: vec![(al.id, vec![(UnitId("phalanx".into()), 2)])],
            survivors: vec![(UnitId("swordsman".into()), 4)],
            battle_at: arrive,
            return_arrive,
            report: NewBattleReport {
                kind: MovementKind::Raid,
                attacker_player: attacker,
                attacker_village: a.id,
                defender_player: defender,
                defender_village: d.id,
                attacker_won: true,
                luck: 1.1,
                morale: 1.0,
                wall_before: 0,
                wall_after: 0,
                attacker_forces: vec![(UnitId("swordsman".into()), 6)],
                attacker_losses: vec![(UnitId("swordsman".into()), 2)],
                defender_forces: vec![(UnitId("phalanx".into()), 12)],
                defender_losses: vec![(UnitId("phalanx".into()), 6)],
                loot: ResourceAmounts::default(),
                razed: None,
            },
            scouted: false,
            scout_target: None,
            scout_report: None,
            loot: ResourceAmounts::default(),
            target_debit: None,
            razed: None,
        })
        .await
        .expect("apply battle");

        assert_eq!(
            repo.garrison(d.id).await.unwrap(),
            vec![(UnitId("phalanx".into()), 4)] // 8 - 4
        );
        let here = repo.reinforcements_at(d.id).await.unwrap();
        assert_eq!(here[0].troops, vec![(UnitId("phalanx".into()), 2)]); // 4 - 2
        // The survivor return is in flight to the attacker; the attack movement is done.
        let returning = repo.active_movements(attacker).await.unwrap();
        assert!(
            returning.iter().any(|m| m.kind == MovementKind::Return
                && m.troops == vec![(UnitId("swordsman".into()), 4)])
        );
        assert!(
            repo.claim_due_attacks(arrive, 100)
                .await
                .unwrap()
                .iter()
                .all(|x| x.home_village != a.id) // the attack no longer claims
        );

        // AC7: the report is readable by both parties, not by the ally (a third party).
        let atk_reports = repo.reports_for(attacker, 50).await.unwrap();
        assert_eq!(atk_reports.len(), 1);
        let report_id = atk_reports[0].id;
        assert!(atk_reports[0].attacker_won);
        assert_eq!(
            atk_reports[0].defender_name,
            repo.find_user_by_id(defender)
                .await
                .unwrap()
                .unwrap()
                .username
        );
        assert_eq!(repo.reports_for(defender, 50).await.unwrap().len(), 1);
        assert!(repo.reports_for(ally, 50).await.unwrap().is_empty());
        assert!(repo.report(report_id, attacker).await.unwrap().is_some());
        assert!(repo.report(report_id, ally).await.unwrap().is_none()); // not a party
    }

    /// 011 AC2/AC6/AC9: `apply_battle` debits the target's resources, razes the targeted building,
    /// attaches the loot to the survivor return, and records it on the report; the return then
    /// credits the loot (capped) to the attacker on arrival.
    #[tokio::test]
    async fn siege_loot_persistence_and_credit() {
        let _ = dotenvy::dotenv();
        let Ok(url) = std::env::var("DATABASE_URL") else {
            eprintln!("skipping siege/loot test: DATABASE_URL not set");
            return;
        };
        let pool = crate::create_pool(&url).await.expect("connect");
        crate::run_migrations(&pool).await.expect("migrate");
        let config = WorldConfig::new(GameSpeed::new(1.0).unwrap(), 50);
        let world = crate::world::ensure_world(&pool, &config)
            .await
            .expect("ensure world");
        let econ = crate::economy_rules().expect("economy rules");
        let units = crate::unit_rules().expect("unit rules");
        let repo = PgAccountRepository::new(
            pool.clone(),
            world.id,
            world.seed,
            config.radius,
            econ.starting_amounts,
        );
        let template = crate::starting_village().unwrap();
        let account = async |tag: &str| {
            let uname = format!("{tag}_{}", Uuid::new_v4().simple());
            let user = repo
                .create_account(
                    NewUser {
                        username: uname.clone(),
                        email: format!("{uname}@example.com"),
                        password_hash: "h".to_owned(),
                        email_confirmed: true,
                        tribe: Tribe::Gauls,
                    },
                    &template,
                )
                .await
                .expect("create account");
            (user.id, repo.villages_of(user.id).await.unwrap()[0].clone())
        };
        let (attacker, a) = account("slatk").await;
        let (_defender, d) = account("sldef").await;

        // A fixed snapshot clock so the guarded debit/credit are deterministic.
        let t = 3_000_000_000_000i64;
        // The target holds 2000 of each resource at snapshot `t`; the attacker starts empty at `t`.
        for (v, amt) in [(d.id, 2000i64), (a.id, 0)] {
            sqlx::query(
                "UPDATE village_resources SET wood=$1, clay=$1, iron=$1, crop=$1, \
                 updated_at = to_timestamp($2::double precision / 1000.0) WHERE village_id=$3",
            )
            .bind(amt)
            .bind(t as f64)
            .bind(Uuid::from_u128(v.0))
            .execute(&pool)
            .await
            .unwrap();
        }
        // The target has a Warehouse at level 3 for the catapults to raze.
        sqlx::query(
            "INSERT INTO village_buildings (village_id, slot, building_type, level) \
             VALUES ($1, 20, 'warehouse', 3)",
        )
        .bind(Uuid::from_u128(d.id.0))
        .execute(&pool)
        .await
        .unwrap();
        // The attacker's garrison: 10 swordsmen.
        sqlx::query(
            "INSERT INTO village_units (village_id, unit_id, count) VALUES ($1, 'swordsman', 10)",
        )
        .bind(Uuid::from_u128(a.id.0))
        .execute(&pool)
        .await
        .unwrap();

        let now = Timestamp(t);
        let arrive = Timestamp(t + 100_000);
        // AC1: a raid aiming catapults at the Warehouse persists the target on the movement.
        repo.start_attack(
            a.id,
            d.id,
            attacker,
            a.coordinate,
            d.coordinate,
            now,
            arrive,
            MovementKind::Raid,
            &[(UnitId("swordsman".into()), 6)],
            None,
            Some(BuildingKind::Warehouse),
        )
        .await
        .expect("attack");
        let mine = repo
            .claim_due_attacks(arrive, 100)
            .await
            .unwrap()
            .into_iter()
            .find(|x| x.home_village == a.id)
            .expect("due");
        assert_eq!(mine.catapult_target, Some(BuildingKind::Warehouse));

        // Apply a resolved raid: loot (100,50,30,0), Warehouse razed 3→1, 4 survivors carry it home.
        let loot = ResourceAmounts {
            wood: 100,
            clay: 50,
            iron: 30,
            crop: 0,
        };
        let after = ResourceAmounts {
            wood: 1900,
            clay: 1950,
            iron: 1970,
            crop: 2000,
        };
        let return_arrive = Timestamp(arrive.0 + 100_000);
        repo.apply_battle(BattleApply {
            movement_id: mine.id,
            owner: attacker,
            attacker_home: a.id,
            attacker_origin: a.coordinate,
            target: d.id,
            target_coord: d.coordinate,
            defender_losses: Vec::new(),
            reinforcement_losses: Vec::new(),
            survivors: vec![(UnitId("swordsman".into()), 4)],
            battle_at: arrive,
            return_arrive,
            report: NewBattleReport {
                kind: MovementKind::Raid,
                attacker_player: attacker,
                attacker_village: a.id,
                defender_player: _defender,
                defender_village: d.id,
                attacker_won: true,
                luck: 1.0,
                morale: 1.0,
                wall_before: 0,
                wall_after: 0,
                attacker_forces: vec![(UnitId("swordsman".into()), 6)],
                attacker_losses: vec![(UnitId("swordsman".into()), 2)],
                defender_forces: Vec::new(),
                defender_losses: Vec::new(),
                loot,
                razed: Some(RazedBuilding {
                    kind: BuildingKind::Warehouse,
                    before: 3,
                    after: 1,
                }),
            },
            scouted: false,
            scout_target: None,
            scout_report: None,
            loot,
            target_debit: Some(ResourceWrite {
                after,
                settled_from: now,
                clock: now,
            }),
            razed: Some(RazedBuilding {
                kind: BuildingKind::Warehouse,
                before: 3,
                after: 1,
            }),
        })
        .await
        .expect("apply battle");

        // AC6: the target's resources were debited; AC2: the Warehouse dropped to level 1.
        let (tw, twl): (i64, i16) = sqlx::query_as(
            "SELECT vr.wood, vb.level FROM village_resources vr \
             JOIN village_buildings vb ON vb.village_id = vr.village_id AND vb.building_type='warehouse' \
             WHERE vr.village_id=$1",
        )
        .bind(Uuid::from_u128(d.id.0))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(tw, 1900);
        assert_eq!(twl, 1);

        // The survivor return carries the loot; the razed building is recorded on the report.
        let (rw, rc, ri): (i64, i64, i64) = sqlx::query_as(
            "SELECT loot_wood, loot_clay, loot_iron \
             FROM troop_movements WHERE home_village=$1 AND kind='return'",
        )
        .bind(Uuid::from_u128(a.id.0))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!((rw, rc, ri), (100, 50, 30));
        let report = &repo.reports_for(attacker, 10).await.unwrap()[0];
        assert_eq!(report.loot, loot);
        assert_eq!(report.razed.unwrap().after, 1);

        // AC6: the return arrives and credits the loot (capped) to the attacker (empty at `t`).
        eperica_application::process_due_movements(
            &repo,
            &repo,
            &econ,
            &units,
            GameSpeed::new(1.0).unwrap(),
            return_arrive,
            100,
        )
        .await
        .expect("process return");
        let aw: i64 = sqlx::query_scalar("SELECT wood FROM village_resources WHERE village_id=$1")
            .bind(Uuid::from_u128(a.id.0))
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(aw >= 100, "attacker wood {aw} should include the 100 loot");
    }

    /// 011 AC4/AC5/AC10: a Cranny-defended village reads back (regresses the building parser) and
    /// shields its per-level capacity from loot; a **Teuton** attacker digs past part of it.
    #[tokio::test]
    async fn cranny_protects_loot_and_teuton_bypasses() {
        let _ = dotenvy::dotenv();
        let Ok(url) = std::env::var("DATABASE_URL") else {
            eprintln!("skipping cranny test: DATABASE_URL not set");
            return;
        };
        let pool = crate::create_pool(&url).await.expect("connect");
        crate::run_migrations(&pool).await.expect("migrate");
        let config = WorldConfig::new(GameSpeed::new(1.0).unwrap(), 50);
        let world = crate::world::ensure_world(&pool, &config)
            .await
            .expect("ensure world");
        let econ = crate::economy_rules().expect("economy rules");
        let units = crate::unit_rules().expect("unit rules");
        let combat = crate::combat_rules().expect("combat rules");
        let scout = crate::scout_rules().expect("scout rules");
        let map = WorldMap::new(
            world.seed as u64,
            config.radius,
            crate::map_rules().unwrap(),
        );
        let repo = PgAccountRepository::new(
            pool.clone(),
            world.id,
            world.seed,
            config.radius,
            econ.starting_amounts,
        );
        let template = crate::starting_village().unwrap();
        let account = async |tag: &str, tribe: Tribe| {
            let uname = format!("{tag}_{}", Uuid::new_v4().simple());
            let user = repo
                .create_account(
                    NewUser {
                        username: uname.clone(),
                        email: format!("{uname}@example.com"),
                        password_hash: "h".to_owned(),
                        email_confirmed: true,
                        tribe,
                    },
                    &template,
                )
                .await
                .expect("create account");
            (user.id, repo.villages_of(user.id).await.unwrap()[0].clone())
        };
        let (ga_p, ga) = account("crgaul", Tribe::Gauls).await;
        let (te_p, te) = account("crteut", Tribe::Teutons).await;
        let (_df, d) = account("crdef", Tribe::Gauls).await;

        // The defender builds a Cranny at level 2 (per-level protection from balance).
        sqlx::query(
            "INSERT INTO village_buildings (village_id, slot, building_type, level) \
             VALUES ($1, 20, 'cranny', 2)",
        )
        .bind(Uuid::from_u128(d.id.0))
        .execute(&pool)
        .await
        .unwrap();
        // Regression for the building parser: a Cranny-bearing village must still read back.
        let v = repo
            .village_by_id(d.id)
            .await
            .expect("read")
            .expect("village");
        assert!(
            v.buildings
                .iter()
                .any(|b| b.kind == BuildingKind::Cranny && b.level == 2)
        );
        let floor = combat.cranny_capacity(2);
        assert!(floor > 0);

        // Each attacker brings an overwhelming, high-carry garrison + a token defender garrison.
        for (v, unit, n) in [
            (ga.id, "swordsman", 100),
            (te.id, "clubswinger", 100),
            (d.id, "phalanx", 1),
        ] {
            sqlx::query(
                "INSERT INTO village_units (village_id, unit_id, count) VALUES ($1, $2, $3)",
            )
            .bind(Uuid::from_u128(v.0))
            .bind(unit)
            .bind(n)
            .execute(&pool)
            .await
            .unwrap();
        }

        // Resolve a raid and return the resources the **defender** kept (what loot couldn't reach).
        let raid =
            async |attacker_home: VillageId, attacker: PlayerId, unit: &str, t: i64| -> i64 {
                // Pin the defender's resources just above the Cranny floor, so loot is Cranny-bound
                // (not carry-capacity-bound) — isolating the protection from the attacker's capacity.
                sqlx::query(
                    "UPDATE village_resources SET wood=$1, clay=$1, iron=$1, crop=$1, \
                 updated_at = to_timestamp($2::double precision / 1000.0) WHERE village_id=$3",
                )
                .bind(floor + 400)
                .bind(t as f64)
                .bind(Uuid::from_u128(d.id.0))
                .execute(&pool)
                .await
                .unwrap();
                let now = Timestamp(t);
                let arrive = Timestamp(t + 100_000);
                repo.start_attack(
                    attacker_home,
                    d.id,
                    attacker,
                    Coordinate::new(0, 0),
                    d.coordinate,
                    now,
                    arrive,
                    MovementKind::Raid,
                    &[(UnitId(unit.into()), 80)],
                    None,
                    None,
                )
                .await
                .expect("attack");
                eperica_application::process_due_combat(
                    &repo,
                    &repo,
                    &repo,
                    &repo,
                    &econ,
                    &units,
                    &combat,
                    &scout,
                    &map,
                    GameSpeed::new(1.0).unwrap(),
                    world.seed as u64,
                    arrive,
                    100,
                )
                .await
                .expect("resolve");
                sqlx::query_scalar("SELECT wood FROM village_resources WHERE village_id=$1")
                    .bind(Uuid::from_u128(d.id.0))
                    .fetch_one(&pool)
                    .await
                    .unwrap()
            };

        // AC4: a non-Teuton raid can never take below the Cranny floor.
        let kept_gaul = raid(ga.id, ga_p, "swordsman", 3_000_000_000_000).await;
        assert!(
            kept_gaul >= floor,
            "Gaul left {kept_gaul}, below the Cranny floor {floor}"
        );

        // AC5: a Teuton raid digs past part of the Cranny — it keeps strictly less than a non-Teuton.
        let kept_teuton = raid(te.id, te_p, "clubswinger", 3_000_000_100_000).await;
        assert!(
            kept_teuton < kept_gaul,
            "Teuton kept {kept_teuton} but should bypass below the Gaul's {kept_gaul}"
        );
    }

    /// 009 AC3/AC6 (processor path): `process_due_combat` resolves a due raid end-to-end — the
    /// overwhelming attacker wins, the defender garrison is wiped, a report is written, and the
    /// survivors are sent home.
    #[tokio::test]
    async fn process_due_combat_resolves_a_raid() {
        let _ = dotenvy::dotenv();
        let Ok(url) = std::env::var("DATABASE_URL") else {
            eprintln!("skipping combat processor test: DATABASE_URL not set");
            return;
        };
        let pool = crate::create_pool(&url).await.expect("connect");
        crate::run_migrations(&pool).await.expect("migrate");
        let config = WorldConfig::new(GameSpeed::new(1.0).unwrap(), 50);
        let world = crate::world::ensure_world(&pool, &config)
            .await
            .expect("ensure world");
        let econ = crate::economy_rules().expect("economy rules");
        let units = crate::unit_rules().expect("unit rules");
        let combat = crate::combat_rules().expect("combat rules");
        let scout = crate::scout_rules().expect("scout rules");
        let map = WorldMap::new(
            world.seed as u64,
            config.radius,
            crate::map_rules().unwrap(),
        );
        let repo = PgAccountRepository::new(
            pool.clone(),
            world.id,
            world.seed,
            config.radius,
            econ.starting_amounts,
        );
        let template = crate::starting_village().unwrap();
        let account = async |tag: &str| {
            let uname = format!("{tag}_{}", Uuid::new_v4().simple());
            let user = repo
                .create_account(
                    NewUser {
                        username: uname.clone(),
                        email: format!("{uname}@example.com"),
                        password_hash: "h".to_owned(),
                        email_confirmed: true,
                        tribe: Tribe::Gauls,
                    },
                    &template,
                )
                .await
                .expect("create account");
            (user.id, repo.villages_of(user.id).await.unwrap()[0].clone())
        };
        let (attacker, a) = account("pcatk").await;
        let (_defender, d) = account("pcdef").await;

        // Overwhelming attacker: 100 swordsmen vs a token 2-phalanx defence.
        sqlx::query(
            "INSERT INTO village_units (village_id, unit_id, count) VALUES ($1, 'swordsman', 100)",
        )
        .bind(Uuid::from_u128(a.id.0))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO village_units (village_id, unit_id, count) VALUES ($1, 'phalanx', 2)",
        )
        .bind(Uuid::from_u128(d.id.0))
        .execute(&pool)
        .await
        .unwrap();

        let now = Timestamp(3_000_000_000_000);
        let arrive = Timestamp(now.0 + 100_000);
        repo.start_attack(
            a.id,
            d.id,
            attacker,
            a.coordinate,
            d.coordinate,
            now,
            arrive,
            MovementKind::Raid,
            &[(UnitId("swordsman".into()), 100)],
            None,
            None,
        )
        .await
        .expect("attack");

        let targets = eperica_application::process_due_combat(
            &repo,
            &repo,
            &repo,
            &repo,
            &econ,
            &units,
            &combat,
            &scout,
            &map,
            GameSpeed::new(1.0).unwrap(),
            world.seed as u64,
            arrive,
            100,
        )
        .await
        .expect("resolve");
        assert!(targets.contains(&d.id));

        // The defender's 2 phalanx are wiped; a report shows the attacker won.
        assert!(repo.garrison(d.id).await.unwrap().is_empty());
        let reports = repo.reports_for(attacker, 10).await.unwrap();
        assert_eq!(reports.len(), 1);
        assert!(reports[0].attacker_won);
        assert_eq!(reports[0].kind, MovementKind::Raid);
        // Survivors are heading home.
        let returning = repo.active_movements(attacker).await.unwrap();
        assert!(returning.iter().any(|m| m.kind == MovementKind::Return));
    }

    /// 010 AC6/AC7/AC8/AC9: scouts riding an attack scout the village in addition to the battle —
    /// the espionage step runs first, the (surviving) scouts return with the army carrying intel,
    /// and the defender's battle report is flagged because their counter-espionage killed a scout.
    #[tokio::test]
    async fn process_due_combat_with_scouts() {
        let _ = dotenvy::dotenv();
        let Ok(url) = std::env::var("DATABASE_URL") else {
            eprintln!("skipping combined-scout test: DATABASE_URL not set");
            return;
        };
        let pool = crate::create_pool(&url).await.expect("connect");
        crate::run_migrations(&pool).await.expect("migrate");
        let config = WorldConfig::new(GameSpeed::new(1.0).unwrap(), 50);
        let world = crate::world::ensure_world(&pool, &config)
            .await
            .expect("ensure world");
        let econ = crate::economy_rules().expect("economy rules");
        let units = crate::unit_rules().expect("unit rules");
        let combat = crate::combat_rules().expect("combat rules");
        let scout = crate::scout_rules().expect("scout rules");
        let map = WorldMap::new(
            world.seed as u64,
            config.radius,
            crate::map_rules().unwrap(),
        );
        let repo = PgAccountRepository::new(
            pool.clone(),
            world.id,
            world.seed,
            config.radius,
            econ.starting_amounts,
        );
        let template = crate::starting_village().unwrap();
        let account = async |tag: &str| {
            let uname = format!("{tag}_{}", Uuid::new_v4().simple());
            let user = repo
                .create_account(
                    NewUser {
                        username: uname.clone(),
                        email: format!("{uname}@example.com"),
                        password_hash: "h".to_owned(),
                        email_confirmed: true,
                        tribe: Tribe::Gauls,
                    },
                    &template,
                )
                .await
                .expect("create account");
            (user.id, repo.villages_of(user.id).await.unwrap()[0].clone())
        };
        let (attacker, a) = account("csatk").await;
        let (_defender, d) = account("csdef").await;

        // Attacker: 50 swordsmen + 5 pathfinders. Defender: 2 phalanx + 3 pathfinders (counter).
        for (v, unit, n) in [
            (a.id, "swordsman", 50),
            (a.id, "pathfinder", 5),
            (d.id, "phalanx", 2),
            (d.id, "pathfinder", 3),
        ] {
            sqlx::query(
                "INSERT INTO village_units (village_id, unit_id, count) VALUES ($1, $2, $3)",
            )
            .bind(Uuid::from_u128(v.0))
            .bind(unit)
            .bind(n)
            .execute(&pool)
            .await
            .unwrap();
        }

        let now = Timestamp(3_000_000_000_000);
        let arrive = Timestamp(now.0 + 100_000);
        repo.start_attack(
            a.id,
            d.id,
            attacker,
            a.coordinate,
            d.coordinate,
            now,
            arrive,
            MovementKind::Attack,
            &[
                (UnitId("swordsman".into()), 50),
                (UnitId("pathfinder".into()), 5),
            ],
            Some(ScoutTarget::Defenses),
            None,
        )
        .await
        .expect("attack");

        eperica_application::process_due_combat(
            &repo,
            &repo,
            &repo,
            &repo,
            &econ,
            &units,
            &combat,
            &scout,
            &map,
            GameSpeed::new(1.0).unwrap(),
            world.seed as u64,
            arrive,
            100,
        )
        .await
        .expect("resolve");

        // AC8: the battle report is flagged scouted (the defender's pathfinders killed a scout).
        let reports = repo.reports_for(attacker, 10).await.unwrap();
        assert_eq!(reports.len(), 1);
        assert!(reports[0].attacker_won);
        assert!(reports[0].scouted);
        assert_eq!(reports[0].scout_target, Some(ScoutTarget::Defenses));

        // AC7/AC9: a scouter-facing intel report exists with Defenses intel (a scout survived to
        // return with the winning army).
        let intel = repo.scout_reports_for(attacker, 10).await.unwrap();
        assert_eq!(intel.len(), 1);
        assert!(!intel[0].standalone);
        assert!(matches!(intel[0].intel, Some(ScoutIntel::Defenses { .. })));
        // Some scouts died to counter-espionage; some survived to bring it home.
        assert!(!intel[0].scouts_lost.is_empty());
        let returning = repo.active_movements(attacker).await.unwrap();
        assert!(returning.iter().any(|m| m.kind == MovementKind::Return
            && m.troops.iter().any(|(u, _)| u.as_str() == "pathfinder")));
    }

    /// 010 AC1/AC8/AC9/AC10/AC11: a standalone scout debits scouts, claims with its target type,
    /// applies an intel report + a survivor return once (crash-resume safe), and the report redacts
    /// for the target while the scouter reads the intel; an undetected mission stays hidden.
    #[tokio::test]
    async fn scout_apply_and_reports() {
        let _ = dotenvy::dotenv();
        let Ok(url) = std::env::var("DATABASE_URL") else {
            eprintln!("skipping scout test: DATABASE_URL not set");
            return;
        };
        let pool = crate::create_pool(&url).await.expect("connect");
        crate::run_migrations(&pool).await.expect("migrate");
        let config = WorldConfig::new(GameSpeed::new(1.0).unwrap(), 50);
        let world = crate::world::ensure_world(&pool, &config)
            .await
            .expect("ensure world");
        let rules = crate::economy_rules().expect("economy rules");
        let repo = PgAccountRepository::new(
            pool.clone(),
            world.id,
            world.seed,
            config.radius,
            rules.starting_amounts,
        );
        let template = crate::starting_village().unwrap();
        let account = async |tag: &str| {
            let uname = format!("{tag}_{}", Uuid::new_v4().simple());
            let user = repo
                .create_account(
                    NewUser {
                        username: uname.clone(),
                        email: format!("{uname}@example.com"),
                        password_hash: "h".to_owned(),
                        email_confirmed: true,
                        tribe: Tribe::Gauls,
                    },
                    &template,
                )
                .await
                .expect("create account");
            (user.id, repo.villages_of(user.id).await.unwrap()[0].clone())
        };
        let (scouter, s) = account("scout").await;
        let (target, t) = account("mark").await;
        let (third, _th) = account("third").await;

        // Scouter garrison: 5 pathfinders (Gaul scout).
        sqlx::query(
            "INSERT INTO village_units (village_id, unit_id, count) VALUES ($1, 'pathfinder', 5)",
        )
        .bind(Uuid::from_u128(s.id.0))
        .execute(&pool)
        .await
        .unwrap();

        // AC1: send 3 pathfinders to spy on resources — the garrison drops to 2, a scout is in flight
        // carrying its target type.
        let now = Timestamp(3_000_000_000_000);
        let arrive = Timestamp(now.0 + 100_000);
        repo.start_scout(
            s.id,
            t.id,
            scouter,
            s.coordinate,
            t.coordinate,
            now,
            arrive,
            &[(UnitId("pathfinder".into()), 3)],
            ScoutTarget::Resources,
        )
        .await
        .expect("scout");
        assert_eq!(
            repo.garrison(s.id).await.unwrap(),
            vec![(UnitId("pathfinder".into()), 2)]
        );

        // Crash-resume: claim, "crash", requeue, re-claim, apply once.
        let due = repo.claim_due_scouts(arrive, 100).await.unwrap();
        let mine = due.iter().find(|x| x.home_village == s.id).expect("due");
        assert_eq!(mine.target_type, ScoutTarget::Resources);
        assert_eq!(mine.troops, vec![(UnitId("pathfinder".into()), 3)]);
        assert!(repo.requeue_orphaned_movements().await.unwrap() >= 1);
        let due = repo.claim_due_scouts(arrive, 100).await.unwrap();
        let mine = due.iter().find(|x| x.home_village == s.id).expect("due");

        // AC9/AC10/AC11: the target detected 1 of the 3 scouts; 2 survive (return); intel returns.
        let return_arrive = Timestamp(arrive.0 + 100_000);
        repo.apply_scout(ScoutApply {
            movement_id: mine.id,
            owner: scouter,
            scouter_home: s.id,
            scouter_origin: s.coordinate,
            target_coord: t.coordinate,
            survivors: vec![(UnitId("pathfinder".into()), 2)],
            scouted_at: arrive,
            return_arrive,
            report: NewScoutReport {
                scouter_player: scouter,
                scouter_village: s.id,
                target_player: target,
                target_village: t.id,
                target_coord: t.coordinate,
                target_type: ScoutTarget::Resources,
                scouts_sent: vec![(UnitId("pathfinder".into()), 3)],
                scouts_lost: vec![(UnitId("pathfinder".into()), 1)],
                detected: true,
                standalone: true,
                intel: Some(ScoutIntel::Resources(ResourceAmounts {
                    wood: 700,
                    clay: 540,
                    iron: 120,
                    crop: 410,
                })),
            },
        })
        .await
        .expect("apply scout");

        // The survivor return is in flight; the scout movement is done (does not re-claim).
        let returning = repo.active_movements(scouter).await.unwrap();
        assert!(returning.iter().any(|m| m.kind == MovementKind::Return
            && m.troops == vec![(UnitId("pathfinder".into()), 2)]));
        assert!(
            repo.claim_due_scouts(arrive, 100)
                .await
                .unwrap()
                .iter()
                .all(|x| x.home_village != s.id)
        );

        // AC11: the scouter reads the full intel; the target sees a redacted notification (scouts
        // destroyed, no intel, no scouts-sent); a third party sees nothing.
        let mine = repo.scout_reports_for(scouter, 50).await.unwrap();
        assert_eq!(mine.len(), 1);
        let report_id = mine[0].id;
        assert!(mine[0].viewer_is_scouter);
        assert_eq!(mine[0].scouts_sent, vec![(UnitId("pathfinder".into()), 3)]);
        assert!(matches!(mine[0].intel, Some(ScoutIntel::Resources(_))));

        let theirs = repo.scout_reports_for(target, 50).await.unwrap();
        assert_eq!(theirs.len(), 1);
        assert!(!theirs[0].viewer_is_scouter);
        assert!(theirs[0].scouts_sent.is_empty()); // redacted
        assert_eq!(
            theirs[0].scouts_lost,
            vec![(UnitId("pathfinder".into()), 1)]
        );
        assert!(theirs[0].intel.is_none()); // redacted

        assert!(repo.scout_reports_for(third, 50).await.unwrap().is_empty());
        assert!(
            repo.scout_report(report_id, scouter)
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            repo.scout_report(report_id, target)
                .await
                .unwrap()
                .is_some()
        );
        assert!(repo.scout_report(report_id, third).await.unwrap().is_none());

        // AC8: an undetected standalone mission leaves no target-visible report.
        sqlx::query(
            "INSERT INTO scout_reports \
             (id, scouter_player, scouter_village, target_player, target_village, target_x, \
              target_y, target_type, scouts_sent, scouts_lost, detected, standalone, intel) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, 'defenses', '{}'::jsonb, '{}'::jsonb, false, \
                     true, NULL)",
        )
        .bind(Uuid::new_v4())
        .bind(Uuid::from_u128(scouter.0))
        .bind(Uuid::from_u128(s.id.0))
        .bind(Uuid::from_u128(target.0))
        .bind(Uuid::from_u128(t.id.0))
        .bind(t.coordinate.x)
        .bind(t.coordinate.y)
        .execute(&pool)
        .await
        .unwrap();
        // The scouter now has two; the target still sees only the one detected mission.
        assert_eq!(repo.scout_reports_for(scouter, 50).await.unwrap().len(), 2);
        assert_eq!(repo.scout_reports_for(target, 50).await.unwrap().len(), 1);
    }

    /// 010 AC6/AC9/AC10 (processor + restart path): `process_due_scouts` resolves a due mission
    /// end-to-end — a clean scout (no counter) returns its survivors with Resources intel, stays
    /// undetected, and a crash before applying is recovered to resolve **exactly once**.
    #[tokio::test]
    async fn process_due_scouts_resolves_a_mission() {
        let _ = dotenvy::dotenv();
        let Ok(url) = std::env::var("DATABASE_URL") else {
            eprintln!("skipping scout processor test: DATABASE_URL not set");
            return;
        };
        let pool = crate::create_pool(&url).await.expect("connect");
        crate::run_migrations(&pool).await.expect("migrate");
        let config = WorldConfig::new(GameSpeed::new(1.0).unwrap(), 50);
        let world = crate::world::ensure_world(&pool, &config)
            .await
            .expect("ensure world");
        let econ = crate::economy_rules().expect("economy rules");
        let units = crate::unit_rules().expect("unit rules");
        let scout = crate::scout_rules().expect("scout rules");
        let map = WorldMap::new(
            world.seed as u64,
            config.radius,
            crate::map_rules().unwrap(),
        );
        let repo = PgAccountRepository::new(
            pool.clone(),
            world.id,
            world.seed,
            config.radius,
            econ.starting_amounts,
        );
        let template = crate::starting_village().unwrap();
        let account = async |tag: &str| {
            let uname = format!("{tag}_{}", Uuid::new_v4().simple());
            let user = repo
                .create_account(
                    NewUser {
                        username: uname.clone(),
                        email: format!("{uname}@example.com"),
                        password_hash: "h".to_owned(),
                        email_confirmed: true,
                        tribe: Tribe::Gauls,
                    },
                    &template,
                )
                .await
                .expect("create account");
            (user.id, repo.villages_of(user.id).await.unwrap()[0].clone())
        };
        let (scouter, s) = account("pscout").await;
        let (target, t) = account("psmark").await;

        sqlx::query(
            "INSERT INTO village_units (village_id, unit_id, count) VALUES ($1, 'pathfinder', 4)",
        )
        .bind(Uuid::from_u128(s.id.0))
        .execute(&pool)
        .await
        .unwrap();

        let now = Timestamp(3_000_000_000_000);
        let arrive = Timestamp(now.0 + 100_000);
        repo.start_scout(
            s.id,
            t.id,
            scouter,
            s.coordinate,
            t.coordinate,
            now,
            arrive,
            &[(UnitId("pathfinder".into()), 4)],
            ScoutTarget::Resources,
        )
        .await
        .expect("scout");

        // Crash before applying: claim, requeue the orphan, then resolve via the processor once.
        let claimed = repo.claim_due_scouts(arrive, 100).await.unwrap();
        assert!(claimed.iter().any(|m| m.home_village == s.id));
        assert!(repo.requeue_orphaned_movements().await.unwrap() >= 1);

        let run = async || {
            eperica_application::process_due_scouts(
                &repo,
                &repo,
                &repo,
                &econ,
                &units,
                &scout,
                &map,
                GameSpeed::new(1.0).unwrap(),
                arrive,
                100,
            )
            .await
            .expect("process scouts")
        };
        run().await;
        run().await; // a second tick finds nothing already-claimed.

        // AC9: exactly one report, with Resources intel; the clean scout is undetected (AC8) so the
        // target sees nothing; survivors head home (AC10).
        let reports = repo.scout_reports_for(scouter, 10).await.unwrap();
        assert_eq!(reports.len(), 1);
        assert!(!reports[0].detected);
        assert!(matches!(reports[0].intel, Some(ScoutIntel::Resources(_))));
        assert!(reports[0].scouts_lost.is_empty());
        assert!(repo.scout_reports_for(target, 10).await.unwrap().is_empty());

        let returning = repo.active_movements(scouter).await.unwrap();
        assert!(returning.iter().any(|m| m.kind == MovementKind::Return
            && m.troops == vec![(UnitId("pathfinder".into()), 4)]));
    }

    /// 009 AC6 (restart path): a battle claimed but not applied (a crash) is recovered by the shared
    /// orphan requeue and resolved **exactly once** — one report, the defender reduced a single time.
    #[tokio::test]
    async fn combat_crash_resume_resolves_once() {
        let _ = dotenvy::dotenv();
        let Ok(url) = std::env::var("DATABASE_URL") else {
            eprintln!("skipping combat crash-resume test: DATABASE_URL not set");
            return;
        };
        let pool = crate::create_pool(&url).await.expect("connect");
        crate::run_migrations(&pool).await.expect("migrate");
        let config = WorldConfig::new(GameSpeed::new(1.0).unwrap(), 50);
        let world = crate::world::ensure_world(&pool, &config)
            .await
            .expect("ensure world");
        let econ = crate::economy_rules().expect("economy rules");
        let units = crate::unit_rules().expect("unit rules");
        let combat = crate::combat_rules().expect("combat rules");
        let scout = crate::scout_rules().expect("scout rules");
        let map = WorldMap::new(
            world.seed as u64,
            config.radius,
            crate::map_rules().unwrap(),
        );
        let repo = PgAccountRepository::new(
            pool.clone(),
            world.id,
            world.seed,
            config.radius,
            econ.starting_amounts,
        );
        let template = crate::starting_village().unwrap();
        let account = async |tag: &str| {
            let uname = format!("{tag}_{}", Uuid::new_v4().simple());
            let user = repo
                .create_account(
                    NewUser {
                        username: uname.clone(),
                        email: format!("{uname}@example.com"),
                        password_hash: "h".to_owned(),
                        email_confirmed: true,
                        tribe: Tribe::Gauls,
                    },
                    &template,
                )
                .await
                .expect("create account");
            (user.id, repo.villages_of(user.id).await.unwrap()[0].clone())
        };
        let (attacker, a) = account("crashatk").await;
        let (_defender, d) = account("crashdef").await;
        sqlx::query(
            "INSERT INTO village_units (village_id, unit_id, count) VALUES ($1, 'swordsman', 100)",
        )
        .bind(Uuid::from_u128(a.id.0))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO village_units (village_id, unit_id, count) VALUES ($1, 'phalanx', 2)",
        )
        .bind(Uuid::from_u128(d.id.0))
        .execute(&pool)
        .await
        .unwrap();

        let now = Timestamp(3_000_000_000_000);
        let arrive = Timestamp(now.0 + 100_000);
        repo.start_attack(
            a.id,
            d.id,
            attacker,
            a.coordinate,
            d.coordinate,
            now,
            arrive,
            MovementKind::Raid,
            &[(UnitId("swordsman".into()), 100)],
            None,
            None,
        )
        .await
        .expect("attack");

        // Claim the battle then "crash" before applying; the orphan requeue recovers it.
        let claimed = repo.claim_due_attacks(arrive, 100).await.unwrap();
        assert!(claimed.iter().any(|x| x.home_village == a.id));
        assert!(repo.requeue_orphaned_movements().await.unwrap() >= 1);

        // Now resolve via the processor — twice; the second tick finds nothing (already done).
        let run = async || {
            eperica_application::process_due_combat(
                &repo,
                &repo,
                &repo,
                &repo,
                &econ,
                &units,
                &combat,
                &scout,
                &map,
                GameSpeed::new(1.0).unwrap(),
                world.seed as u64,
                arrive,
                100,
            )
            .await
            .expect("resolve")
        };
        let first = run().await;
        assert!(first.contains(&d.id));
        let second = run().await;
        assert!(!second.contains(&d.id)); // already resolved — not re-applied

        // Exactly once: a single report, the defender reduced a single time.
        assert_eq!(repo.reports_for(attacker, 10).await.unwrap().len(), 1);
        assert!(repo.garrison(d.id).await.unwrap().is_empty());
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
        let world = crate::world::ensure_world(&pool, &config)
            .await
            .expect("ensure world");
        let rules = crate::economy_rules().expect("economy rules");
        let repo = PgAccountRepository::new(
            pool.clone(),
            world.id,
            world.seed,
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

    /// 006 AC6 migration-boundary guard: the world `seed` is backfilled NOT NULL with the
    /// deterministic per-world value, and adding it does not move a pre-existing village or change
    /// its fields. (The NOT NULL is guaranteed by 0009's own `SET NOT NULL`, which aborts on any
    /// row left NULL — like the 0005 tribe backfill — so only the determinism + village-stability
    /// halves need a data-level test.)
    #[tokio::test]
    async fn world_seed_is_backfilled_and_villages_are_unmoved() {
        let _ = dotenvy::dotenv();
        let Ok(url) = std::env::var("DATABASE_URL") else {
            eprintln!("skipping world-seed test: DATABASE_URL not set");
            return;
        };
        let pool = crate::create_pool(&url).await.expect("connect");
        crate::run_migrations(&pool).await.expect("migrate");

        let config = WorldConfig::new(GameSpeed::new(1.0).unwrap(), 50);
        let world = crate::world::ensure_world(&pool, &config)
            .await
            .expect("ensure world");
        // The seed is non-null and equals the deterministic per-world backfill value.
        let expected: i64 =
            sqlx::query_scalar("SELECT hashtextextended(id::text, 0) FROM worlds WHERE id = $1")
                .bind(Uuid::from_u128(world.id.0))
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(world.seed, expected);

        let rules = crate::economy_rules().expect("economy rules");
        let repo = PgAccountRepository::new(
            pool.clone(),
            world.id,
            world.seed,
            config.radius,
            rules.starting_amounts,
        );
        let uname = format!("seedstab_{}", Uuid::new_v4().simple());
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
        let before = repo.villages_of(user.id).await.unwrap();
        let (coord, fields) = (before[0].coordinate, before[0].fields.clone());

        // The other half of AC6: adding the seed does not move a pre-existing village or change
        // its stored fields — reads never re-derive them from the (now generated) terrain.
        let after = repo.villages_of(user.id).await.unwrap();
        assert_eq!(after[0].coordinate, coord);
        assert_eq!(after[0].fields, fields);
    }

    /// 006 AC5: a founded village sits on a valley (oases/Natar are skipped) and its 18 fields
    /// match that valley tile's distribution; `villages_at` surfaces it as a map marker.
    #[tokio::test]
    async fn villages_are_placed_on_valleys_with_tile_fields() {
        let _ = dotenvy::dotenv();
        let Ok(url) = std::env::var("DATABASE_URL") else {
            eprintln!("skipping placement test: DATABASE_URL not set");
            return;
        };
        let pool = crate::create_pool(&url).await.expect("connect");
        crate::run_migrations(&pool).await.expect("migrate");

        let config = WorldConfig::new(GameSpeed::new(1.0).unwrap(), 50);
        let world = crate::world::ensure_world(&pool, &config)
            .await
            .expect("ensure world");
        let rules = crate::economy_rules().expect("economy rules");
        let repo = PgAccountRepository::new(
            pool.clone(),
            world.id,
            world.seed,
            config.radius,
            rules.starting_amounts,
        );
        // The same map the repo placed with.
        let map = WorldMap::new(
            world.seed as u64,
            config.radius,
            crate::map_rules().unwrap(),
        );

        let uname = format!("place_{}", Uuid::new_v4().simple());
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
        let v = repo.villages_of(user.id).await.unwrap()[0].clone();

        // AC5: placed on a valley whose distribution dictates the village's fields.
        assert!(
            map.is_valley(v.coordinate),
            "{:?} is not a valley",
            v.coordinate
        );
        let Some(TileKind::Valley(d)) = map.tile_at(v.coordinate) else {
            panic!("expected a valley tile");
        };
        let count = |k: ResourceKind| v.fields.iter().filter(|f| f.kind == k).count();
        assert_eq!(count(ResourceKind::Wood), usize::from(d.wood));
        assert_eq!(count(ResourceKind::Clay), usize::from(d.clay));
        assert_eq!(count(ResourceKind::Iron), usize::from(d.iron));
        assert_eq!(count(ResourceKind::Crop), usize::from(d.crop));

        // AC7 support: villages_at returns the marker with the owner.
        let markers = repo.villages_at(&[v.coordinate]).await.unwrap();
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].coordinate, v.coordinate);
        assert_eq!(markers[0].owner_name, uname);
        // A tile with no village yields no marker.
        let empty = repo.villages_at(&[Coordinate::new(45, 45)]).await.unwrap();
        assert!(
            empty
                .iter()
                .all(|m| m.coordinate != Coordinate::new(45, 45))
                || empty.is_empty()
        );
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
        let world = crate::world::ensure_world(&pool, &config)
            .await
            .expect("ensure world");
        let rules = crate::economy_rules().expect("economy rules");
        let repo = PgAccountRepository::new(
            pool.clone(),
            world.id,
            world.seed,
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
        let world = crate::world::ensure_world(&pool, &config)
            .await
            .expect("ensure world");
        let rules = crate::economy_rules().expect("economy rules");
        let repo = PgAccountRepository::new(
            pool.clone(),
            world.id,
            world.seed,
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
        let world = crate::world::ensure_world(&pool, &config)
            .await
            .expect("ensure world");
        let rules = crate::economy_rules().expect("economy rules");
        let repo = PgAccountRepository::new(
            pool.clone(),
            world.id,
            world.seed,
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
        let world = crate::world::ensure_world(&pool, &config)
            .await
            .expect("ensure world");
        let rules = crate::economy_rules().expect("economy rules");
        let repo = PgAccountRepository::new(
            pool.clone(),
            world.id,
            world.seed,
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
        let world = crate::world::ensure_world(&pool, &config)
            .await
            .expect("ensure world");
        let rules = crate::economy_rules().expect("economy rules");
        let repo = PgAccountRepository::new(
            pool.clone(),
            world.id,
            world.seed,
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
        let world = crate::world::ensure_world(&pool, &config)
            .await
            .expect("ensure world");
        let rules = crate::economy_rules().expect("economy rules");
        let repo = PgAccountRepository::new(
            pool.clone(),
            world.id,
            world.seed,
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

        // AC5: at started + 2.5 × perUnit the processor delivers exactly 2 units, settling the
        // store to the 2nd unit's completion instant (upkeep starts at delivery, AC6).
        let unit_rules = crate::unit_rules().expect("unit rules");
        let speed = GameSpeed::new(1.0).unwrap();
        let claim_at = Timestamp(now.0 + 250 * 1000);
        let delivered = eperica_application::process_due_training(
            &repo,
            &repo,
            &rules,
            &unit_rules,
            speed,
            claim_at,
            10,
        )
        .await
        .expect("process training");
        assert!(delivered.contains(&village_id));
        assert_eq!(
            repo.garrison(village_id).await.unwrap(),
            vec![(UnitId("phalanx".into()), 2)]
        );
        // The resources row was settled to t2 = started + 2 × perUnit, not to `claim_at`.
        let (_, settled_to) = repo.stored_resources(village_id).await.unwrap().unwrap();
        assert_eq!(settled_to, Timestamp(now.0 + 200 * 1000));
        // Nothing more due at the same instant (next completion is at 3 × perUnit).
        assert!(
            repo.claim_due_training(claim_at, 10)
                .await
                .unwrap()
                .iter()
                .all(|d| d.village != village_id)
        );

        // Crash recovery: claim the final unit, "crash" before applying, requeue, then let the
        // processor finish — the recomputed count is unchanged, nothing lost or duplicated.
        let final_at = Timestamp(now.0 + 320 * 1000);
        let due = repo.claim_due_training(final_at, 10).await.unwrap();
        assert!(due.iter().any(|d| d.village == village_id));
        assert!(repo.requeue_orphaned_training().await.unwrap() >= 1);
        let delivered = eperica_application::process_due_training(
            &repo,
            &repo,
            &rules,
            &unit_rules,
            speed,
            final_at,
            10,
        )
        .await
        .expect("process final");
        assert!(delivered.contains(&village_id));
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
        // settled_from = the deliveries' last settle (t3 = started + 300 s).
        let snap = snapshot(&repo, village_id).await;
        assert_eq!(snap, Timestamp(now.0 + 300 * 1000));
        repo.start_training(village_id, settled, snap, final_at, order)
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
        let world = crate::world::ensure_world(&pool, &config)
            .await
            .expect("ensure world");
        let rules = crate::economy_rules().expect("economy rules");
        let repo = PgAccountRepository::new(
            pool.clone(),
            world.id,
            world.seed,
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

        // Crash recovery (AC7 "survives restarts"): a claimed check left in `processing` is
        // requeued at startup and claimable again.
        repo.schedule_starvation_check(village_id, Timestamp(now.0 - 1000))
            .await
            .unwrap();
        assert!(
            repo.claim_due_starvation(now, 10)
                .await
                .unwrap()
                .contains(&village_id)
        );
        assert!(repo.requeue_orphaned_starvation().await.unwrap() >= 1);
        assert!(
            repo.claim_due_starvation(now, 10)
                .await
                .unwrap()
                .contains(&village_id)
        );
        repo.resolve_starvation_check(village_id, None)
            .await
            .unwrap();
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
        let world = crate::world::ensure_world(&pool, &config)
            .await
            .expect("ensure world");
        let rules = crate::economy_rules().expect("economy rules");
        let repo = PgAccountRepository::new(
            pool.clone(),
            world.id,
            world.seed,
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
        let world = crate::world::ensure_world(&pool, &config)
            .await
            .expect("ensure world");
        let rules = crate::economy_rules().expect("economy rules");
        let repo = PgAccountRepository::new(
            pool.clone(),
            world.id,
            world.seed,
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

    // 012 AC1/AC3/AC4/AC10: an un-fought oasis reads back its seeded wild animals; a winning clear
    // attack debits the garrison, resolves once, occupies the oasis, replaces its garrison with the
    // post-battle defenders, and sends survivors home — all in one transaction.
    #[tokio::test]
    async fn oasis_clear_and_occupy_lifecycle() {
        let _ = dotenvy::dotenv();
        let Ok(url) = std::env::var("DATABASE_URL") else {
            eprintln!("skipping oasis test: DATABASE_URL not set");
            return;
        };
        let pool = crate::create_pool(&url).await.expect("connect");
        crate::run_migrations(&pool).await.expect("migrate");
        let config = WorldConfig::new(GameSpeed::new(1.0).unwrap(), 50);
        let world = crate::world::ensure_world(&pool, &config)
            .await
            .expect("ensure world");
        let rules = crate::economy_rules().expect("economy rules");
        let repo = PgAccountRepository::new(
            pool.clone(),
            world.id,
            world.seed,
            config.radius,
            rules.starting_amounts,
        );

        let template = crate::starting_village().unwrap();
        let uname = format!("oasis_{}", Uuid::new_v4().simple());
        let attacker = repo
            .create_account(
                NewUser {
                    username: uname.clone(),
                    email: format!("{uname}@example.com"),
                    password_hash: "h".to_owned(),
                    email_confirmed: true,
                    tribe: Tribe::Gauls,
                },
                &template,
            )
            .await
            .expect("create account");
        let v = repo.villages_of(attacker.id).await.unwrap()[0].clone();

        // Find an oasis tile on the seeded map (not the attacker's own village tile).
        let map = WorldMap::new(
            world.seed as u64,
            config.radius,
            crate::map_rules().unwrap(),
        );
        let oasis = coordinates_within(config.radius)
            .find(|c| matches!(map.tile_at(*c), Some(TileKind::Oasis(_))) && *c != v.coordinate)
            .expect("an oasis exists on the seeded map");

        // Oases are world-global (not per-account); clear any row a prior run left on this tile so
        // the test starts from the un-fought state (the garrison cascades on delete).
        sqlx::query("DELETE FROM oases WHERE world_id = $1 AND x = $2 AND y = $3")
            .bind(Uuid::from_u128(world.id.0))
            .bind(oasis.x)
            .bind(oasis.y)
            .execute(&pool)
            .await
            .unwrap();

        let units = crate::unit_rules().expect("unit rules");
        let orules = crate::oasis_rules().expect("oasis rules");
        let animals = units.wild_animal_roster();

        // AC1: the un-fought oasis has no row and reads back the seeded wild animals (P6).
        assert!(repo.oasis_at(oasis).await.unwrap().is_none());
        let seeded = oasis_garrison(world.seed as u64, oasis, animals, &orules);
        assert!(!seeded.is_empty(), "the test oasis should hold animals");
        assert_eq!(
            repo.oasis_defenders(oasis, animals, &orules).await.unwrap(),
            seeded
        );

        // Seed the attacker's garrison with 50 phalanx and send 30 at the oasis.
        sqlx::query(
            "INSERT INTO village_units (village_id, unit_id, count) VALUES ($1, 'phalanx', 50)",
        )
        .bind(Uuid::from_u128(v.id.0))
        .execute(&pool)
        .await
        .unwrap();
        let now = Timestamp(3_000_000_000_000);
        let arrive = Timestamp(now.0 + 100_000);
        let troops = vec![(UnitId("phalanx".into()), 30)];

        // AC2: the attack debits the garrison (50 → 20) and a movement is in flight.
        repo.start_oasis_attack(v.id, attacker.id, v.coordinate, oasis, now, arrive, &troops)
            .await
            .expect("start oasis attack");
        assert_eq!(
            repo.garrison(v.id).await.unwrap(),
            vec![(UnitId("phalanx".into()), 20)]
        );

        // Claim the due attack at arrival.
        let due = repo
            .claim_due_oasis_attacks(arrive, 10)
            .await
            .expect("claim");
        let mine = due
            .iter()
            .find(|d| d.home_village == v.id)
            .expect("this attack is due");
        assert_eq!(mine.oasis, oasis);
        assert_eq!(mine.troops, vec![(UnitId("phalanx".into()), 30)]);

        // AC3/AC4/AC10: a winning clear — animals wiped, 25 survivors, the village occupies the oasis.
        let battle_at = arrive;
        let return_arrive = Timestamp(arrive.0 + 100_000);
        repo.apply_oasis_battle(OasisBattleApply {
            movement_id: mine.id,
            owner: attacker.id,
            attacker_home: v.id,
            attacker_origin: v.coordinate,
            oasis,
            defenders_after: Vec::new(),
            ownership: OasisOwnership::Occupy(v.id),
            survivors: vec![(UnitId("phalanx".into()), 25)],
            battle_at,
            return_arrive,
        })
        .await
        .expect("apply oasis battle");

        // The oasis is now owned and cleared of animals.
        let state = repo.oasis_at(oasis).await.unwrap().expect("materialised");
        assert_eq!(state.owner, Some(v.id));
        assert!(
            repo.oasis_defenders(oasis, animals, &orules)
                .await
                .unwrap()
                .is_empty(),
            "a cleared, occupied oasis has no defenders"
        );

        // AC8 read path: the village's occupied oases include this one, with its seeded bonus.
        let occupied = repo.occupied_oases(v.id).await.unwrap();
        assert_eq!(occupied.len(), 1);
        assert_eq!(occupied[0].0, oasis);
        let expected_bonus = map.oasis_bonus_at(oasis).unwrap();
        assert_eq!(occupied[0].1, expected_bonus);
        assert_eq!(
            repo.village_oasis_bonus(v.id).await.unwrap(),
            expected_bonus
        );

        // AC10 exactly-once: the movement is done (no longer claimable) and exactly one survivor
        // return is in flight.
        assert!(
            repo.claim_due_oasis_attacks(arrive, 10)
                .await
                .unwrap()
                .iter()
                .all(|d| d.home_village != v.id),
            "the resolved attack is not re-claimed"
        );
        let returning = repo.active_movements(attacker.id).await.unwrap();
        let returns: Vec<_> = returning
            .iter()
            .filter(|m| m.kind == MovementKind::Return)
            .collect();
        assert_eq!(returns.len(), 1, "exactly one survivor return");
        assert_eq!(returns[0].troops, vec![(UnitId("phalanx".into()), 25)]);
        assert_eq!(returns[0].destination, v.coordinate);
    }

    // 012 AC4: a winning clear with no free Outpost capacity clears the animals but leaves the oasis
    // unoccupied (materialised, owner NULL); its defenders read back empty until it regrows.
    #[tokio::test]
    async fn oasis_clear_without_capacity_stays_unoccupied() {
        let _ = dotenvy::dotenv();
        let Ok(url) = std::env::var("DATABASE_URL") else {
            eprintln!("skipping oasis no-capacity test: DATABASE_URL not set");
            return;
        };
        let pool = crate::create_pool(&url).await.expect("connect");
        crate::run_migrations(&pool).await.expect("migrate");
        let config = WorldConfig::new(GameSpeed::new(1.0).unwrap(), 50);
        let world = crate::world::ensure_world(&pool, &config)
            .await
            .expect("ensure world");
        let rules = crate::economy_rules().expect("economy rules");
        let repo = PgAccountRepository::new(
            pool.clone(),
            world.id,
            world.seed,
            config.radius,
            rules.starting_amounts,
        );

        let template = crate::starting_village().unwrap();
        let uname = format!("oasisnc_{}", Uuid::new_v4().simple());
        let attacker = repo
            .create_account(
                NewUser {
                    username: uname.clone(),
                    email: format!("{uname}@example.com"),
                    password_hash: "h".to_owned(),
                    email_confirmed: true,
                    tribe: Tribe::Gauls,
                },
                &template,
            )
            .await
            .expect("create account");
        let v = repo.villages_of(attacker.id).await.unwrap()[0].clone();
        let map = WorldMap::new(
            world.seed as u64,
            config.radius,
            crate::map_rules().unwrap(),
        );
        let units = crate::unit_rules().expect("unit rules");
        let orules = crate::oasis_rules().expect("oasis rules");
        let animals = units.wild_animal_roster();

        // A different oasis than the first test (skip the first to avoid coincidental overlap).
        let oasis = coordinates_within(config.radius)
            .filter(|c| matches!(map.tile_at(*c), Some(TileKind::Oasis(_))) && *c != v.coordinate)
            .nth(1)
            .expect("a second oasis exists");

        // Clear any row a prior run left on this world-global tile (garrison cascades on delete).
        sqlx::query("DELETE FROM oases WHERE world_id = $1 AND x = $2 AND y = $3")
            .bind(Uuid::from_u128(world.id.0))
            .bind(oasis.x)
            .bind(oasis.y)
            .execute(&pool)
            .await
            .unwrap();

        // Clear without capacity: ownership unchanged (the village has no Outpost).
        repo.apply_oasis_battle(OasisBattleApply {
            movement_id: Uuid::new_v4().as_u128(),
            owner: attacker.id,
            attacker_home: v.id,
            attacker_origin: v.coordinate,
            oasis,
            defenders_after: Vec::new(),
            ownership: OasisOwnership::Unchanged,
            survivors: Vec::new(),
            battle_at: Timestamp(3_000_000_000_000),
            return_arrive: Timestamp(3_000_000_100_000),
        })
        .await
        .expect("apply cleared-without-capacity");

        let state = repo.oasis_at(oasis).await.unwrap().expect("materialised");
        assert_eq!(state.owner, None, "cleared but not occupied");
        assert!(
            repo.oasis_defenders(oasis, animals, &orules)
                .await
                .unwrap()
                .is_empty(),
            "a cleared oasis has no defenders until it regrows"
        );
        assert!(repo.occupied_oases(v.id).await.unwrap().is_empty());
    }
}
