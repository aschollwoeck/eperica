//! PostgreSQL adapter for the application's [`AccountRepository`] port.

use async_trait::async_trait;
use eperica_application::{
    AccountRepository, AchievementRepository, ActiveBuild, ActiveTraining, ActiveUnitOrder,
    AdminAccount, AdminOverview, AdminRepository, AdminWorld, AllianceHit, AllianceLeaderboardRow,
    AllianceRepository, AllianceStats, AlliedVillage, ArtifactRepository, BattleApply,
    BattleReportView, BoardScope, BuildRepository, CombatRepository, CommsRepository,
    ConflictMetric, ConquestRepository, ConversationSummary, CultureRepository, DefenderReport,
    DiplomacyEntry, DueAttack, DueBuild, DueMovement, DueOasisAttack, DueOasisRegrow,
    DueOasisReinforce, DueScout, DueSettle, DueTrade, DueTraining, DueUnitOrder, ForumPost,
    HeldArtifact, IncomingAttack, LeaderboardRow, LifecycleRepository, LoyaltyApply, MedalAward,
    MedalRepository, MedalSubjectKind, MedalView, Membership, MessageView, ModerationRepository,
    MovementRepository, MovementView, NewBuildOrder, NewNotification, NewOasisReport,
    NewScoutReport, NewTrainingOrder, NewUnitOrder, NewUser, NotificationRepository,
    NotificationView, OasisBattleApply, OasisOwnership, OasisReinforceOutcome, OasisRepository,
    OasisState, OutgoingInvite, PendingInvite, PlayerHit, PlayerStats, PlayerWorld, ProfileView,
    QuestRepository, RankingRepository, RazedBuilding, RepoError, ReportView, ResourceWrite,
    RosterEntry, ScoutApply, ScoutIntel, ScoutReportView, ScoutRepository, SettleApply,
    SettleOutcome, SettleRepository, SitterActionView, StarvationRepository, StationedGroup,
    ThreadHead, ThreadSummary, TradeRepository, TradeView, TrainingRepository, UnitOrderKind,
    UnitRepository, UserRecord, VillageMarker, WonderOutcome, WonderRepository, WonderStanding,
};
use eperica_domain::{
    AchievementDef, AchievementId, AllianceId, AllianceRole, ArtifactDef, ArtifactEffects,
    ArtifactId, ArtifactKind, ArtifactScope, BuildTarget, BuildingKind, BuildingSlot, Coordinate,
    DiplomacyStance, DiplomacyStatus, EconomyRules, GameSpeed, MedalCategory, MovementKind,
    NotificationKind, OasisBonus, OasisRules, PlayerId, PlayerProgress, Quadrant, QuestDef,
    QuestId, QuestProgress, QueueLane, ReportReason, ResourceAmounts, ResourceField, ResourceKind,
    Reward, RightSet, SanctionKind, ScoutTarget, StartingVillage, TileKind, Timestamp, TradeKind,
    Tribe, UnitCounts, UnitId, UnitSpec, Village, VillageId, WorldConfig, WorldId, WorldMap,
    aggregate_effects, capacities, coordinates_within, deposit_capped, oasis_garrison,
    protection_expiry,
};
use sqlx::{Acquire, PgPool, Row, postgres::PgRow};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// SQLx-backed account repository bound to a single world.
#[derive(Debug, Clone)]
pub struct PgAccountRepository {
    pool: PgPool,
    world_id: WorldId,
    map: WorldMap,
    starting_amounts: ResourceAmounts,
    /// Beginner's-protection window (base seconds, 019) granted at spawn; speed-scaled with `speed`.
    protection_window_secs: i64,
    /// World speed — scales the protection window (P7).
    speed: GameSpeed,
}

impl PgAccountRepository {
    /// Create a repository for `world_id`. The world's `seed` + `radius` (with the embedded map
    /// balance) drive the generated map used for village placement (006); `starting_amounts` are
    /// seeded into each new village's resources. `protection_window_secs` + `speed` set the
    /// beginner's-protection window granted at spawn (019, speed-scaled, P7).
    pub fn new(
        pool: PgPool,
        world_id: WorldId,
        seed: i64,
        radius: u32,
        starting_amounts: ResourceAmounts,
        protection_window_secs: i64,
        speed: GameSpeed,
    ) -> Self {
        let rules = crate::balance::map_rules().expect("embedded map balance is valid");
        Self {
            pool,
            world_id,
            map: WorldMap::new(seed as u64, radius, rules),
            starting_amounts,
            protection_window_secs,
            speed,
        }
    }

    /// This repository's world id (e.g. for the `eperica-perf` scale tool to seed/measure, 023).
    pub fn world_id(&self) -> WorldId {
        self.world_id
    }

    /// Place a starting village for `owner` (a player) in **this repo's world** within `tx`, on the first
    /// free valley in the deterministic ring order, then seed the culture accumulator. Shared by
    /// registration ([`create_account`]) and joining another world (042). Each placement attempt is a
    /// SAVEPOINT so a coordinate clash rolls back just that insert.
    async fn place_starting_village(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        owner: Uuid,
        tribe: Tribe,
        template: &StartingVillage,
    ) -> Result<(), RepoError> {
        let world_uuid = Uuid::from_u128(self.world_id.0);
        let mut placed = false;
        for coord in coordinates_within(self.map.radius()) {
            let Some(TileKind::Valley(distribution)) = self.map.tile_at(coord) else {
                continue;
            };
            let village_uuid = Uuid::new_v4();
            let village = Village::found(
                VillageId(village_uuid.as_u128()),
                PlayerId(owner.as_u128()),
                coord,
                tribe,
                distribution,
                template,
            );

            let mut sp = (&mut *tx).begin().await.map_err(backend)?;
            let insert_village = sqlx::query(
                "INSERT INTO villages (id, world_id, owner_id, x, y, tribe) \
                 VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(village_uuid)
            .bind(world_uuid)
            .bind(owner)
            .bind(coord.x)
            .bind(coord.y)
            .bind(tribe.slug())
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

        // Seed the per-player culture accumulator (013): value 0, anchored now; the rate accrues live
        // from this village's Town Hall (none yet) on read.
        sqlx::query(
            "INSERT INTO player_culture (player_id, value, updated_at) VALUES ($1, 0, now())",
        )
        .bind(owner)
        .execute(&mut **tx)
        .await
        .map_err(backend)?;
        Ok(())
    }

    /// Create a **player** for an existing account in this repo's world (042): a fresh player id + a
    /// starting village placed on this world's map. The home world's player reuses the user id (037); a
    /// second world's player gets a fresh id. Returns the new player id.
    ///
    /// # Errors
    /// [`RepoError::Duplicate`] if the account already has a player in this world; [`RepoError::WorldFull`]
    /// if no free valley remains; otherwise a backend error.
    pub async fn create_player_in_world(
        &self,
        user: PlayerId,
        tribe: Tribe,
        template: &StartingVillage,
    ) -> Result<PlayerId, RepoError> {
        let mut tx = self.pool.begin().await.map_err(backend)?;
        let player_uuid = Uuid::new_v4();
        let insert_player = sqlx::query(
            "INSERT INTO players (id, user_id, world_id, tribe) VALUES ($1, $2, $3, $4)",
        )
        .bind(player_uuid)
        .bind(Uuid::from_u128(user.0))
        .bind(Uuid::from_u128(self.world_id.0))
        .bind(tribe.slug())
        .execute(&mut *tx)
        .await;
        if let Err(e) = insert_player {
            return Err(if is_unique_violation(&e) {
                RepoError::Duplicate // already joined this world
            } else {
                backend(e)
            });
        }
        self.place_starting_village(&mut tx, player_uuid, tribe, template)
            .await?;
        tx.commit().await.map_err(backend)?;
        Ok(PlayerId(player_uuid.as_u128()))
    }

    /// The artifact effects in force for a village (020 AC6), folded into its read like the oasis bonus:
    /// the village's own **small** holdings plus the account's **large/unique**. `NONE` for a Natar/NPC
    /// village (it never benefits from the artifacts it guards) or when the owner holds none.
    async fn artifact_effects_for(
        &self,
        owner: PlayerId,
        village: VillageId,
        is_natar: bool,
    ) -> Result<ArtifactEffects, RepoError> {
        if is_natar {
            return Ok(ArtifactEffects::NONE);
        }
        let held = self.held_by_player(owner).await?;
        Ok(artifact_effects_from(&held, village, is_natar))
    }

    /// Reset build orders stuck in `processing` (e.g. left by a crash) back to `pending` so they are
    /// reprocessed. `apply_build` is idempotent (it sets an absolute level), so this is safe.
    pub async fn requeue_orphaned_builds(&self) -> Result<u64, RepoError> {
        let result = sqlx::query(
            "UPDATE build_orders SET status = 'pending' WHERE status = 'processing' \
             AND village_id IN (SELECT id FROM villages WHERE world_id = $1)",
        )
        .bind(Uuid::from_u128(self.world_id.0))
        .execute(&self.pool)
        .await
        .map_err(backend)?;
        Ok(result.rows_affected())
    }

    /// Reset unit orders stuck in `processing` back to `pending` (crash recovery).
    /// `apply_unit_order` is idempotent, so reprocessing is safe.
    pub async fn requeue_orphaned_unit_orders(&self) -> Result<u64, RepoError> {
        let result = sqlx::query(
            "UPDATE unit_orders SET status = 'pending' WHERE status = 'processing' \
             AND village_id IN (SELECT id FROM villages WHERE world_id = $1)",
        )
        .bind(Uuid::from_u128(self.world_id.0))
        .execute(&self.pool)
        .await
        .map_err(backend)?;
        Ok(result.rows_affected())
    }

    /// Reset training batches stuck in `processing` back to `active` (crash recovery). Safe:
    /// `apply_training` moves garrison and progress in one transaction, so a re-claim recomputes
    /// completions from the unchanged `count_done` (AC5).
    pub async fn requeue_orphaned_training(&self) -> Result<u64, RepoError> {
        let result = sqlx::query(
            "UPDATE training_orders SET status = 'active' WHERE status = 'processing' \
             AND village_id IN (SELECT id FROM villages WHERE world_id = $1)",
        )
        .bind(Uuid::from_u128(self.world_id.0))
        .execute(&self.pool)
        .await
        .map_err(backend)?;
        Ok(result.rows_affected())
    }

    /// Reset starvation checks stuck in `processing` back to `pending` (crash recovery). Safe:
    /// the handler re-validates from live state at fire time (AC7).
    pub async fn requeue_orphaned_starvation(&self) -> Result<u64, RepoError> {
        let result = sqlx::query(
            "UPDATE starvation_checks SET status = 'pending' WHERE status = 'processing' \
             AND village_id IN (SELECT id FROM villages WHERE world_id = $1)",
        )
        .bind(Uuid::from_u128(self.world_id.0))
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
            "UPDATE troop_movements SET status = 'in_transit' WHERE status = 'processing' \
             AND home_village IN (SELECT id FROM villages WHERE world_id = $1)",
        )
        .bind(Uuid::from_u128(self.world_id.0))
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
            "UPDATE trade_movements SET status = 'in_transit' WHERE status = 'processing' \
             AND home_village IN (SELECT id FROM villages WHERE world_id = $1)",
        )
        .bind(Uuid::from_u128(self.world_id.0))
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

fn alliance_role_str(role: AllianceRole) -> &'static str {
    match role {
        AllianceRole::Founder => "founder",
        AllianceRole::Leader => "leader",
        AllianceRole::Member => "member",
    }
}

fn parse_alliance_role(s: &str) -> Result<AllianceRole, RepoError> {
    match s {
        "founder" => Ok(AllianceRole::Founder),
        "leader" => Ok(AllianceRole::Leader),
        "member" => Ok(AllianceRole::Member),
        other => Err(RepoError::Backend(format!(
            "unknown alliance role: {other}"
        ))),
    }
}

fn stance_str(s: DiplomacyStance) -> &'static str {
    match s {
        DiplomacyStance::War => "war",
        DiplomacyStance::Confederation => "confederation",
    }
}

fn parse_stance(s: &str) -> Result<DiplomacyStance, RepoError> {
    match s {
        "war" => Ok(DiplomacyStance::War),
        "confederation" => Ok(DiplomacyStance::Confederation),
        other => Err(RepoError::Backend(format!("unknown stance: {other}"))),
    }
}

fn status_str(s: DiplomacyStatus) -> &'static str {
    match s {
        DiplomacyStatus::Proposed => "proposed",
        DiplomacyStatus::Active => "active",
    }
}

fn parse_status(s: &str) -> Result<DiplomacyStatus, RepoError> {
    match s {
        "proposed" => Ok(DiplomacyStatus::Proposed),
        "active" => Ok(DiplomacyStatus::Active),
        other => Err(RepoError::Backend(format!(
            "unknown diplomacy status: {other}"
        ))),
    }
}

/// Normalise an alliance pair to `(lo, hi)` with `lo < hi` (matching the DB CHECK + composite PK), so
/// the pair has a single canonical row regardless of argument order.
fn normalise_pair(a: AllianceId, b: AllianceId) -> (Uuid, Uuid) {
    if a.0 <= b.0 {
        (Uuid::from_u128(a.0), Uuid::from_u128(b.0))
    } else {
        (Uuid::from_u128(b.0), Uuid::from_u128(a.0))
    }
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
        BuildingKind::Embassy => "embassy",
        BuildingKind::Wall => "wall",
        BuildingKind::Barracks => "barracks",
        BuildingKind::Academy => "academy",
        BuildingKind::Smithy => "smithy",
        BuildingKind::Stable => "stable",
        BuildingKind::Workshop => "workshop",
        BuildingKind::Residence => "residence",
        BuildingKind::Cranny => "cranny",
        BuildingKind::Outpost => "outpost",
        BuildingKind::TownHall => "town_hall",
        BuildingKind::Palace => "palace",
        BuildingKind::Treasury => "treasury",
        BuildingKind::Wonder => "wonder",
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
        "embassy" => Ok(BuildingKind::Embassy),
        "wall" => Ok(BuildingKind::Wall),
        "barracks" => Ok(BuildingKind::Barracks),
        "academy" => Ok(BuildingKind::Academy),
        "smithy" => Ok(BuildingKind::Smithy),
        "stable" => Ok(BuildingKind::Stable),
        "workshop" => Ok(BuildingKind::Workshop),
        "residence" => Ok(BuildingKind::Residence),
        "cranny" => Ok(BuildingKind::Cranny),
        "outpost" => Ok(BuildingKind::Outpost),
        "town_hall" => Ok(BuildingKind::TownHall),
        "palace" => Ok(BuildingKind::Palace),
        "treasury" => Ok(BuildingKind::Treasury),
        "wonder" => Ok(BuildingKind::Wonder),
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
        abandoned: r.try_get("abandoned").map_err(backend)?,
        is_moderator: r.try_get("is_moderator").map_err(backend)?,
        is_admin: r.try_get("is_admin").map_err(backend)?,
        banned_at: r
            .try_get::<Option<i64>, _>("banned_ms")
            .map_err(backend)?
            .map(Timestamp),
        suspended_until: r
            .try_get::<Option<i64>, _>("suspended_ms")
            .map_err(backend)?
            .map(Timestamp),
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
        // Beginner's protection (019 AC1): immune to attack until now + the speed-scaled window.
        let protected_until =
            protection_expiry(crate::now(), self.protection_window_secs, self.speed);
        let insert_user = sqlx::query(
            "INSERT INTO users (id, username, email, password_hash, email_confirmed, tribe, \
             protected_until) \
             VALUES ($1, $2, $3, $4, $5, $6, to_timestamp($7::double precision / 1000.0))",
        )
        .bind(user_id)
        .bind(&user.username)
        .bind(&user.email)
        .bind(&user.password_hash)
        .bind(user.email_confirmed)
        .bind(user.tribe.slug())
        .bind(protected_until.0)
        .execute(&mut *tx)
        .await;
        if let Err(e) = insert_user {
            return Err(if is_unique_violation(&e) {
                RepoError::Duplicate
            } else {
                backend(e)
            });
        }

        let world_uuid = Uuid::from_u128(self.world_id.0);

        // The per-world player profile (037): one row per (user, world). In the home world a player's id
        // equals the user's id; villages.owner_id (042) references players(id).
        sqlx::query("INSERT INTO players (id, user_id, world_id, tribe) VALUES ($1, $1, $2, $3)")
            .bind(user_id)
            .bind(world_uuid)
            .bind(user.tribe.slug())
            .execute(&mut *tx)
            .await
            .map_err(backend)?;

        // Place the starting village + seed the culture accumulator for this player, in this world.
        self.place_starting_village(&mut tx, user_id, user.tribe, template)
            .await?;

        tx.commit().await.map_err(backend)?;
        Ok(UserRecord {
            id: PlayerId(user_id.as_u128()),
            username: user.username,
            email: user.email,
            password_hash: user.password_hash,
            email_confirmed: user.email_confirmed,
            tribe: user.tribe,
            abandoned: false,
            is_moderator: false,
            is_admin: false,
            banned_at: None,
            suspended_until: None,
        })
    }

    async fn find_user_by_username(&self, username: &str) -> Result<Option<UserRecord>, RepoError> {
        let row = sqlx::query(
            "SELECT id, username, email, password_hash, email_confirmed, tribe, \
             (abandoned_at IS NOT NULL) AS abandoned, is_moderator, is_admin, \
             (EXTRACT(EPOCH FROM banned_at) * 1000)::bigint AS banned_ms, \
             (EXTRACT(EPOCH FROM suspended_until) * 1000)::bigint AS suspended_ms \
             FROM users WHERE username = $1",
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await
        .map_err(backend)?;
        row.as_ref().map(row_to_user).transpose()
    }

    async fn find_user_by_id(&self, id: PlayerId) -> Result<Option<UserRecord>, RepoError> {
        let row = sqlx::query(
            "SELECT id, username, email, password_hash, email_confirmed, tribe, \
             (abandoned_at IS NOT NULL) AS abandoned, is_moderator, is_admin, \
             (EXTRACT(EPOCH FROM banned_at) * 1000)::bigint AS banned_ms, \
             (EXTRACT(EPOCH FROM suspended_until) * 1000)::bigint AS suspended_ms \
             FROM users WHERE id = $1",
        )
        .bind(Uuid::from_u128(id.0))
        .fetch_optional(&self.pool)
        .await
        .map_err(backend)?;
        row.as_ref().map(row_to_user).transpose()
    }

    async fn player_in_world(
        &self,
        user: PlayerId,
        world: WorldId,
    ) -> Result<Option<PlayerId>, RepoError> {
        let id: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM players WHERE user_id = $1 AND world_id = $2")
                .bind(Uuid::from_u128(user.0))
                .bind(Uuid::from_u128(world.0))
                .fetch_optional(&self.pool)
                .await
                .map_err(backend)?;
        Ok(id.map(|u| PlayerId(u.as_u128())))
    }

    async fn worlds_of_user(&self, user: PlayerId) -> Result<Vec<PlayerWorld>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, world_id, tribe FROM players WHERE user_id = $1 ORDER BY created_at, id",
        )
        .bind(Uuid::from_u128(user.0))
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;
        rows.iter()
            .map(|r| {
                let id: Uuid = r.try_get("id").map_err(backend)?;
                let world: Uuid = r.try_get("world_id").map_err(backend)?;
                let tribe_str: String = r.try_get("tribe").map_err(backend)?;
                let tribe = Tribe::from_slug(&tribe_str)
                    .ok_or_else(|| RepoError::Backend(format!("unknown tribe: {tribe_str}")))?;
                Ok(PlayerWorld {
                    player: PlayerId(id.as_u128()),
                    world: WorldId(world.as_u128()),
                    tribe,
                })
            })
            .collect()
    }

    async fn villages_of(&self, owner: PlayerId) -> Result<Vec<Village>, RepoError> {
        let owner_uuid = Uuid::from_u128(owner.0);
        let village_rows = sqlx::query(
            "SELECT id, x, y, tribe, is_capital, is_natar, is_wonder_site FROM villages \
             WHERE owner_id = $1 ORDER BY created_at, id",
        )
        .bind(owner_uuid)
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;

        // The owner's artifact holdings, fetched once and reused for every village (no N+1, P11).
        let held = self.held_by_player(owner).await?;
        let mut villages = Vec::with_capacity(village_rows.len());
        for r in &village_rows {
            let vid: Uuid = r.try_get("id").map_err(backend)?;
            let x: i32 = r.try_get("x").map_err(backend)?;
            let y: i32 = r.try_get("y").map_err(backend)?;
            let tribe_raw: Option<String> = r.try_get("tribe").map_err(backend)?;
            let is_capital: bool = r.try_get("is_capital").map_err(backend)?;
            let is_natar: bool = r.try_get("is_natar").map_err(backend)?;
            let is_wonder_site: bool = r.try_get("is_wonder_site").map_err(backend)?;

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

            let vid_typed = VillageId(vid.as_u128());
            villages.push(Village {
                id: vid_typed,
                owner,
                coordinate: Coordinate::new(x, y),
                tribe: parse_tribe(tribe_raw)?,
                fields,
                buildings,
                // Fold the village's occupied-oasis bonus into the read (012, AC8) so every economy
                // computation that takes this `Village` sees it.
                oasis_bonus: self.village_oasis_bonus(vid_typed).await?,
                is_capital,
                is_natar,
                is_wonder_site,
                artifact_effects: artifact_effects_from(&held, vid_typed, is_natar),
            });
        }
        Ok(villages)
    }

    async fn village_by_id(&self, village: VillageId) -> Result<Option<Village>, RepoError> {
        let vid = Uuid::from_u128(village.0);
        let Some(r) = sqlx::query(
            "SELECT owner_id, x, y, tribe, is_capital, is_natar, is_wonder_site \
             FROM villages WHERE id = $1",
        )
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
        let is_capital: bool = r.try_get("is_capital").map_err(backend)?;
        let is_natar: bool = r.try_get("is_natar").map_err(backend)?;
        let is_wonder_site: bool = r.try_get("is_wonder_site").map_err(backend)?;

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
            // Fold the village's occupied-oasis bonus into the read (012, AC8).
            oasis_bonus: self.village_oasis_bonus(village).await?,
            is_capital,
            is_natar,
            is_wonder_site,
            artifact_effects: self
                .artifact_effects_for(PlayerId(owner.as_u128()), village, is_natar)
                .await?,
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
            "SELECT v.x, v.y, u.username, al.tag AS alliance_tag, \
             (EXTRACT(EPOCH FROM u.last_activity) * 1000)::bigint AS last_activity_ms \
             FROM villages v JOIN players pu ON pu.id = v.owner_id JOIN users u ON u.id = pu.user_id \
             LEFT JOIN alliance_members am ON am.player_id = v.owner_id \
             LEFT JOIN alliances al ON al.id = am.alliance_id \
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
                let alliance_tag: Option<String> = r.try_get("alliance_tag").map_err(backend)?;
                let last_activity_ms: i64 = r.try_get("last_activity_ms").map_err(backend)?;
                Ok(VillageMarker {
                    coordinate: Coordinate::new(x, y),
                    owner_name,
                    alliance_tag,
                    owner_last_activity: Timestamp(last_activity_ms),
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

    async fn protection_of(&self, player: PlayerId) -> Result<Option<Timestamp>, RepoError> {
        let ms: Option<i64> = sqlx::query_scalar(
            "SELECT (EXTRACT(EPOCH FROM protected_until) * 1000)::bigint FROM users WHERE id = $1",
        )
        .bind(Uuid::from_u128(player.0))
        .fetch_optional(&self.pool)
        .await
        .map_err(backend)?
        .flatten();
        Ok(ms.map(Timestamp))
    }

    async fn end_protection(&self, player: PlayerId, now: Timestamp) -> Result<(), RepoError> {
        // Only ends an *active* window; never extends or re-arms (idempotent — AC3/AC4).
        sqlx::query(
            "UPDATE users SET protected_until = to_timestamp($2::double precision / 1000.0) \
             WHERE id = $1 AND protected_until > to_timestamp($2::double precision / 1000.0)",
        )
        .bind(Uuid::from_u128(player.0))
        .bind(now.0)
        .execute(&self.pool)
        .await
        .map_err(backend)?;
        Ok(())
    }

    async fn touch_activity(&self, player: PlayerId, now: Timestamp) -> Result<(), RepoError> {
        // Throttled (AC5): rewrite only when the stored value is staler than ACTIVITY_THROTTLE_MS, so
        // an authenticated view costs at most one tiny write per throttle window, not per request.
        let cutoff = now.0 - ACTIVITY_THROTTLE_MS;
        sqlx::query(
            "UPDATE users SET last_activity = to_timestamp($2::double precision / 1000.0) \
             WHERE id = $1 AND last_activity < to_timestamp($3::double precision / 1000.0)",
        )
        .bind(Uuid::from_u128(player.0))
        .bind(now.0)
        .bind(cutoff)
        .execute(&self.pool)
        .await
        .map_err(backend)?;
        Ok(())
    }

    async fn set_bio(&self, player: PlayerId, bio: &str) -> Result<(), RepoError> {
        sqlx::query("UPDATE users SET bio = $2 WHERE id = $1")
            .bind(Uuid::from_u128(player.0))
            .bind(bio)
            .execute(&self.pool)
            .await
            .map_err(backend)?;
        Ok(())
    }

    async fn profile_of(&self, player: PlayerId) -> Result<Option<ProfileView>, RepoError> {
        let row = sqlx::query(
            "SELECT username, bio, (EXTRACT(EPOCH FROM last_activity) * 1000)::bigint AS last_ms \
             FROM users WHERE id = $1",
        )
        .bind(Uuid::from_u128(player.0))
        .fetch_optional(&self.pool)
        .await
        .map_err(backend)?;
        row.map(|r| {
            Ok::<_, RepoError>(ProfileView {
                player,
                name: r.try_get("username").map_err(backend)?,
                bio: r.try_get("bio").map_err(backend)?,
                last_activity: Timestamp(r.try_get("last_ms").map_err(backend)?),
            })
        })
        .transpose()
    }

    async fn search_players(&self, query: &str, limit: i64) -> Result<Vec<PlayerHit>, RepoError> {
        // Case-insensitive username **prefix**, abandoned/NPC excluded (like the leaderboard). The LIKE
        // pattern escapes the user's `%`/`_`/`\` so they are literal, and the anchored prefix uses the
        // 0039 functional index (P11). 046: scoped to this repo's world — drive from `users` (the prefix
        // index), join `players` for the world player id, and return that id (so `/stats/player/{id}`
        // resolves under the same selected-world repo).
        let rows = sqlx::query(
            "SELECT p.id, u.username FROM users u \
             JOIN players p ON p.user_id = u.id AND p.world_id = $3 \
             WHERE u.abandoned_at IS NULL AND u.is_npc = false \
               AND lower(u.username) LIKE \
                   replace(replace(replace(lower($1), '\\', '\\\\'), '%', '\\%'), '_', '\\_') || '%' \
                   ESCAPE '\\' \
             ORDER BY u.username ASC LIMIT $2",
        )
        .bind(query)
        .bind(limit)
        .bind(Uuid::from_u128(self.world_id.0))
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;
        rows.iter()
            .map(|r| {
                let id: Uuid = r.try_get("id").map_err(backend)?;
                Ok(PlayerHit {
                    player: PlayerId(id.as_u128()),
                    name: r.try_get("username").map_err(backend)?,
                })
            })
            .collect()
    }

    // ---- Account sitting (030) ----

    async fn grant_sitter(&self, owner: PlayerId, sitter: PlayerId) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT INTO account_sitters (owner_id, sitter_id) VALUES ($1, $2) \
             ON CONFLICT (owner_id, sitter_id) DO NOTHING",
        )
        .bind(Uuid::from_u128(owner.0))
        .bind(Uuid::from_u128(sitter.0))
        .execute(&self.pool)
        .await
        .map_err(backend)?;
        Ok(())
    }

    async fn revoke_sitter(&self, owner: PlayerId, sitter: PlayerId) -> Result<(), RepoError> {
        sqlx::query("DELETE FROM account_sitters WHERE owner_id = $1 AND sitter_id = $2")
            .bind(Uuid::from_u128(owner.0))
            .bind(Uuid::from_u128(sitter.0))
            .execute(&self.pool)
            .await
            .map_err(backend)?;
        Ok(())
    }

    async fn is_sitter(&self, owner: PlayerId, sitter: PlayerId) -> Result<bool, RepoError> {
        let n: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM account_sitters WHERE owner_id = $1 AND sitter_id = $2",
        )
        .bind(Uuid::from_u128(owner.0))
        .bind(Uuid::from_u128(sitter.0))
        .fetch_one(&self.pool)
        .await
        .map_err(backend)?;
        Ok(n > 0)
    }

    async fn count_sitters(&self, owner: PlayerId) -> Result<i64, RepoError> {
        sqlx::query_scalar("SELECT count(*) FROM account_sitters WHERE owner_id = $1")
            .bind(Uuid::from_u128(owner.0))
            .fetch_one(&self.pool)
            .await
            .map_err(backend)
    }

    async fn sitters_of(&self, owner: PlayerId) -> Result<Vec<PlayerHit>, RepoError> {
        let rows = sqlx::query(
            "SELECT u.id, u.username FROM account_sitters s JOIN users u ON u.id = s.sitter_id \
             WHERE s.owner_id = $1 ORDER BY u.username ASC",
        )
        .bind(Uuid::from_u128(owner.0))
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;
        sitter_hits(&rows)
    }

    async fn sitting_for(&self, sitter: PlayerId) -> Result<Vec<PlayerHit>, RepoError> {
        let rows = sqlx::query(
            "SELECT u.id, u.username FROM account_sitters s JOIN users u ON u.id = s.owner_id \
             WHERE s.sitter_id = $1 ORDER BY u.username ASC",
        )
        .bind(Uuid::from_u128(sitter.0))
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;
        sitter_hits(&rows)
    }

    async fn log_sitter_action(
        &self,
        owner: PlayerId,
        sitter: PlayerId,
        action: &str,
        now: Timestamp,
    ) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT INTO sitter_actions (id, owner_id, sitter_id, action, created_at) \
             VALUES ($1, $2, $3, $4, to_timestamp($5::double precision / 1000.0))",
        )
        .bind(Uuid::new_v4())
        .bind(Uuid::from_u128(owner.0))
        .bind(Uuid::from_u128(sitter.0))
        .bind(action)
        .bind(now.0)
        .execute(&self.pool)
        .await
        .map_err(backend)?;
        Ok(())
    }

    async fn sitter_actions(
        &self,
        owner: PlayerId,
        limit: i64,
    ) -> Result<Vec<SitterActionView>, RepoError> {
        let rows = sqlx::query(
            "SELECT u.username AS sitter_name, a.action, \
                    (EXTRACT(EPOCH FROM a.created_at) * 1000)::bigint AS created_ms \
             FROM sitter_actions a JOIN users u ON u.id = a.sitter_id \
             WHERE a.owner_id = $1 ORDER BY a.created_at DESC, a.id DESC LIMIT $2",
        )
        .bind(Uuid::from_u128(owner.0))
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;
        rows.iter()
            .map(|r| {
                Ok(SitterActionView {
                    sitter_name: r.try_get("sitter_name").map_err(backend)?,
                    action: r.try_get("action").map_err(backend)?,
                    created_ms: r.try_get("created_ms").map_err(backend)?,
                })
            })
            .collect()
    }
}

/// Map `(id, username)` rows to [`PlayerHit`]s (the sitter lists).
fn sitter_hits(rows: &[PgRow]) -> Result<Vec<PlayerHit>, RepoError> {
    rows.iter()
        .map(|r| {
            let id: Uuid = r.try_get("id").map_err(backend)?;
            Ok(PlayerHit {
                player: PlayerId(id.as_u128()),
                name: r.try_get("username").map_err(backend)?,
            })
        })
        .collect()
}

/// Freshness window for [`PgAccountRepository::touch_activity`] — `last_activity` is rewritten only
/// when older than this (5 minutes). An implementation constant, not game balance.
const ACTIVITY_THROTTLE_MS: i64 = 5 * 60 * 1000;

/// How long fixed-window `rate_limits` rows are retained (24h) before `bump_rate` prunes them — bounds
/// the table (P11) while keeping recent history for the inhuman-action-rate detection signal (022).
const RATE_LIMIT_RETENTION_SECS: i64 = 24 * 60 * 60;

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
                   AND village_id IN (SELECT id FROM villages WHERE world_id = $3) \
                 ORDER BY complete_at, id LIMIT $2 FOR UPDATE SKIP LOCKED \
             ) RETURNING id, village_id, target_table, slot, building_type, target_level, \
                 (EXTRACT(EPOCH FROM complete_at) * 1000)::bigint AS complete_ms",
        )
        .bind(now.0 as f64)
        .bind(limit)
        .bind(Uuid::from_u128(self.world_id.0))
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
            let complete_ms: i64 = r.try_get("complete_ms").map_err(backend)?;
            out.push(DueBuild {
                id: id.as_u128(),
                village: VillageId(village.as_u128()),
                target: parse_target(&table, slot, building_type)?,
                target_level: u8::try_from(target_level).unwrap_or(0),
                complete_at: Timestamp(complete_ms),
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

                // 013 AC9: completing a Palace makes this village the owner's capital — exactly one
                // per player, so any prior capital is cleared and the **previous Palace building is
                // removed** (at most one Palace per player; the old one cannot remain).
                if kind == BuildingKind::Palace {
                    sqlx::query(
                        "UPDATE villages SET is_capital = (id = $1) \
                         WHERE owner_id = (SELECT owner_id FROM villages WHERE id = $1)",
                    )
                    .bind(vid)
                    .execute(&mut *tx)
                    .await
                    .map_err(backend)?;
                    // Demolish any Palace the owner holds in another village (relocation).
                    sqlx::query(
                        "DELETE FROM village_buildings \
                         WHERE building_type = 'palace' AND village_id <> $1 \
                           AND village_id IN ( \
                               SELECT id FROM villages \
                               WHERE owner_id = (SELECT owner_id FROM villages WHERE id = $1))",
                    )
                    .bind(vid)
                    .execute(&mut *tx)
                    .await
                    .map_err(backend)?;
                }
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
                   AND village_id IN (SELECT id FROM villages WHERE world_id = $3) \
                 ORDER BY complete_at, id LIMIT $2 FOR UPDATE SKIP LOCKED \
             ) RETURNING id, village_id, kind, unit_id, target_level",
        )
        .bind(now.0 as f64)
        .bind(limit)
        .bind(Uuid::from_u128(self.world_id.0))
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
                   AND village_id IN (SELECT id FROM villages WHERE world_id = $3) \
                 ORDER BY next_complete_at, id LIMIT $2 FOR UPDATE SKIP LOCKED \
             ) RETURNING id, village_id, unit_id, count_total, count_done, per_unit_secs, \
                         (EXTRACT(EPOCH FROM started_at) * 1000)::bigint AS started_ms",
        )
        .bind(now.0 as f64)
        .bind(limit)
        .bind(Uuid::from_u128(self.world_id.0))
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
                   AND village_id IN (SELECT id FROM villages WHERE world_id = $3) \
                 ORDER BY due_at, village_id LIMIT $2 FOR UPDATE SKIP LOCKED \
             ) RETURNING village_id",
        )
        .bind(now.0 as f64)
        .bind(limit)
        .bind(Uuid::from_u128(self.world_id.0))
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
        MovementKind::Settle => "settle",
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
        "settle" => Ok(MovementKind::Settle),
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
             JOIN players pu ON pu.id = hv.owner_id JOIN users u ON u.id = pu.user_id \
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
             JOIN players pu ON pu.id = hostv.owner_id JOIN users u ON u.id = pu.user_id \
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
                   AND home_village IN (SELECT id FROM villages WHERE world_id = $3) \
                 ORDER BY arrive_at, id LIMIT $2 FOR UPDATE SKIP LOCKED \
             ) RETURNING id, kind, home_village, deliver_village, \
                 loot_wood, loot_clay, loot_iron, loot_crop",
        )
        .bind(now.0 as f64)
        .bind(limit)
        .bind(Uuid::from_u128(self.world_id.0))
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
                | MovementKind::OasisReinforce
                | MovementKind::Settle => {
                    return Err(RepoError::Backend(
                        "combat/scout/oasis/settle movement routed to apply_movement".into(),
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
                   AND home_village IN (SELECT id FROM villages WHERE world_id = $3) \
                 ORDER BY arrive_at, id LIMIT $2 FOR UPDATE SKIP LOCKED \
             ) RETURNING id, kind, owner_id, home_village, target_village, \
                 origin_x, origin_y, dest_x, dest_y, wood, clay, iron, crop, merchants, \
                 (EXTRACT(EPOCH FROM arrive_at) * 1000)::bigint AS arrive_ms",
        )
        .bind(now.0 as f64)
        .bind(limit)
        .bind(Uuid::from_u128(self.world_id.0))
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
    let dp: Option<Uuid> = r.try_get("defender_player").map_err(backend)?;
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
        defender_player: dp.map(|u| PlayerId(u.as_u128())),
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
        loyalty_before: r
            .try_get::<Option<i16>, _>("loyalty_before")
            .map_err(backend)?
            .map(i64::from),
        loyalty_after: r
            .try_get::<Option<i16>, _>("loyalty_after")
            .map_err(backend)?
            .map(i64::from),
        conquered: r.try_get("conquered").map_err(backend)?,
    })
}

/// The `SELECT` of a battle report joined to player names + village coordinates (inbox/detail).
const REPORT_SELECT: &str = "SELECT br.id, \
    (EXTRACT(EPOCH FROM br.occurred_at) * 1000)::bigint AS occurred_ms, br.kind, \
    au.username AS attacker_name, COALESCE(av.x, br.attacker_x) AS ax, \
    COALESCE(av.y, br.attacker_y) AS ay, \
    COALESCE(du.username, br.defender_label) AS defender_name, \
    COALESCE(dv.x, br.defender_x) AS dx, COALESCE(dv.y, br.defender_y) AS dy, \
    br.attacker_player, br.defender_player, br.attacker_won, br.luck, br.morale, \
    br.wall_before, br.wall_after, br.attacker_forces, br.attacker_losses, \
    br.defender_forces, br.defender_losses, br.scouted, br.scout_target, \
    br.loot_wood, br.loot_clay, br.loot_iron, br.loot_crop, \
    br.razed_building, br.razed_before, br.razed_after, \
    br.loyalty_before, br.loyalty_after, br.conquered \
    FROM battle_reports br \
    JOIN players pau ON pau.id = br.attacker_player JOIN users au ON au.id = pau.user_id \
    LEFT JOIN villages av ON av.id = br.attacker_village \
    LEFT JOIN players pdu ON pdu.id = br.defender_player LEFT JOIN users du ON du.id = pdu.user_id \
    LEFT JOIN villages dv ON dv.id = br.defender_village";

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
                   AND home_village IN (SELECT id FROM villages WHERE world_id = $3) \
                 ORDER BY arrive_at, id LIMIT $2 FOR UPDATE SKIP LOCKED \
             ) RETURNING id, kind, owner_id, home_village, deliver_village, \
                 origin_x, origin_y, dest_x, dest_y, scout_target, catapult_target, \
                 (EXTRACT(EPOCH FROM arrive_at) * 1000)::bigint AS arrive_ms",
        )
        .bind(now.0 as f64)
        .bind(limit)
        .bind(Uuid::from_u128(self.world_id.0))
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
        let report_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO battle_reports \
             (id, kind, attacker_player, attacker_village, defender_player, defender_village, \
              attacker_won, luck, morale, wall_before, wall_after, \
              attacker_forces, attacker_losses, defender_forces, defender_losses, \
              scouted, scout_target, loot_wood, loot_clay, loot_iron, loot_crop, \
              razed_building, razed_before, razed_after, \
              loyalty_before, loyalty_after, conquered, attack_points, \
              attacker_x, attacker_y, defender_x, defender_y) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, \
                     $18, $19, $20, $21, $22, $23, $24, $25, $26, $27, $28, $29, $30, $31, $32)",
        )
        .bind(report_id)
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
        .bind(r.loyalty_before.and_then(|v| i16::try_from(v).ok()))
        .bind(r.loyalty_after.and_then(|v| i16::try_from(v).ok()))
        .bind(r.conquered)
        .bind(apply.attack_points)
        // 019: fallback coords so the report stays readable if a village is later deleted.
        .bind(apply.attacker_origin.x)
        .bind(apply.attacker_origin.y)
        .bind(apply.target_coord.x)
        .bind(apply.target_coord.y)
        .execute(&mut *tx)
        .await
        .map_err(backend)?;

        // 016 AC3/AC4: one `battle_defenders` row per defending player (owner + each reinforcer),
        // recording their forces/losses, contributed defensive value, and split defense points — so
        // a reinforcer sees their own report and defense points are faithfully shared. Same tx as the
        // report ⇒ exactly-once with the movement claim (no duplication on crash-resume).
        for c in &apply.defender_contributions {
            sqlx::query(
                "INSERT INTO battle_defenders \
                 (id, battle_id, player_id, village_id, is_owner, forces, losses, \
                  defense_value, defense_points) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
            )
            .bind(Uuid::new_v4())
            .bind(report_id)
            .bind(Uuid::from_u128(c.player.0))
            .bind(Uuid::from_u128(c.village.0))
            .bind(c.is_owner)
            .bind(counts_to_json(&c.forces))
            .bind(counts_to_json(&c.losses))
            .bind(c.defense_value)
            .bind(c.defense_points)
            .execute(&mut *tx)
            .await
            .map_err(backend)?;
        }

        // 026 AC2: notify the attacker + each distinct defending participant that their report is ready —
        // in the report transaction (so a notification is never orphaned), with the live `notif:<uuid>`
        // nudge fired on commit. One bulk insert + per-recipient pg_notify in a single statement
        // (the same shape as `record`), so a heavily-reinforced battle stays one round-trip (P11). The
        // feed links to `/reports/{id}` (parses a u128), so the ref_id is the report id in that form.
        let mut recipients: Vec<Uuid> = vec![Uuid::from_u128(r.attacker_player.0)];
        for c in &apply.defender_contributions {
            let d = Uuid::from_u128(c.player.0);
            if !recipients.contains(&d) {
                recipients.push(d);
            }
        }
        let note_ids: Vec<Uuid> = recipients.iter().map(|_| Uuid::new_v4()).collect();
        sqlx::query(
            "WITH ins AS ( \
                INSERT INTO notifications \
                    (id, world_id, player_id, kind, ref_kind, ref_id, body, created_at) \
                SELECT u.id, $1, u.player_id, 'battle_report', 'report', $4, '', \
                       to_timestamp($5::double precision / 1000.0) \
                FROM unnest($2::uuid[], $3::uuid[]) AS u(id, player_id) \
                WHERE NOT EXISTS ( \
                    SELECT 1 FROM notification_mutes m \
                     WHERE m.player_id = u.player_id AND m.kind = 'battle_report') \
                RETURNING player_id \
             ) \
             SELECT pg_notify('notifications', json_build_object( \
                'key', 'notif:' || player_id::text, 'kind', 'battle_report')::text) FROM ins",
        )
        .bind(Uuid::from_u128(self.world_id.0))
        .bind(&note_ids)
        .bind(&recipients)
        .bind(report_id.as_u128().to_string())
        .bind(apply.battle_at.0)
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

        // 014: the post-battle loyalty step — lower loyalty, or transfer ownership (a conquest).
        match &apply.loyalty {
            None => {}
            Some(LoyaltyApply::Reduced { new_loyalty }) => {
                let clamped = i16::try_from((*new_loyalty).clamp(0, eperica_domain::MAX_LOYALTY))
                    .unwrap_or(0);
                sqlx::query(
                    "UPDATE villages SET loyalty = $2, \
                         loyalty_updated_at = to_timestamp($3::double precision / 1000.0) \
                     WHERE id = $1",
                )
                .bind(target)
                .bind(clamped)
                .bind(apply.battle_at.0 as f64)
                .execute(&mut *tx)
                .await
                .map_err(backend)?;
            }
            Some(LoyaltyApply::Conquered(t)) => {
                // Re-point ownership, **guarded** on the loser still owning the village (a concurrent
                // conquest wins the race ⇒ Conflict ⇒ the whole apply rolls back and re-resolves).
                // Reset loyalty and clear the capital flag — a conquered village is never a capital.
                let post = i16::try_from(
                    t.post_conquest_loyalty
                        .clamp(0, eperica_domain::MAX_LOYALTY),
                )
                .unwrap_or(0);
                let moved = sqlx::query(
                    "UPDATE villages SET owner_id = $2, loyalty = $3, \
                         loyalty_updated_at = to_timestamp($4::double precision / 1000.0), \
                         is_capital = false \
                     WHERE id = $1 AND owner_id = $5",
                )
                .bind(target)
                .bind(Uuid::from_u128(t.new_owner.0))
                .bind(post)
                .bind(apply.battle_at.0 as f64)
                .bind(Uuid::from_u128(t.loser.0))
                .execute(&mut *tx)
                .await
                .map_err(backend)?;
                if moved.rows_affected() == 0 {
                    return Err(RepoError::Conflict);
                }

                // Empty the garrison (the defenders lost the battle that enabled the conquest).
                sqlx::query("DELETE FROM village_units WHERE village_id = $1")
                    .bind(target)
                    .execute(&mut *tx)
                    .await
                    .map_err(backend)?;

                // Send surviving third-party reinforcements home (007), then clear any stationed here.
                for ret in &t.reinforcement_returns {
                    insert_movement(
                        &mut tx,
                        Uuid::new_v4(),
                        ret.owner,
                        MovementKind::Return,
                        ret.home_village,
                        ret.home_village,
                        apply.target_coord,
                        ret.home_coord,
                        apply.battle_at,
                        ret.arrive_at,
                        &ret.troops,
                    )
                    .await?;
                }
                sqlx::query("DELETE FROM reinforcements WHERE host_village = $1")
                    .bind(target)
                    .execute(&mut *tx)
                    .await
                    .map_err(backend)?;

                // Every remaining `village_id`-keyed dependency is resolved here so the transfer is
                // complete (AC7) and nothing is left dangling under the old owner. The full
                // enumeration and the disposition of each:
                //   • village_units (garrison)                  — emptied above (the defenders fell).
                //   • reinforcements host_village = target       — third parties stationed here; sent
                //                                                  home (007) + cleared above.
                //   • build / unit / training orders             — cancelled below (the new owner
                //     (village_id = target)                        starts every queue fresh).
                //   • troop_movements (home_village = target)    — completed below: the loser's
                //     OUTGOING movements from the village are cancelled (AC7, "outgoing movements"),
                //     and any troops still RETURNING to it are forfeited — the village is no longer
                //     theirs, so there is no loyal home to arrive at (leaving them would land the
                //     loser's army inside what is now an enemy village).
                //   • reinforcements home_village = target       — the village's OWN troops stationed
                //     at other villages: left in place. Stationed-troop ownership is derived from the
                //     home village's owner, so they pass to the new owner with the village (its
                //     standing army follows it) — no row change is needed or wanted.
                //   • trades (home_village = target)             — in-flight merchants/shipments are
                //     likewise bound to the village and follow it to the new owner.
                //   • oases (owner = village_id)                 — occupied oases are owned by the
                //     village, so they transfer with it implicitly.
                //   • starvation_checks (PK village_id)          — left to self-resolve: the garrison
                //     is emptied above, so when the pending check fires it finds no troops and
                //     finishes as a no-op (`starve_village`); no need to cancel it in this tx.
                //   • player_culture (both players)              — re-anchored below (013 AC1).
                // The principle (AC7): assets located in or owned by the village pass with it; troops
                // and shipments in transit that can no longer reach a loyal village are forfeited.
                for sql in [
                    "DELETE FROM build_orders WHERE village_id = $1",
                    "DELETE FROM unit_orders WHERE village_id = $1",
                    "DELETE FROM training_orders WHERE village_id = $1",
                ] {
                    sqlx::query(sql)
                        .bind(target)
                        .execute(&mut *tx)
                        .await
                        .map_err(backend)?;
                }
                sqlx::query(
                    "UPDATE troop_movements SET status = 'done' \
                     WHERE home_village = $1 AND status = 'in_transit'",
                )
                .bind(target)
                .execute(&mut *tx)
                .await
                .map_err(backend)?;

                // Re-anchor both players' culture at the battle instant (013 AC1): settle each at the
                // OLD rate before the village count/rate moves between them.
                for (player, value) in [
                    (t.loser, t.loser_culture_value),
                    (t.new_owner, t.gainer_culture_value),
                ] {
                    sqlx::query(
                        "INSERT INTO player_culture (player_id, value, updated_at) \
                         VALUES ($1, $2, to_timestamp($3::double precision / 1000.0)) \
                         ON CONFLICT (player_id) DO UPDATE \
                           SET value = EXCLUDED.value, updated_at = EXCLUDED.updated_at",
                    )
                    .bind(Uuid::from_u128(player.0))
                    .bind(value)
                    .bind(apply.battle_at.0 as f64)
                    .execute(&mut *tx)
                    .await
                    .map_err(backend)?;
                }
            }
        }

        sqlx::query("UPDATE troop_movements SET status = 'done' WHERE id = $1")
            .bind(Uuid::from_u128(apply.movement_id))
            .execute(&mut *tx)
            .await
            .map_err(backend)?;

        // 020 AC4/AC5: a captured artifact moves to the attacking village, in the battle transaction.
        // Guarded on the expected current holder so a concurrent capture affects zero rows (P5).
        if let Some(cap) = &apply.artifact_capture {
            sqlx::query(
                "UPDATE artifacts SET holder_village = $1 WHERE id = $2 AND holder_village = $3",
            )
            .bind(Uuid::from_u128(cap.to_village.0))
            .bind(&cap.artifact_id)
            .bind(Uuid::from_u128(cap.from_village.0))
            .execute(&mut *tx)
            .await
            .map_err(backend)?;
        }
        // 021 AC2: transfer a captured Wonder plan, guarded on the expected current holder (P5).
        if let Some(cap) = &apply.plan_capture {
            sqlx::query(
                "UPDATE wonder_plans SET holder_village = $1 WHERE id = $2 AND holder_village = $3",
            )
            .bind(Uuid::from_u128(cap.to_village.0))
            .bind(&cap.plan_id)
            .bind(Uuid::from_u128(cap.from_village.0))
            .execute(&mut *tx)
            .await
            .map_err(backend)?;
        }
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

/// Insert an **oasis** battle report into `battle_reports` (012 AC11) within an open transaction.
/// The defender is a village-less oasis: its tile + a synthetic label stand in for the joined
/// defender village, and `defender_player`/`defender_village` are NULL unless the oasis was occupied.
async fn insert_oasis_report(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    r: &NewOasisReport,
) -> Result<(), RepoError> {
    sqlx::query(
        "INSERT INTO battle_reports \
         (id, kind, attacker_player, attacker_village, defender_player, defender_village, \
          defender_x, defender_y, defender_label, \
          attacker_won, luck, morale, wall_before, wall_after, \
          attacker_forces, attacker_losses, defender_forces, defender_losses, \
          attacker_x, attacker_y) \
         VALUES ($1, 'oasis_attack', $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, 0, 0, \
                 $12, $13, $14, $15, \
                 (SELECT x FROM villages WHERE id = $3), (SELECT y FROM villages WHERE id = $3))",
    )
    .bind(Uuid::new_v4())
    .bind(Uuid::from_u128(r.attacker_player.0))
    .bind(Uuid::from_u128(r.attacker_village.0))
    .bind(r.defender_player.map(|p| Uuid::from_u128(p.0)))
    .bind(r.defender_village.map(|v| Uuid::from_u128(v.0)))
    .bind(r.oasis.x)
    .bind(r.oasis.y)
    .bind(r.label.as_str())
    .bind(r.attacker_won)
    .bind(r.luck)
    .bind(r.morale)
    .bind(counts_to_json(&r.attacker_forces))
    .bind(counts_to_json(&r.attacker_losses))
    .bind(counts_to_json(&r.defender_forces))
    .bind(counts_to_json(&r.defender_losses))
    .execute(&mut **tx)
    .await
    .map_err(backend)?;
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

    async fn oasis_owners_at(
        &self,
        coords: &[Coordinate],
    ) -> Result<Vec<(Coordinate, String)>, RepoError> {
        if coords.is_empty() {
            return Ok(Vec::new());
        }
        let xs: Vec<i32> = coords.iter().map(|c| c.x).collect();
        let ys: Vec<i32> = coords.iter().map(|c| c.y).collect();
        let rows = sqlx::query(
            "SELECT o.x, o.y, u.username FROM oases o \
             JOIN villages v ON v.id = o.owner_village \
             JOIN players pu ON pu.id = v.owner_id JOIN users u ON u.id = pu.user_id \
             WHERE o.world_id = $1 AND o.owner_village IS NOT NULL \
               AND (o.x, o.y) IN (SELECT * FROM unnest($2::int[], $3::int[]))",
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
                let owner: String = r.try_get("username").map_err(backend)?;
                Ok((Coordinate::new(x, y), owner))
            })
            .collect()
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
                   AND home_village IN (SELECT id FROM villages WHERE world_id = $3) \
                 ORDER BY arrive_at, id LIMIT $2 FOR UPDATE SKIP LOCKED \
             ) RETURNING id, owner_id, home_village, origin_x, origin_y, dest_x, dest_y, \
                 (EXTRACT(EPOCH FROM arrive_at) * 1000)::bigint AS arrive_ms",
        )
        .bind(now.0 as f64)
        .bind(limit)
        .bind(Uuid::from_u128(self.world_id.0))
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

        // Schedule (or clear) the animal regrow (012 AC9): set when the oasis ends unoccupied (so its
        // animals top back up over time), NULL when it ends occupied. The row exists after the upsert.
        sqlx::query(
            "UPDATE oases SET regrow_at = to_timestamp($1::double precision / 1000.0) \
             WHERE world_id = $2 AND x = $3 AND y = $4",
        )
        .bind(apply.regrow_at.map(|t| t.0 as f64))
        .bind(world)
        .bind(coord.x)
        .bind(coord.y)
        .execute(&mut *tx)
        .await
        .map_err(backend)?;

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

        // The battle report (AC11), on the 009 rails — visible to the attacker (and the owner, if any).
        insert_oasis_report(&mut tx, &apply.report).await?;

        sqlx::query("UPDATE troop_movements SET status = 'done' WHERE id = $1")
            .bind(Uuid::from_u128(apply.movement_id))
            .execute(&mut *tx)
            .await
            .map_err(backend)?;
        tx.commit().await.map_err(backend)?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn start_oasis_reinforce(
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
            MovementKind::OasisReinforce,
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

    async fn claim_due_oasis_reinforcements(
        &self,
        now: Timestamp,
        limit: i64,
    ) -> Result<Vec<DueOasisReinforce>, RepoError> {
        let rows = sqlx::query(
            "UPDATE troop_movements SET status = 'processing' WHERE id IN ( \
                 SELECT id FROM troop_movements \
                 WHERE status = 'in_transit' AND kind = 'oasis_reinforce' \
                   AND arrive_at <= to_timestamp($1::double precision / 1000.0) \
                   AND home_village IN (SELECT id FROM villages WHERE world_id = $3) \
                 ORDER BY arrive_at, id LIMIT $2 FOR UPDATE SKIP LOCKED \
             ) RETURNING id, owner_id, home_village, origin_x, origin_y, dest_x, dest_y, \
                 (EXTRACT(EPOCH FROM arrive_at) * 1000)::bigint AS arrive_ms",
        )
        .bind(now.0 as f64)
        .bind(limit)
        .bind(Uuid::from_u128(self.world_id.0))
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
            out.push(DueOasisReinforce {
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

    async fn apply_oasis_reinforce(
        &self,
        due: &DueOasisReinforce,
        outcome: OasisReinforceOutcome,
    ) -> Result<(), RepoError> {
        let world = Uuid::from_u128(self.world_id.0);
        let mut tx = self.pool.begin().await.map_err(backend)?;
        match outcome {
            OasisReinforceOutcome::Station => {
                // Add the troops to the oasis's defenders (the oasis row exists — it is occupied).
                for (unit, n) in due.troops.iter().filter(|(_, n)| *n > 0) {
                    sqlx::query(
                        "INSERT INTO oasis_garrison (world_id, x, y, unit_id, count) \
                         VALUES ($1, $2, $3, $4, $5) \
                         ON CONFLICT (world_id, x, y, unit_id) \
                         DO UPDATE SET count = oasis_garrison.count + EXCLUDED.count",
                    )
                    .bind(world)
                    .bind(due.oasis.x)
                    .bind(due.oasis.y)
                    .bind(unit.as_str())
                    .bind(i32::try_from(*n).unwrap_or(i32::MAX))
                    .execute(&mut *tx)
                    .await
                    .map_err(backend)?;
                }
            }
            OasisReinforceOutcome::BounceHome {
                home_coord,
                return_arrive,
            } => {
                // The sender lost the oasis in flight — send the troops home (a `return`).
                insert_movement(
                    &mut tx,
                    Uuid::new_v4(),
                    due.owner,
                    MovementKind::Return,
                    due.home_village,
                    due.home_village,
                    due.oasis,
                    home_coord,
                    due.arrive_at,
                    return_arrive,
                    &due.troops,
                )
                .await?;
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

    #[allow(clippy::too_many_arguments)]
    async fn start_oasis_recall(
        &self,
        oasis: Coordinate,
        home: VillageId,
        owner: PlayerId,
        home_coord: Coordinate,
        now: Timestamp,
        arrive_at: Timestamp,
    ) -> Result<UnitCounts, RepoError> {
        let world = Uuid::from_u128(self.world_id.0);
        let mut tx = self.pool.begin().await.map_err(backend)?;

        // Atomically read+delete the oasis garrison (lock it against a concurrent recall/battle).
        let rows = sqlx::query(
            "SELECT unit_id, count FROM oasis_garrison \
             WHERE world_id = $1 AND x = $2 AND y = $3 ORDER BY unit_id FOR UPDATE",
        )
        .bind(world)
        .bind(oasis.x)
        .bind(oasis.y)
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

        sqlx::query("DELETE FROM oasis_garrison WHERE world_id = $1 AND x = $2 AND y = $3")
            .bind(world)
            .bind(oasis.x)
            .bind(oasis.y)
            .execute(&mut *tx)
            .await
            .map_err(backend)?;

        insert_movement(
            &mut tx,
            Uuid::new_v4(),
            owner,
            MovementKind::Return,
            home,
            home,
            oasis,
            home_coord,
            now,
            arrive_at,
            &troops,
        )
        .await?;

        tx.commit().await.map_err(backend)?;
        Ok(troops)
    }

    async fn claim_due_oasis_regrows(
        &self,
        now: Timestamp,
        limit: i64,
    ) -> Result<Vec<DueOasisRegrow>, RepoError> {
        let world = Uuid::from_u128(self.world_id.0);
        let rows = sqlx::query(
            "SELECT x, y, (EXTRACT(EPOCH FROM regrow_at) * 1000)::bigint AS regrow_ms \
             FROM oases \
             WHERE world_id = $1 AND owner_village IS NULL AND regrow_at IS NOT NULL \
               AND regrow_at <= to_timestamp($2::double precision / 1000.0) \
             ORDER BY regrow_at, x, y LIMIT $3",
        )
        .bind(world)
        .bind(now.0 as f64)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;

        let mut out = Vec::with_capacity(rows.len());
        for r in &rows {
            let coord = Coordinate::new(
                r.try_get("x").map_err(backend)?,
                r.try_get("y").map_err(backend)?,
            );
            let regrow_ms: i64 = r.try_get("regrow_ms").map_err(backend)?;
            let current = read_oasis_garrison(&self.pool, world, coord).await?;
            out.push(DueOasisRegrow {
                oasis: coord,
                current,
                regrow_at: Timestamp(regrow_ms),
            });
        }
        Ok(out)
    }

    async fn apply_oasis_regrow(
        &self,
        oasis: Coordinate,
        garrison: &UnitCounts,
        prev_regrow_at: Timestamp,
        next_regrow_at: Option<Timestamp>,
    ) -> Result<(), RepoError> {
        let world = Uuid::from_u128(self.world_id.0);
        let mut tx = self.pool.begin().await.map_err(backend)?;

        // Guard: the oasis must still be unoccupied and hold the same `regrow_at` we claimed — so
        // occupying it in flight cancels the regrow and a concurrent tick applies exactly once.
        let updated = sqlx::query(
            "UPDATE oases SET regrow_at = to_timestamp($1::double precision / 1000.0) \
             WHERE world_id = $2 AND x = $3 AND y = $4 AND owner_village IS NULL \
               AND (EXTRACT(EPOCH FROM regrow_at) * 1000)::bigint = $5",
        )
        .bind(next_regrow_at.map(|t| t.0 as f64))
        .bind(world)
        .bind(oasis.x)
        .bind(oasis.y)
        .bind(prev_regrow_at.0)
        .execute(&mut *tx)
        .await
        .map_err(backend)?;
        if updated.rows_affected() == 0 {
            // Occupied or already advanced — leave the garrison untouched.
            tx.rollback().await.map_err(backend)?;
            return Ok(());
        }

        // Set the garrison to the topped-up composition (replace; single animal kind).
        sqlx::query("DELETE FROM oasis_garrison WHERE world_id = $1 AND x = $2 AND y = $3")
            .bind(world)
            .bind(oasis.x)
            .bind(oasis.y)
            .execute(&mut *tx)
            .await
            .map_err(backend)?;
        for (unit, n) in garrison.iter().filter(|(_, n)| *n > 0) {
            sqlx::query(
                "INSERT INTO oasis_garrison (world_id, x, y, unit_id, count) \
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(world)
            .bind(oasis.x)
            .bind(oasis.y)
            .bind(unit.as_str())
            .bind(i32::try_from(*n).unwrap_or(i32::MAX))
            .execute(&mut *tx)
            .await
            .map_err(backend)?;
        }

        tx.commit().await.map_err(backend)?;
        Ok(())
    }
}

// ---------------------------------------------------------------- culture (013)

#[async_trait]
impl CultureRepository for PgAccountRepository {
    async fn player_culture(&self, player: PlayerId) -> Result<(i64, Timestamp), RepoError> {
        let row = sqlx::query(
            "SELECT value, (EXTRACT(EPOCH FROM updated_at) * 1000)::bigint AS updated_ms \
             FROM player_culture WHERE player_id = $1",
        )
        .bind(Uuid::from_u128(player.0))
        .fetch_optional(&self.pool)
        .await
        .map_err(backend)?;
        match row {
            Some(r) => Ok((
                r.try_get("value").map_err(backend)?,
                Timestamp(r.try_get("updated_ms").map_err(backend)?),
            )),
            // No row (a defensive edge — `create_account` seeds one and migration 0021 backfills
            // pre-013 accounts): treat as zero CP anchored at **now**, never the epoch. Anchoring at
            // the epoch would settle `rate × decades` of CP on the first read, vaulting the player
            // past the expansion thresholds (013 AC1/AC4); anchoring at now yields 0 CP.
            None => Ok((0, crate::now())),
        }
    }

    async fn settle_culture(
        &self,
        player: PlayerId,
        value: i64,
        at: Timestamp,
    ) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT INTO player_culture (player_id, value, updated_at) \
             VALUES ($1, $2, to_timestamp($3::double precision / 1000.0)) \
             ON CONFLICT (player_id) DO UPDATE SET value = EXCLUDED.value, updated_at = EXCLUDED.updated_at",
        )
        .bind(Uuid::from_u128(player.0))
        .bind(value)
        .bind(at.0 as f64)
        .execute(&self.pool)
        .await
        .map_err(backend)?;
        Ok(())
    }

    async fn village_town_hall_levels(&self, player: PlayerId) -> Result<Vec<u8>, RepoError> {
        // One entry per village: the Town Hall level (0 when the village has none).
        let rows = sqlx::query(
            "SELECT COALESCE(vb.level, 0) AS th_level \
             FROM villages v \
             LEFT JOIN village_buildings vb \
               ON vb.village_id = v.id AND vb.building_type = 'town_hall' \
             WHERE v.owner_id = $1 ORDER BY v.created_at, v.id",
        )
        .bind(Uuid::from_u128(player.0))
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;
        rows.iter()
            .map(|r| {
                let level: i32 = r.try_get("th_level").map_err(backend)?;
                Ok(u8::try_from(level).unwrap_or(0))
            })
            .collect()
    }
}

// ---------------------------------------------------------------- loyalty / conquest (014)

#[async_trait]
impl ConquestRepository for PgAccountRepository {
    async fn village_loyalty(
        &self,
        village: VillageId,
    ) -> Result<Option<(i64, Timestamp)>, RepoError> {
        let row = sqlx::query(
            "SELECT loyalty, (EXTRACT(EPOCH FROM loyalty_updated_at) * 1000)::bigint AS updated_ms \
             FROM villages WHERE id = $1",
        )
        .bind(Uuid::from_u128(village.0))
        .fetch_optional(&self.pool)
        .await
        .map_err(backend)?;
        match row {
            Some(r) => {
                let loyalty: i16 = r.try_get("loyalty").map_err(backend)?;
                Ok(Some((
                    i64::from(loyalty),
                    Timestamp(r.try_get("updated_ms").map_err(backend)?),
                )))
            }
            None => Ok(None),
        }
    }

    async fn set_loyalty(
        &self,
        village: VillageId,
        value: i64,
        at: Timestamp,
    ) -> Result<(), RepoError> {
        sqlx::query(
            "UPDATE villages SET loyalty = $2, \
                 loyalty_updated_at = to_timestamp($3::double precision / 1000.0) WHERE id = $1",
        )
        .bind(Uuid::from_u128(village.0))
        .bind(i16::try_from(value.clamp(0, eperica_domain::MAX_LOYALTY)).unwrap_or(0))
        .bind(at.0 as f64)
        .execute(&self.pool)
        .await
        .map_err(backend)?;
        Ok(())
    }
}

// ---------------------------------------------------------------- alliances (015)

#[async_trait]
impl AllianceRepository for PgAccountRepository {
    async fn max_embassy_level(&self, player: PlayerId) -> Result<u8, RepoError> {
        let level: Option<i32> = sqlx::query_scalar(
            "SELECT MAX(vb.level)::int FROM village_buildings vb \
             JOIN villages v ON v.id = vb.village_id \
             WHERE v.owner_id = $1 AND vb.building_type = 'embassy'",
        )
        .bind(Uuid::from_u128(player.0))
        .fetch_one(&self.pool)
        .await
        .map_err(backend)?;
        Ok(u8::try_from(level.unwrap_or(0)).unwrap_or(u8::MAX))
    }

    async fn alliance_of(&self, player: PlayerId) -> Result<Option<Membership>, RepoError> {
        let row = sqlx::query(
            "SELECT alliance_id, role, rights FROM alliance_members WHERE player_id = $1",
        )
        .bind(Uuid::from_u128(player.0))
        .fetch_optional(&self.pool)
        .await
        .map_err(backend)?;
        let Some(r) = row else { return Ok(None) };
        let aid: Uuid = r.try_get("alliance_id").map_err(backend)?;
        let role = parse_alliance_role(&r.try_get::<String, _>("role").map_err(backend)?)?;
        let rights: i32 = r.try_get("rights").map_err(backend)?;
        Ok(Some(Membership {
            alliance: AllianceId(aid.as_u128()),
            role,
            rights: RightSet::from_bits(rights as u8),
        }))
    }

    async fn member_count(&self, alliance: AllianceId) -> Result<u32, RepoError> {
        let n: i64 =
            sqlx::query_scalar("SELECT count(*) FROM alliance_members WHERE alliance_id = $1")
                .bind(Uuid::from_u128(alliance.0))
                .fetch_one(&self.pool)
                .await
                .map_err(backend)?;
        Ok(u32::try_from(n).unwrap_or(u32::MAX))
    }

    async fn alliance_summary(
        &self,
        alliance: AllianceId,
    ) -> Result<Option<(String, String)>, RepoError> {
        let row = sqlx::query("SELECT name, tag FROM alliances WHERE id = $1")
            .bind(Uuid::from_u128(alliance.0))
            .fetch_optional(&self.pool)
            .await
            .map_err(backend)?;
        let Some(r) = row else { return Ok(None) };
        Ok(Some((
            r.try_get("name").map_err(backend)?,
            r.try_get("tag").map_err(backend)?,
        )))
    }

    async fn roster(&self, alliance: AllianceId) -> Result<Vec<RosterEntry>, RepoError> {
        // Order by rank (founder, leader, member) then name for a stable roster.
        let rows = sqlx::query(
            "SELECT m.player_id, u.username, m.role, m.rights FROM alliance_members m \
             JOIN players pm ON pm.id = m.player_id JOIN users u ON u.id = pm.user_id \
             WHERE m.alliance_id = $1 \
             ORDER BY CASE m.role WHEN 'founder' THEN 0 WHEN 'leader' THEN 1 ELSE 2 END, u.username",
        )
        .bind(Uuid::from_u128(alliance.0))
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;
        let mut out = Vec::with_capacity(rows.len());
        for r in &rows {
            let pid: Uuid = r.try_get("player_id").map_err(backend)?;
            let rights: i32 = r.try_get("rights").map_err(backend)?;
            out.push(RosterEntry {
                player: PlayerId(pid.as_u128()),
                name: r.try_get("username").map_err(backend)?,
                role: parse_alliance_role(&r.try_get::<String, _>("role").map_err(backend)?)?,
                rights: RightSet::from_bits(rights as u8),
            });
        }
        Ok(out)
    }

    async fn create_alliance(
        &self,
        name: &str,
        tag: &str,
        founder: PlayerId,
    ) -> Result<AllianceId, RepoError> {
        let id = Uuid::new_v4();
        let mut tx = self.pool.begin().await.map_err(backend)?;
        sqlx::query("INSERT INTO alliances (id, name, tag, founder_id) VALUES ($1, $2, $3, $4)")
            .bind(id)
            .bind(name)
            .bind(tag)
            .bind(Uuid::from_u128(founder.0))
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                if is_unique_violation(&e) {
                    RepoError::Duplicate
                } else {
                    backend(e)
                }
            })?;
        sqlx::query(
            "INSERT INTO alliance_members (player_id, alliance_id, role, rights) \
             VALUES ($1, $2, 'founder', $3)",
        )
        .bind(Uuid::from_u128(founder.0))
        .bind(id)
        .bind(i32::from(RightSet::all().bits()))
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            if is_unique_violation(&e) {
                RepoError::Duplicate
            } else {
                backend(e)
            }
        })?;
        tx.commit().await.map_err(backend)?;
        Ok(AllianceId(id.as_u128()))
    }

    async fn insert_invite(
        &self,
        alliance: AllianceId,
        invitee: PlayerId,
    ) -> Result<(), RepoError> {
        sqlx::query("INSERT INTO alliance_invitations (alliance_id, invitee_id) VALUES ($1, $2)")
            .bind(Uuid::from_u128(alliance.0))
            .bind(Uuid::from_u128(invitee.0))
            .execute(&self.pool)
            .await
            .map_err(|e| {
                if is_unique_violation(&e) {
                    RepoError::Duplicate
                } else {
                    backend(e)
                }
            })?;
        Ok(())
    }

    async fn delete_invite(
        &self,
        alliance: AllianceId,
        invitee: PlayerId,
    ) -> Result<(), RepoError> {
        sqlx::query("DELETE FROM alliance_invitations WHERE alliance_id = $1 AND invitee_id = $2")
            .bind(Uuid::from_u128(alliance.0))
            .bind(Uuid::from_u128(invitee.0))
            .execute(&self.pool)
            .await
            .map_err(backend)?;
        Ok(())
    }

    async fn has_invite(&self, alliance: AllianceId, invitee: PlayerId) -> Result<bool, RepoError> {
        let found: Option<i32> = sqlx::query_scalar(
            "SELECT 1 FROM alliance_invitations WHERE alliance_id = $1 AND invitee_id = $2",
        )
        .bind(Uuid::from_u128(alliance.0))
        .bind(Uuid::from_u128(invitee.0))
        .fetch_optional(&self.pool)
        .await
        .map_err(backend)?;
        Ok(found.is_some())
    }

    async fn pending_invites_for(&self, player: PlayerId) -> Result<Vec<PendingInvite>, RepoError> {
        let rows = sqlx::query(
            "SELECT a.id, a.name, a.tag FROM alliance_invitations i \
             JOIN alliances a ON a.id = i.alliance_id \
             WHERE i.invitee_id = $1 ORDER BY i.created_at",
        )
        .bind(Uuid::from_u128(player.0))
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;
        let mut out = Vec::with_capacity(rows.len());
        for r in &rows {
            let aid: Uuid = r.try_get("id").map_err(backend)?;
            out.push(PendingInvite {
                alliance: AllianceId(aid.as_u128()),
                alliance_name: r.try_get("name").map_err(backend)?,
                alliance_tag: r.try_get("tag").map_err(backend)?,
            });
        }
        Ok(out)
    }

    async fn invites_of(&self, alliance: AllianceId) -> Result<Vec<OutgoingInvite>, RepoError> {
        let rows = sqlx::query(
            "SELECT i.invitee_id, u.username FROM alliance_invitations i \
             JOIN players pi ON pi.id = i.invitee_id JOIN users u ON u.id = pi.user_id \
             WHERE i.alliance_id = $1 ORDER BY u.username",
        )
        .bind(Uuid::from_u128(alliance.0))
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;
        let mut out = Vec::with_capacity(rows.len());
        for r in &rows {
            let pid: Uuid = r.try_get("invitee_id").map_err(backend)?;
            out.push(OutgoingInvite {
                invitee: PlayerId(pid.as_u128()),
                invitee_name: r.try_get("username").map_err(backend)?,
            });
        }
        Ok(out)
    }

    async fn add_member(
        &self,
        alliance: AllianceId,
        player: PlayerId,
        role: AllianceRole,
        rights: RightSet,
        cap: u32,
    ) -> Result<(), RepoError> {
        // A single guarded insert: only when the alliance is still below the cap (AC4). The player_id
        // PK rejects a player already in an alliance (→ Duplicate).
        let inserted = sqlx::query(
            "INSERT INTO alliance_members (player_id, alliance_id, role, rights) \
             SELECT $1, $2, $3, $4 \
             WHERE (SELECT count(*) FROM alliance_members WHERE alliance_id = $2) < $5",
        )
        .bind(Uuid::from_u128(player.0))
        .bind(Uuid::from_u128(alliance.0))
        .bind(alliance_role_str(role))
        .bind(i32::from(rights.bits()))
        .bind(i64::from(cap))
        .execute(&self.pool)
        .await
        .map_err(|e| {
            if is_unique_violation(&e) {
                RepoError::Duplicate
            } else {
                backend(e)
            }
        })?;
        if inserted.rows_affected() == 0 {
            return Err(RepoError::Conflict);
        }
        Ok(())
    }

    async fn remove_member(&self, player: PlayerId) -> Result<(), RepoError> {
        sqlx::query("DELETE FROM alliance_members WHERE player_id = $1")
            .bind(Uuid::from_u128(player.0))
            .execute(&self.pool)
            .await
            .map_err(backend)?;
        Ok(())
    }

    async fn set_member_role(
        &self,
        alliance: AllianceId,
        player: PlayerId,
        role: AllianceRole,
        rights: RightSet,
    ) -> Result<(), RepoError> {
        sqlx::query(
            "UPDATE alliance_members SET role = $3, rights = $4 \
             WHERE player_id = $1 AND alliance_id = $2",
        )
        .bind(Uuid::from_u128(player.0))
        .bind(Uuid::from_u128(alliance.0))
        .bind(alliance_role_str(role))
        .bind(i32::from(rights.bits()))
        .execute(&self.pool)
        .await
        .map_err(backend)?;
        Ok(())
    }

    async fn transfer_founder(
        &self,
        alliance: AllianceId,
        from: PlayerId,
        to: PlayerId,
    ) -> Result<(), RepoError> {
        let mut tx = self.pool.begin().await.map_err(backend)?;
        // The old founder becomes a plain member; the target becomes founder. Guarded on the alliance.
        sqlx::query(
            "UPDATE alliance_members SET role = 'member', rights = 0 \
             WHERE player_id = $1 AND alliance_id = $2",
        )
        .bind(Uuid::from_u128(from.0))
        .bind(Uuid::from_u128(alliance.0))
        .execute(&mut *tx)
        .await
        .map_err(backend)?;
        sqlx::query(
            "UPDATE alliance_members SET role = 'founder', rights = $3 \
             WHERE player_id = $1 AND alliance_id = $2",
        )
        .bind(Uuid::from_u128(to.0))
        .bind(Uuid::from_u128(alliance.0))
        .bind(i32::from(RightSet::all().bits()))
        .execute(&mut *tx)
        .await
        .map_err(backend)?;
        sqlx::query("UPDATE alliances SET founder_id = $2 WHERE id = $1")
            .bind(Uuid::from_u128(alliance.0))
            .bind(Uuid::from_u128(to.0))
            .execute(&mut *tx)
            .await
            .map_err(backend)?;
        tx.commit().await.map_err(backend)?;
        Ok(())
    }

    async fn disband(&self, alliance: AllianceId) -> Result<(), RepoError> {
        // Deleting the alliance cascades to members, invitations, and diplomacy (the FKs are
        // ON DELETE CASCADE), so the whole group is cleared in one statement.
        sqlx::query("DELETE FROM alliances WHERE id = $1")
            .bind(Uuid::from_u128(alliance.0))
            .execute(&self.pool)
            .await
            .map_err(backend)?;
        Ok(())
    }

    async fn diplomacy_state(
        &self,
        a: AllianceId,
        b: AllianceId,
    ) -> Result<Option<(DiplomacyStance, DiplomacyStatus, Option<AllianceId>)>, RepoError> {
        let (lo, hi) = normalise_pair(a, b);
        let row = sqlx::query(
            "SELECT stance, status, proposed_by FROM alliance_diplomacy \
             WHERE alliance_lo = $1 AND alliance_hi = $2",
        )
        .bind(lo)
        .bind(hi)
        .fetch_optional(&self.pool)
        .await
        .map_err(backend)?;
        let Some(r) = row else { return Ok(None) };
        let stance = parse_stance(&r.try_get::<String, _>("stance").map_err(backend)?)?;
        let status = parse_status(&r.try_get::<String, _>("status").map_err(backend)?)?;
        let proposed_by: Option<Uuid> = r.try_get("proposed_by").map_err(backend)?;
        Ok(Some((
            stance,
            status,
            proposed_by.map(|u| AllianceId(u.as_u128())),
        )))
    }

    async fn set_diplomacy_state(
        &self,
        a: AllianceId,
        b: AllianceId,
        stance: DiplomacyStance,
        status: DiplomacyStatus,
        proposed_by: Option<AllianceId>,
    ) -> Result<(), RepoError> {
        let (lo, hi) = normalise_pair(a, b);
        sqlx::query(
            "INSERT INTO alliance_diplomacy (alliance_lo, alliance_hi, stance, status, proposed_by) \
             VALUES ($1, $2, $3, $4, $5) \
             ON CONFLICT (alliance_lo, alliance_hi) \
             DO UPDATE SET stance = EXCLUDED.stance, status = EXCLUDED.status, \
                           proposed_by = EXCLUDED.proposed_by",
        )
        .bind(lo)
        .bind(hi)
        .bind(stance_str(stance))
        .bind(status_str(status))
        .bind(proposed_by.map(|p| Uuid::from_u128(p.0)))
        .execute(&self.pool)
        .await
        .map_err(backend)?;
        Ok(())
    }

    async fn clear_diplomacy(&self, a: AllianceId, b: AllianceId) -> Result<(), RepoError> {
        let (lo, hi) = normalise_pair(a, b);
        sqlx::query("DELETE FROM alliance_diplomacy WHERE alliance_lo = $1 AND alliance_hi = $2")
            .bind(lo)
            .bind(hi)
            .execute(&self.pool)
            .await
            .map_err(backend)?;
        Ok(())
    }

    async fn diplomacy_of(&self, alliance: AllianceId) -> Result<Vec<DiplomacyEntry>, RepoError> {
        // The alliance is either side of the normalised pair; join the *other* side for its name/tag.
        let me = Uuid::from_u128(alliance.0);
        let rows = sqlx::query(
            "SELECT d.stance, d.status, d.proposed_by, \
                    CASE WHEN d.alliance_lo = $1 THEN d.alliance_hi ELSE d.alliance_lo END AS other, \
                    o.name AS other_name, o.tag AS other_tag \
             FROM alliance_diplomacy d \
             JOIN alliances o ON o.id = CASE WHEN d.alliance_lo = $1 THEN d.alliance_hi \
                                                                     ELSE d.alliance_lo END \
             WHERE d.alliance_lo = $1 OR d.alliance_hi = $1 \
             ORDER BY o.name",
        )
        .bind(me)
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;
        let mut out = Vec::with_capacity(rows.len());
        for r in &rows {
            let other: Uuid = r.try_get("other").map_err(backend)?;
            let proposed_by: Option<Uuid> = r.try_get("proposed_by").map_err(backend)?;
            out.push(DiplomacyEntry {
                other: AllianceId(other.as_u128()),
                other_name: r.try_get("other_name").map_err(backend)?,
                other_tag: r.try_get("other_tag").map_err(backend)?,
                stance: parse_stance(&r.try_get::<String, _>("stance").map_err(backend)?)?,
                status: parse_status(&r.try_get::<String, _>("status").map_err(backend)?)?,
                proposed_by: proposed_by.map(|u| AllianceId(u.as_u128())),
            });
        }
        Ok(out)
    }

    async fn confederate_alliances(
        &self,
        alliance: AllianceId,
    ) -> Result<Vec<AllianceId>, RepoError> {
        let me = Uuid::from_u128(alliance.0);
        let rows = sqlx::query(
            "SELECT CASE WHEN alliance_lo = $1 THEN alliance_hi ELSE alliance_lo END AS other \
             FROM alliance_diplomacy \
             WHERE (alliance_lo = $1 OR alliance_hi = $1) \
               AND stance = 'confederation' AND status = 'active'",
        )
        .bind(me)
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;
        let mut out = Vec::with_capacity(rows.len());
        for r in &rows {
            let other: Uuid = r.try_get("other").map_err(backend)?;
            out.push(AllianceId(other.as_u128()));
        }
        Ok(out)
    }

    async fn alliance_member_villages(
        &self,
        alliances: &[AllianceId],
    ) -> Result<Vec<AlliedVillage>, RepoError> {
        if alliances.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<Uuid> = alliances.iter().map(|a| Uuid::from_u128(a.0)).collect();
        let rows = sqlx::query(
            "SELECT m.player_id, u.username, v.id, v.x, v.y \
             FROM alliance_members m \
             JOIN players pm ON pm.id = m.player_id JOIN users u ON u.id = pm.user_id \
             JOIN villages v ON v.owner_id = m.player_id \
             WHERE m.alliance_id = ANY($1) \
             ORDER BY u.username, v.id",
        )
        .bind(&ids)
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;
        let mut out = Vec::with_capacity(rows.len());
        for r in &rows {
            let pid: Uuid = r.try_get("player_id").map_err(backend)?;
            let vid: Uuid = r.try_get("id").map_err(backend)?;
            let x: i32 = r.try_get("x").map_err(backend)?;
            let y: i32 = r.try_get("y").map_err(backend)?;
            out.push(AlliedVillage {
                player: PlayerId(pid.as_u128()),
                owner_name: r.try_get("username").map_err(backend)?,
                village: VillageId(vid.as_u128()),
                coordinate: Coordinate::new(x, y),
            });
        }
        Ok(out)
    }

    async fn incoming_against(
        &self,
        villages: &[VillageId],
    ) -> Result<Vec<IncomingAttack>, RepoError> {
        if villages.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<Uuid> = villages.iter().map(|v| Uuid::from_u128(v.0)).collect();
        // Only attack/raid (force that lands), only in-transit, and **no** movement_troops join — the
        // composition stays hidden (P4/§7.3). Combat movements key the attacked village on
        // `deliver_village` (the troop-movement model, 009/010).
        let rows = sqlx::query(
            "SELECT deliver_village, dest_x, dest_y, \
                    (EXTRACT(EPOCH FROM arrive_at) * 1000)::bigint AS arrive_ms \
             FROM troop_movements \
             WHERE kind IN ('attack', 'raid') AND status = 'in_transit' \
               AND deliver_village = ANY($1) \
             ORDER BY arrive_at, id",
        )
        .bind(&ids)
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;
        let mut out = Vec::with_capacity(rows.len());
        for r in &rows {
            let tv: Uuid = r.try_get("deliver_village").map_err(backend)?;
            let x: i32 = r.try_get("dest_x").map_err(backend)?;
            let y: i32 = r.try_get("dest_y").map_err(backend)?;
            let arrive_ms: i64 = r.try_get("arrive_ms").map_err(backend)?;
            out.push(IncomingAttack {
                target: VillageId(tv.as_u128()),
                coordinate: Coordinate::new(x, y),
                arrive_at: Timestamp(arrive_ms),
            });
        }
        Ok(out)
    }

    // ---- Alliance forum (027) ----

    async fn create_thread(
        &self,
        alliance: AllianceId,
        author: PlayerId,
        title: &str,
        body: &str,
        announcement: bool,
        now: Timestamp,
    ) -> Result<u128, RepoError> {
        let thread_id = Uuid::new_v4();
        let mut tx = self.pool.begin().await.map_err(backend)?;
        sqlx::query(
            "INSERT INTO alliance_threads \
                (id, world_id, alliance_id, author_id, title, announcement, created_at, last_post_at) \
             VALUES ($1, $2, $3, $4, $5, $6, to_timestamp($7::double precision / 1000.0), \
                     to_timestamp($7::double precision / 1000.0))",
        )
        .bind(thread_id)
        .bind(Uuid::from_u128(self.world_id.0))
        .bind(Uuid::from_u128(alliance.0))
        .bind(Uuid::from_u128(author.0))
        .bind(title)
        .bind(announcement)
        .bind(now.0)
        .execute(&mut *tx)
        .await
        .map_err(backend)?;
        sqlx::query(
            "INSERT INTO alliance_posts (id, world_id, thread_id, author_id, body, created_at) \
             VALUES ($1, $2, $3, $4, $5, to_timestamp($6::double precision / 1000.0))",
        )
        .bind(Uuid::new_v4())
        .bind(Uuid::from_u128(self.world_id.0))
        .bind(thread_id)
        .bind(Uuid::from_u128(author.0))
        .bind(body)
        .bind(now.0)
        .execute(&mut *tx)
        .await
        .map_err(backend)?;
        tx.commit().await.map_err(backend)?;
        Ok(thread_id.as_u128())
    }

    async fn list_threads(
        &self,
        alliance: AllianceId,
        limit: i64,
    ) -> Result<Vec<ThreadSummary>, RepoError> {
        let rows = sqlx::query(
            "SELECT t.id, t.title, u.username AS author_name, t.announcement, \
                    (EXTRACT(EPOCH FROM t.last_post_at) * 1000)::bigint AS last_post_ms, \
                    (SELECT count(*) FROM alliance_posts p WHERE p.thread_id = t.id) AS post_count \
             FROM alliance_threads t JOIN players pt ON pt.id = t.author_id JOIN users u ON u.id = pt.user_id \
             WHERE t.world_id = $1 AND t.alliance_id = $2 \
             ORDER BY t.last_post_at DESC, t.id DESC LIMIT $3",
        )
        .bind(Uuid::from_u128(self.world_id.0))
        .bind(Uuid::from_u128(alliance.0))
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;
        rows.iter()
            .map(|r| {
                let id: Uuid = r.try_get("id").map_err(backend)?;
                Ok(ThreadSummary {
                    id: id.as_u128(),
                    title: r.try_get("title").map_err(backend)?,
                    author_name: r.try_get("author_name").map_err(backend)?,
                    announcement: r.try_get("announcement").map_err(backend)?,
                    post_count: r.try_get("post_count").map_err(backend)?,
                    last_post_ms: r.try_get("last_post_ms").map_err(backend)?,
                })
            })
            .collect()
    }

    async fn thread_head(&self, thread: u128) -> Result<Option<ThreadHead>, RepoError> {
        let row = sqlx::query(
            "SELECT alliance_id, title, announcement FROM alliance_threads \
             WHERE world_id = $1 AND id = $2",
        )
        .bind(Uuid::from_u128(self.world_id.0))
        .bind(Uuid::from_u128(thread))
        .fetch_optional(&self.pool)
        .await
        .map_err(backend)?;
        row.map(|r| {
            let aid: Uuid = r.try_get("alliance_id").map_err(backend)?;
            Ok(ThreadHead {
                alliance: AllianceId(aid.as_u128()),
                title: r.try_get("title").map_err(backend)?,
                announcement: r.try_get("announcement").map_err(backend)?,
            })
        })
        .transpose()
    }

    async fn add_post(
        &self,
        thread: u128,
        author: PlayerId,
        body: &str,
        now: Timestamp,
    ) -> Result<u128, RepoError> {
        let post_id = Uuid::new_v4();
        let mut tx = self.pool.begin().await.map_err(backend)?;
        sqlx::query(
            "INSERT INTO alliance_posts (id, world_id, thread_id, author_id, body, created_at) \
             VALUES ($1, $2, $3, $4, $5, to_timestamp($6::double precision / 1000.0))",
        )
        .bind(post_id)
        .bind(Uuid::from_u128(self.world_id.0))
        .bind(Uuid::from_u128(thread))
        .bind(Uuid::from_u128(author.0))
        .bind(body)
        .bind(now.0)
        .execute(&mut *tx)
        .await
        .map_err(backend)?;
        sqlx::query(
            "UPDATE alliance_threads SET last_post_at = to_timestamp($2::double precision / 1000.0) \
             WHERE id = $1",
        )
        .bind(Uuid::from_u128(thread))
        .bind(now.0)
        .execute(&mut *tx)
        .await
        .map_err(backend)?;
        tx.commit().await.map_err(backend)?;
        Ok(post_id.as_u128())
    }

    async fn list_posts(&self, thread: u128, limit: i64) -> Result<Vec<ForumPost>, RepoError> {
        // Fetch the **newest** `limit` posts (so a long thread shows recent replies, not just the opening
        // window), then return them oldest→newest for display.
        let mut rows = sqlx::query(
            "SELECT u.username AS author_name, p.body, \
                    (EXTRACT(EPOCH FROM p.created_at) * 1000)::bigint AS created_ms \
             FROM alliance_posts p JOIN players pa ON pa.id = p.author_id JOIN users u ON u.id = pa.user_id \
             WHERE p.world_id = $1 AND p.thread_id = $2 \
             ORDER BY p.created_at DESC, p.id DESC LIMIT $3",
        )
        .bind(Uuid::from_u128(self.world_id.0))
        .bind(Uuid::from_u128(thread))
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;
        rows.reverse();
        rows.iter()
            .map(|r| {
                Ok(ForumPost {
                    author_name: r.try_get("author_name").map_err(backend)?,
                    body: r.try_get("body").map_err(backend)?,
                    created_ms: r.try_get("created_ms").map_err(backend)?,
                })
            })
            .collect()
    }

    async fn search_alliances(
        &self,
        query: &str,
        limit: i64,
    ) -> Result<Vec<AllianceHit>, RepoError> {
        // Case-insensitive prefix on name OR tag; the user's LIKE metacharacters are escaped to literals.
        let rows = sqlx::query(
            "SELECT id, name, tag FROM alliances \
             WHERE lower(name) LIKE \
                   replace(replace(replace(lower($1), '\\', '\\\\'), '%', '\\%'), '_', '\\_') || '%' \
                   ESCAPE '\\' \
                OR lower(tag) LIKE \
                   replace(replace(replace(lower($1), '\\', '\\\\'), '%', '\\%'), '_', '\\_') || '%' \
                   ESCAPE '\\' \
             ORDER BY name ASC LIMIT $2",
        )
        .bind(query)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;
        rows.iter()
            .map(|r| {
                let id: Uuid = r.try_get("id").map_err(backend)?;
                Ok(AllianceHit {
                    alliance: AllianceId(id.as_u128()),
                    name: r.try_get("name").map_err(backend)?,
                    tag: r.try_get("tag").map_err(backend)?,
                })
            })
            .collect()
    }
}

// ---------------------------------------------------------------- settling (013)

#[async_trait]
impl SettleRepository for PgAccountRepository {
    #[allow(clippy::too_many_arguments)]
    async fn start_settle(
        &self,
        home: VillageId,
        owner: PlayerId,
        origin: Coordinate,
        target: Coordinate,
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
            MovementKind::Settle,
            home,
            origin,
            target,
            now,
            arrive_at,
            troops,
        )
        .await?;
        tx.commit().await.map_err(backend)?;
        Ok(())
    }

    async fn claim_due_settles(
        &self,
        now: Timestamp,
        limit: i64,
    ) -> Result<Vec<DueSettle>, RepoError> {
        let rows = sqlx::query(
            "UPDATE troop_movements SET status = 'processing' WHERE id IN ( \
                 SELECT id FROM troop_movements \
                 WHERE status = 'in_transit' AND kind = 'settle' \
                   AND arrive_at <= to_timestamp($1::double precision / 1000.0) \
                   AND home_village IN (SELECT id FROM villages WHERE world_id = $3) \
                 ORDER BY arrive_at, id LIMIT $2 FOR UPDATE SKIP LOCKED \
             ) RETURNING id, owner_id, home_village, origin_x, origin_y, dest_x, dest_y, \
                 (EXTRACT(EPOCH FROM arrive_at) * 1000)::bigint AS arrive_ms",
        )
        .bind(now.0 as f64)
        .bind(limit)
        .bind(Uuid::from_u128(self.world_id.0))
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
            out.push(DueSettle {
                id: id.as_u128(),
                owner: PlayerId(owner.as_u128()),
                home_village: VillageId(home.as_u128()),
                origin: Coordinate::new(
                    r.try_get("origin_x").map_err(backend)?,
                    r.try_get("origin_y").map_err(backend)?,
                ),
                target: Coordinate::new(
                    r.try_get("dest_x").map_err(backend)?,
                    r.try_get("dest_y").map_err(backend)?,
                ),
                arrive_at: Timestamp(arrive_ms),
                troops: troops.remove(id).unwrap_or_default(),
            });
        }
        Ok(out)
    }

    async fn apply_settle(
        &self,
        apply: SettleApply,
        template: &StartingVillage,
    ) -> Result<(), RepoError> {
        let world = Uuid::from_u128(self.world_id.0);
        let mut tx = self.pool.begin().await.map_err(backend)?;

        match apply.outcome {
            SettleOutcome::Found { culture_value } => {
                // The new village's fields come from the seeded tile (the application validated it is a
                // free valley); a concurrent founding on the same tile loses the unique insert.
                let Some(TileKind::Valley(distribution)) = self.map.tile_at(apply.target) else {
                    return Err(RepoError::Backend("settle target is not a valley".into()));
                };
                let vid = Uuid::new_v4();
                let inserted = sqlx::query(
                    "INSERT INTO villages (id, world_id, owner_id, x, y, tribe) \
                     VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT (world_id, x, y) DO NOTHING",
                )
                .bind(vid)
                .bind(world)
                .bind(Uuid::from_u128(apply.owner.0))
                .bind(apply.target.x)
                .bind(apply.target.y)
                .bind(apply.tribe.slug())
                .execute(&mut *tx)
                .await
                .map_err(backend)?;
                if inserted.rows_affected() == 0 {
                    // The tile was taken in flight — bounce on a later tick after re-validation.
                    return Err(RepoError::Conflict);
                }
                for (slot, f) in distribution.fields().iter().enumerate() {
                    sqlx::query(
                        "INSERT INTO village_fields (village_id, slot, resource_type, level) \
                         VALUES ($1, $2, $3, $4)",
                    )
                    .bind(vid)
                    .bind(slot as i16)
                    .bind(resource_str(f.kind))
                    .bind(i16::from(f.level))
                    .execute(&mut *tx)
                    .await
                    .map_err(backend)?;
                }
                for (slot, b) in template.buildings().iter().enumerate() {
                    sqlx::query(
                        "INSERT INTO village_buildings (village_id, slot, building_type, level) \
                         VALUES ($1, $2, $3, $4)",
                    )
                    .bind(vid)
                    .bind(slot as i16)
                    .bind(building_str(b.kind))
                    .bind(i16::from(b.level))
                    .execute(&mut *tx)
                    .await
                    .map_err(backend)?;
                }
                sqlx::query(
                    "INSERT INTO village_resources (village_id, wood, clay, iron, crop, updated_at) \
                     VALUES ($1, $2, $3, $4, $5, to_timestamp($6::double precision / 1000.0))",
                )
                .bind(vid)
                .bind(self.starting_amounts.wood)
                .bind(self.starting_amounts.clay)
                .bind(self.starting_amounts.iron)
                .bind(self.starting_amounts.crop)
                .bind(apply.battle_at.0 as f64)
                .execute(&mut *tx)
                .await
                .map_err(backend)?;

                // Re-anchor the player's culture at the founding instant: the new village joins the
                // (live) rate from here, so the prior period is credited at the old rate (013 AC1/P2).
                sqlx::query(
                    "INSERT INTO player_culture (player_id, value, updated_at) \
                     VALUES ($1, $2, to_timestamp($3::double precision / 1000.0)) \
                     ON CONFLICT (player_id) DO UPDATE SET value = EXCLUDED.value, updated_at = EXCLUDED.updated_at",
                )
                .bind(Uuid::from_u128(apply.owner.0))
                .bind(culture_value)
                .bind(apply.battle_at.0 as f64)
                .execute(&mut *tx)
                .await
                .map_err(backend)?;
            }
            SettleOutcome::Bounce { return_arrive } => {
                // The tile was taken or the slot lost — send the settlers home (a `return`).
                insert_movement(
                    &mut tx,
                    Uuid::new_v4(),
                    apply.owner,
                    MovementKind::Return,
                    apply.home_village,
                    apply.home_village,
                    apply.target,
                    apply.home_coord,
                    apply.battle_at,
                    return_arrive,
                    &apply.troops,
                )
                .await?;
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
    JOIN players psu ON psu.id = sr.scouter_player JOIN users su ON su.id = psu.user_id \
    JOIN villages sv ON sv.id = sr.scouter_village \
    JOIN players ptu ON ptu.id = sr.target_player JOIN users tu ON tu.id = ptu.user_id";

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
                   AND home_village IN (SELECT id FROM villages WHERE world_id = $3) \
                 ORDER BY arrive_at, id LIMIT $2 FOR UPDATE SKIP LOCKED \
             ) RETURNING id, owner_id, home_village, deliver_village, \
                 origin_x, origin_y, dest_x, dest_y, scout_target, \
                 (EXTRACT(EPOCH FROM arrive_at) * 1000)::bigint AS arrive_ms",
        )
        .bind(now.0 as f64)
        .bind(limit)
        .bind(Uuid::from_u128(self.world_id.0))
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

// ---------------------------------------------------------------- 016: ranking & statistics reads

/// A SQL `i32` board-scope code: `-1` = whole world, else the quadrant ordinal (NE 0, NW 1, SW 2, SE
/// 3 — matching `domain::quadrant`'s sign rule).
fn scope_code(scope: BoardScope) -> i32 {
    match scope {
        BoardScope::World => -1,
        BoardScope::Quadrant(Quadrant::Ne) => 0,
        BoardScope::Quadrant(Quadrant::Nw) => 1,
        BoardScope::Quadrant(Quadrant::Sw) => 2,
        BoardScope::Quadrant(Quadrant::Se) => 3,
    }
}

/// The quadrant of a candidate capital `cap`, as the same ordinal `scope_code` uses (P6 sign rule).
const CAPITAL_QUADRANT_CASE: &str = "(CASE WHEN cap.x >= 0 AND cap.y >= 0 THEN 0 \
     WHEN cap.x < 0 AND cap.y >= 0 THEN 1 WHEN cap.x < 0 AND cap.y < 0 THEN 2 ELSE 3 END)";

/// A `WHERE`-clause fragment that passes when the scope is the world (`bind` = -1) or the player's
/// **capital** lies in the scoped quadrant (016 AC7). `pid` is the player-id column to match on.
fn quadrant_filter(pid: &str, bind: &str) -> String {
    format!(
        "({bind} = -1 OR EXISTS (SELECT 1 FROM villages cap \
          WHERE cap.owner_id = {pid} AND cap.is_capital AND {CAPITAL_QUADRANT_CASE} = {bind}))"
    )
}

/// A correlated scalar SQL expression for one village's population (016 AC1), summing the per-level
/// field + building contributions from the balance tables passed as array binds (`p_*`). `vid` is the
/// village-id column to correlate on.
fn village_pop_expr(
    vid: &str,
    p_fields: &str,
    p_kinds: &str,
    p_levels: &str,
    p_pops: &str,
) -> String {
    format!(
        "(COALESCE((SELECT SUM(fp.pop) FROM village_fields vf \
            JOIN unnest({p_fields}::bigint[]) WITH ORDINALITY AS fp(pop, lvl) \
              ON (fp.lvl - 1) = vf.level WHERE vf.village_id = {vid}), 0) \
        + COALESCE((SELECT SUM(bp.pop) FROM village_buildings vb \
            JOIN unnest({p_kinds}::text[], {p_levels}::int[], {p_pops}::bigint[]) AS bp(kind, lvl, pop) \
              ON bp.kind = vb.building_type AND bp.lvl = vb.level WHERE vb.village_id = {vid}), 0))"
    )
}

/// Flatten the economy population balance into array binds: `(field_pops, b_kinds, b_levels, b_pops)`
/// (the per-level field table, and the building (kind, level, pop) triples).
/// Set-based per-player population board SQL (023 tuning) — two grouped aggregations over the world's
/// `village_fields`/`village_buildings`, instead of [`village_pop_expr`]'s two correlated subqueries per
/// village (O(villages)). `qf` is the quadrant filter on `p.id`. Placeholders: `$1` world, `$2` field-pops,
/// `$3`/`$4`/`$5` building kinds/levels/pops, `$6` quadrant, `$7` limit.
///
/// Driven `FROM (field_pop ∪ bldg_pop) owners` — the population CTEs are world-scoped (`v.world_id = $1`),
/// so each owner is a world-`$1` village owner = a world-`$1` player; the board then **PK-joins** `players`
/// (`p.id = owners.oid`) → `users` for the name (046). This both world-scopes and name-resolves without a
/// global `players`/`users` scan (keeping the 023/P11 scale budget) — second-world players (`p.id !=
/// user.id`) resolve correctly; home parity holds via the reuse-UUID invariant.
fn population_board_sql(qf: &str) -> String {
    format!(
        "WITH field_pop AS ( \
            SELECT v.owner_id AS oid, SUM(fp.pop) AS pop FROM villages v \
            JOIN village_fields vf ON vf.village_id = v.id \
            JOIN unnest($2::bigint[]) WITH ORDINALITY AS fp(pop, lvl) ON (fp.lvl - 1) = vf.level \
            WHERE v.world_id = $1 GROUP BY v.owner_id \
         ), bldg_pop AS ( \
            SELECT v.owner_id AS oid, SUM(bp.pop) AS pop FROM villages v \
            JOIN village_buildings vb ON vb.village_id = v.id \
            JOIN unnest($3::text[], $4::int[], $5::bigint[]) AS bp(kind, lvl, pop) \
              ON bp.kind = vb.building_type AND bp.lvl = vb.level \
            WHERE v.world_id = $1 GROUP BY v.owner_id \
         ) \
         SELECT p.id, u.username, (COALESCE(f.pop, 0) + COALESCE(b.pop, 0))::bigint AS total, \
                (EXTRACT(EPOCH FROM u.last_activity) * 1000)::bigint AS last_activity \
         FROM (SELECT oid FROM field_pop UNION SELECT oid FROM bldg_pop) owners \
         JOIN players p ON p.id = owners.oid \
         JOIN users u ON u.id = p.user_id \
         LEFT JOIN field_pop f ON f.oid = owners.oid \
         LEFT JOIN bldg_pop b ON b.oid = owners.oid \
         WHERE u.abandoned_at IS NULL AND u.is_npc = false AND {qf} \
           AND (COALESCE(f.pop, 0) + COALESCE(b.pop, 0)) > 0 \
         ORDER BY total DESC, p.id ASC LIMIT $7"
    )
}

fn population_arrays(econ: &EconomyRules) -> (Vec<i64>, Vec<String>, Vec<i32>, Vec<i64>) {
    let field_pops = econ.field_population_per_level.clone();
    let (mut kinds, mut levels, mut pops) = (Vec::new(), Vec::new(), Vec::new());
    for (kind, table) in &econ.building_population_per_level {
        for (level, &pop) in table.iter().enumerate() {
            kinds.push(building_str(*kind).to_owned());
            levels.push(i32::try_from(level).unwrap_or(i32::MAX));
            pops.push(pop);
        }
    }
    (field_pops, kinds, levels, pops)
}

/// `(table, value_expr, player_id_col, occurred_at_col)` for a conflict metric (016 AC5/AC6).
fn conflict_source(
    metric: ConflictMetric,
) -> (&'static str, &'static str, &'static str, &'static str) {
    match metric {
        ConflictMetric::Attack => (
            "battle_reports br",
            "br.attack_points",
            "br.attacker_player",
            "br.occurred_at",
        ),
        ConflictMetric::Defense => (
            "battle_defenders bd",
            "bd.defense_points",
            "bd.player_id",
            "bd.occurred_at",
        ),
        ConflictMetric::Raided => (
            "battle_reports br",
            "(br.loot_wood + br.loot_clay + br.loot_iron + br.loot_crop)",
            "br.attacker_player",
            "br.occurred_at",
        ),
    }
}

#[async_trait]
impl RankingRepository for PgAccountRepository {
    async fn population_board(
        &self,
        econ: &EconomyRules,
        scope: BoardScope,
        limit: i64,
    ) -> Result<Vec<LeaderboardRow>, RepoError> {
        let (fields, kinds, levels, pops) = population_arrays(econ);
        let sql = population_board_sql(&quadrant_filter("p.id", "$6"));
        let rows: Vec<(Uuid, String, i64, i64)> = sqlx::query_as(&sql)
            .bind(Uuid::from_u128(self.world_id.0))
            .bind(&fields)
            .bind(&kinds)
            .bind(&levels)
            .bind(&pops)
            .bind(scope_code(scope))
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(backend)?;
        Ok(rows.into_iter().map(leaderboard_row).collect())
    }

    async fn conflict_board(
        &self,
        metric: ConflictMetric,
        scope: BoardScope,
        since: Option<Timestamp>,
        until: Option<Timestamp>,
        limit: i64,
    ) -> Result<Vec<LeaderboardRow>, RepoError> {
        let (table, val, pid, occ) = conflict_source(metric);
        let qf = quadrant_filter(pid, "$3");
        // 046: the player id is world-specific, so `JOIN players … AND world_id = $5` both world-scopes
        // (battle tables carry no world_id) and resolves the name (`p.user_id → users`).
        let sql = format!(
            "SELECT p.id, u.username, COALESCE(SUM({val}), 0)::bigint AS total, \
                    (EXTRACT(EPOCH FROM u.last_activity) * 1000)::bigint AS last_activity \
             FROM {table} JOIN players p ON p.id = {pid} AND p.world_id = $5 \
             JOIN users u ON u.id = p.user_id \
             WHERE ($1::double precision IS NULL OR {occ} >= to_timestamp($1 / 1000.0)) \
               AND ($2::double precision IS NULL OR {occ} < to_timestamp($2 / 1000.0)) AND {qf} \
               AND u.abandoned_at IS NULL AND u.is_npc = false \
             GROUP BY p.id, u.username, u.last_activity HAVING COALESCE(SUM({val}), 0) > 0 \
             ORDER BY total DESC, p.id ASC LIMIT $4"
        );
        let rows: Vec<(Uuid, String, i64, i64)> = sqlx::query_as(&sql)
            .bind(since.map(|t| t.0 as f64))
            .bind(until.map(|t| t.0 as f64))
            .bind(scope_code(scope))
            .bind(limit)
            .bind(Uuid::from_u128(self.world_id.0))
            .fetch_all(&self.pool)
            .await
            .map_err(backend)?;
        Ok(rows.into_iter().map(leaderboard_row).collect())
    }

    async fn alliance_population_board(
        &self,
        econ: &EconomyRules,
        scope: BoardScope,
        limit: i64,
    ) -> Result<Vec<AllianceLeaderboardRow>, RepoError> {
        let (fields, kinds, levels, pops) = population_arrays(econ);
        let pop = village_pop_expr("v.id", "$2", "$3", "$4", "$5");
        let qf = quadrant_filter("am.player_id", "$6");
        let sql = format!(
            "SELECT a.id, a.name, a.tag, COALESCE(SUM({pop}), 0)::bigint AS total \
             FROM alliances a \
             JOIN alliance_members am ON am.alliance_id = a.id \
             JOIN players p ON p.id = am.player_id AND p.world_id = $1 \
             JOIN users u ON u.id = p.user_id \
             JOIN villages v ON v.owner_id = am.player_id AND v.world_id = $1 \
             WHERE {qf} AND u.abandoned_at IS NULL AND u.is_npc = false \
             GROUP BY a.id, a.name, a.tag HAVING COALESCE(SUM({pop}), 0) > 0 \
             ORDER BY total DESC, a.id ASC LIMIT $7"
        );
        let rows: Vec<(Uuid, String, String, i64)> = sqlx::query_as(&sql)
            .bind(Uuid::from_u128(self.world_id.0))
            .bind(&fields)
            .bind(&kinds)
            .bind(&levels)
            .bind(&pops)
            .bind(scope_code(scope))
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(backend)?;
        Ok(rows.into_iter().map(alliance_row).collect())
    }

    async fn alliance_conflict_board(
        &self,
        metric: ConflictMetric,
        scope: BoardScope,
        since: Option<Timestamp>,
        until: Option<Timestamp>,
        limit: i64,
    ) -> Result<Vec<AllianceLeaderboardRow>, RepoError> {
        let (table, val, pid, occ) = conflict_source(metric);
        let qf = quadrant_filter("am.player_id", "$3");
        // 046: world-scope the members to this world's players (`p.world_id = $5`) + resolve the name.
        let sql = format!(
            "SELECT a.id, a.name, a.tag, COALESCE(SUM(pv.val), 0)::bigint AS total \
             FROM alliances a \
             JOIN alliance_members am ON am.alliance_id = a.id \
             JOIN players p ON p.id = am.player_id AND p.world_id = $5 \
             JOIN users u ON u.id = p.user_id \
             JOIN (SELECT {pid} AS pid, SUM({val}) AS val FROM {table} \
                   WHERE ($1::double precision IS NULL OR {occ} >= to_timestamp($1 / 1000.0)) \
                     AND ($2::double precision IS NULL OR {occ} < to_timestamp($2 / 1000.0)) \
                   GROUP BY {pid}) pv ON pv.pid = am.player_id \
             WHERE {qf} AND u.abandoned_at IS NULL AND u.is_npc = false \
             GROUP BY a.id, a.name, a.tag HAVING COALESCE(SUM(pv.val), 0) > 0 \
             ORDER BY total DESC, a.id ASC LIMIT $4"
        );
        let rows: Vec<(Uuid, String, String, i64)> = sqlx::query_as(&sql)
            .bind(since.map(|t| t.0 as f64))
            .bind(until.map(|t| t.0 as f64))
            .bind(scope_code(scope))
            .bind(limit)
            .bind(Uuid::from_u128(self.world_id.0))
            .fetch_all(&self.pool)
            .await
            .map_err(backend)?;
        Ok(rows.into_iter().map(alliance_row).collect())
    }

    async fn player_stats(
        &self,
        econ: &EconomyRules,
        player: PlayerId,
    ) -> Result<Option<PlayerStats>, RepoError> {
        let pid = Uuid::from_u128(player.0);
        // 019 AC8: an abandoned account is hidden from its stat page (treated as not found). 046: the name
        // resolves through `players`, and a player not in this repo's world is treated as not found.
        let Some(name): Option<String> = sqlx::query_scalar(
            "SELECT u.username FROM players p JOIN users u ON u.id = p.user_id \
             WHERE p.id = $1 AND p.world_id = $2 AND u.abandoned_at IS NULL AND u.is_npc = false",
        )
        .bind(pid)
        .bind(Uuid::from_u128(self.world_id.0))
        .fetch_optional(&self.pool)
        .await
        .map_err(backend)?
        else {
            return Ok(None);
        };
        let pop = village_pop_expr("v.id", "$2", "$3", "$4", "$5");
        let (fields, kinds, levels, pops) = population_arrays(econ);
        let vsql = format!(
            "SELECT v.id, v.x, v.y, {pop}::bigint AS pop \
             FROM villages v WHERE v.owner_id = $1 AND v.world_id = $6 ORDER BY v.created_at"
        );
        let vrows: Vec<(Uuid, i32, i32, i64)> = sqlx::query_as(&vsql)
            .bind(pid)
            .bind(&fields)
            .bind(&kinds)
            .bind(&levels)
            .bind(&pops)
            .bind(Uuid::from_u128(self.world_id.0))
            .fetch_all(&self.pool)
            .await
            .map_err(backend)?;
        let villages: Vec<(VillageId, Coordinate, i64)> = vrows
            .into_iter()
            .map(|(id, x, y, pop)| (VillageId(id.as_u128()), Coordinate::new(x, y), pop))
            .collect();
        let population = villages.iter().map(|(_, _, p)| p).sum();
        let attack_points: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(attack_points), 0)::bigint FROM battle_reports \
             WHERE attacker_player = $1",
        )
        .bind(pid)
        .fetch_one(&self.pool)
        .await
        .map_err(backend)?;
        let defense_points: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(defense_points), 0)::bigint FROM battle_defenders \
             WHERE player_id = $1",
        )
        .bind(pid)
        .fetch_one(&self.pool)
        .await
        .map_err(backend)?;
        let loot_total: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(loot_wood + loot_clay + loot_iron + loot_crop), 0)::bigint \
             FROM battle_reports WHERE attacker_player = $1",
        )
        .bind(pid)
        .fetch_one(&self.pool)
        .await
        .map_err(backend)?;
        Ok(Some(PlayerStats {
            player,
            name,
            population,
            villages,
            attack_points,
            defense_points,
            loot_total,
        }))
    }

    async fn alliance_stats(
        &self,
        econ: &EconomyRules,
        alliance: AllianceId,
    ) -> Result<Option<AllianceStats>, RepoError> {
        let aid = Uuid::from_u128(alliance.0);
        // 046: an alliance with no member in this repo's world is not in this world — treat as not found
        // (symmetry with `player_stats`' world guard), so a foreign-world alliance id yields no page.
        let Some((name, tag)): Option<(String, String)> = sqlx::query_as(
            "SELECT a.name, a.tag FROM alliances a \
             WHERE a.id = $1 AND EXISTS (SELECT 1 FROM alliance_members am \
               JOIN players p ON p.id = am.player_id \
               WHERE am.alliance_id = a.id AND p.world_id = $2)",
        )
        .bind(aid)
        .bind(Uuid::from_u128(self.world_id.0))
        .fetch_optional(&self.pool)
        .await
        .map_err(backend)?
        else {
            return Ok(None);
        };
        let pop = village_pop_expr("v.id", "$3", "$4", "$5", "$6");
        let (fields, kinds, levels, pops) = population_arrays(econ);
        let sql = format!(
            "SELECT am.player_id, u.username, \
               COALESCE((SELECT SUM({pop}) FROM villages v WHERE v.owner_id = am.player_id AND v.world_id = $2), 0)::bigint AS pop, \
               COALESCE((SELECT SUM(attack_points) FROM battle_reports WHERE attacker_player = am.player_id), 0)::bigint AS atk, \
               COALESCE((SELECT SUM(defense_points) FROM battle_defenders WHERE player_id = am.player_id), 0)::bigint AS def \
             FROM alliance_members am JOIN players p ON p.id = am.player_id JOIN users u ON u.id = p.user_id \
             WHERE am.alliance_id = $1 ORDER BY pop DESC, am.player_id ASC"
        );
        let rows: Vec<(Uuid, String, i64, i64, i64)> = sqlx::query_as(&sql)
            .bind(aid)
            .bind(Uuid::from_u128(self.world_id.0))
            .bind(&fields)
            .bind(&kinds)
            .bind(&levels)
            .bind(&pops)
            .fetch_all(&self.pool)
            .await
            .map_err(backend)?;
        let members: Vec<(PlayerId, String, i64, i64, i64)> = rows
            .into_iter()
            .map(|(id, n, p, a, d)| (PlayerId(id.as_u128()), n, p, a, d))
            .collect();
        Ok(Some(AllianceStats {
            alliance,
            name,
            tag,
            population: members.iter().map(|(_, _, p, _, _)| p).sum(),
            attack_points: members.iter().map(|(_, _, _, a, _)| a).sum(),
            defense_points: members.iter().map(|(_, _, _, _, d)| d).sum(),
            members,
        }))
    }

    async fn defender_reports_for(
        &self,
        player: PlayerId,
        limit: i64,
    ) -> Result<Vec<DefenderReport>, RepoError> {
        let rows = sqlx::query(
            "SELECT battle_id, village_id, is_owner, forces, losses, defense_points, \
                    (EXTRACT(EPOCH FROM occurred_at) * 1000)::bigint AS occ_ms \
             FROM battle_defenders WHERE player_id = $1 ORDER BY occurred_at DESC LIMIT $2",
        )
        .bind(Uuid::from_u128(player.0))
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;
        let mut out = Vec::with_capacity(rows.len());
        for r in &rows {
            let battle_id: Uuid = r.try_get("battle_id").map_err(backend)?;
            let village_id: Uuid = r.try_get("village_id").map_err(backend)?;
            let forces: serde_json::Value = r.try_get("forces").map_err(backend)?;
            let losses: serde_json::Value = r.try_get("losses").map_err(backend)?;
            let occ_ms: i64 = r.try_get("occ_ms").map_err(backend)?;
            out.push(DefenderReport {
                battle_id: battle_id.as_u128(),
                occurred_at: Timestamp(occ_ms),
                at_village: VillageId(village_id.as_u128()),
                is_owner: r.try_get("is_owner").map_err(backend)?,
                forces: counts_from_json(&forces),
                losses: counts_from_json(&losses),
                defense_points: r.try_get("defense_points").map_err(backend)?,
            });
        }
        Ok(out)
    }
}

/// Map a `(id, name, value)` row to a [`LeaderboardRow`].
fn leaderboard_row((id, name, value, last_activity): (Uuid, String, i64, i64)) -> LeaderboardRow {
    LeaderboardRow {
        player: PlayerId(id.as_u128()),
        name,
        value,
        last_activity: Timestamp(last_activity),
    }
}

/// Map an `(id, name, tag, value)` row to an [`AllianceLeaderboardRow`].
fn alliance_row((id, name, tag, value): (Uuid, String, String, i64)) -> AllianceLeaderboardRow {
    AllianceLeaderboardRow {
        alliance: AllianceId(id.as_u128()),
        name,
        tag,
        value,
    }
}

// ---------------------------------------------------------------- 017: medals & population snapshots

/// The climber metric — population gained from the previous snapshot — as a SQL expression over a
/// `cur`/`prev` snapshot join. Shared by the climbers board and the settlement so the delta + filter +
/// tie-break never drift (P6/AC13).
const CLIMBER_DELTA: &str = "(cur.population - COALESCE(prev.population, 0))";

/// Credit a one-time reward to a player's capital (or oldest village) inside an open transaction
/// (017/018): culture points added, resources credited capped at the capital's storage, and a troop
/// count upserted into the capital's garrison. Any component may be empty/`None`.
async fn credit_reward(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    econ: &EconomyRules,
    world: Uuid,
    player: Uuid,
    resources: ResourceAmounts,
    culture: i64,
    troops: Option<(&UnitId, u32)>,
) -> Result<(), RepoError> {
    if culture != 0 {
        sqlx::query(
            "INSERT INTO player_culture (player_id, value, updated_at) VALUES ($1, $2, now()) \
             ON CONFLICT (player_id) DO UPDATE SET value = player_culture.value + $2",
        )
        .bind(player)
        .bind(culture)
        .execute(&mut **tx)
        .await
        .map_err(backend)?;
    }
    let has_resources =
        resources.wood > 0 || resources.clay > 0 || resources.iron > 0 || resources.crop > 0;
    if !has_resources && troops.is_none() {
        return Ok(());
    }
    // The capital (else the oldest village) receives resource/troop rewards.
    let Some(row) = sqlx::query(
        "SELECT id FROM villages WHERE owner_id = $1 AND world_id = $2 \
         ORDER BY is_capital DESC, created_at ASC LIMIT 1",
    )
    .bind(player)
    .bind(world)
    .fetch_optional(&mut **tx)
    .await
    .map_err(backend)?
    else {
        return Ok(());
    };
    let cap_id: Uuid = row.try_get("id").map_err(backend)?;
    if has_resources {
        let brows =
            sqlx::query("SELECT building_type, level FROM village_buildings WHERE village_id = $1")
                .bind(cap_id)
                .fetch_all(&mut **tx)
                .await
                .map_err(backend)?;
        let mut buildings = Vec::with_capacity(brows.len());
        for br in &brows {
            let kind = parse_building(&br.try_get::<String, _>("building_type").map_err(backend)?)?;
            let level: i16 = br.try_get("level").map_err(backend)?;
            buildings.push(BuildingSlot {
                kind,
                level: u8::try_from(level).unwrap_or(0),
            });
        }
        let caps = capacities(&buildings, econ);
        let (w, c, i, cr): (i64, i64, i64, i64) = sqlx::query_as(
            "SELECT wood, clay, iron, crop FROM village_resources WHERE village_id = $1",
        )
        .bind(cap_id)
        .fetch_one(&mut **tx)
        .await
        .map_err(backend)?;
        let after = deposit_capped(
            ResourceAmounts {
                wood: w,
                clay: c,
                iron: i,
                crop: cr,
            },
            resources,
            caps,
        );
        sqlx::query(
            "UPDATE village_resources SET wood = $2, clay = $3, iron = $4, crop = $5 \
             WHERE village_id = $1",
        )
        .bind(cap_id)
        .bind(after.wood)
        .bind(after.clay)
        .bind(after.iron)
        .bind(after.crop)
        .execute(&mut **tx)
        .await
        .map_err(backend)?;
    }
    if let Some((unit, count)) = troops {
        sqlx::query(
            "INSERT INTO village_units (village_id, unit_id, count) VALUES ($1, $2, $3) \
             ON CONFLICT (village_id, unit_id) DO UPDATE SET count = village_units.count + EXCLUDED.count",
        )
        .bind(cap_id)
        .bind(&unit.0)
        .bind(i32::try_from(count).unwrap_or(i32::MAX))
        .execute(&mut **tx)
        .await
        .map_err(backend)?;
    }
    Ok(())
}

/// Insert one medal row inside an open transaction, idempotent per `(period, category, rank)`.
async fn insert_medal(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    period: i64,
    category: &str,
    rank: i32,
    subject_kind: &str,
    subject_id: Uuid,
) -> Result<(), RepoError> {
    sqlx::query(
        "INSERT INTO medals (id, period, category, rank, subject_kind, subject_id) \
         VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT (period, category, rank) DO NOTHING",
    )
    .bind(Uuid::new_v4())
    .bind(period)
    .bind(category)
    .bind(rank)
    .bind(subject_kind)
    .bind(subject_id)
    .execute(&mut **tx)
    .await
    .map_err(backend)?;
    Ok(())
}

#[async_trait]
impl MedalRepository for PgAccountRepository {
    async fn latest_settled_period(&self) -> Result<Option<i64>, RepoError> {
        sqlx::query_scalar("SELECT MAX(period) FROM population_snapshots WHERE world_id = $1")
            .bind(Uuid::from_u128(self.world_id.0))
            .fetch_one(&self.pool)
            .await
            .map_err(backend)
    }

    async fn snapshot_population(&self, econ: &EconomyRules, period: i64) -> Result<(), RepoError> {
        let (fields, kinds, levels, pops) = population_arrays(econ);
        let pop = village_pop_expr("v.id", "$2", "$3", "$4", "$5");
        // One snapshot per player (every owner of a village in this world); idempotent per period.
        let sql = format!(
            "INSERT INTO population_snapshots (world_id, player_id, period, population) \
             SELECT $1, v.owner_id, $6, SUM({pop})::bigint \
             FROM villages v WHERE v.world_id = $1 \
             GROUP BY v.owner_id ON CONFLICT DO NOTHING"
        );
        sqlx::query(&sql)
            .bind(Uuid::from_u128(self.world_id.0))
            .bind(&fields)
            .bind(&kinds)
            .bind(&levels)
            .bind(&pops)
            .bind(period)
            .execute(&self.pool)
            .await
            .map_err(backend)?;
        Ok(())
    }

    async fn award_medals(&self, period: i64, awards: &[MedalAward]) -> Result<(), RepoError> {
        let mut tx = self.pool.begin().await.map_err(backend)?;
        for a in awards {
            insert_medal(
                &mut tx,
                period,
                a.category.as_str(),
                i32::try_from(a.rank).unwrap_or(i32::MAX),
                a.subject_kind.as_str(),
                Uuid::from_u128(a.subject_id),
            )
            .await?;
        }
        tx.commit().await.map_err(backend)?;
        Ok(())
    }

    async fn settle_period(
        &self,
        econ: &EconomyRules,
        period: i64,
        climber_limit: Option<i64>,
        awards: &[MedalAward],
    ) -> Result<(), RepoError> {
        let world = Uuid::from_u128(self.world_id.0);
        let mut tx = self.pool.begin().await.map_err(backend)?;

        // 1. Snapshot population for the period (idempotent). This advances the watermark, so it must
        //    commit in the same transaction as the medals (AC6).
        let (fields, kinds, levels, pops) = population_arrays(econ);
        let pop = village_pop_expr("v.id", "$2", "$3", "$4", "$5");
        let snap_sql = format!(
            "INSERT INTO population_snapshots (world_id, player_id, period, population) \
             SELECT $1, v.owner_id, $6, SUM({pop})::bigint \
             FROM villages v WHERE v.world_id = $1 GROUP BY v.owner_id ON CONFLICT DO NOTHING"
        );
        sqlx::query(&snap_sql)
            .bind(world)
            .bind(&fields)
            .bind(&kinds)
            .bind(&levels)
            .bind(&pops)
            .bind(period)
            .execute(&mut *tx)
            .await
            .map_err(backend)?;

        // 2. Climber medals — top population gainers `period` vs `period-1`, read from the snapshot
        //    just written (within this transaction). World-scoped (medals are world-wide).
        if let Some(limit) = climber_limit {
            let delta = CLIMBER_DELTA;
            let climber_sql = format!(
                "SELECT cur.player_id FROM population_snapshots cur \
                 LEFT JOIN population_snapshots prev \
                   ON prev.world_id = cur.world_id AND prev.player_id = cur.player_id \
                      AND prev.period = $2 \
                 WHERE cur.world_id = $1 AND cur.period = $3 AND {delta} > 0 \
                 ORDER BY {delta} DESC, cur.player_id ASC LIMIT $4"
            );
            let rows: Vec<(Uuid,)> = sqlx::query_as(&climber_sql)
                .bind(world)
                .bind(period - 1)
                .bind(period)
                .bind(limit)
                .fetch_all(&mut *tx)
                .await
                .map_err(backend)?;
            for (i, (player,)) in rows.iter().enumerate() {
                insert_medal(
                    &mut tx,
                    period,
                    MedalCategory::Climber.as_str(),
                    i32::try_from(i + 1).unwrap_or(i32::MAX),
                    MedalSubjectKind::Player.as_str(),
                    *player,
                )
                .await?;
            }
        }

        // 3. The pre-computed non-climber medals.
        for a in awards {
            insert_medal(
                &mut tx,
                period,
                a.category.as_str(),
                i32::try_from(a.rank).unwrap_or(i32::MAX),
                a.subject_kind.as_str(),
                Uuid::from_u128(a.subject_id),
            )
            .await?;
        }

        tx.commit().await.map_err(backend)?;
        Ok(())
    }

    async fn medals_for(
        &self,
        subject_kind: MedalSubjectKind,
        subject_id: u128,
    ) -> Result<Vec<MedalView>, RepoError> {
        let rows: Vec<(i64, String, i32)> = sqlx::query_as(
            "SELECT period, category, rank FROM medals \
             WHERE subject_kind = $1 AND subject_id = $2 ORDER BY awarded_at DESC, rank ASC",
        )
        .bind(subject_kind.as_str())
        .bind(Uuid::from_u128(subject_id))
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;
        Ok(rows
            .into_iter()
            .filter_map(|(period, cat, rank)| {
                MedalCategory::parse(&cat).map(|category| MedalView {
                    period,
                    category,
                    rank: i64::from(rank),
                })
            })
            .collect())
    }

    async fn climber_board(
        &self,
        period: i64,
        prev: i64,
        scope: BoardScope,
        limit: i64,
    ) -> Result<Vec<LeaderboardRow>, RepoError> {
        let qf = quadrant_filter("cur.player_id", "$4");
        let delta = CLIMBER_DELTA;
        let sql = format!(
            "SELECT cur.player_id, u.username, {delta}::bigint AS delta, \
                    (EXTRACT(EPOCH FROM u.last_activity) * 1000)::bigint AS last_activity \
             FROM population_snapshots cur \
             JOIN players p ON p.id = cur.player_id \
             JOIN users u ON u.id = p.user_id \
             LEFT JOIN population_snapshots prev \
               ON prev.world_id = cur.world_id AND prev.player_id = cur.player_id AND prev.period = $2 \
             WHERE cur.world_id = $1 AND cur.period = $3 AND {delta} > 0 AND {qf} \
               AND u.abandoned_at IS NULL AND u.is_npc = false \
             ORDER BY {delta} DESC, cur.player_id ASC LIMIT $5"
        );
        let rows: Vec<(Uuid, String, i64, i64)> = sqlx::query_as(&sql)
            .bind(Uuid::from_u128(self.world_id.0))
            .bind(prev)
            .bind(period)
            .bind(scope_code(scope))
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(backend)?;
        Ok(rows.into_iter().map(leaderboard_row).collect())
    }

    async fn population_history(&self, player: PlayerId) -> Result<Vec<(i64, i64)>, RepoError> {
        sqlx::query_as(
            "SELECT period, population FROM population_snapshots \
             WHERE world_id = $1 AND player_id = $2 ORDER BY period",
        )
        .bind(Uuid::from_u128(self.world_id.0))
        .bind(Uuid::from_u128(player.0))
        .fetch_all(&self.pool)
        .await
        .map_err(backend)
    }
}

// ---------------------------------------------------------------- 017: achievements

#[async_trait]
impl AchievementRepository for PgAccountRepository {
    async fn held_achievements(
        &self,
        player: PlayerId,
    ) -> Result<HashSet<AchievementId>, RepoError> {
        let rows: Vec<(String,)> =
            sqlx::query_as("SELECT achievement_id FROM player_achievements WHERE player_id = $1")
                .bind(Uuid::from_u128(player.0))
                .fetch_all(&self.pool)
                .await
                .map_err(backend)?;
        Ok(rows.into_iter().map(|(id,)| AchievementId(id)).collect())
    }

    async fn player_progress(
        &self,
        econ: &EconomyRules,
        player: PlayerId,
    ) -> Result<PlayerProgress, RepoError> {
        let pid = Uuid::from_u128(player.0);
        let world = Uuid::from_u128(self.world_id.0);
        let village_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM villages WHERE owner_id = $1 AND world_id = $2",
        )
        .bind(pid)
        .bind(world)
        .fetch_one(&self.pool)
        .await
        .map_err(backend)?;
        let defensive_wins: i64 = sqlx::query_scalar(
            "SELECT count(DISTINCT bd.battle_id) FROM battle_defenders bd \
             JOIN battle_reports br ON br.id = bd.battle_id \
             WHERE bd.player_id = $1 AND br.attacker_won = false",
        )
        .bind(pid)
        .fetch_one(&self.pool)
        .await
        .map_err(backend)?;
        let oases_held: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM oases o JOIN villages v ON v.id = o.owner_village \
             WHERE v.owner_id = $1 AND o.world_id = $2",
        )
        .bind(pid)
        .bind(world)
        .fetch_one(&self.pool)
        .await
        .map_err(backend)?;
        let units_researched: i64 = sqlx::query_scalar(
            "SELECT count(DISTINCT r.unit_id) FROM village_research r \
             JOIN villages v ON v.id = r.village_id WHERE v.owner_id = $1",
        )
        .bind(pid)
        .fetch_one(&self.pool)
        .await
        .map_err(backend)?;
        let (fields, kinds, levels, pops) = population_arrays(econ);
        let pop_expr = village_pop_expr("v.id", "$3", "$4", "$5", "$6");
        let population: i64 = sqlx::query_scalar(&format!(
            "SELECT COALESCE(SUM({pop_expr}), 0)::bigint FROM villages v \
             WHERE v.owner_id = $1 AND v.world_id = $2"
        ))
        .bind(pid)
        .bind(world)
        .bind(&fields)
        .bind(&kinds)
        .bind(&levels)
        .bind(&pops)
        .fetch_one(&self.pool)
        .await
        .map_err(backend)?;
        Ok(PlayerProgress {
            village_count,
            defensive_wins,
            oases_held,
            population,
            units_researched,
            tribe_unit_count: 0, // filled by the caller from the unit roster
        })
    }

    async fn grant_achievement(
        &self,
        econ: &EconomyRules,
        player: PlayerId,
        def: &AchievementDef,
    ) -> Result<bool, RepoError> {
        let pid = Uuid::from_u128(player.0);
        let world = Uuid::from_u128(self.world_id.0);
        let mut tx = self.pool.begin().await.map_err(backend)?;
        let inserted = sqlx::query(
            "INSERT INTO player_achievements (player_id, achievement_id) VALUES ($1, $2) \
             ON CONFLICT DO NOTHING",
        )
        .bind(pid)
        .bind(&def.id.0)
        .execute(&mut *tx)
        .await
        .map_err(backend)?;
        if inserted.rows_affected() == 0 {
            tx.commit().await.map_err(backend)?;
            return Ok(false); // already held — no double grant or reward
        }
        match &def.reward {
            Reward::None => {}
            Reward::Culture(cp) => {
                credit_reward(
                    &mut tx,
                    econ,
                    world,
                    pid,
                    ResourceAmounts::default(),
                    *cp,
                    None,
                )
                .await?;
            }
            Reward::Resources(amount) => {
                credit_reward(&mut tx, econ, world, pid, *amount, 0, None).await?;
            }
        }
        tx.commit().await.map_err(backend)?;
        Ok(true)
    }
}

#[async_trait]
impl QuestRepository for PgAccountRepository {
    async fn completed_quests(&self, player: PlayerId) -> Result<HashSet<QuestId>, RepoError> {
        let rows: Vec<(String,)> =
            sqlx::query_as("SELECT quest_id FROM player_quests WHERE player_id = $1")
                .bind(Uuid::from_u128(player.0))
                .fetch_all(&self.pool)
                .await
                .map_err(backend)?;
        Ok(rows.into_iter().map(|(id,)| QuestId(id)).collect())
    }

    async fn quest_progress(
        &self,
        econ: &EconomyRules,
        player: PlayerId,
    ) -> Result<QuestProgress, RepoError> {
        let pid = Uuid::from_u128(player.0);
        let world = Uuid::from_u128(self.world_id.0);
        let max_field_level: i32 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(f.level), 0)::int FROM village_fields f \
             JOIN villages v ON v.id = f.village_id \
             WHERE v.owner_id = $1 AND v.world_id = $2",
        )
        .bind(pid)
        .bind(world)
        .fetch_one(&self.pool)
        .await
        .map_err(backend)?;
        let brows = sqlx::query(
            "SELECT b.building_type, MAX(b.level) AS level FROM village_buildings b \
             JOIN villages v ON v.id = b.village_id \
             WHERE v.owner_id = $1 AND v.world_id = $2 GROUP BY b.building_type",
        )
        .bind(pid)
        .bind(world)
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;
        let mut building_levels = HashMap::with_capacity(brows.len());
        for br in &brows {
            let kind = parse_building(&br.try_get::<String, _>("building_type").map_err(backend)?)?;
            let level: i16 = br.try_get("level").map_err(backend)?;
            building_levels.insert(kind, u8::try_from(level).unwrap_or(0));
        }
        let has_troops: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM village_units u JOIN villages v ON v.id = u.village_id \
             WHERE v.owner_id = $1 AND v.world_id = $2 AND u.count > 0)",
        )
        .bind(pid)
        .bind(world)
        .fetch_one(&self.pool)
        .await
        .map_err(backend)?;
        let has_raided: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM battle_reports WHERE attacker_player = $1)",
        )
        .bind(pid)
        .fetch_one(&self.pool)
        .await
        .map_err(backend)?;
        let (fields, kinds, levels, pops) = population_arrays(econ);
        let pop_expr = village_pop_expr("v.id", "$3", "$4", "$5", "$6");
        let population: i64 = sqlx::query_scalar(&format!(
            "SELECT COALESCE(SUM({pop_expr}), 0)::bigint FROM villages v \
             WHERE v.owner_id = $1 AND v.world_id = $2"
        ))
        .bind(pid)
        .bind(world)
        .bind(&fields)
        .bind(&kinds)
        .bind(&levels)
        .bind(&pops)
        .fetch_one(&self.pool)
        .await
        .map_err(backend)?;
        Ok(QuestProgress {
            max_field_level: u8::try_from(max_field_level).unwrap_or(0),
            building_levels,
            has_troops,
            has_raided,
            population,
        })
    }

    async fn complete_quest(
        &self,
        econ: &EconomyRules,
        player: PlayerId,
        def: &QuestDef,
    ) -> Result<bool, RepoError> {
        let pid = Uuid::from_u128(player.0);
        let world = Uuid::from_u128(self.world_id.0);
        let mut tx = self.pool.begin().await.map_err(backend)?;
        let inserted = sqlx::query(
            "INSERT INTO player_quests (player_id, quest_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        )
        .bind(pid)
        .bind(&def.id.0)
        .execute(&mut *tx)
        .await
        .map_err(backend)?;
        if inserted.rows_affected() == 0 {
            tx.commit().await.map_err(backend)?;
            return Ok(false); // already completed — no double reward
        }
        credit_reward(
            &mut tx,
            econ,
            world,
            pid,
            def.reward.resources,
            def.reward.culture,
            def.reward.troops.as_ref().map(|(u, c)| (u, *c)),
        )
        .await?;
        tx.commit().await.map_err(backend)?;
        Ok(true)
    }
}

#[async_trait]
impl LifecycleRepository for PgAccountRepository {
    async fn latest_swept_period(&self) -> Result<Option<i64>, RepoError> {
        sqlx::query_scalar("SELECT MAX(period) FROM inactivity_sweeps WHERE world_id = $1")
            .bind(Uuid::from_u128(self.world_id.0))
            .fetch_one(&self.pool)
            .await
            .map_err(backend)
    }

    async fn sweep_abandoned(&self, period: i64, cutoff: Timestamp) -> Result<usize, RepoError> {
        let world = Uuid::from_u128(self.world_id.0);
        let mut tx = self.pool.begin().await.map_err(backend)?;
        // The live accounts idle past the period's cutoff (already-abandoned excluded — idempotent).
        // `FOR UPDATE` locks the rows so a concurrent `touch_activity` cannot make one active between
        // this read and the deletes below (it blocks until this transaction commits).
        let victims: Vec<Uuid> = sqlx::query_scalar(
            "SELECT id FROM users WHERE abandoned_at IS NULL AND is_npc = false \
             AND last_activity < to_timestamp($1::double precision / 1000.0) FOR UPDATE",
        )
        .bind(cutoff.0)
        .fetch_all(&mut *tx)
        .await
        .map_err(backend)?;
        // Claim the period (watermark). If already recorded, another tick swept it — no double work.
        let claimed = sqlx::query(
            "INSERT INTO inactivity_sweeps (world_id, period, abandoned_count) VALUES ($1, $2, $3) \
             ON CONFLICT (world_id, period) DO NOTHING",
        )
        .bind(world)
        .bind(period)
        .bind(i32::try_from(victims.len()).unwrap_or(i32::MAX))
        .execute(&mut *tx)
        .await
        .map_err(backend)?;
        if claimed.rows_affected() == 0 {
            tx.commit().await.map_err(backend)?;
            return Ok(0); // period already swept — idempotent no-op
        }
        if !victims.is_empty() {
            // Remove their villages in this world — frees the valleys (cascades village-scoped rows).
            sqlx::query("DELETE FROM villages WHERE owner_id = ANY($1) AND world_id = $2")
                .bind(&victims)
                .bind(world)
                .execute(&mut *tx)
                .await
                .map_err(backend)?;
            // Retire (soft-delete) the accounts: kept for referential history, but cannot log in.
            sqlx::query("UPDATE users SET abandoned_at = now() WHERE id = ANY($1)")
                .bind(&victims)
                .execute(&mut *tx)
                .await
                .map_err(backend)?;
        }
        tx.commit().await.map_err(backend)?;
        Ok(victims.len())
    }
}

fn artifact_kind_str(k: ArtifactKind) -> &'static str {
    match k {
        ArtifactKind::Speed => "speed",
        ArtifactKind::Storage => "storage",
        ArtifactKind::Sustenance => "sustenance",
        ArtifactKind::Trainer => "trainer",
        ArtifactKind::Architect => "architect",
        ArtifactKind::Eyes => "eyes",
        ArtifactKind::Confuser => "confuser",
        ArtifactKind::Fool => "fool",
    }
}

fn parse_artifact_kind(s: &str) -> Result<ArtifactKind, RepoError> {
    match s {
        "speed" => Ok(ArtifactKind::Speed),
        "storage" => Ok(ArtifactKind::Storage),
        "sustenance" => Ok(ArtifactKind::Sustenance),
        "trainer" => Ok(ArtifactKind::Trainer),
        "architect" => Ok(ArtifactKind::Architect),
        "eyes" => Ok(ArtifactKind::Eyes),
        "confuser" => Ok(ArtifactKind::Confuser),
        "fool" => Ok(ArtifactKind::Fool),
        other => Err(RepoError::Backend(format!(
            "unknown artifact kind: {other}"
        ))),
    }
}

fn artifact_scope_str(s: ArtifactScope) -> &'static str {
    match s {
        ArtifactScope::Small => "small",
        ArtifactScope::Large => "large",
        ArtifactScope::Unique => "unique",
    }
}

fn parse_artifact_scope(s: &str) -> Result<ArtifactScope, RepoError> {
    match s {
        "small" => Ok(ArtifactScope::Small),
        "large" => Ok(ArtifactScope::Large),
        "unique" => Ok(ArtifactScope::Unique),
        other => Err(RepoError::Backend(format!(
            "unknown artifact scope: {other}"
        ))),
    }
}

/// Aggregate a player's already-fetched holdings into the effects for one of their villages (020 AC6):
/// the village's own **small** artifacts plus the account's **large/unique**. `NONE` for a Natar
/// village. Pure, so `villages_of` can fetch the holdings once and reuse them across the loop.
fn artifact_effects_from(
    held: &[HeldArtifact],
    village: VillageId,
    is_natar: bool,
) -> ArtifactEffects {
    if is_natar || held.is_empty() {
        return ArtifactEffects::NONE;
    }
    let small: Vec<ArtifactDef> = held
        .iter()
        .filter(|h| h.holder == village && h.def.scope == ArtifactScope::Small)
        .map(|h| h.def.clone())
        .collect();
    let account_wide: Vec<ArtifactDef> = held
        .iter()
        .filter(|h| matches!(h.def.scope, ArtifactScope::Large | ArtifactScope::Unique))
        .map(|h| h.def.clone())
        .collect();
    aggregate_effects(&small, &account_wide)
}

fn artifact_from_row(r: &PgRow) -> Result<ArtifactDef, RepoError> {
    Ok(ArtifactDef {
        id: ArtifactId(r.try_get::<String, _>("id").map_err(backend)?),
        kind: parse_artifact_kind(&r.try_get::<String, _>("kind").map_err(backend)?)?,
        scope: parse_artifact_scope(&r.try_get::<String, _>("scope").map_err(backend)?)?,
        magnitude: r.try_get("magnitude").map_err(backend)?,
    })
}

#[async_trait]
impl ArtifactRepository for PgAccountRepository {
    async fn release_artifacts(
        &self,
        release_at: Timestamp,
        now: Timestamp,
        catalogue: &[ArtifactDef],
        garrison_unit: &str,
        garrison_base_count: i64,
        garrison_per_index: i64,
    ) -> Result<usize, RepoError> {
        if now.0 < release_at.0 {
            return Ok(0);
        }
        let world = Uuid::from_u128(self.world_id.0);
        let mut tx = self.pool.begin().await.map_err(backend)?;
        // Idempotency (AC1): release happens at most once per world.
        let existing: i64 =
            sqlx::query_scalar("SELECT count(*) FROM artifacts WHERE world_id = $1")
                .bind(world)
                .fetch_one(&mut *tx)
                .await
                .map_err(backend)?;
        if existing > 0 {
            tx.commit().await.map_err(backend)?;
            return Ok(0);
        }
        // The synthetic Natar NPC owner (flagged out of boards/stats/sweep). Romans match the garrison.
        let npc_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO users (id, username, email, password_hash, email_confirmed, tribe, is_npc) \
             VALUES ($1, 'Natars', 'natars@system.local', '!', true, 'romans', true) \
             ON CONFLICT (username) DO NOTHING",
        )
        .bind(npc_id)
        .execute(&mut *tx)
        .await
        .map_err(backend)?;
        let npc: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'Natars'")
            .fetch_one(&mut *tx)
            .await
            .map_err(backend)?;
        // The NPC's player (042) — id = NPC user id (reuse-UUID), so NPC villages satisfy the players FK
        // and every owner→user read for the NPC still resolves. The `Natars` user is global (one row), so
        // there is a single NPC player owning NPC villages in every world; `ON CONFLICT (id)` makes this
        // idempotent and collision-safe once more than one world reaches its end-game.
        sqlx::query(
            "INSERT INTO players (id, user_id, world_id, tribe) VALUES ($1, $1, $2, 'romans') \
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(npc)
        .bind(world)
        .execute(&mut *tx)
        .await
        .map_err(backend)?;
        // The reserved Natar tiles, in deterministic ring order (P6) — one per artifact.
        let natar_tiles: Vec<Coordinate> = coordinates_within(self.map.radius())
            .filter(|c| matches!(self.map.tile_at(*c), Some(TileKind::Natar)))
            .take(catalogue.len())
            .collect();
        let mut released = 0usize;
        for (i, def) in catalogue.iter().enumerate() {
            let Some(coord) = natar_tiles.get(i) else {
                break; // fewer reserved Natar tiles than artifacts — release what fits
            };
            let village_id = Uuid::new_v4();
            sqlx::query(
                "INSERT INTO villages (id, world_id, owner_id, x, y, tribe, is_natar) \
                 VALUES ($1, $2, $3, $4, $5, 'romans', true)",
            )
            .bind(village_id)
            .bind(world)
            .bind(npc)
            .bind(coord.x)
            .bind(coord.y)
            .execute(&mut *tx)
            .await
            .map_err(backend)?;
            // A developed Main Building gives the Natar vault a population, so attacking it is a normal
            // battle (not a morale-crushed strike against an empty village).
            sqlx::query(
                "INSERT INTO village_buildings (village_id, slot, building_type, level) \
                 VALUES ($1, 0, 'main_building', 10)",
            )
            .bind(village_id)
            .execute(&mut *tx)
            .await
            .map_err(backend)?;
            let count = garrison_base_count + garrison_per_index * i as i64;
            sqlx::query(
                "INSERT INTO village_units (village_id, unit_id, count) VALUES ($1, $2, $3)",
            )
            .bind(village_id)
            .bind(garrison_unit)
            .bind(i32::try_from(count).unwrap_or(i32::MAX))
            .execute(&mut *tx)
            .await
            .map_err(backend)?;
            sqlx::query(
                "INSERT INTO artifacts \
                 (id, world_id, kind, scope, magnitude, holder_village, origin_x, origin_y, released_at) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, to_timestamp($9::double precision / 1000.0))",
            )
            .bind(&def.id.0)
            .bind(world)
            .bind(artifact_kind_str(def.kind))
            .bind(artifact_scope_str(def.scope))
            .bind(def.magnitude)
            .bind(village_id)
            .bind(coord.x)
            .bind(coord.y)
            .bind(now.0)
            .execute(&mut *tx)
            .await
            .map_err(backend)?;
            released += 1;
        }
        tx.commit().await.map_err(backend)?;
        Ok(released)
    }

    async fn artifact_at_village(
        &self,
        village: VillageId,
    ) -> Result<Option<ArtifactDef>, RepoError> {
        let row = sqlx::query(
            "SELECT id, kind, scope, magnitude FROM artifacts WHERE holder_village = $1 LIMIT 1",
        )
        .bind(Uuid::from_u128(village.0))
        .fetch_optional(&self.pool)
        .await
        .map_err(backend)?;
        row.as_ref().map(artifact_from_row).transpose()
    }

    async fn held_by_player(&self, player: PlayerId) -> Result<Vec<HeldArtifact>, RepoError> {
        let rows = sqlx::query(
            "SELECT a.id, a.kind, a.scope, a.magnitude, a.holder_village \
             FROM artifacts a JOIN villages v ON v.id = a.holder_village \
             WHERE v.owner_id = $1",
        )
        .bind(Uuid::from_u128(player.0))
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;
        rows.iter()
            .map(|r| {
                let holder: Uuid = r.try_get("holder_village").map_err(backend)?;
                Ok(HeldArtifact {
                    def: artifact_from_row(r)?,
                    holder: VillageId(holder.as_u128()),
                })
            })
            .collect()
    }
}

#[async_trait]
impl WonderRepository for PgAccountRepository {
    #[allow(clippy::too_many_arguments)]
    async fn release_wonder(
        &self,
        release_at: Timestamp,
        now: Timestamp,
        plan_count: u32,
        site_count: u32,
        garrison_unit: &str,
        garrison_base_count: i64,
        garrison_per_index: i64,
    ) -> Result<usize, RepoError> {
        if now.0 < release_at.0 {
            return Ok(0);
        }
        let world = Uuid::from_u128(self.world_id.0);
        let mut tx = self.pool.begin().await.map_err(backend)?;
        // Idempotency (AC1): the Wonder release happens at most once per world.
        let existing: i64 =
            sqlx::query_scalar("SELECT count(*) FROM wonder_plans WHERE world_id = $1")
                .bind(world)
                .fetch_one(&mut *tx)
                .await
                .map_err(backend)?;
        let existing_sites: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM villages WHERE is_wonder_site AND world_id = $1",
        )
        .bind(world)
        .fetch_one(&mut *tx)
        .await
        .map_err(backend)?;
        if existing > 0 || existing_sites > 0 {
            tx.commit().await.map_err(backend)?;
            return Ok(0);
        }
        // The synthetic Natar NPC owner (shared with the artifact release, 020). Romans match the garrison.
        let npc_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO users (id, username, email, password_hash, email_confirmed, tribe, is_npc) \
             VALUES ($1, 'Natars', 'natars@system.local', '!', true, 'romans', true) \
             ON CONFLICT (username) DO NOTHING",
        )
        .bind(npc_id)
        .execute(&mut *tx)
        .await
        .map_err(backend)?;
        let npc: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'Natars'")
            .fetch_one(&mut *tx)
            .await
            .map_err(backend)?;
        // The NPC's player (042) — id = NPC user id (reuse-UUID), so NPC villages satisfy the players FK
        // and every owner→user read for the NPC still resolves. The `Natars` user is global (one row), so
        // there is a single NPC player owning NPC villages in every world; `ON CONFLICT (id)` makes this
        // idempotent and collision-safe once more than one world reaches its end-game.
        sqlx::query(
            "INSERT INTO players (id, user_id, world_id, tribe) VALUES ($1, $1, $2, 'romans') \
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(npc)
        .bind(world)
        .execute(&mut *tx)
        .await
        .map_err(backend)?;
        // Reserved Natar tiles not already taken by the artifact release (placed earlier) — deterministic
        // ring order (P6). Sites first, then plan vaults.
        let occupied: HashSet<(i32, i32)> =
            sqlx::query("SELECT x, y FROM villages WHERE world_id = $1")
                .bind(world)
                .fetch_all(&mut *tx)
                .await
                .map_err(backend)?
                .iter()
                .map(|r| {
                    Ok::<_, RepoError>((
                        r.try_get::<i32, _>("x").map_err(backend)?,
                        r.try_get::<i32, _>("y").map_err(backend)?,
                    ))
                })
                .collect::<Result<_, _>>()?;
        let needed = site_count as usize + plan_count as usize;
        let free_tiles: Vec<Coordinate> = coordinates_within(self.map.radius())
            .filter(|c| matches!(self.map.tile_at(*c), Some(TileKind::Natar)))
            .filter(|c| !occupied.contains(&(c.x, c.y)))
            .take(needed)
            .collect();

        // Place a garrisoned Natar village; `is_site` marks it conquerable (a Wonder construction site).
        let place = async |tx: &mut sqlx::PgConnection,
                           coord: &Coordinate,
                           index: i64,
                           is_site: bool|
               -> Result<Uuid, RepoError> {
            let village_id = Uuid::new_v4();
            sqlx::query(
                "INSERT INTO villages (id, world_id, owner_id, x, y, tribe, is_natar, is_wonder_site) \
                 VALUES ($1, $2, $3, $4, $5, 'romans', true, $6)",
            )
            .bind(village_id)
            .bind(world)
            .bind(npc)
            .bind(coord.x)
            .bind(coord.y)
            .bind(is_site)
            .execute(&mut *tx)
            .await
            .map_err(backend)?;
            // A developed Main Building gives the Natar village a population, so attacking it is a normal
            // battle (not a morale-crushed strike against an empty village).
            sqlx::query(
                "INSERT INTO village_buildings (village_id, slot, building_type, level) \
                 VALUES ($1, 0, 'main_building', 10)",
            )
            .bind(village_id)
            .execute(&mut *tx)
            .await
            .map_err(backend)?;
            let count = garrison_base_count + garrison_per_index * index;
            sqlx::query(
                "INSERT INTO village_units (village_id, unit_id, count) VALUES ($1, $2, $3)",
            )
            .bind(village_id)
            .bind(garrison_unit)
            .bind(i32::try_from(count).unwrap_or(i32::MAX))
            .execute(&mut *tx)
            .await
            .map_err(backend)?;
            Ok(village_id)
        };

        let mut materialized = 0usize;
        let mut index = 0i64;
        // Conquerable Wonder construction sites.
        for _ in 0..site_count as usize {
            let Some(coord) = free_tiles.get(materialized) else {
                break; // fewer free Natar tiles than requested — release what fits
            };
            place(&mut tx, coord, index, true).await?;
            materialized += 1;
            index += 1;
        }
        // Capturable plan vaults (not conquerable — taken by force, 020 mechanic).
        for p in 0..plan_count as usize {
            let Some(coord) = free_tiles.get(materialized) else {
                break;
            };
            let vault = place(&mut tx, coord, index, false).await?;
            sqlx::query(
                "INSERT INTO wonder_plans \
                 (id, world_id, holder_village, origin_x, origin_y, released_at) \
                 VALUES ($1, $2, $3, $4, $5, to_timestamp($6::double precision / 1000.0))",
            )
            .bind(format!("wonder-plan-{world}-{p}"))
            .bind(world)
            .bind(vault)
            .bind(coord.x)
            .bind(coord.y)
            .bind(now.0)
            .execute(&mut *tx)
            .await
            .map_err(backend)?;
            materialized += 1;
            index += 1;
        }
        tx.commit().await.map_err(backend)?;
        Ok(materialized)
    }

    async fn plan_at_village(&self, village: VillageId) -> Result<Option<String>, RepoError> {
        sqlx::query_scalar("SELECT id FROM wonder_plans WHERE holder_village = $1 LIMIT 1")
            .bind(Uuid::from_u128(village.0))
            .fetch_optional(&self.pool)
            .await
            .map_err(backend)
    }

    async fn alliance_holds_plan(&self, alliance: AllianceId) -> Result<bool, RepoError> {
        sqlx::query_scalar(
            "SELECT EXISTS( \
               SELECT 1 FROM wonder_plans p \
               JOIN villages v ON v.id = p.holder_village \
               JOIN alliance_members m ON m.player_id = v.owner_id \
               WHERE m.alliance_id = $1)",
        )
        .bind(Uuid::from_u128(alliance.0))
        .fetch_one(&self.pool)
        .await
        .map_err(backend)
    }

    async fn wonder_level(&self, village: VillageId) -> Result<u8, RepoError> {
        let level: Option<i32> = sqlx::query_scalar(
            "SELECT level::int FROM village_buildings \
             WHERE village_id = $1 AND building_type = 'wonder'",
        )
        .bind(Uuid::from_u128(village.0))
        .fetch_optional(&self.pool)
        .await
        .map_err(backend)?;
        Ok(level.unwrap_or(0).clamp(0, i32::from(u8::MAX)) as u8)
    }

    async fn top_wonders(&self) -> Result<Vec<WonderStanding>, RepoError> {
        let rows = sqlx::query(
            // Only Wonders on conquered Wonder **sites** count (021) — a Wonder can be built nowhere else,
            // so an off-site `wonder` row (should never exist) never reaches the standings or wins.
            "SELECT a.id AS alliance_id, a.tag, a.name, MAX(vb.level)::int AS lvl \
             FROM alliance_members m \
             JOIN alliances a ON a.id = m.alliance_id \
             JOIN villages v ON v.owner_id = m.player_id AND v.world_id = $1 AND v.is_wonder_site \
             JOIN village_buildings vb ON vb.village_id = v.id AND vb.building_type = 'wonder' \
             GROUP BY a.id, a.tag, a.name \
             ORDER BY lvl DESC, a.tag ASC",
        )
        .bind(Uuid::from_u128(self.world_id.0))
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;
        rows.iter()
            .map(|r| {
                let id: Uuid = r.try_get("alliance_id").map_err(backend)?;
                let lvl: i32 = r.try_get("lvl").map_err(backend)?;
                Ok(WonderStanding {
                    alliance: AllianceId(id.as_u128()),
                    tag: r.try_get("tag").map_err(backend)?,
                    name: r.try_get("name").map_err(backend)?,
                    level: lvl.clamp(0, i32::from(u8::MAX)) as u8,
                })
            })
            .collect()
    }

    async fn world_ended(&self) -> Result<Option<WonderOutcome>, RepoError> {
        let row = sqlx::query(
            "SELECT winner_alliance_id, (EXTRACT(EPOCH FROM won_at) * 1000)::bigint AS won_ms \
             FROM worlds WHERE id = $1",
        )
        .bind(Uuid::from_u128(self.world_id.0))
        .fetch_optional(&self.pool)
        .await
        .map_err(backend)?;
        let Some(row) = row else { return Ok(None) };
        let winner: Option<Uuid> = row.try_get("winner_alliance_id").map_err(backend)?;
        let won_ms: Option<i64> = row.try_get("won_ms").map_err(backend)?;
        match (winner, won_ms) {
            (Some(w), Some(ms)) => Ok(Some(WonderOutcome {
                winner: AllianceId(w.as_u128()),
                won_at: Timestamp(ms),
            })),
            _ => Ok(None),
        }
    }

    async fn record_victory(
        &self,
        winner: AllianceId,
        won_at: Timestamp,
    ) -> Result<bool, RepoError> {
        // Guarded (AC6): records the winner only while the round is still open, so the first complete
        // Wonder wins and a later one cannot overwrite it.
        let affected = sqlx::query(
            "UPDATE worlds \
             SET winner_alliance_id = $1, won_at = to_timestamp($2::double precision / 1000.0) \
             WHERE id = $3 AND won_at IS NULL",
        )
        .bind(Uuid::from_u128(winner.0))
        .bind(won_at.0)
        .bind(Uuid::from_u128(self.world_id.0))
        .execute(&self.pool)
        .await
        .map_err(backend)?
        .rows_affected();
        Ok(affected > 0)
    }
}

/// Read a `ReportView` from a joined `reports`/`users` row.
fn moderation_report_from_row(r: &PgRow) -> Result<ReportView, RepoError> {
    let id: Uuid = r.try_get("id").map_err(backend)?;
    let reporter: Uuid = r.try_get("reporter_id").map_err(backend)?;
    let subject: Uuid = r.try_get("subject_id").map_err(backend)?;
    let reason_str: String = r.try_get("reason").map_err(backend)?;
    Ok(ReportView {
        id: id.as_u128(),
        reporter: PlayerId(reporter.as_u128()),
        reporter_name: r.try_get("reporter_name").map_err(backend)?,
        subject: PlayerId(subject.as_u128()),
        subject_name: r.try_get("subject_name").map_err(backend)?,
        reason: ReportReason::parse(&reason_str).unwrap_or(ReportReason::Other),
        note: r.try_get("note").map_err(backend)?,
        created_ms: r.try_get("created_ms").map_err(backend)?,
    })
}

/// Apply a sanction to a user within an existing transaction (022): ban stamps `banned_at`, suspend
/// sets `suspended_until`, warn changes no block state.
async fn apply_sanction_tx(
    tx: &mut sqlx::PgConnection,
    subject: Uuid,
    now: Timestamp,
    kind: SanctionKind,
    suspended_until: Option<Timestamp>,
) -> Result<(), RepoError> {
    match kind {
        SanctionKind::Warn => {}
        SanctionKind::Ban => {
            sqlx::query(
                "UPDATE users SET banned_at = to_timestamp($1::double precision / 1000.0) \
                 WHERE id = $2",
            )
            .bind(now.0)
            .bind(subject)
            .execute(&mut *tx)
            .await
            .map_err(backend)?;
        }
        SanctionKind::Suspend => {
            sqlx::query(
                "UPDATE users SET suspended_until = to_timestamp($1::double precision / 1000.0) \
                 WHERE id = $2",
            )
            .bind(suspended_until.unwrap_or(now).0)
            .bind(subject)
            .execute(&mut *tx)
            .await
            .map_err(backend)?;
        }
    }
    Ok(())
}

#[async_trait]
impl ModerationRepository for PgAccountRepository {
    async fn set_moderator(&self, player: PlayerId, is_moderator: bool) -> Result<(), RepoError> {
        sqlx::query("UPDATE users SET is_moderator = $2 WHERE id = $1")
            .bind(Uuid::from_u128(player.0))
            .bind(is_moderator)
            .execute(&self.pool)
            .await
            .map_err(backend)?;
        Ok(())
    }

    async fn record_registration_ip(&self, player: PlayerId, ip: &str) -> Result<(), RepoError> {
        sqlx::query("UPDATE users SET registration_ip = $2 WHERE id = $1")
            .bind(Uuid::from_u128(player.0))
            .bind(ip)
            .execute(&self.pool)
            .await
            .map_err(backend)?;
        Ok(())
    }

    async fn file_report(
        &self,
        reporter: PlayerId,
        subject: PlayerId,
        reason: ReportReason,
        note: &str,
    ) -> Result<bool, RepoError> {
        // ON CONFLICT on the partial unique index collapses a duplicate **open** report (022 AC2).
        let affected = sqlx::query(
            "INSERT INTO reports (id, world_id, reporter_id, subject_id, reason, note) \
             VALUES ($1, $2, $3, $4, $5, $6) \
             ON CONFLICT (reporter_id, subject_id) WHERE status = 'open' DO NOTHING",
        )
        .bind(Uuid::new_v4())
        .bind(Uuid::from_u128(self.world_id.0))
        .bind(Uuid::from_u128(reporter.0))
        .bind(Uuid::from_u128(subject.0))
        .bind(reason.as_str())
        .bind(note)
        .execute(&self.pool)
        .await
        .map_err(backend)?
        .rows_affected();
        Ok(affected > 0)
    }

    async fn open_reports(&self, limit: i64) -> Result<Vec<ReportView>, RepoError> {
        let rows = sqlx::query(
            "SELECT r.id, r.reporter_id, r.subject_id, r.reason, r.note, \
             (EXTRACT(EPOCH FROM r.created_at) * 1000)::bigint AS created_ms, \
             rep.username AS reporter_name, sub.username AS subject_name \
             FROM reports r \
             JOIN users rep ON rep.id = r.reporter_id \
             JOIN users sub ON sub.id = r.subject_id \
             WHERE r.status = 'open' AND r.world_id = $1 \
             ORDER BY r.created_at ASC, r.id LIMIT $2",
        )
        .bind(Uuid::from_u128(self.world_id.0))
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;
        rows.iter().map(moderation_report_from_row).collect()
    }

    async fn resolve_report(
        &self,
        report_id: u128,
        moderator: PlayerId,
        now: Timestamp,
        resolution: &str,
        sanction_kind: Option<SanctionKind>,
        suspended_until: Option<Timestamp>,
    ) -> Result<bool, RepoError> {
        let mut tx = self.pool.begin().await.map_err(backend)?;
        // Guarded on status='open' so resolving twice is a no-op (022 AC4).
        let row = sqlx::query(
            "UPDATE reports \
             SET status = 'resolved', resolved_by = $2, \
                 resolved_at = to_timestamp($3::double precision / 1000.0), resolution = $4 \
             WHERE id = $1 AND status = 'open' RETURNING subject_id",
        )
        .bind(Uuid::from_u128(report_id))
        .bind(Uuid::from_u128(moderator.0))
        .bind(now.0)
        .bind(resolution)
        .fetch_optional(&mut *tx)
        .await
        .map_err(backend)?;
        let Some(row) = row else {
            tx.commit().await.map_err(backend)?;
            return Ok(false);
        };
        if let Some(kind) = sanction_kind {
            let subject: Uuid = row.try_get("subject_id").map_err(backend)?;
            apply_sanction_tx(&mut tx, subject, now, kind, suspended_until).await?;
        }
        tx.commit().await.map_err(backend)?;
        Ok(true)
    }

    async fn apply_sanction(
        &self,
        subject: PlayerId,
        now: Timestamp,
        kind: SanctionKind,
        suspended_until: Option<Timestamp>,
    ) -> Result<(), RepoError> {
        let mut tx = self.pool.begin().await.map_err(backend)?;
        apply_sanction_tx(
            &mut tx,
            Uuid::from_u128(subject.0),
            now,
            kind,
            suspended_until,
        )
        .await?;
        tx.commit().await.map_err(backend)?;
        Ok(())
    }

    async fn ip_association_count(&self, subject: PlayerId) -> Result<u32, RepoError> {
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM users \
             WHERE registration_ip IS NOT NULL \
               AND registration_ip = (SELECT registration_ip FROM users WHERE id = $1)",
        )
        .bind(Uuid::from_u128(subject.0))
        .fetch_one(&self.pool)
        .await
        .map_err(backend)?;
        Ok(u32::try_from(count).unwrap_or(u32::MAX))
    }

    async fn peak_action_count(&self, subject: PlayerId) -> Result<u32, RepoError> {
        // Bounded to the retained window (older rows are pruned in `bump_rate`) — a recent burst is what
        // the inhuman-rate signal cares about (P11: no full-history scan).
        let peak: Option<i32> = sqlx::query_scalar(
            "SELECT MAX(count) FROM rate_limits \
             WHERE subject = $1 AND action = 'action' \
               AND window_start >= now() - make_interval(secs => $2::double precision)",
        )
        .bind(subject.0.to_string())
        .bind(RATE_LIMIT_RETENTION_SECS as f64)
        .fetch_one(&self.pool)
        .await
        .map_err(backend)?;
        Ok(u32::try_from(peak.unwrap_or(0)).unwrap_or(u32::MAX))
    }

    async fn bump_rate(
        &self,
        subject: &str,
        action: &str,
        now: Timestamp,
        window_secs: i64,
    ) -> Result<u32, RepoError> {
        // Fixed window: snap `now` down to the window boundary (P5 — counters are DB-side, stateless web).
        let window = window_secs.max(1);
        let window_start_secs = (now.0 / 1000 / window) * window;
        let count: i32 = sqlx::query_scalar(
            "INSERT INTO rate_limits (subject, action, window_start, count) \
             VALUES ($1, $2, to_timestamp($3::double precision), 1) \
             ON CONFLICT (subject, action, window_start) \
             DO UPDATE SET count = rate_limits.count + 1 RETURNING count",
        )
        .bind(subject)
        .bind(action)
        .bind(window_start_secs as f64)
        .fetch_one(&self.pool)
        .await
        .map_err(backend)?;
        // Prune this subject/action's windows older than the retention bound, so the table stays small
        // (P11) while keeping recent history for the detection signal. Targeted by the PK prefix index.
        sqlx::query(
            "DELETE FROM rate_limits WHERE subject = $1 AND action = $2 \
             AND window_start < to_timestamp($3::double precision)",
        )
        .bind(subject)
        .bind(action)
        .bind((window_start_secs - RATE_LIMIT_RETENTION_SECS) as f64)
        .execute(&self.pool)
        .await
        .map_err(backend)?;
        Ok(u32::try_from(count).unwrap_or(u32::MAX))
    }
}

/// Map a row to an admin-console account listing entry (036).
fn row_to_admin_account(r: &PgRow) -> Result<AdminAccount, RepoError> {
    let id: Uuid = r.try_get("id").map_err(backend)?;
    Ok(AdminAccount {
        id: PlayerId(id.as_u128()),
        username: r.try_get("username").map_err(backend)?,
        is_moderator: r.try_get("is_moderator").map_err(backend)?,
        is_admin: r.try_get("is_admin").map_err(backend)?,
        abandoned: r.try_get("abandoned").map_err(backend)?,
    })
}

#[async_trait]
impl AdminRepository for PgAccountRepository {
    async fn set_admin(&self, player: PlayerId, is_admin: bool) -> Result<(), RepoError> {
        sqlx::query("UPDATE users SET is_admin = $2 WHERE id = $1")
            .bind(Uuid::from_u128(player.0))
            .bind(is_admin)
            .execute(&self.pool)
            .await
            .map_err(backend)?;
        Ok(())
    }

    async fn admin_overview(&self) -> Result<AdminOverview, RepoError> {
        // The single active world row + its end-game schedule / win state.
        let w = sqlx::query(
            "SELECT speed, radius, seed, \
             (EXTRACT(EPOCH FROM created_at) * 1000)::bigint AS created_ms, \
             (EXTRACT(EPOCH FROM artifact_release_at) * 1000)::bigint AS artifact_ms, \
             (EXTRACT(EPOCH FROM wonder_release_at) * 1000)::bigint AS wonder_ms, \
             (EXTRACT(EPOCH FROM won_at) * 1000)::bigint AS won_ms \
             FROM worlds LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(backend)?
        .ok_or_else(|| RepoError::Backend("no world row".to_owned()))?;

        // Live counts — derived on read (P1/P5), each a cheap aggregate.
        let accounts: i64 =
            sqlx::query_scalar("SELECT count(*) FROM users WHERE abandoned_at IS NULL")
                .fetch_one(&self.pool)
                .await
                .map_err(backend)?;
        let villages: i64 = sqlx::query_scalar("SELECT count(*) FROM villages")
            .fetch_one(&self.pool)
            .await
            .map_err(backend)?;
        let pending_events: i64 =
            sqlx::query_scalar("SELECT count(*) FROM scheduled_events WHERE status = 'pending'")
                .fetch_one(&self.pool)
                .await
                .map_err(backend)?;

        Ok(AdminOverview {
            speed: w.try_get("speed").map_err(backend)?,
            radius: u32::try_from(w.try_get::<i32, _>("radius").map_err(backend)?).unwrap_or(0),
            seed: w.try_get("seed").map_err(backend)?,
            created_ms: w.try_get("created_ms").map_err(backend)?,
            artifact_release_ms: w.try_get("artifact_ms").map_err(backend)?,
            wonder_release_ms: w.try_get("wonder_ms").map_err(backend)?,
            won_ms: w.try_get("won_ms").map_err(backend)?,
            accounts,
            villages,
            pending_events,
        })
    }

    async fn recent_accounts(&self, limit: i64) -> Result<Vec<AdminAccount>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, username, is_moderator, is_admin, (abandoned_at IS NOT NULL) AS abandoned \
             FROM users ORDER BY created_at DESC LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;
        rows.iter().map(row_to_admin_account).collect()
    }

    async fn admin_account(&self, player: PlayerId) -> Result<Option<AdminAccount>, RepoError> {
        let row = sqlx::query(
            "SELECT id, username, is_moderator, is_admin, (abandoned_at IS NOT NULL) AS abandoned \
             FROM users WHERE id = $1",
        )
        .bind(Uuid::from_u128(player.0))
        .fetch_optional(&self.pool)
        .await
        .map_err(backend)?;
        row.as_ref().map(row_to_admin_account).transpose()
    }

    async fn list_worlds(&self) -> Result<Vec<AdminWorld>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, name, speed, radius, \
             (EXTRACT(EPOCH FROM created_at) * 1000)::bigint AS created_ms, \
             (EXTRACT(EPOCH FROM won_at) * 1000)::bigint AS won_ms \
             FROM worlds ORDER BY created_at, id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;
        rows.iter()
            .map(|r| {
                let id: Uuid = r.try_get("id").map_err(backend)?;
                Ok(AdminWorld {
                    id: WorldId(id.as_u128()),
                    name: r.try_get("name").map_err(backend)?,
                    speed: r.try_get("speed").map_err(backend)?,
                    radius: u32::try_from(r.try_get::<i32, _>("radius").map_err(backend)?)
                        .unwrap_or(0),
                    created_ms: r.try_get("created_ms").map_err(backend)?,
                    won_ms: r.try_get("won_ms").map_err(backend)?,
                })
            })
            .collect()
    }

    async fn create_world(
        &self,
        speed: f64,
        radius: u32,
        artifact_offset_secs: i64,
        wonder_offset_secs: i64,
        rule_preset: &str,
        name: &str,
    ) -> Result<WorldId, RepoError> {
        let config = WorldConfig::new(
            GameSpeed::new(speed).map_err(|e| RepoError::Backend(e.to_string()))?,
            radius,
        );
        let world = crate::world::create_world(
            &self.pool,
            &config,
            artifact_offset_secs,
            wonder_offset_secs,
            rule_preset,
            name,
        )
        .await
        .map_err(backend)?;
        Ok(world.id)
    }
}

/// Read a conversation message line from a joined row.
fn message_from_row(r: &PgRow) -> Result<MessageView, RepoError> {
    let id: Uuid = r.try_get("id").map_err(backend)?;
    let sender: Uuid = r.try_get("sender_id").map_err(backend)?;
    Ok(MessageView {
        id: id.as_u128(),
        sender: PlayerId(sender.as_u128()),
        sender_name: r.try_get("sender_name").map_err(backend)?,
        body: r.try_get("body").map_err(backend)?,
        created_ms: r.try_get("created_ms").map_err(backend)?,
    })
}

#[async_trait]
impl CommsRepository for PgAccountRepository {
    async fn send_dm(
        &self,
        sender: PlayerId,
        recipient: PlayerId,
        body: &str,
        now: Timestamp,
    ) -> Result<u128, RepoError> {
        let id = Uuid::new_v4();
        // Insert + notify in one statement: the row is the source of truth, the NOTIFY is the live nudge
        // (024). The payload carries the **pair-canonical** key `dmp:<lo>:<hi>` (LEAST/GREATEST) — both
        // parties derive the same one and only they can, so a third party can't subscribe to the thread.
        // Do NOT switch this back to per-party `dm:<uuid>` keys: that key isn't pair-unique and would let
        // anyone wiretap a player's DMs.
        // 026 AC3: a DM also records a NewMessage notification for the recipient + nudges their private
        // `notif:<uuid>` stream. `ins` + `note` are data-modifying CTEs (always run); both pg_notify calls
        // live in the final UNION'd SELECT so each is guaranteed to execute.
        let notif_id = Uuid::new_v4();
        sqlx::query(
            "WITH ins AS ( \
                INSERT INTO direct_messages (id, world_id, sender_id, recipient_id, body, created_at) \
                VALUES ($1, $2, $3, $4, $5, to_timestamp($6::double precision / 1000.0)) \
                RETURNING created_at \
             ), note AS ( \
                INSERT INTO notifications \
                    (id, world_id, player_id, kind, ref_kind, ref_id, body, created_at) \
                SELECT $7, $2, $4, 'new_message', 'dm', $3::text, '', \
                       to_timestamp($6::double precision / 1000.0) \
                WHERE NOT EXISTS ( \
                    SELECT 1 FROM notification_mutes m \
                     WHERE m.player_id = $4 AND m.kind = 'new_message') \
                RETURNING player_id \
             ) \
             SELECT pg_notify('comms', json_build_object( \
                'keys', json_build_array( \
                    'dmp:' || LEAST($3::text, $4::text) || ':' || GREATEST($3::text, $4::text)), \
                'sender_name', (SELECT username FROM users WHERE id = $3), \
                'body', $5::text, \
                'created_ms', (EXTRACT(EPOCH FROM (SELECT created_at FROM ins)) * 1000)::bigint \
             )::text) \
             UNION ALL \
             SELECT pg_notify('notifications', json_build_object( \
                'key', 'notif:' || player_id::text, 'kind', 'new_message')::text) FROM note",
        )
        .bind(id)
        .bind(Uuid::from_u128(self.world_id.0))
        .bind(Uuid::from_u128(sender.0))
        .bind(Uuid::from_u128(recipient.0))
        .bind(body)
        .bind(now.0)
        .bind(notif_id)
        .execute(&self.pool)
        .await
        .map_err(backend)?;
        Ok(id.as_u128())
    }

    async fn dm_history(
        &self,
        viewer: PlayerId,
        other: PlayerId,
        limit: i64,
    ) -> Result<Vec<MessageView>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, sender_id, sender_name, body, created_ms FROM ( \
                SELECT dm.id, dm.sender_id, u.username AS sender_name, dm.body, dm.created_at, \
                       (EXTRACT(EPOCH FROM dm.created_at) * 1000)::bigint AS created_ms \
                FROM direct_messages dm JOIN users u ON u.id = dm.sender_id \
                WHERE dm.world_id = $1 \
                  AND ((dm.sender_id = $2 AND dm.recipient_id = $3) \
                    OR (dm.sender_id = $3 AND dm.recipient_id = $2)) \
                ORDER BY dm.created_at DESC LIMIT $4 \
             ) t ORDER BY created_at ASC",
        )
        .bind(Uuid::from_u128(self.world_id.0))
        .bind(Uuid::from_u128(viewer.0))
        .bind(Uuid::from_u128(other.0))
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;
        rows.iter().map(message_from_row).collect()
    }

    async fn dm_threads(&self, viewer: PlayerId) -> Result<Vec<ConversationSummary>, RepoError> {
        let v = Uuid::from_u128(viewer.0);
        let rows = sqlx::query(
            "WITH mine AS ( \
                SELECT id, body, created_at, sender_id, \
                       CASE WHEN sender_id = $1 THEN recipient_id ELSE sender_id END AS other \
                FROM direct_messages WHERE world_id = $2 AND (sender_id = $1 OR recipient_id = $1) \
             ), latest AS ( \
                SELECT DISTINCT ON (other) other, body, created_at FROM mine \
                ORDER BY other, created_at DESC \
             ) \
             SELECT l.other, u.username, l.body, \
                    (EXTRACT(EPOCH FROM l.created_at) * 1000)::bigint AS last_ms, \
                    (EXTRACT(EPOCH FROM u.last_activity) * 1000)::bigint AS other_last_activity, \
                    (SELECT count(*) FROM mine m WHERE m.other = l.other AND m.sender_id <> $1 \
                       AND m.created_at > COALESCE( \
                            (SELECT last_read_at FROM conversation_reads \
                              WHERE player_id = $1 AND conversation = 'dm:' || l.other::text), \
                            to_timestamp(0))) AS unread \
             FROM latest l JOIN users u ON u.id = l.other \
             ORDER BY last_ms DESC",
        )
        .bind(v)
        .bind(Uuid::from_u128(self.world_id.0))
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;
        rows.iter()
            .map(|r| {
                let other: Uuid = r.try_get("other").map_err(backend)?;
                Ok(ConversationSummary {
                    key: format!("dm:{other}"),
                    title: r.try_get("username").map_err(backend)?,
                    last_body: r.try_get("body").map_err(backend)?,
                    last_ms: r.try_get("last_ms").map_err(backend)?,
                    unread: r.try_get("unread").map_err(backend)?,
                    other_last_activity: r.try_get("other_last_activity").map_err(backend)?,
                })
            })
            .collect()
    }

    async fn post_chat(
        &self,
        channel_key: &str,
        sender: PlayerId,
        body: &str,
        now: Timestamp,
    ) -> Result<u128, RepoError> {
        let id = Uuid::new_v4();
        // Insert + notify in one statement (024): the channel line persists and is nudged live to the
        // channel key's subscribers.
        sqlx::query(
            "WITH ins AS ( \
                INSERT INTO chat_messages (id, world_id, channel, sender_id, body, created_at) \
                VALUES ($1, $2, $3, $4, $5, to_timestamp($6::double precision / 1000.0)) \
                RETURNING created_at \
             ) \
             SELECT pg_notify('comms', json_build_object( \
                'keys', json_build_array($3::text), \
                'sender_name', (SELECT username FROM users WHERE id = $4), \
                'body', $5::text, \
                'created_ms', (EXTRACT(EPOCH FROM (SELECT created_at FROM ins)) * 1000)::bigint \
             )::text)",
        )
        .bind(id)
        .bind(Uuid::from_u128(self.world_id.0))
        .bind(channel_key)
        .bind(Uuid::from_u128(sender.0))
        .bind(body)
        .bind(now.0)
        .execute(&self.pool)
        .await
        .map_err(backend)?;
        Ok(id.as_u128())
    }

    async fn chat_history(
        &self,
        channel_key: &str,
        limit: i64,
    ) -> Result<Vec<MessageView>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, sender_id, sender_name, body, created_ms FROM ( \
                SELECT c.id, c.sender_id, u.username AS sender_name, c.body, c.created_at, \
                       (EXTRACT(EPOCH FROM c.created_at) * 1000)::bigint AS created_ms \
                FROM chat_messages c JOIN users u ON u.id = c.sender_id \
                WHERE c.world_id = $1 AND c.channel = $2 \
                ORDER BY c.created_at DESC LIMIT $3 \
             ) t ORDER BY created_at ASC",
        )
        .bind(Uuid::from_u128(self.world_id.0))
        .bind(channel_key)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;
        rows.iter().map(message_from_row).collect()
    }

    async fn channel_latest(&self, channel_key: &str) -> Result<Option<(String, i64)>, RepoError> {
        let row = sqlx::query(
            "SELECT body, (EXTRACT(EPOCH FROM created_at) * 1000)::bigint AS created_ms \
             FROM chat_messages WHERE world_id = $1 AND channel = $2 \
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(Uuid::from_u128(self.world_id.0))
        .bind(channel_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(backend)?;
        row.map(|r| {
            Ok::<_, RepoError>((
                r.try_get("body").map_err(backend)?,
                r.try_get("created_ms").map_err(backend)?,
            ))
        })
        .transpose()
    }

    async fn mark_read(
        &self,
        player: PlayerId,
        conversation: &str,
        now: Timestamp,
    ) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT INTO conversation_reads (player_id, conversation, last_read_at) \
             VALUES ($1, $2, to_timestamp($3::double precision / 1000.0)) \
             ON CONFLICT (player_id, conversation) \
             DO UPDATE SET last_read_at = GREATEST(conversation_reads.last_read_at, EXCLUDED.last_read_at)",
        )
        .bind(Uuid::from_u128(player.0))
        .bind(conversation)
        .bind(now.0)
        .execute(&self.pool)
        .await
        .map_err(backend)?;
        Ok(())
    }

    async fn channel_unread(&self, player: PlayerId, channel_key: &str) -> Result<i64, RepoError> {
        let p = Uuid::from_u128(player.0);
        sqlx::query_scalar(
            "SELECT count(*) FROM chat_messages c \
             WHERE c.world_id = $1 AND c.channel = $2 AND c.sender_id <> $3 \
               AND c.created_at > COALESCE( \
                    (SELECT last_read_at FROM conversation_reads \
                      WHERE player_id = $3 AND conversation = $2), to_timestamp(0))",
        )
        .bind(Uuid::from_u128(self.world_id.0))
        .bind(channel_key)
        .bind(p)
        .fetch_one(&self.pool)
        .await
        .map_err(backend)
    }

    async fn dm_total_unread(&self, player: PlayerId) -> Result<i64, RepoError> {
        // Messages received by `player` after their per-thread read watermark — summed across all senders
        // in one query (the read key is viewer-relative `dm:<sender>`).
        let p = Uuid::from_u128(player.0);
        sqlx::query_scalar(
            "SELECT count(*) FROM direct_messages dm \
             WHERE dm.world_id = $1 AND dm.recipient_id = $2 \
               AND dm.created_at > COALESCE( \
                    (SELECT last_read_at FROM conversation_reads \
                      WHERE player_id = $2 AND conversation = 'dm:' || dm.sender_id::text), \
                    to_timestamp(0))",
        )
        .bind(Uuid::from_u128(self.world_id.0))
        .bind(p)
        .fetch_one(&self.pool)
        .await
        .map_err(backend)
    }
}

/// Map a notifications row to a [`NotificationView`] (unrecognised kinds are skipped by the caller).
fn notification_from_row(r: &PgRow) -> Result<Option<NotificationView>, RepoError> {
    let id: Uuid = r.try_get("id").map_err(backend)?;
    let kind_str: String = r.try_get("kind").map_err(backend)?;
    let Some(kind) = NotificationKind::parse(&kind_str) else {
        return Ok(None);
    };
    Ok(Some(NotificationView {
        id: id.as_u128(),
        kind,
        ref_kind: r.try_get("ref_kind").map_err(backend)?,
        ref_id: r.try_get("ref_id").map_err(backend)?,
        body: r.try_get("body").map_err(backend)?,
        created_ms: r.try_get("created_ms").map_err(backend)?,
        read: r
            .try_get::<Option<bool>, _>("read")
            .map_err(backend)?
            .unwrap_or(false),
    }))
}

#[async_trait]
impl NotificationRepository for PgAccountRepository {
    async fn record(&self, notes: &[NewNotification], now: Timestamp) -> Result<(), RepoError> {
        if notes.is_empty() {
            return Ok(());
        }
        // Bulk insert via UNNEST, then a per-recipient pg_notify in the same statement (persist-then-notify,
        // 026 AC6). The live key is the recipient's private `notif:<uuid>` — a player can only ever subscribe
        // to their own, so no cross-player leak.
        let ids: Vec<Uuid> = notes.iter().map(|_| Uuid::new_v4()).collect();
        let players: Vec<Uuid> = notes.iter().map(|n| Uuid::from_u128(n.player.0)).collect();
        let kinds: Vec<String> = notes.iter().map(|n| n.kind.as_str().to_owned()).collect();
        let ref_kinds: Vec<Option<String>> = notes.iter().map(|n| n.ref_kind.clone()).collect();
        let ref_ids: Vec<Option<String>> = notes.iter().map(|n| n.ref_id.clone()).collect();
        let bodies: Vec<String> = notes.iter().map(|n| n.body.clone()).collect();
        sqlx::query(
            "WITH ins AS ( \
                INSERT INTO notifications \
                    (id, world_id, player_id, kind, ref_kind, ref_id, body, created_at) \
                SELECT u.id, $1, u.player_id, u.kind, u.ref_kind, u.ref_id, u.body, \
                       to_timestamp($8::double precision / 1000.0) \
                FROM unnest($2::uuid[], $3::uuid[], $4::text[], $5::text[], $6::text[], $7::text[]) \
                     AS u(id, player_id, kind, ref_kind, ref_id, body) \
                WHERE NOT EXISTS ( \
                    SELECT 1 FROM notification_mutes m \
                     WHERE m.player_id = u.player_id AND m.kind = u.kind) \
                RETURNING player_id, kind \
             ) \
             SELECT pg_notify('notifications', json_build_object( \
                'key', 'notif:' || player_id::text, \
                'kind', kind \
             )::text) FROM ins",
        )
        .bind(Uuid::from_u128(self.world_id.0))
        .bind(&ids)
        .bind(&players)
        .bind(&kinds)
        .bind(&ref_kinds)
        .bind(&ref_ids)
        .bind(&bodies)
        .bind(now.0)
        .execute(&self.pool)
        .await
        .map_err(backend)?;
        Ok(())
    }

    async fn list(&self, player: PlayerId, limit: i64) -> Result<Vec<NotificationView>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, kind, ref_kind, ref_id, body, \
                    (EXTRACT(EPOCH FROM created_at) * 1000)::bigint AS created_ms, \
                    (read_at IS NOT NULL) AS read \
             FROM notifications WHERE world_id = $1 AND player_id = $2 \
             ORDER BY created_at DESC, id DESC LIMIT $3",
        )
        .bind(Uuid::from_u128(self.world_id.0))
        .bind(Uuid::from_u128(player.0))
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;
        Ok(rows
            .iter()
            .map(notification_from_row)
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect())
    }

    async fn unread_count(&self, player: PlayerId) -> Result<i64, RepoError> {
        sqlx::query_scalar(
            "SELECT count(*) FROM notifications \
             WHERE world_id = $1 AND player_id = $2 AND read_at IS NULL",
        )
        .bind(Uuid::from_u128(self.world_id.0))
        .bind(Uuid::from_u128(player.0))
        .fetch_one(&self.pool)
        .await
        .map_err(backend)
    }

    async fn mark_read(&self, player: PlayerId, now: Timestamp) -> Result<(), RepoError> {
        sqlx::query(
            "UPDATE notifications SET read_at = to_timestamp($3::double precision / 1000.0) \
             WHERE world_id = $1 AND player_id = $2 AND read_at IS NULL",
        )
        .bind(Uuid::from_u128(self.world_id.0))
        .bind(Uuid::from_u128(player.0))
        .bind(now.0)
        .execute(&self.pool)
        .await
        .map_err(backend)?;
        Ok(())
    }

    async fn muted_kinds(&self, player: PlayerId) -> Result<Vec<NotificationKind>, RepoError> {
        let rows: Vec<(String,)> =
            sqlx::query_as("SELECT kind FROM notification_mutes WHERE player_id = $1")
                .bind(Uuid::from_u128(player.0))
                .fetch_all(&self.pool)
                .await
                .map_err(backend)?;
        // Unrecognised stored kinds (e.g. from a future version) are simply ignored.
        Ok(rows
            .iter()
            .filter_map(|(k,)| NotificationKind::parse(k))
            .collect())
    }

    async fn set_mute(
        &self,
        player: PlayerId,
        kind: NotificationKind,
        muted: bool,
    ) -> Result<(), RepoError> {
        if muted {
            sqlx::query(
                "INSERT INTO notification_mutes (player_id, kind) VALUES ($1, $2) \
                 ON CONFLICT (player_id, kind) DO NOTHING",
            )
            .bind(Uuid::from_u128(player.0))
            .bind(kind.as_str())
            .execute(&self.pool)
            .await
            .map_err(backend)?;
        } else {
            sqlx::query("DELETE FROM notification_mutes WHERE player_id = $1 AND kind = $2")
                .bind(Uuid::from_u128(player.0))
                .bind(kind.as_str())
                .execute(&self.pool)
                .await
                .map_err(backend)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::World;
    use eperica_application::{
        BoardScope, ConflictMetric, ConquestTransfer, DefenderContribution, NewBattleReport,
        ReinforcementReturn,
    };
    use eperica_domain::{
        AttackMode, EconomyRules, GameSpeed, LifecycleRules, WorldConfig, is_protected,
    };

    /// The resources row's last-settled time — the snapshot orders must be computed from.
    async fn snapshot(repo: &PgAccountRepository, village: VillageId) -> Timestamp {
        repo.stored_resources(village).await.unwrap().unwrap().1
    }

    /// Per-test fixture: every `#[sqlx::test]` gets its own freshly-migrated, isolated database, so
    /// the world row, event queue, oases and map tiles are private — no cross-test contention, no
    /// `--test-threads=1`. All tests use the same world config (speed 1.0, radius 50).
    struct Setup {
        config: WorldConfig,
        world: World,
        econ: EconomyRules,
        repo: PgAccountRepository,
        template: StartingVillage,
    }

    async fn setup(pool: PgPool) -> Setup {
        let config = WorldConfig::new(GameSpeed::new(1.0).unwrap(), 50);
        let world = crate::world::ensure_world(&pool, &config)
            .await
            .expect("ensure world");
        let econ = crate::economy_rules().expect("economy rules");
        let lifecycle = crate::lifecycle_rules().expect("lifecycle rules");
        let repo = PgAccountRepository::new(
            pool.clone(),
            world.id,
            world.seed,
            config.radius,
            econ.starting_amounts,
            lifecycle.beginner_protection_secs,
            config.speed,
        );
        let template = crate::starting_village().unwrap();
        Setup {
            config,
            world,
            econ,
            repo,
            template,
        }
    }

    /// Create a bare account (no extra villages) and return its player id.
    async fn make_account(
        repo: &PgAccountRepository,
        template: &StartingVillage,
        tag: &str,
    ) -> PlayerId {
        let uname = format!("{tag}_{}", Uuid::new_v4().simple());
        repo.create_account(
            NewUser {
                username: uname.clone(),
                email: format!("{uname}@example.com"),
                password_hash: "h".to_owned(),
                email_confirmed: true,
                tribe: Tribe::Gauls,
            },
            template,
        )
        .await
        .expect("create account")
        .id
    }

    /// 019 AC1: registration grants beginner's protection — `protected_until` is set ahead of now by
    /// the (speed-scaled) window.
    #[sqlx::test(migrations = "../../migrations")]
    async fn create_account_grants_beginner_protection(pool: PgPool) {
        let Setup { repo, template, .. } = setup(pool.clone()).await;
        let lifecycle = crate::lifecycle_rules().unwrap();
        let player = make_account(&repo, &template, "prot").await;
        let until = repo
            .protection_of(player)
            .await
            .unwrap()
            .expect("a new account is protected");
        let now = crate::now();
        assert!(until.0 > now.0, "protection extends into the future");
        // At speed 1.0 the window is the base seconds; allow generous slack for clock + insert latency.
        let window_ms = lifecycle.beginner_protection_secs * 1000;
        assert!(
            until.0 >= now.0 + window_ms - 60_000,
            "≈ the full window remains"
        );
        assert!(
            is_protected(Some(until), now),
            "the player reads as protected"
        );
    }

    /// 019 AC3: `end_protection` ends an active window and is idempotent / never re-arms.
    #[sqlx::test(migrations = "../../migrations")]
    async fn end_protection_is_one_way(pool: PgPool) {
        let Setup { repo, template, .. } = setup(pool.clone()).await;
        let player = make_account(&repo, &template, "end").await;
        let t = crate::now();
        repo.end_protection(player, t).await.unwrap();
        let after = repo.protection_of(player).await.unwrap().unwrap();
        assert!(
            !is_protected(Some(after), crate::now()),
            "protection has ended"
        );
        // Re-ending later does not push protection further out (no re-arm).
        repo.end_protection(player, Timestamp(t.0 + 1_000_000))
            .await
            .unwrap();
        let after2 = repo.protection_of(player).await.unwrap().unwrap();
        assert_eq!(after2.0, after.0, "already-ended protection is not moved");
    }

    /// 019 AC5: `touch_activity` is throttled — a no-op while fresh, a write once stale.
    #[sqlx::test(migrations = "../../migrations")]
    async fn touch_activity_is_throttled(pool: PgPool) {
        let Setup { repo, template, .. } = setup(pool.clone()).await;
        let player = make_account(&repo, &template, "act").await;
        let pid = Uuid::from_u128(player.0);
        let read = async |pool: &PgPool| -> i64 {
            sqlx::query_scalar("SELECT (EXTRACT(EPOCH FROM last_activity) * 1000)::bigint FROM users WHERE id = $1")
                .bind(pid)
                .fetch_one(pool)
                .await
                .unwrap()
        };
        let seeded = read(&pool).await;
        // A touch within the throttle window is a no-op.
        repo.touch_activity(player, Timestamp(seeded + 1000))
            .await
            .unwrap();
        assert_eq!(read(&pool).await, seeded, "fresh activity is not rewritten");
        // A touch past the throttle window writes.
        let later = Timestamp(seeded + ACTIVITY_THROTTLE_MS + 1000);
        repo.touch_activity(player, later).await.unwrap();
        assert_eq!(read(&pool).await, later.0, "stale activity is refreshed");
    }

    /// 019 AC8: an abandoned account surfaces as `abandoned` (which blocks login at the use-case).
    #[sqlx::test(migrations = "../../migrations")]
    async fn abandoned_flag_surfaces(pool: PgPool) {
        let Setup { repo, template, .. } = setup(pool.clone()).await;
        let player = make_account(&repo, &template, "aband").await;
        let rec = repo.find_user_by_id(player).await.unwrap().unwrap();
        assert!(!rec.abandoned, "a live account is not abandoned");
        sqlx::query("UPDATE users SET abandoned_at = now() WHERE id = $1")
            .bind(Uuid::from_u128(player.0))
            .execute(&pool)
            .await
            .unwrap();
        let rec = repo.find_user_by_id(player).await.unwrap().unwrap();
        assert!(rec.abandoned, "the sweep flag surfaces on the user record");
    }

    /// 022 AC1/AC5/AC8: the moderator + sanction fields round-trip, and a sanctioned account reads as
    /// blocked via the pure `account_blocked` predicate.
    #[sqlx::test(migrations = "../../migrations")]
    async fn user_sanction_and_role_fields_round_trip(pool: PgPool) {
        use eperica_domain::account_blocked;
        let Setup { repo, template, .. } = setup(pool.clone()).await;
        let player = make_account(&repo, &template, "sanc").await;

        // Fresh account: no role, no sanction, not blocked.
        let rec = repo.find_user_by_id(player).await.unwrap().unwrap();
        assert!(!rec.is_moderator);
        assert_eq!(rec.banned_at, None);
        assert_eq!(rec.suspended_until, None);
        assert!(!account_blocked(
            rec.banned_at,
            rec.suspended_until,
            crate::now()
        ));

        // Grant the moderator role + ban — both round-trip and the account reads as blocked.
        sqlx::query("UPDATE users SET is_moderator = true, banned_at = now() WHERE id = $1")
            .bind(Uuid::from_u128(player.0))
            .execute(&pool)
            .await
            .unwrap();
        let rec = repo.find_user_by_id(player).await.unwrap().unwrap();
        assert!(rec.is_moderator, "the moderator role surfaces");
        assert!(rec.banned_at.is_some(), "the ban instant surfaces");
        assert!(account_blocked(
            rec.banned_at,
            rec.suspended_until,
            crate::now()
        ));

        // A past suspension (no ban) does not block; a future one does.
        sqlx::query(
            "UPDATE users SET banned_at = NULL, suspended_until = now() - interval '1 hour' \
             WHERE id = $1",
        )
        .bind(Uuid::from_u128(player.0))
        .execute(&pool)
        .await
        .unwrap();
        let rec = repo.find_user_by_id(player).await.unwrap().unwrap();
        assert!(!account_blocked(
            rec.banned_at,
            rec.suspended_until,
            crate::now()
        ));
        sqlx::query("UPDATE users SET suspended_until = now() + interval '1 hour' WHERE id = $1")
            .bind(Uuid::from_u128(player.0))
            .execute(&pool)
            .await
            .unwrap();
        let rec = repo.find_user_by_id(player).await.unwrap().unwrap();
        assert!(account_blocked(
            rec.banned_at,
            rec.suspended_until,
            crate::now()
        ));
    }

    /// 023 AC1/AC2: in a large seeded world the hot read paths (population board, `villages_of`, map
    /// viewport, player stats) stay within their latency budgets (best-of-N). Catches an order-of-magnitude
    /// regression (e.g. a new sequential scan), not micro-jitter.
    #[sqlx::test(migrations = "../../migrations")]
    async fn scale_hot_reads_within_budget(pool: PgPool) {
        let Setup {
            repo, econ, world, ..
        } = setup(pool.clone()).await;
        // Seed a large world via the shared seeder (AC1) — 10k players, the launch-target scale.
        let players = 10_000u32;
        let summary = crate::perf::seed_world(&pool, world.id, players)
            .await
            .expect("seed");
        assert!(
            summary.players >= i64::from(players),
            "all perf players seeded"
        );
        assert!(summary.villages >= i64::from(players), "a village each");

        // A seeded player for the per-player reads.
        let pid_uuid: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'perf_1'")
            .fetch_one(&pool)
            .await
            .unwrap();
        let pid = PlayerId(pid_uuid.as_u128());

        // A realistic map viewport overlapping the seeded block.
        let w = crate::perf::seed_block_width(players).min(31);
        let viewport: Vec<Coordinate> = (0..w)
            .flat_map(|x| (0..w).map(move |y| Coordinate::new(x, y)))
            .collect();

        // best-of-5 wall-clock for each hot path.
        let mut board_best = std::time::Duration::MAX;
        let mut vof_best = std::time::Duration::MAX;
        let mut map_best = std::time::Duration::MAX;
        let mut stats_best = std::time::Duration::MAX;
        for _ in 0..5 {
            let t = std::time::Instant::now();
            repo.population_board(&econ, BoardScope::World, 100)
                .await
                .unwrap();
            board_best = board_best.min(t.elapsed());

            let t = std::time::Instant::now();
            repo.villages_of(pid).await.unwrap();
            vof_best = vof_best.min(t.elapsed());

            let t = std::time::Instant::now();
            repo.villages_at(&viewport).await.unwrap();
            map_best = map_best.min(t.elapsed());

            let t = std::time::Instant::now();
            eperica_application::player_statistics(&repo, &econ, pid)
                .await
                .unwrap();
            stats_best = stats_best.min(t.elapsed());
        }

        // Generous best-of-5 ceilings (local PG is far faster; a seq-scan regression blows past these).
        assert!(
            board_best.as_millis() < 1000,
            "population board over {players} players too slow: {board_best:?}"
        );
        assert!(
            vof_best.as_millis() < 250,
            "villages_of too slow: {vof_best:?}"
        );
        assert!(
            map_best.as_millis() < 500,
            "map viewport too slow: {map_best:?}"
        );
        assert!(
            stats_best.as_millis() < 500,
            "player stats too slow: {stats_best:?}"
        );
    }

    /// 023 AC3: the scheduler drains a large due-event backlog within a generous time ceiling and above a
    /// throughput floor, processing every event exactly once.
    #[sqlx::test(migrations = "../../migrations")]
    async fn scheduler_throughput_drains_backlog(pool: PgPool) {
        let Setup { world, .. } = setup(pool.clone()).await; // ensure the world exists
        let backlog = 2000u32;
        crate::perf::seed_heartbeats(&pool, backlog).await.unwrap();
        let store = crate::PgEventStore::new(pool.clone(), world.id);
        let now = crate::now();

        let start = std::time::Instant::now();
        let mut processed = 0usize;
        loop {
            let n = eperica_application::process_due(&store, now, 200)
                .await
                .unwrap();
            processed += n;
            if n == 0 {
                break;
            }
        }
        let elapsed = start.elapsed();
        assert_eq!(
            processed, backlog as usize,
            "every due event processed once"
        );
        // Conservative bounds (local PG is far faster) that still flag an order-of-magnitude regression.
        assert!(
            elapsed.as_secs() < 20,
            "draining {backlog} events took {elapsed:?}"
        );
        let per_sec = backlog as f64 / elapsed.as_secs_f64().max(0.001);
        assert!(per_sec > 100.0, "throughput floor: {per_sec:.0} events/s");

        // Idempotent drain: nothing left.
        assert_eq!(
            eperica_application::process_due(&store, now, 200)
                .await
                .unwrap(),
            0
        );
    }

    /// 023 AC3 (determinism): a claim takes the **earliest** due events in `(due_at, seq)` order — every
    /// claimed (`processing`) event has a lower `seq` than every still-`pending` one, so same-instant
    /// ordering is deterministic (P6/P11), not left to scheduling chance.
    #[sqlx::test(migrations = "../../migrations")]
    async fn claim_takes_earliest_in_due_order(pool: PgPool) {
        use eperica_application::EventStore;
        let Setup { world, .. } = setup(pool.clone()).await;
        crate::perf::seed_heartbeats(&pool, 200).await.unwrap();
        let store = crate::PgEventStore::new(pool.clone(), world.id);
        // Claim a strict subset (all share due_at = now()-1s, so only seq breaks the tie).
        let claimed = store.claim_due(crate::now(), 50).await.unwrap();
        assert_eq!(claimed.len(), 50);

        let max_claimed: i64 =
            sqlx::query_scalar("SELECT max(seq) FROM scheduled_events WHERE status = 'processing'")
                .fetch_one(&pool)
                .await
                .unwrap();
        let min_pending: i64 =
            sqlx::query_scalar("SELECT min(seq) FROM scheduled_events WHERE status = 'pending'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(
            max_claimed < min_pending,
            "the claim took the earliest events by seq ({max_claimed} < {min_pending})"
        );
    }

    /// 023 AC5: two scheduler instances claiming the same backlog process each event **exactly once** —
    /// the `FOR UPDATE SKIP LOCKED` guarantee that makes the scheduler horizontally scalable (P5).
    #[sqlx::test(migrations = "../../migrations")]
    async fn concurrent_claim_processes_each_once(pool: PgPool) {
        use eperica_application::EventStore;
        let Setup { world, .. } = setup(pool.clone()).await;
        let backlog = 500u32;
        crate::perf::seed_heartbeats(&pool, backlog).await.unwrap();
        let now = crate::now();
        let a = crate::PgEventStore::new(pool.clone(), world.id);
        let b = crate::PgEventStore::new(pool.clone(), world.id);

        // Two instances claim concurrently.
        let (ra, rb) = tokio::join!(
            a.claim_due(now, backlog as i64),
            b.claim_due(now, backlog as i64)
        );
        let ca = ra.unwrap();
        let cb = rb.unwrap();

        let ids_a: std::collections::HashSet<u128> = ca.iter().map(|e| e.id).collect();
        let ids_b: std::collections::HashSet<u128> = cb.iter().map(|e| e.id).collect();
        assert!(
            ids_a.is_disjoint(&ids_b),
            "no event is claimed by both instances"
        );
        assert_eq!(
            ids_a.len() + ids_b.len(),
            backlog as usize,
            "together they claim every event exactly once"
        );
    }

    /// 024 AC1–AC5: DM send/history + per-conversation unread/mark-read, and channel access (global open,
    /// alliance members-only), driven through the comms use-cases.
    #[sqlx::test(migrations = "../../migrations")]
    async fn conversations_dm_and_channel_flow(pool: PgPool) {
        use eperica_application::{
            CommsError, conversation_list, dm_key, open_chat, open_dm, send_chat, send_dm,
            unread_badge,
        };
        use eperica_domain::PlayerId;
        let Setup { repo, template, .. } = setup(pool.clone()).await;
        let a = make_account(&repo, &template, "alice").await;
        let b = make_account(&repo, &template, "bob").await;
        let now = crate::now();

        // AC1: self-DM + unknown recipient rejected.
        assert!(matches!(
            send_dm(&repo, &repo, a, a, "hi me", now).await,
            Err(CommsError::SelfSend)
        ));
        assert!(matches!(
            send_dm(&repo, &repo, a, PlayerId(123_456_789), "ghost", now).await,
            Err(CommsError::RecipientUnavailable)
        ));
        assert!(matches!(
            send_dm(&repo, &repo, a, b, "   ", now).await,
            Err(CommsError::Invalid)
        ));

        // AC1/AC2: A DMs B; both see the thread.
        send_dm(&repo, &repo, a, b, "hello bob", now).await.unwrap();
        send_dm(&repo, &repo, b, a, "hi alice", Timestamp(now.0 + 1000))
            .await
            .unwrap();
        let from_b = repo.dm_history(b, a, 50).await.unwrap();
        assert_eq!(from_b.len(), 2);
        assert_eq!(from_b[0].body, "hello bob"); // oldest first
        assert_eq!(from_b[1].body, "hi alice");

        // 026 AC3: each DM recorded a new-message notification for its recipient.
        let bob_notes = NotificationRepository::list(&repo, b, 10).await.unwrap();
        assert_eq!(bob_notes.len(), 1);
        assert_eq!(bob_notes[0].kind, NotificationKind::NewMessage);
        assert_eq!(bob_notes[0].ref_kind.as_deref(), Some("dm"));
        assert_eq!(
            NotificationRepository::unread_count(&repo, a)
                .await
                .unwrap(),
            1,
            "alice was notified of bob's reply"
        );

        // AC3/AC4: B has 1 unread from A (B's own reply doesn't count); opening clears it.
        let badge_before = unread_badge(&repo, &repo, b).await.unwrap();
        assert_eq!(badge_before, 1, "one unread DM from alice");
        open_dm(&repo, b, a, 50, Timestamp(now.0 + 2000))
            .await
            .unwrap();
        assert_eq!(
            unread_badge(&repo, &repo, b).await.unwrap(),
            0,
            "read clears it"
        );

        // AC3: the conversations list shows the DM thread + the global channel.
        let list = conversation_list(&repo, &repo, b).await.unwrap();
        assert!(
            list.iter()
                .any(|c| c.key == dm_key(a) && c.title.starts_with("alice")),
            "the DM thread with alice is listed"
        );
        assert!(list.iter().any(|c| c.key == "global"));

        // AC5: global is open to all; a non-member alliance channel is forbidden.
        send_chat(&repo, &repo, a, "global", "gg all", now)
            .await
            .unwrap();
        assert_eq!(repo.chat_history("global", 50).await.unwrap().len(), 1);
        assert!(matches!(
            send_chat(&repo, &repo, a, "alliance:999", "secret", now).await,
            Err(CommsError::Forbidden)
        ));
        assert!(matches!(
            open_chat(&repo, &repo, a, "alliance:999", 50, now).await,
            Err(CommsError::Forbidden)
        ));

        // A joins an alliance → may post + read its channel; B (non-member) may not.
        let alliance = Uuid::new_v4();
        sqlx::query("INSERT INTO alliances (id, name, tag, founder_id) VALUES ($1,'A','AAA',$2)")
            .bind(alliance)
            .bind(Uuid::from_u128(a.0))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO alliance_members (player_id, alliance_id, role) VALUES ($1,$2,'founder')",
        )
        .bind(Uuid::from_u128(a.0))
        .bind(alliance)
        .execute(&pool)
        .await
        .unwrap();
        let akey = format!("alliance:{}", alliance.as_u128());
        send_chat(&repo, &repo, a, &akey, "team only", now)
            .await
            .unwrap();
        assert_eq!(repo.chat_history(&akey, 50).await.unwrap().len(), 1);
        assert!(
            matches!(
                open_chat(&repo, &repo, b, &akey, 50, now).await,
                Err(CommsError::Forbidden)
            ),
            "non-member cannot open the alliance channel"
        );
    }

    /// 030 AC1/AC5: sitter grant/revoke + is_sitter/count round-trip; the audit log records + reads back.
    #[sqlx::test(migrations = "../../migrations")]
    async fn account_sitters_and_audit_log(pool: PgPool) {
        let Setup { repo, template, .. } = setup(pool.clone()).await;
        let owner = make_account(&repo, &template, "owner").await;
        let sitter = make_account(&repo, &template, "sitter").await;
        let other = make_account(&repo, &template, "other").await;

        // Default: nothing authorised.
        assert!(!repo.is_sitter(owner, sitter).await.unwrap());
        assert_eq!(repo.count_sitters(owner).await.unwrap(), 0);

        // Grant (idempotent) → is_sitter + count + lists reflect it.
        repo.grant_sitter(owner, sitter).await.unwrap();
        repo.grant_sitter(owner, sitter).await.unwrap(); // idempotent
        assert!(repo.is_sitter(owner, sitter).await.unwrap());
        assert!(!repo.is_sitter(owner, other).await.unwrap());
        assert_eq!(repo.count_sitters(owner).await.unwrap(), 1);
        let sitters = repo.sitters_of(owner).await.unwrap();
        assert_eq!(sitters.len(), 1);
        assert_eq!(sitters[0].player, sitter);
        assert_eq!(repo.sitting_for(sitter).await.unwrap().len(), 1);

        // Audit log records sitter actions, most-recent first.
        repo.log_sitter_action(owner, sitter, "POST /village/build", crate::now())
            .await
            .unwrap();
        repo.log_sitter_action(
            owner,
            sitter,
            "POST /village/train",
            Timestamp(crate::now().0 + 1000),
        )
        .await
        .unwrap();
        let log = repo.sitter_actions(owner, 10).await.unwrap();
        assert_eq!(log.len(), 2);
        assert_eq!(log[0].action, "POST /village/train"); // newest first
        assert!(log[0].sitter_name.starts_with("sitter")); // make_account names are "sitter_<uuid>"

        // Revoke → de-authorised.
        repo.revoke_sitter(owner, sitter).await.unwrap();
        assert!(!repo.is_sitter(owner, sitter).await.unwrap());
        assert_eq!(repo.count_sitters(owner).await.unwrap(), 0);
    }

    /// 028 AC1/AC2/AC5: who-is search — username prefix (abandoned/NPC excluded) + alliance name/tag prefix.
    #[sqlx::test(migrations = "../../migrations")]
    async fn search_players_and_alliances(pool: PgPool) {
        use eperica_domain::AllianceId;
        let Setup { repo, template, .. } = setup(pool.clone()).await;
        let alice = make_account(&repo, &template, "alice").await;
        let _alvin = make_account(&repo, &template, "alvin").await;
        let bob = make_account(&repo, &template, "bob").await;
        // Rename to deterministic usernames for prefix assertions.
        for (p, name) in [(alice, "Alaric"), (bob, "Boris")] {
            sqlx::query("UPDATE users SET username = $2 WHERE id = $1")
                .bind(Uuid::from_u128(p.0))
                .bind(name)
                .execute(&pool)
                .await
                .unwrap();
        }
        // An abandoned account must not surface.
        let ghost = make_account(&repo, &template, "Alabaster").await;
        sqlx::query("UPDATE users SET abandoned_at = now() WHERE id = $1")
            .bind(Uuid::from_u128(ghost.0))
            .execute(&pool)
            .await
            .unwrap();

        // Prefix "ala" (case-insensitive) → Alaric, not Boris, not the abandoned Alabaster.
        let hits = repo.search_players("ala", 10).await.unwrap();
        let names: Vec<&str> = hits.iter().map(|h| h.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["Alaric"],
            "prefix match excludes abandoned + non-matches"
        );
        // The cap is respected.
        assert!(repo.search_players("a", 1).await.unwrap().len() <= 1);
        // A wildcard char is literal (no injection of LIKE semantics).
        assert!(repo.search_players("%", 10).await.unwrap().is_empty());

        // Alliances: match by name and by tag.
        let aid = AllianceId(Uuid::new_v4().as_u128());
        sqlx::query(
            "INSERT INTO alliances (id, name, tag, founder_id, created_at) \
             VALUES ($1, 'Iron Pact', 'IRON', $2, now())",
        )
        .bind(Uuid::from_u128(aid.0))
        .bind(Uuid::from_u128(alice.0))
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(
            repo.search_alliances("iron p", 10).await.unwrap().len(),
            1,
            "by name"
        );
        assert_eq!(
            repo.search_alliances("iro", 10).await.unwrap().len(),
            1,
            "by tag"
        );
        assert!(repo.search_alliances("zzz", 10).await.unwrap().is_empty());
    }

    /// 029 AC2/AC3/AC4: muting a kind suppresses its generation for that player only; un-muting restores it.
    #[sqlx::test(migrations = "../../migrations")]
    async fn notification_mutes_gate_generation(pool: PgPool) {
        let Setup { repo, template, .. } = setup(pool.clone()).await;
        let a = make_account(&repo, &template, "muter").await;
        let b = make_account(&repo, &template, "other").await;

        // Default: nothing muted.
        assert!(repo.muted_kinds(a).await.unwrap().is_empty());

        // Mute NewMessage for `a` (idempotent), leave `b` alone.
        NotificationRepository::set_mute(&repo, a, NotificationKind::NewMessage, true)
            .await
            .unwrap();
        NotificationRepository::set_mute(&repo, a, NotificationKind::NewMessage, true)
            .await
            .unwrap(); // idempotent
        assert_eq!(
            repo.muted_kinds(a).await.unwrap(),
            vec![NotificationKind::NewMessage]
        );

        // record() a NewMessage for both: `a` (muted) gets none, `b` does.
        let note = |p: PlayerId| NewNotification {
            player: p,
            kind: NotificationKind::NewMessage,
            ref_kind: Some("dm".to_owned()),
            ref_id: None,
            body: String::new(),
        };
        repo.record(&[note(a), note(b)], crate::now())
            .await
            .unwrap();
        assert_eq!(
            NotificationRepository::unread_count(&repo, a)
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            NotificationRepository::unread_count(&repo, b)
                .await
                .unwrap(),
            1
        );

        // A non-muted kind for `a` still records.
        repo.record(
            &[NewNotification {
                player: a,
                kind: NotificationKind::IncomingAttack,
                ref_kind: None,
                ref_id: None,
                body: String::new(),
            }],
            crate::now(),
        )
        .await
        .unwrap();
        assert_eq!(
            NotificationRepository::unread_count(&repo, a)
                .await
                .unwrap(),
            1
        );

        // Un-mute → generation restored.
        NotificationRepository::set_mute(&repo, a, NotificationKind::NewMessage, false)
            .await
            .unwrap();
        assert!(repo.muted_kinds(a).await.unwrap().is_empty());
        repo.record(&[note(a)], crate::now()).await.unwrap();
        assert_eq!(
            NotificationRepository::unread_count(&repo, a)
                .await
                .unwrap(),
            2
        );
    }

    /// 027 AC1–AC3: forum threads + posts round-trip; a reply bumps the thread's activity; `thread_head`
    /// returns the owning alliance + announcement flag.
    #[sqlx::test(migrations = "../../migrations")]
    async fn alliance_forum_threads_and_posts(pool: PgPool) {
        use eperica_domain::AllianceId;
        let Setup { repo, template, .. } = setup(pool.clone()).await;
        let founder = make_account(&repo, &template, "ffound").await;
        // Seed an alliance + membership directly (founding has an eligibility gate not relevant here).
        let alliance = AllianceId(Uuid::new_v4().as_u128());
        sqlx::query(
            "INSERT INTO alliances (id, name, tag, founder_id, created_at) \
             VALUES ($1, 'Iron Pact', 'IRON', $2, now())",
        )
        .bind(Uuid::from_u128(alliance.0))
        .bind(Uuid::from_u128(founder.0))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO alliance_members (player_id, alliance_id, role, rights, joined_at) \
             VALUES ($1, $2, 'founder', 0, now())",
        )
        .bind(Uuid::from_u128(founder.0))
        .bind(Uuid::from_u128(alliance.0))
        .execute(&pool)
        .await
        .unwrap();
        let now = crate::now();

        // Start an ordinary thread (+ first post) and an announcement.
        let tid = repo
            .create_thread(
                alliance,
                founder,
                "Muster tonight",
                "Be online at 20:00",
                false,
                now,
            )
            .await
            .unwrap();
        let aid_thread = repo
            .create_thread(
                alliance,
                founder,
                "Rules",
                "Read the rules",
                true,
                Timestamp(now.0 + 1000),
            )
            .await
            .unwrap();

        let threads = repo.list_threads(alliance, 50).await.unwrap();
        assert_eq!(threads.len(), 2);
        // Most-recent first ⇒ the announcement (later) leads.
        assert_eq!(threads[0].id, aid_thread);
        assert!(threads[0].announcement);
        assert_eq!(threads[0].post_count, 1);
        assert!(threads.iter().any(|t| t.id == tid && !t.announcement));

        // thread_head exposes the owner + flag.
        let head = repo.thread_head(tid).await.unwrap().unwrap();
        assert_eq!(head.alliance, alliance);
        assert!(!head.announcement);
        assert!(repo.thread_head(0).await.unwrap().is_none());

        // A reply lands + bumps last_post_at so the ordinary thread now leads.
        repo.add_post(tid, founder, "Confirmed", Timestamp(now.0 + 5000))
            .await
            .unwrap();
        let posts = repo.list_posts(tid, 50).await.unwrap();
        assert_eq!(posts.len(), 2);
        assert_eq!(posts[0].body, "Be online at 20:00"); // oldest first
        assert_eq!(posts[1].body, "Confirmed");
        let threads = repo.list_threads(alliance, 50).await.unwrap();
        assert_eq!(
            threads[0].id, tid,
            "the just-replied thread is now most-recent"
        );

        // Sanity: an unrelated alliance id sees nothing.
        assert!(
            repo.list_threads(AllianceId(0), 50)
                .await
                .unwrap()
                .is_empty()
        );
    }

    /// 026 AC4/AC5: notifications record → list/unread reflect them; `mark_read` clears only the caller's.
    #[sqlx::test(migrations = "../../migrations")]
    async fn notifications_record_list_and_mark_read(pool: PgPool) {
        let Setup { repo, template, .. } = setup(pool.clone()).await;
        let a = make_account(&repo, &template, "alice").await;
        let b = make_account(&repo, &template, "bob").await;
        let now = crate::now();

        // Record two for Alice, one for Bob, in one batch.
        repo.record(
            &[
                NewNotification {
                    player: a,
                    kind: NotificationKind::NewMessage,
                    ref_kind: Some("dm".to_owned()),
                    ref_id: Some(Uuid::from_u128(b.0).to_string()),
                    body: "msg".to_owned(),
                },
                NewNotification {
                    player: a,
                    kind: NotificationKind::IncomingAttack,
                    ref_kind: Some("village".to_owned()),
                    ref_id: Some("1|2".to_owned()),
                    body: "arrives soon".to_owned(),
                },
                NewNotification {
                    player: b,
                    kind: NotificationKind::BattleReport,
                    ref_kind: Some("report".to_owned()),
                    ref_id: Some(Uuid::new_v4().to_string()),
                    body: String::new(),
                },
            ],
            now,
        )
        .await
        .unwrap();

        // Alice sees both of hers (and only hers), all unread.
        let alice_feed = repo.list(a, 50).await.unwrap();
        assert_eq!(alice_feed.len(), 2);
        assert!(
            alice_feed
                .iter()
                .any(|n| n.kind == NotificationKind::IncomingAttack)
        );
        assert!(
            alice_feed
                .iter()
                .any(|n| n.kind == NotificationKind::NewMessage)
        );
        assert!(alice_feed.iter().all(|n| !n.read));
        assert_eq!(repo.unread_count(a).await.unwrap(), 2);
        assert_eq!(repo.unread_count(b).await.unwrap(), 1);

        // Alice marks read — her count drops to 0, Bob's is untouched.
        NotificationRepository::mark_read(&repo, a, crate::now())
            .await
            .unwrap();
        assert_eq!(repo.unread_count(a).await.unwrap(), 0);
        assert_eq!(repo.unread_count(b).await.unwrap(), 1);
        assert!(repo.list(a, 50).await.unwrap().iter().all(|n| n.read));

        // An empty batch is a no-op.
        repo.record(&[], crate::now()).await.unwrap();
        assert_eq!(repo.list(a, 50).await.unwrap().len(), 2);
    }

    /// 025 AC1/AC2/AC3: a profile's bio round-trips via the edit use-case (invalid rejected), and presence
    /// is derived from the persisted last_activity.
    #[sqlx::test(migrations = "../../migrations")]
    async fn profile_bio_and_presence(pool: PgPool) {
        use eperica_application::{ProfileError, edit_bio, view_profile};
        use eperica_domain::{Presence, presence};
        let Setup { repo, template, .. } = setup(pool.clone()).await;
        let player = make_account(&repo, &template, "prof").await;

        // Fresh profile: empty bio, a recent last_activity (set at spawn).
        let p = view_profile(&repo, player).await.unwrap();
        assert_eq!(p.bio, "");
        assert!(p.name.starts_with("prof"));

        // Edit the bio (trimmed); a too-long bio is rejected.
        edit_bio(&repo, player, "  Founder of the Iron Pact.  ")
            .await
            .unwrap();
        assert_eq!(
            view_profile(&repo, player).await.unwrap().bio,
            "Founder of the Iron Pact."
        );
        assert!(matches!(
            edit_bio(&repo, player, &"x".repeat(5000)).await,
            Err(ProfileError::Invalid)
        ));

        // Presence: a freshly-active account is Online; a stale last_activity reads LastSeen.
        let now = crate::now();
        let fresh = view_profile(&repo, player).await.unwrap();
        assert_eq!(presence(fresh.last_activity, now, 600), Presence::Online);
        sqlx::query("UPDATE users SET last_activity = now() - interval '1 hour' WHERE id = $1")
            .bind(Uuid::from_u128(player.0))
            .execute(&pool)
            .await
            .unwrap();
        let stale = view_profile(&repo, player).await.unwrap();
        assert!(matches!(
            presence(stale.last_activity, now, 600),
            Presence::LastSeen(_)
        ));
    }

    /// 022 AC1–AC5: a player reports an account; a duplicate open report + a self-report are rejected; a
    /// non-moderator cannot review; a moderator reviews and resolves with a ban (idempotently).
    #[sqlx::test(migrations = "../../migrations")]
    async fn moderation_report_review_resolve_flow(pool: PgPool) {
        use eperica_application::ModerationError;
        use eperica_domain::{ReportReason, SanctionKind, account_blocked};
        let Setup { repo, template, .. } = setup(pool.clone()).await;
        let rules = crate::fair_play_rules().unwrap();
        let reporter = make_account(&repo, &template, "reporter").await;
        let subject = make_account(&repo, &template, "subject").await;
        let moderator = make_account(&repo, &template, "moderator").await;
        repo.set_moderator(moderator, true).await.unwrap();

        // AC2: a self-report is rejected.
        assert!(matches!(
            eperica_application::file_report(&repo, reporter, reporter, ReportReason::Botting, "")
                .await,
            Err(ModerationError::SelfReport)
        ));

        // AC2: a first report is created; a duplicate open report collapses.
        assert!(
            eperica_application::file_report(
                &repo,
                reporter,
                subject,
                ReportReason::Botting,
                "scripting at night"
            )
            .await
            .unwrap()
        );
        assert!(
            !eperica_application::file_report(
                &repo,
                reporter,
                subject,
                ReportReason::Pushing,
                "again"
            )
            .await
            .unwrap(),
            "a duplicate open report collapses"
        );

        // AC3: a non-moderator cannot review; a moderator sees the one open report.
        assert!(matches!(
            eperica_application::review_queue(&repo, &repo, reporter, 50).await,
            Err(ModerationError::NotAuthorized)
        ));
        let queue = eperica_application::review_queue(&repo, &repo, moderator, 50)
            .await
            .unwrap();
        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].subject, subject);
        let report_id = queue[0].id;

        // AC4/AC5: resolve with a ban — the subject is blocked; the queue empties; re-resolve is a no-op.
        let now = crate::now();
        assert!(
            eperica_application::resolve_report(
                &repo,
                &repo,
                &rules,
                moderator,
                report_id,
                now,
                "confirmed botting",
                Some(SanctionKind::Ban),
                None,
            )
            .await
            .unwrap()
        );
        let sub = repo.find_user_by_id(subject).await.unwrap().unwrap();
        assert!(
            account_blocked(sub.banned_at, sub.suspended_until, now),
            "the subject is banned"
        );
        assert!(
            eperica_application::review_queue(&repo, &repo, moderator, 50)
                .await
                .unwrap()
                .is_empty(),
            "the resolved report leaves the queue"
        );
        assert!(
            !eperica_application::resolve_report(
                &repo, &repo, &rules, moderator, report_id, now, "again", None, None,
            )
            .await
            .unwrap(),
            "resolving twice is a no-op"
        );
    }

    /// 022 AC6: the fixed-window rate limiter counts within a window and trips once the count exceeds
    /// the limit; a fresh window resets.
    #[sqlx::test(migrations = "../../migrations")]
    async fn rate_limit_counts_and_trips(pool: PgPool) {
        use eperica_application::ModerationError;
        let Setup { repo, .. } = setup(pool.clone()).await;
        let rules = crate::fair_play_rules().unwrap();
        let limit = 2u32;
        let now = Timestamp(1_000_000_000);
        let check = async |ts: Timestamp| {
            eperica_application::check_rate_limit(&repo, &rules, "subjX", "action", limit, ts).await
        };

        // Two within the window pass; the third trips.
        assert!(check(now).await.is_ok());
        assert!(check(now).await.is_ok());
        assert!(matches!(
            check(now).await,
            Err(ModerationError::RateLimited)
        ));

        // A later window (past window_secs) resets the count.
        let next_window = Timestamp(now.0 + rules.rate_window_secs * 1000 + 1000);
        assert!(check(next_window).await.is_ok(), "a new window resets");
    }

    /// 022 AC7/AC8: the detection signals are reproducible from persisted state — the shared-IP count
    /// counts accounts on the same registration IP, and the inhuman-action-rate flag trips at the
    /// threshold; both are moderator-gated.
    #[sqlx::test(migrations = "../../migrations")]
    async fn detection_signals_are_reproducible(pool: PgPool) {
        use eperica_application::ModerationError;
        let Setup { repo, template, .. } = setup(pool.clone()).await;
        let rules = crate::fair_play_rules().unwrap();
        let moderator = make_account(&repo, &template, "mod").await;
        repo.set_moderator(moderator, true).await.unwrap();
        let a = make_account(&repo, &template, "shared_a").await;
        let b = make_account(&repo, &template, "shared_b").await;
        let c = make_account(&repo, &template, "shared_c").await;

        // Three accounts share one registration IP.
        for p in [a, b, c] {
            sqlx::query("UPDATE users SET registration_ip = '203.0.113.7' WHERE id = $1")
                .bind(Uuid::from_u128(p.0))
                .execute(&pool)
                .await
                .unwrap();
        }

        // A non-moderator is denied.
        assert!(matches!(
            eperica_application::account_signals(&repo, &repo, &rules, a, b).await,
            Err(ModerationError::NotAuthorized)
        ));

        // The shared-IP signal counts all three and flags (threshold 3).
        let sig = eperica_application::account_signals(&repo, &repo, &rules, moderator, a)
            .await
            .unwrap();
        assert_eq!(sig.ip_association_count, 3);
        assert!(sig.shared_ip_flagged);
        // No action tally yet ⇒ no inhuman-rate flag.
        assert_eq!(sig.peak_action_count, 0);
        assert!(!sig.inhuman_action_rate);

        // Seed a window action tally at the inhuman threshold for account `a`.
        sqlx::query(
            "INSERT INTO rate_limits (subject, action, window_start, count) \
             VALUES ($1, 'action', now(), $2)",
        )
        .bind(a.0.to_string())
        .bind(i32::try_from(rules.inhuman_rate_threshold).unwrap())
        .execute(&pool)
        .await
        .unwrap();
        let sig = eperica_application::account_signals(&repo, &repo, &rules, moderator, a)
            .await
            .unwrap();
        assert_eq!(sig.peak_action_count, rules.inhuman_rate_threshold);
        assert!(sig.inhuman_action_rate, "the inhuman-rate flag trips");
    }

    /// 019 AC2/AC3: a protected player cannot be attacked (no movement created); once a player attacks,
    /// their own protection ends. Drives the real `order_attack` use-case against the Pg repo.
    #[sqlx::test(migrations = "../../migrations")]
    async fn protection_blocks_attack_and_attacking_ends_it(pool: PgPool) {
        let Setup {
            repo,
            econ,
            template,
            config,
            world,
            ..
        } = setup(pool.clone()).await;
        let map = WorldMap::new(
            world.seed as u64,
            config.radius,
            crate::map_rules().unwrap(),
        );
        let units = crate::unit_rules().unwrap();
        let attacker = make_account(&repo, &template, "atk").await;
        let target = make_account(&repo, &template, "tgt").await;
        let atk_v = repo.villages_of(attacker).await.unwrap()[0].clone();
        let tgt_v = repo.villages_of(target).await.unwrap()[0].clone();
        sqlx::query(
            "INSERT INTO village_units (village_id, unit_id, count) VALUES ($1, 'phalanx', 50)",
        )
        .bind(Uuid::from_u128(atk_v.id.0))
        .execute(&pool)
        .await
        .unwrap();
        let now = crate::now();
        let troops = vec![(UnitId("phalanx".into()), 10)];

        // AC2: the fresh target is protected ⇒ the attack is rejected and no movement is created.
        let res = eperica_application::order_attack(
            &repo,
            &repo,
            &repo,
            &repo,
            &econ,
            &units,
            &map,
            config.speed,
            now,
            attacker,
            None,
            tgt_v.coordinate,
            troops.clone(),
            AttackMode::Raid,
            None,
            None,
        )
        .await;
        assert!(
            matches!(res, Err(eperica_application::CombatError::TargetProtected)),
            "a protected target is rejected, got {res:?}"
        );
        let moves: i64 = sqlx::query_scalar("SELECT count(*) FROM troop_movements")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(moves, 0, "no movement is created for a rejected attack");

        // End the target's protection ⇒ the same attack now launches.
        repo.end_protection(target, now).await.unwrap();
        eperica_application::order_attack(
            &repo,
            &repo,
            &repo,
            &repo,
            &econ,
            &units,
            &map,
            config.speed,
            now,
            attacker,
            None,
            tgt_v.coordinate,
            troops,
            AttackMode::Raid,
            None,
            None,
        )
        .await
        .expect("attack launches against an unprotected target");
        let moves: i64 = sqlx::query_scalar("SELECT count(*) FROM troop_movements")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(moves, 1, "the launched attack created a movement");

        // AC3: launching the attack ended the attacker's own protection.
        assert!(
            !is_protected(repo.protection_of(attacker).await.unwrap(), crate::now()),
            "attacking ended the attacker's protection"
        );

        // 026 AC1: the defender got an incoming-attack notification; the attacker did not notify themselves.
        assert_eq!(
            NotificationRepository::unread_count(&repo, target)
                .await
                .unwrap(),
            1,
            "the defender is warned of the incoming attack"
        );
        assert_eq!(
            NotificationRepository::unread_count(&repo, attacker)
                .await
                .unwrap(),
            0,
            "the attacker does not notify themselves"
        );
        let feed = NotificationRepository::list(&repo, target, 10)
            .await
            .unwrap();
        assert_eq!(feed[0].kind, NotificationKind::IncomingAttack);
    }

    /// 019 AC4: protection ends early once the player is established (population ≥ threshold), via the
    /// lazy `end_protection_if_established`; it does not re-arm.
    #[sqlx::test(migrations = "../../migrations")]
    async fn protection_ends_at_population_threshold(pool: PgPool) {
        let Setup {
            repo,
            econ,
            template,
            ..
        } = setup(pool.clone()).await;
        let rules = crate::lifecycle_rules().unwrap();
        let player = make_account(&repo, &template, "grow").await;
        let v = repo.villages_of(player).await.unwrap()[0].clone();
        let now = crate::now();

        // A fresh village is far below the threshold ⇒ protection stays.
        assert!(
            !eperica_application::end_protection_if_established(&repo, &econ, &rules, player, now)
                .await
                .unwrap()
        );
        assert!(is_protected(repo.protection_of(player).await.unwrap(), now));

        // Grow population past the threshold (max-level fields), then it ends on evaluation.
        sqlx::query("UPDATE village_fields SET level = 10 WHERE village_id = $1")
            .bind(Uuid::from_u128(v.id.0))
            .execute(&pool)
            .await
            .unwrap();
        assert!(
            eperica_application::end_protection_if_established(&repo, &econ, &rules, player, now)
                .await
                .unwrap(),
            "an established player's protection ends"
        );
        assert!(!is_protected(
            repo.protection_of(player).await.unwrap(),
            now
        ));
        // Idempotent: a second evaluation does nothing.
        assert!(
            !eperica_application::end_protection_if_established(&repo, &econ, &rules, player, now)
                .await
                .unwrap()
        );
    }

    /// 020 AC1/AC7 + 021 AC1/AC8: `ensure_world` persists both the artifact- and Wonder-release dates
    /// (created + offset), returned stably on later calls.
    #[sqlx::test(migrations = "../../migrations")]
    async fn world_carries_artifact_release_date(pool: PgPool) {
        let config = WorldConfig::new(GameSpeed::new(1.0).unwrap(), 50);
        let world = crate::world::ensure_world_with_release(&pool, &config, 3600, 7200)
            .await
            .unwrap();
        let release = world.artifact_release_at.expect("a release is scheduled");
        let delta = release.0 - world.created_at.0;
        assert!(
            (delta - 3_600_000).abs() < 5_000,
            "release ≈ created + 1h, got {delta}ms"
        );
        let wonder = world
            .wonder_release_at
            .expect("a Wonder release is scheduled");
        let wonder_delta = wonder.0 - world.created_at.0;
        assert!(
            (wonder_delta - 7_200_000).abs() < 5_000,
            "Wonder release ≈ created + 2h, got {wonder_delta}ms"
        );
        // A later call returns the persisted releases, not recomputed ones.
        let again = crate::world::ensure_world_with_release(&pool, &config, 999, 1234)
            .await
            .unwrap();
        assert_eq!(again.artifact_release_at, world.artifact_release_at);
        assert_eq!(again.wonder_release_at, world.wonder_release_at);
    }

    /// 020 AC1/AC2/AC7: the artifact release is gated on the date, materializes Natar NPC villages +
    /// garrisons + artifacts once, and is idempotent.
    #[sqlx::test(migrations = "../../migrations")]
    async fn artifact_release_materializes_once(pool: PgPool) {
        let Setup { repo, .. } = setup(pool.clone()).await;
        let cat = crate::artifact_catalogue().expect("catalogue");
        let spec = eperica_application::ReleaseSpec {
            catalogue: &cat.artifacts,
            garrison_unit: &cat.garrison_unit,
            garrison_base_count: cat.garrison_base_count,
            garrison_per_index: cat.garrison_per_index,
        };
        let release_at = Timestamp(10_000_000_000_000);

        // Before the date: nothing is released.
        let n0 = eperica_application::process_due_artifact_release(
            &repo,
            Some(release_at),
            Timestamp(1_000),
            &spec,
        )
        .await
        .unwrap();
        assert_eq!(n0, 0);
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM artifacts")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0, "no artifacts before the release date");

        // At/after the date: the full set materializes once.
        let now = Timestamp(release_at.0 + 1);
        let n =
            eperica_application::process_due_artifact_release(&repo, Some(release_at), now, &spec)
                .await
                .unwrap();
        assert_eq!(n, cat.artifacts.len(), "the whole set released");
        let natar: i64 =
            sqlx::query_scalar("SELECT count(*) FROM villages WHERE is_natar AND world_id = $1")
                .bind(Uuid::from_u128(repo.world_id.0))
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(natar as usize, n, "one Natar village per artifact");
        let npc: i64 = sqlx::query_scalar("SELECT count(*) FROM users WHERE is_npc")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(npc, 1, "one synthetic Natar owner");
        let garrisoned: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM village_units u JOIN villages v ON v.id = u.village_id \
             WHERE v.is_natar AND u.count > 0",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(garrisoned as usize, n, "every Natar village has a garrison");

        // AC2/AC8: Natar villages (NPC-owned) are excluded from the leaderboards.
        let pop = repo
            .population_board(&crate::economy_rules().unwrap(), BoardScope::World, 100)
            .await
            .unwrap();
        assert!(
            pop.is_empty(),
            "Natar/NPC villages do not appear on the population board"
        );

        // Idempotent: a second release is a no-op.
        let again =
            eperica_application::process_due_artifact_release(&repo, Some(release_at), now, &spec)
                .await
                .unwrap();
        assert_eq!(again, 0, "release happens at most once");
    }

    /// 021 AC1/AC8: the Wonder release is gated on the date, materializes `site_count` conquerable
    /// sites + `plan_count` capturable plan vaults (garrisoned, NPC-owned) once, and is idempotent.
    #[sqlx::test(migrations = "../../migrations")]
    async fn wonder_release_materializes_plans_and_sites(pool: PgPool) {
        let Setup { repo, .. } = setup(pool.clone()).await;
        let rules = crate::wonder_rules().expect("wonder rules");
        let spec = eperica_application::WonderReleaseSpec {
            plan_count: rules.plan_count,
            site_count: rules.site_count,
            garrison_unit: &rules.garrison_unit,
            garrison_base_count: rules.garrison_base_count,
            garrison_per_index: rules.garrison_per_index,
        };
        let release_at = Timestamp(10_000_000_000_000);

        // Before the date: nothing releases.
        let n0 = eperica_application::process_due_wonder_release(
            &repo,
            Some(release_at),
            Timestamp(1_000),
            &spec,
        )
        .await
        .unwrap();
        assert_eq!(n0, 0);
        let sites0: i64 = sqlx::query_scalar("SELECT count(*) FROM villages WHERE is_wonder_site")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(sites0, 0, "no Wonder sites before the release date");

        // At/after the date: plans + sites materialize once.
        let now = Timestamp(release_at.0 + 1);
        let n =
            eperica_application::process_due_wonder_release(&repo, Some(release_at), now, &spec)
                .await
                .unwrap();
        let expected = rules.plan_count as usize + rules.site_count as usize;
        assert_eq!(n, expected, "all plans + sites released");

        let sites: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM villages WHERE is_wonder_site AND world_id = $1",
        )
        .bind(Uuid::from_u128(repo.world_id.0))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(sites as u32, rules.site_count, "one Natar village per site");

        let plans: i64 =
            sqlx::query_scalar("SELECT count(*) FROM wonder_plans WHERE world_id = $1")
                .bind(Uuid::from_u128(repo.world_id.0))
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(plans as u32, rules.plan_count, "one plan per vault");

        // Every released Natar village is garrisoned.
        let garrisoned: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM village_units u JOIN villages v ON v.id = u.village_id \
             WHERE v.is_natar AND u.count > 0",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            garrisoned as usize, expected,
            "every Natar village garrisoned"
        );

        // The synthetic Natar owner exists (shared with the artifact release).
        let npc: i64 = sqlx::query_scalar("SELECT count(*) FROM users WHERE is_npc")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(npc, 1, "one synthetic Natar owner");

        // Idempotent: a second release is a no-op.
        let again =
            eperica_application::process_due_wonder_release(&repo, Some(release_at), now, &spec)
                .await
                .unwrap();
        assert_eq!(again, 0, "the Wonder release happens at most once");
    }

    /// 021 AC3: read through the repo, a Wonder-site Natar village reads as **conquerable** while an
    /// artifact-vault Natar village does not — the column the 014 conquest guard consumes round-trips.
    #[sqlx::test(migrations = "../../migrations")]
    async fn wonder_site_reads_as_conquerable_vault_does_not(pool: PgPool) {
        let Setup { repo, .. } = setup(pool.clone()).await;
        // Release artifacts (creates vaults) and the Wonder (creates conquerable sites); both coexist on
        // distinct Natar tiles.
        let cat = crate::artifact_catalogue().unwrap();
        let now = Timestamp(10_000_000_000_000);
        eperica_application::process_due_artifact_release(
            &repo,
            Some(Timestamp(now.0 - 2)),
            now,
            &eperica_application::ReleaseSpec {
                catalogue: &cat.artifacts,
                garrison_unit: &cat.garrison_unit,
                garrison_base_count: cat.garrison_base_count,
                garrison_per_index: cat.garrison_per_index,
            },
        )
        .await
        .unwrap();
        let rules = crate::wonder_rules().unwrap();
        eperica_application::process_due_wonder_release(
            &repo,
            Some(Timestamp(now.0 - 1)),
            now,
            &eperica_application::WonderReleaseSpec {
                plan_count: rules.plan_count,
                site_count: rules.site_count,
                garrison_unit: &rules.garrison_unit,
                garrison_base_count: rules.garrison_base_count,
                garrison_per_index: rules.garrison_per_index,
            },
        )
        .await
        .unwrap();

        let site: Uuid = sqlx::query_scalar("SELECT id FROM villages WHERE is_wonder_site LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        let vault: Uuid = sqlx::query_scalar(
            "SELECT id FROM villages WHERE is_natar AND NOT is_wonder_site LIMIT 1",
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        let site_v = repo
            .village_by_id(VillageId(site.as_u128()))
            .await
            .unwrap()
            .expect("site exists");
        let vault_v = repo
            .village_by_id(VillageId(vault.as_u128()))
            .await
            .unwrap()
            .expect("vault exists");
        assert!(site_v.is_conquerable(), "a Wonder site is conquerable");
        assert!(
            !vault_v.is_conquerable(),
            "an artifact vault is not conquerable"
        );
    }

    /// 020 AC4: a winning attack from a Treasury village claims a Natar village's artifact; an
    /// attacker without a Treasury wins but takes nothing.
    #[sqlx::test(migrations = "../../migrations")]
    async fn artifact_captured_only_with_treasury(pool: PgPool) {
        let Setup {
            repo,
            econ,
            template,
            config,
            world,
        } = setup(pool.clone()).await;
        let map = WorldMap::new(
            world.seed as u64,
            config.radius,
            crate::map_rules().unwrap(),
        );
        let units = crate::unit_rules().unwrap();
        let cat = crate::artifact_catalogue().unwrap();
        let spec = eperica_application::ReleaseSpec {
            catalogue: &cat.artifacts,
            garrison_unit: &cat.garrison_unit,
            garrison_base_count: cat.garrison_base_count,
            garrison_per_index: cat.garrison_per_index,
        };
        let release_at = Timestamp(1_000_000_000_000);
        eperica_application::process_due_artifact_release(
            &repo,
            Some(release_at),
            Timestamp(release_at.0 + 1),
            &spec,
        )
        .await
        .unwrap();

        // Two small-scope artifacts in their Natar vaults; weaken the garrisons so an attack wins.
        let smalls: Vec<(String, Uuid, i32, i32)> = sqlx::query_as(
            "SELECT a.id, a.holder_village, v.x, v.y FROM artifacts a \
             JOIN villages v ON v.id = a.holder_village WHERE a.scope = 'small' ORDER BY a.id LIMIT 2",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert!(
            smalls.len() >= 2,
            "need two small artifacts to test both paths"
        );
        for (_, vid, _, _) in &smalls {
            sqlx::query("UPDATE village_units SET count = 1 WHERE village_id = $1")
                .bind(vid)
                .execute(&pool)
                .await
                .unwrap();
        }

        let speed = config.speed;
        let attack_natar = async |tag: &str,
                                  treasury: Option<i16>,
                                  tx: i32,
                                  ty: i32|
               -> VillageId {
            let player = make_account(&repo, &template, tag).await;
            let v = repo.villages_of(player).await.unwrap()[0].clone();
            if let Some(level) = treasury {
                sqlx::query(
                    "INSERT INTO village_buildings (village_id, slot, building_type, level) \
                     VALUES ($1, 30, 'treasury', $2)",
                )
                .bind(Uuid::from_u128(v.id.0))
                .bind(level)
                .execute(&pool)
                .await
                .unwrap();
            }
            sqlx::query(
                "INSERT INTO village_units (village_id, unit_id, count) VALUES ($1, 'swordsman', 200)",
            )
            .bind(Uuid::from_u128(v.id.0))
            .execute(&pool)
            .await
            .unwrap();
            eperica_application::order_attack(
                &repo,
                &repo,
                &repo,
                &repo,
                &econ,
                &units,
                &map,
                speed,
                crate::now(),
                player,
                None,
                Coordinate::new(tx, ty),
                vec![(UnitId("swordsman".into()), 150)],
                AttackMode::Attack,
                None,
                None,
            )
            .await
            .expect("attack launches");
            v.id
        };

        // With a qualifying Treasury (level 5 ≥ small's 3): the artifact is claimed.
        let (art_a, _natar_a, ax, ay) = smalls[0].clone();
        let cap_village = attack_natar("treasured", Some(5), ax, ay).await;
        // Without a Treasury: the attacker wins but takes nothing.
        let (art_b, natar_b, bx, by) = smalls[1].clone();
        attack_natar("treasuryless", None, bx, by).await;

        // Resolve all due attacks.
        eperica_application::process_due_combat(
            &repo,
            &repo,
            &repo,
            &repo,
            &econ,
            &units,
            &crate::combat_rules().unwrap(),
            &crate::scout_rules().unwrap(),
            &crate::culture_rules().unwrap(),
            &crate::loyalty_rules().unwrap(),
            &crate::ranking_rules().unwrap(),
            &map,
            speed,
            world.seed as u64,
            Timestamp(crate::now().0 + 100_000_000_000),
            100,
            (cat.treasury_small, cat.treasury_large, cat.treasury_unique),
        )
        .await
        .unwrap();

        // AC4: the Treasury attacker now holds artifact A.
        let holder_a: Option<Uuid> =
            sqlx::query_scalar("SELECT holder_village FROM artifacts WHERE id = $1")
                .bind(&art_a)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            holder_a,
            Some(Uuid::from_u128(cap_village.0)),
            "captured with a Treasury"
        );
        // Artifact B stayed in its Natar vault (no Treasury ⇒ no transfer).
        let holder_b: Option<Uuid> =
            sqlx::query_scalar("SELECT holder_village FROM artifacts WHERE id = $1")
                .bind(&art_b)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(holder_b, Some(natar_b), "no Treasury ⇒ artifact not taken");
    }

    /// 021 AC2: a winning attack from a top-Treasury village captures a Natar vault's Wonder plan;
    /// an attacker without a Treasury wins but takes nothing.
    #[sqlx::test(migrations = "../../migrations")]
    async fn wonder_plan_captured_only_with_treasury(pool: PgPool) {
        let Setup {
            repo,
            econ,
            template,
            config,
            world,
        } = setup(pool.clone()).await;
        let map = WorldMap::new(
            world.seed as u64,
            config.radius,
            crate::map_rules().unwrap(),
        );
        let units = crate::unit_rules().unwrap();
        let rules = crate::wonder_rules().unwrap();
        let cat = crate::artifact_catalogue().unwrap();
        let spec = eperica_application::WonderReleaseSpec {
            plan_count: rules.plan_count,
            site_count: rules.site_count,
            garrison_unit: &rules.garrison_unit,
            garrison_base_count: rules.garrison_base_count,
            garrison_per_index: rules.garrison_per_index,
        };
        let release_at = Timestamp(1_000_000_000_000);
        eperica_application::process_due_wonder_release(
            &repo,
            Some(release_at),
            Timestamp(release_at.0 + 1),
            &spec,
        )
        .await
        .unwrap();

        // Two plan vaults; weaken their garrisons so an attack wins.
        let plans: Vec<(String, Uuid, i32, i32)> = sqlx::query_as(
            "SELECT p.id, p.holder_village, v.x, v.y FROM wonder_plans p \
             JOIN villages v ON v.id = p.holder_village ORDER BY p.id LIMIT 2",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert!(plans.len() >= 2, "need two plan vaults to test both paths");
        for (_, vid, _, _) in &plans {
            sqlx::query("UPDATE village_units SET count = 1 WHERE village_id = $1")
                .bind(vid)
                .execute(&pool)
                .await
                .unwrap();
        }

        let speed = config.speed;
        let attack_vault = async |tag: &str,
                                  treasury: Option<i16>,
                                  tx: i32,
                                  ty: i32|
               -> VillageId {
            let player = make_account(&repo, &template, tag).await;
            let v = repo.villages_of(player).await.unwrap()[0].clone();
            if let Some(level) = treasury {
                sqlx::query(
                    "INSERT INTO village_buildings (village_id, slot, building_type, level) \
                     VALUES ($1, 30, 'treasury', $2)",
                )
                .bind(Uuid::from_u128(v.id.0))
                .bind(level)
                .execute(&pool)
                .await
                .unwrap();
            }
            sqlx::query(
                "INSERT INTO village_units (village_id, unit_id, count) VALUES ($1, 'swordsman', 400)",
            )
            .bind(Uuid::from_u128(v.id.0))
            .execute(&pool)
            .await
            .unwrap();
            eperica_application::order_attack(
                &repo,
                &repo,
                &repo,
                &repo,
                &econ,
                &units,
                &map,
                speed,
                crate::now(),
                player,
                None,
                Coordinate::new(tx, ty),
                vec![(UnitId("swordsman".into()), 350)],
                AttackMode::Attack,
                None,
                None,
            )
            .await
            .expect("attack launches");
            v.id
        };

        // With a top (unique-tier) Treasury: the plan is captured.
        let unique = i16::from(cat.treasury_unique);
        let (plan_a, _vault_a, ax, ay) = plans[0].clone();
        let captor = attack_vault("planner", Some(unique), ax, ay).await;
        // Without a Treasury: the attacker wins but takes nothing.
        let (plan_b, vault_b, bx, by) = plans[1].clone();
        attack_vault("planless", None, bx, by).await;

        eperica_application::process_due_combat(
            &repo,
            &repo,
            &repo,
            &repo,
            &econ,
            &units,
            &crate::combat_rules().unwrap(),
            &crate::scout_rules().unwrap(),
            &crate::culture_rules().unwrap(),
            &crate::loyalty_rules().unwrap(),
            &crate::ranking_rules().unwrap(),
            &map,
            speed,
            world.seed as u64,
            Timestamp(crate::now().0 + 100_000_000_000),
            100,
            (cat.treasury_small, cat.treasury_large, cat.treasury_unique),
        )
        .await
        .unwrap();

        // AC2: the Treasury attacker now holds plan A.
        let holder_a: Option<Uuid> =
            sqlx::query_scalar("SELECT holder_village FROM wonder_plans WHERE id = $1")
                .bind(&plan_a)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            holder_a,
            Some(Uuid::from_u128(captor.0)),
            "plan captured with a Treasury"
        );
        // Plan B stayed in its Natar vault (no Treasury ⇒ no transfer).
        let holder_b: Option<Uuid> =
            sqlx::query_scalar("SELECT holder_village FROM wonder_plans WHERE id = $1")
                .bind(&plan_b)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(holder_b, Some(vault_b), "no Treasury ⇒ plan not taken");
    }

    /// 021 AC4/AC5: a Wonder build is gated (site control + alliance-holds-plan + level < 100), then
    /// advances one level through the ordinary build queue; an order at 100 is rejected.
    #[sqlx::test(migrations = "../../migrations")]
    async fn wonder_build_gated_and_advances(pool: PgPool) {
        let Setup {
            repo,
            econ,
            template,
            ..
        } = setup(pool.clone()).await;
        let units = crate::unit_rules().unwrap();
        let build_rules = crate::build_rules().unwrap();
        let speed = GameSpeed::new(1.0).unwrap();
        let rules = crate::wonder_rules().unwrap();
        let spec = eperica_application::WonderReleaseSpec {
            plan_count: rules.plan_count,
            site_count: rules.site_count,
            garrison_unit: &rules.garrison_unit,
            garrison_base_count: rules.garrison_base_count,
            garrison_per_index: rules.garrison_per_index,
        };

        let player = make_account(&repo, &template, "wbuild").await;
        let home = repo.villages_of(player).await.unwrap()[0].id;

        // Release the Wonder, then "conquer" a site into the player's hands.
        let release_at = Timestamp(1_000_000_000_000);
        eperica_application::process_due_wonder_release(
            &repo,
            Some(release_at),
            Timestamp(release_at.0 + 1),
            &spec,
        )
        .await
        .unwrap();
        let site: Uuid = sqlx::query_scalar("SELECT id FROM villages WHERE is_wonder_site LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE villages SET owner_id = $1, is_natar = false WHERE id = $2")
            .bind(Uuid::from_u128(player.0))
            .bind(site)
            .execute(&pool)
            .await
            .unwrap();
        let now = crate::now();
        sqlx::query(
            "INSERT INTO village_resources (village_id, wood, clay, iron, crop, updated_at) \
             VALUES ($1, 100000000, 100000000, 100000000, 100000000, \
                     to_timestamp($2::double precision / 1000.0))",
        )
        .bind(site)
        .bind(now.0 as f64)
        .execute(&pool)
        .await
        .unwrap();
        // Storage so the site can actually hold the Wonder's cost (else compute_economy caps it).
        sqlx::query(
            "INSERT INTO village_buildings (village_id, slot, building_type, level) \
             VALUES ($1, 16, 'warehouse', 10), ($1, 17, 'granary', 10)",
        )
        .bind(site)
        .execute(&pool)
        .await
        .unwrap();
        let site_id = VillageId(site.as_u128());

        let order = async |sel: VillageId| -> Result<(), eperica_application::WonderError> {
            eperica_application::order_wonder_build(
                &repo,
                &repo,
                &repo,
                &repo,
                &repo,
                &econ,
                &build_rules,
                &units,
                speed,
                crate::now(),
                player,
                Some(sel),
            )
            .await
        };

        // Gate: a non-site village is rejected.
        assert!(matches!(
            order(home).await,
            Err(eperica_application::WonderError::NotASite)
        ));
        // Gate: controlling the site but in no alliance is rejected.
        assert!(matches!(
            order(site_id).await,
            Err(eperica_application::WonderError::NoAlliance)
        ));

        // Put the player in an alliance (direct rows — bypassing the Embassy gate, irrelevant here).
        let alliance = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO alliances (id, name, tag, founder_id) VALUES ($1, 'A', 'AAA', $2)",
        )
        .bind(alliance)
        .bind(Uuid::from_u128(player.0))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO alliance_members (player_id, alliance_id, role) VALUES ($1, $2, 'founder')",
        )
        .bind(Uuid::from_u128(player.0))
        .bind(alliance)
        .execute(&pool)
        .await
        .unwrap();

        // Gate: the alliance holds no plan yet.
        assert!(matches!(
            order(site_id).await,
            Err(eperica_application::WonderError::NoPlan)
        ));

        // Hand a plan to the player's home village (so their alliance holds one).
        sqlx::query(
            "UPDATE wonder_plans SET holder_village = $1 \
             WHERE id = (SELECT id FROM wonder_plans LIMIT 1)",
        )
        .bind(Uuid::from_u128(home.0))
        .execute(&pool)
        .await
        .unwrap();

        // Accepted: all gates pass, a Wonder build is enqueued on the site.
        order(site_id).await.expect("the Wonder build is accepted");
        let active = repo.active_builds(site_id).await.unwrap();
        assert_eq!(active.len(), 1, "a Wonder build is queued");
        assert_eq!(active[0].target_level, 1);

        // It advances one level once due.
        eperica_application::process_due_builds(
            &repo,
            &repo,
            &repo,
            &crate::culture_rules().unwrap(),
            Timestamp(crate::now().0 + 1_000_000_000_000),
            100,
        )
        .await
        .unwrap();
        assert_eq!(
            repo.wonder_level(site_id).await.unwrap(),
            1,
            "the Wonder is now level 1"
        );

        // AC5: an order at level 100 is rejected.
        sqlx::query(
            "UPDATE village_buildings SET level = 100 \
             WHERE village_id = $1 AND building_type = 'wonder'",
        )
        .bind(site)
        .execute(&pool)
        .await
        .unwrap();
        assert!(matches!(
            order(site_id).await,
            Err(eperica_application::WonderError::AlreadyComplete)
        ));
    }

    /// 021 AC6: the first alliance to a complete (level-100) Wonder is recorded as the winner exactly
    /// once — a later completion does not overwrite it, and the world reads as ended.
    #[sqlx::test(migrations = "../../migrations")]
    async fn wonder_victory_records_first_alliance_once(pool: PgPool) {
        let Setup { repo, template, .. } = setup(pool.clone()).await;

        // A helper: a player in a fresh alliance whose village holds a level-`lvl` Wonder.
        let with_wonder = async |tag: &str, lvl: i16| -> Uuid {
            let player = make_account(&repo, &template, tag).await;
            let village = repo.villages_of(player).await.unwrap()[0].id;
            let alliance = Uuid::new_v4();
            sqlx::query(
                "INSERT INTO alliances (id, name, tag, founder_id) VALUES ($1, $2, $3, $4)",
            )
            .bind(alliance)
            .bind(tag)
            .bind(tag)
            .bind(Uuid::from_u128(player.0))
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO alliance_members (player_id, alliance_id, role) \
                 VALUES ($1, $2, 'founder')",
            )
            .bind(Uuid::from_u128(player.0))
            .bind(alliance)
            .execute(&pool)
            .await
            .unwrap();
            // The Wonder counts only on a conquered Wonder site, so flag the village as one.
            sqlx::query("UPDATE villages SET is_wonder_site = true WHERE id = $1")
                .bind(Uuid::from_u128(village.0))
                .execute(&pool)
                .await
                .unwrap();
            sqlx::query(
                "INSERT INTO village_buildings (village_id, slot, building_type, level) \
                 VALUES ($1, 18, 'wonder', $2)",
            )
            .bind(Uuid::from_u128(village.0))
            .bind(lvl)
            .execute(&pool)
            .await
            .unwrap();
            alliance
        };

        // No complete Wonder yet (only level 99): no victory.
        let _alpha_partial = with_wonder("partial", 99).await;
        assert!(
            !eperica_application::process_due_wonder_victory(&repo, crate::now())
                .await
                .unwrap(),
            "a level-99 Wonder does not win"
        );
        assert!(repo.world_ended().await.unwrap().is_none());

        // First alliance to 100 wins.
        let winner = with_wonder("winner", 100).await;
        assert!(
            eperica_application::process_due_wonder_victory(&repo, crate::now())
                .await
                .unwrap(),
            "the first complete Wonder wins"
        );
        let outcome = repo.world_ended().await.unwrap().expect("the world is won");
        assert_eq!(outcome.winner, AllianceId(winner.as_u128()));

        // A later completion does not overwrite the recorded winner, and the check is idempotent.
        let _latecomer = with_wonder("late", 100).await;
        assert!(
            !eperica_application::process_due_wonder_victory(&repo, crate::now())
                .await
                .unwrap(),
            "a later complete Wonder does not win"
        );
        let again = repo.world_ended().await.unwrap().expect("still won");
        assert_eq!(
            again.winner,
            AllianceId(winner.as_u128()),
            "winner unchanged"
        );
    }

    /// 020 AC5: a winning attack from a Treasury village against a **player** holding an artifact steals
    /// it (the holder loses it; the attacker gains it).
    #[sqlx::test(migrations = "../../migrations")]
    async fn artifact_stolen_from_player_holder(pool: PgPool) {
        let Setup {
            repo,
            econ,
            template,
            config,
            world,
        } = setup(pool.clone()).await;
        let map = WorldMap::new(
            world.seed as u64,
            config.radius,
            crate::map_rules().unwrap(),
        );
        let units = crate::unit_rules().unwrap();

        // The victim: a player holding a small artifact in their (lightly defended) village.
        let victim = make_account(&repo, &template, "victim").await;
        let vv = repo.villages_of(victim).await.unwrap()[0].clone();
        sqlx::query(
            "INSERT INTO artifacts (id, world_id, kind, scope, magnitude, holder_village, origin_x, origin_y) \
             VALUES ('steal_me', $1, 'storage', 'small', 1.5, $2, 0, 0)",
        )
        .bind(Uuid::from_u128(world.id.0))
        .bind(Uuid::from_u128(vv.id.0))
        .execute(&pool)
        .await
        .unwrap();

        // The thief: a Treasury village with an army.
        let thief = make_account(&repo, &template, "thief").await;
        let tv = repo.villages_of(thief).await.unwrap()[0].clone();
        sqlx::query(
            "INSERT INTO village_buildings (village_id, slot, building_type, level) \
             VALUES ($1, 30, 'treasury', 5)",
        )
        .bind(Uuid::from_u128(tv.id.0))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO village_units (village_id, unit_id, count) VALUES ($1, 'swordsman', 200)",
        )
        .bind(Uuid::from_u128(tv.id.0))
        .execute(&pool)
        .await
        .unwrap();
        // Clear any beginner's protection on the victim so the attack lands.
        sqlx::query("UPDATE users SET protected_until = NULL")
            .execute(&pool)
            .await
            .unwrap();

        eperica_application::order_attack(
            &repo,
            &repo,
            &repo,
            &repo,
            &econ,
            &units,
            &map,
            config.speed,
            crate::now(),
            thief,
            None,
            vv.coordinate,
            vec![(UnitId("swordsman".into()), 150)],
            AttackMode::Attack,
            None,
            None,
        )
        .await
        .expect("attack launches");
        eperica_application::process_due_combat(
            &repo,
            &repo,
            &repo,
            &repo,
            &econ,
            &units,
            &crate::combat_rules().unwrap(),
            &crate::scout_rules().unwrap(),
            &crate::culture_rules().unwrap(),
            &crate::loyalty_rules().unwrap(),
            &crate::ranking_rules().unwrap(),
            &map,
            config.speed,
            world.seed as u64,
            Timestamp(crate::now().0 + 100_000_000_000),
            100,
            (3, 6, 10),
        )
        .await
        .unwrap();

        let holder: Option<Uuid> =
            sqlx::query_scalar("SELECT holder_village FROM artifacts WHERE id = 'steal_me'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            holder,
            Some(Uuid::from_u128(tv.id.0)),
            "the artifact was stolen to the thief's village"
        );
    }

    /// 020 AC6: a held Storage artifact raises the holding village's warehouse/granary capacity on the
    /// economy read; losing it reverts on the next read (effects fold into the read, no stored mutation).
    #[sqlx::test(migrations = "../../migrations")]
    async fn storage_artifact_raises_capacity(pool: PgPool) {
        let Setup {
            repo,
            econ,
            template,
            config,
            world,
        } = setup(pool.clone()).await;
        let units = crate::unit_rules().unwrap();
        let player = make_account(&repo, &template, "stor").await;
        let v = repo.villages_of(player).await.unwrap()[0].clone();
        let load = async || {
            eperica_application::load_economy(
                &repo,
                &econ,
                &units,
                config.speed,
                crate::now(),
                player,
                None,
            )
            .await
            .unwrap()
            .unwrap()
        };
        let base = load().await.economy.capacities.warehouse;

        // A large Storage artifact (×2.0) held by the player's village.
        sqlx::query(
            "INSERT INTO artifacts (id, world_id, kind, scope, magnitude, holder_village, origin_x, origin_y) \
             VALUES ('t_stor', $1, 'storage', 'large', 2.0, $2, 0, 0)",
        )
        .bind(Uuid::from_u128(world.id.0))
        .bind(Uuid::from_u128(v.id.0))
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(
            load().await.economy.capacities.warehouse,
            base * 2,
            "Storage artifact doubled the warehouse capacity"
        );

        // Losing it reverts on the next read.
        sqlx::query("DELETE FROM artifacts WHERE id = 't_stor'")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            load().await.economy.capacities.warehouse,
            base,
            "reverts when lost"
        );
    }

    /// 019 AC7/AC8: the abandonment sweep abandons an idle account — deleting its village (freeing the
    /// valley) and retiring (but **retaining**) the account row — leaves an active account untouched,
    /// and is idempotent per period.
    #[sqlx::test(migrations = "../../migrations")]
    async fn sweep_abandons_inactive_frees_map_and_is_idempotent(pool: PgPool) {
        let Setup { repo, template, .. } = setup(pool.clone()).await;
        let gone = make_account(&repo, &template, "gone").await;
        let active = make_account(&repo, &template, "live").await;
        let gone_v = repo.villages_of(gone).await.unwrap()[0].clone();
        // `gone` last acted in the deep past; `active` is fresh (seeded at creation).
        sqlx::query("UPDATE users SET last_activity = to_timestamp(1) WHERE id = $1")
            .bind(Uuid::from_u128(gone.0))
            .execute(&pool)
            .await
            .unwrap();
        // A cutoff between `gone`'s ancient activity and `active`'s fresh activity.
        let cutoff = Timestamp(1_000_000);

        let count = repo.sweep_abandoned(0, cutoff).await.unwrap();
        assert_eq!(count, 1, "the idle account was abandoned");
        // AC7: the village is gone — the valley is free again.
        assert!(
            repo.village_at(gone_v.coordinate).await.unwrap().is_none(),
            "the abandoned village's valley is freed"
        );
        assert!(repo.villages_of(gone).await.unwrap().is_empty());
        // AC8: the account row is retained (history-safe) but flagged abandoned.
        let rec = repo
            .find_user_by_id(gone)
            .await
            .unwrap()
            .expect("the user row is retained");
        assert!(rec.abandoned, "the account is retired");
        // The active account is untouched.
        assert!(
            !repo
                .find_user_by_id(active)
                .await
                .unwrap()
                .unwrap()
                .abandoned
        );
        assert!(!repo.villages_of(active).await.unwrap().is_empty());
        // Idempotent: re-sweeping the recorded period is a no-op.
        assert_eq!(repo.sweep_abandoned(0, cutoff).await.unwrap(), 0);
        assert_eq!(repo.latest_swept_period().await.unwrap(), Some(0));
    }

    /// 019 AC7/AC10: `process_due_lifecycle` settles every complete period once (watermark-driven) and
    /// catches up, then no-ops when caught up.
    #[sqlx::test(migrations = "../../migrations")]
    async fn process_due_lifecycle_settles_complete_periods(pool: PgPool) {
        let Setup { repo, template, .. } = setup(pool.clone()).await;
        let gone = make_account(&repo, &template, "abandon").await;
        let gone_v = repo.villages_of(gone).await.unwrap()[0].clone();
        sqlx::query("UPDATE users SET last_activity = to_timestamp(0) WHERE id = $1")
            .bind(Uuid::from_u128(gone.0))
            .execute(&pool)
            .await
            .unwrap();
        let rules = LifecycleRules {
            beginner_protection_secs: 1,
            protection_population_threshold: 1,
            inactive_after_secs: 1,
            abandon_after_secs: 1,
            sweep_interval_secs: 10,
            presence_online_secs: 600,
        };
        let world_start = Timestamp(0);
        let now = Timestamp(60_000); // period 6 ⇒ periods 0..=5 are complete

        let swept = eperica_application::process_due_lifecycle(&repo, world_start, now, &rules)
            .await
            .unwrap();
        assert_eq!(swept.len(), 6, "periods 0..=5 each settle once");
        let total: usize = swept.iter().map(|(_, c)| *c).sum();
        assert_eq!(
            total, 1,
            "exactly one account abandoned across the catch-up"
        );
        assert!(
            repo.village_at(gone_v.coordinate).await.unwrap().is_none(),
            "the abandoned account's valley is freed"
        );
        // Caught up: a second run settles nothing.
        let again = eperica_application::process_due_lifecycle(&repo, world_start, now, &rules)
            .await
            .unwrap();
        assert!(again.is_empty(), "no further periods to settle");
    }

    /// 019 AC8: an abandoned account is excluded from the leaderboards (by a read-time filter, not by
    /// destroying rows) and its stat page 404s — while a still-active opponent **keeps** its battle
    /// report and ranking points (P6 audit retention).
    #[sqlx::test(migrations = "../../migrations")]
    async fn abandoned_excluded_but_opponent_history_preserved(pool: PgPool) {
        let Setup {
            repo,
            econ,
            template,
            world,
            ..
        } = setup(pool.clone()).await;
        let gone = make_account(&repo, &template, "gboard").await;
        let live = make_account(&repo, &template, "lboard").await;
        let gone_v = repo.villages_of(gone).await.unwrap()[0].clone();
        let live_v = repo.villages_of(live).await.unwrap()[0].clone();
        // Two resolved raids (with fallback coords): each side scores attack points against the other.
        let report = async |attacker: PlayerId, av: &Village, defender: PlayerId, dv: &Village| {
            sqlx::query(
                "INSERT INTO battle_reports (id, kind, attacker_player, attacker_village, \
                 defender_player, defender_village, attacker_won, luck, morale, wall_before, \
                 wall_after, attacker_forces, attacker_losses, defender_forces, defender_losses, \
                 attack_points, attacker_x, attacker_y, defender_x, defender_y) \
                 VALUES ($1,'raid',$2,$3,$4,$5,true,0,0,0,0,'{}','{}','{}','{}',50,$6,$7,$8,$9)",
            )
            .bind(Uuid::new_v4())
            .bind(Uuid::from_u128(attacker.0))
            .bind(Uuid::from_u128(av.id.0))
            .bind(Uuid::from_u128(defender.0))
            .bind(Uuid::from_u128(dv.id.0))
            .bind(av.coordinate.x)
            .bind(av.coordinate.y)
            .bind(dv.coordinate.x)
            .bind(dv.coordinate.y)
            .execute(&pool)
            .await
            .unwrap();
        };
        report(gone, &gone_v, live, &live_v).await; // gone raids live
        report(live, &live_v, gone, &gone_v).await; // live raids gone

        // Snapshots so `gone` is a top-climber (period 0 → 1 is a positive delta).
        let world_id = Uuid::from_u128(world.id.0);
        for (period, popn) in [(0i64, 10i64), (1, 100)] {
            sqlx::query(
                "INSERT INTO population_snapshots (world_id, player_id, period, population) \
                 VALUES ($1, $2, $3, $4)",
            )
            .bind(world_id)
            .bind(Uuid::from_u128(gone.0))
            .bind(period)
            .bind(popn)
            .execute(&pool)
            .await
            .unwrap();
        }

        let on = |b: &[LeaderboardRow], p: PlayerId| b.iter().any(|r| r.player == p);
        let attack_board = async || {
            repo.conflict_board(ConflictMetric::Attack, BoardScope::World, None, None, 100)
                .await
                .unwrap()
        };
        let climbers = async || {
            repo.climber_board(1, 0, BoardScope::World, 100)
                .await
                .unwrap()
        };
        // Before: both are on the attack + population boards; `gone` tops the climber board.
        assert!(on(&attack_board().await, gone) && on(&attack_board().await, live));
        assert!(
            on(&climbers().await, gone),
            "gone is a climber before abandonment"
        );
        assert!(repo.player_stats(&econ, gone).await.unwrap().is_some());

        // Abandon `gone` (idle past the cutoff) — deletes its village, keeps its (now NULL-village) row.
        sqlx::query("UPDATE users SET last_activity = to_timestamp(1) WHERE id = $1")
            .bind(Uuid::from_u128(gone.0))
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            repo.sweep_abandoned(0, Timestamp(1_000_000)).await.unwrap(),
            1
        );

        // `gone` is excluded everywhere; its stat page 404s.
        let pop = repo
            .population_board(&econ, BoardScope::World, 100)
            .await
            .unwrap();
        assert!(
            !on(&pop, gone),
            "abandoned account left the population board"
        );
        assert!(on(&pop, live), "the live account remains");
        assert!(
            !on(&attack_board().await, gone),
            "abandoned left the attack board"
        );
        assert!(
            !on(&climbers().await, gone),
            "abandoned left the climber board"
        );
        assert!(
            repo.player_stats(&econ, gone).await.unwrap().is_none(),
            "stat page 404s"
        );

        // AC8/P6: the opponent KEEPS its report and points (history was not destroyed).
        assert!(
            on(&attack_board().await, live),
            "live keeps its attack points"
        );
        assert!(repo.player_stats(&econ, live).await.unwrap().is_some());
        let live_reports = repo.reports_for(live, 100).await.unwrap();
        assert!(
            !live_reports.is_empty(),
            "the opponent's battle report survived the abandonment"
        );
    }

    /// 007 AC1/AC4/AC5: a reinforcement debits the source garrison, arrives once (crash-resume
    /// safe), stations at the target, and the return rejoins the source garrison.
    #[sqlx::test(migrations = "../../migrations")]
    async fn movement_reinforce_and_return_lifecycle(pool: PgPool) {
        let Setup { repo, template, .. } = setup(pool.clone()).await;

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
    #[sqlx::test(migrations = "../../migrations")]
    async fn start_reinforcement_over_garrison_removes_nothing(pool: PgPool) {
        let Setup { repo, template, .. } = setup(pool.clone()).await;
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
    #[sqlx::test(migrations = "../../migrations")]
    async fn trade_send_deliver_and_return_lifecycle(pool: PgPool) {
        let Setup { repo, template, .. } = setup(pool.clone()).await;
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
    #[sqlx::test(migrations = "../../migrations")]
    async fn process_due_trades_delivers_and_frees_merchants(pool: PgPool) {
        let Setup {
            repo,
            econ,
            world,
            config,
            template,
        } = setup(pool.clone()).await;
        let units = crate::unit_rules().expect("unit rules");
        let merchants = crate::merchant_rules().expect("merchant rules");
        let map = WorldMap::new(
            world.seed as u64,
            config.radius,
            crate::map_rules().unwrap(),
        );
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
    #[sqlx::test(migrations = "../../migrations")]
    async fn late_delivery_does_not_regress_the_resource_clock(pool: PgPool) {
        let Setup {
            repo,
            econ,
            world,
            config,
            template,
        } = setup(pool.clone()).await;
        let units = crate::unit_rules().expect("unit rules");
        let merchants = crate::merchant_rules().expect("merchant rules");
        let map = WorldMap::new(
            world.seed as u64,
            config.radius,
            crate::map_rules().unwrap(),
        );
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
    #[sqlx::test(migrations = "../../migrations")]
    async fn combat_apply_battle_and_reports(pool: PgPool) {
        let Setup { repo, template, .. } = setup(pool.clone()).await;
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
                loyalty_before: None,
                loyalty_after: None,
                conquered: false,
            },
            scouted: false,
            scout_target: None,
            scout_report: None,
            loot: ResourceAmounts::default(),
            target_debit: None,
            razed: None,
            loyalty: None,
            // 016 AC3/AC4: the battle's attack points + the per-defender split (owner 75, ally 25 of
            // a 100-point defense total, weighted 3:1 by contributed defensive value).
            attack_points: 30,
            defender_contributions: vec![
                DefenderContribution {
                    player: defender,
                    village: d.id,
                    is_owner: true,
                    forces: vec![(UnitId("phalanx".into()), 8)],
                    losses: vec![(UnitId("phalanx".into()), 4)],
                    defense_value: 75,
                    defense_points: 75,
                },
                DefenderContribution {
                    player: ally,
                    village: al.id,
                    is_owner: false,
                    forces: vec![(UnitId("phalanx".into()), 4)],
                    losses: vec![(UnitId("phalanx".into()), 2)],
                    defense_value: 25,
                    defense_points: 25,
                },
            ],
            artifact_capture: None,
            plan_capture: None,
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

        // 016 AC3/AC4 (T2): the battle's attack points persist on the report, and one
        // `battle_defenders` row per defending player (owner + the ally reinforcer) persists with the
        // split defense points — written exactly-once in the battle transaction.
        let rid = Uuid::from_u128(report_id);
        let attack_points: i64 =
            sqlx::query_scalar("SELECT attack_points FROM battle_reports WHERE id = $1")
                .bind(rid)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(attack_points, 30);
        let defenders: Vec<(Uuid, bool, i64)> = sqlx::query_as(
            "SELECT player_id, is_owner, defense_points FROM battle_defenders \
             WHERE battle_id = $1 ORDER BY is_owner DESC",
        )
        .bind(rid)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(defenders.len(), 2);
        assert_eq!(defenders[0], (Uuid::from_u128(defender.0), true, 75)); // owner
        assert_eq!(defenders[1], (Uuid::from_u128(ally.0), false, 25)); // reinforcer
        // Defense points sum to the battle's defense total (no points lost/created).
        assert_eq!(
            defenders.iter().map(|(_, _, p)| p).sum::<i64>(),
            100,
            "shares sum to the defense total"
        );
    }

    /// 011 AC2/AC6/AC9: `apply_battle` debits the target's resources, razes the targeted building,
    /// attaches the loot to the survivor return, and records it on the report; the return then
    /// credits the loot (capped) to the attacker on arrival.
    #[sqlx::test(migrations = "../../migrations")]
    async fn siege_loot_persistence_and_credit(pool: PgPool) {
        let Setup {
            repo,
            econ,
            template,
            ..
        } = setup(pool.clone()).await;
        let units = crate::unit_rules().expect("unit rules");
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
                loyalty_before: None,
                loyalty_after: None,
                conquered: false,
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
            loyalty: None,
            attack_points: 0,
            defender_contributions: Vec::new(),
            artifact_capture: None,
            plan_capture: None,
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
    #[sqlx::test(migrations = "../../migrations")]
    async fn cranny_protects_loot_and_teuton_bypasses(pool: PgPool) {
        let Setup {
            repo,
            econ,
            world,
            config,
            template,
        } = setup(pool.clone()).await;
        let units = crate::unit_rules().expect("unit rules");
        let combat = crate::combat_rules().expect("combat rules");
        let scout = crate::scout_rules().expect("scout rules");
        let map = WorldMap::new(
            world.seed as u64,
            config.radius,
            crate::map_rules().unwrap(),
        );
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
                    &crate::culture_rules().unwrap(),
                    &crate::loyalty_rules().unwrap(),
                    &crate::ranking_rules().unwrap(),
                    &map,
                    GameSpeed::new(1.0).unwrap(),
                    world.seed as u64,
                    arrive,
                    100,
                    (3, 6, 10),
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
    #[sqlx::test(migrations = "../../migrations")]
    async fn process_due_combat_resolves_a_raid(pool: PgPool) {
        let Setup {
            repo,
            econ,
            world,
            config,
            template,
        } = setup(pool.clone()).await;
        let units = crate::unit_rules().expect("unit rules");
        let combat = crate::combat_rules().expect("combat rules");
        let scout = crate::scout_rules().expect("scout rules");
        let map = WorldMap::new(
            world.seed as u64,
            config.radius,
            crate::map_rules().unwrap(),
        );
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
        let (defender, d) = account("pcdef").await;
        let (ally, al) = account("pcally").await;

        // 016 AC3: the ally reinforces the defender — resolve_one must build a contribution for both.
        sqlx::query(
            "INSERT INTO reinforcements (host_village, home_village, unit_id, count) \
             VALUES ($1, $2, 'phalanx', 2)",
        )
        .bind(Uuid::from_u128(d.id.0))
        .bind(Uuid::from_u128(al.id.0))
        .execute(&pool)
        .await
        .unwrap();

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

        // 029 AC3: the reinforcing ally mutes battle reports before the battle resolves; the gate in
        // `apply_battle` must then skip their notification while still notifying the attacker + owner.
        NotificationRepository::set_mute(&repo, ally, NotificationKind::BattleReport, true)
            .await
            .unwrap();

        let targets = eperica_application::process_due_combat(
            &repo,
            &repo,
            &repo,
            &repo,
            &econ,
            &units,
            &combat,
            &scout,
            &crate::culture_rules().unwrap(),
            &crate::loyalty_rules().unwrap(),
            &crate::ranking_rules().unwrap(),
            &map,
            GameSpeed::new(1.0).unwrap(),
            world.seed as u64,
            arrive,
            100,
            (3, 6, 10),
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

        // 016 AC3/AC4: resolve_one built a per-defender contribution for the **owner** and the **ally
        // reinforcer** (2 rows), and valued the battle's attack points = the 4 phalanx destroyed
        // (point_value 1 each).
        let rid = Uuid::from_u128(reports[0].id);
        let attack_points: i64 =
            sqlx::query_scalar("SELECT attack_points FROM battle_reports WHERE id = $1")
                .bind(rid)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(attack_points, 4);
        let defs: Vec<(Uuid, bool)> = sqlx::query_as(
            "SELECT player_id, is_owner FROM battle_defenders WHERE battle_id = $1 \
             ORDER BY is_owner DESC",
        )
        .bind(rid)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(defs.len(), 2);
        assert_eq!(defs[0], (Uuid::from_u128(defender.0), true)); // garrison owner
        assert_eq!(defs[1], (Uuid::from_u128(ally.0), false)); // reinforcer

        // 026 AC2: the attacker + the (non-muting) owner got a battle-report notification.
        let has_report = async |who: PlayerId| {
            NotificationRepository::list(&repo, who, 10)
                .await
                .unwrap()
                .iter()
                .any(|n| {
                    n.kind == NotificationKind::BattleReport
                        && n.ref_id.as_deref() == Some(&reports[0].id.to_string())
                })
        };
        assert!(
            has_report(attacker).await,
            "the attacker got a battle-report notification"
        );
        assert!(
            has_report(defender).await,
            "the owner got a battle-report notification"
        );
        // 029 AC3: the muting reinforcer got none (the apply_battle gate skipped it).
        assert!(
            !has_report(ally).await,
            "the reinforcer muted battle reports and got none"
        );
    }

    /// 010 AC6/AC7/AC8/AC9: scouts riding an attack scout the village in addition to the battle —
    /// the espionage step runs first, the (surviving) scouts return with the army carrying intel,
    /// and the defender's battle report is flagged because their counter-espionage killed a scout.
    #[sqlx::test(migrations = "../../migrations")]
    async fn process_due_combat_with_scouts(pool: PgPool) {
        let Setup {
            repo,
            econ,
            world,
            config,
            template,
        } = setup(pool.clone()).await;
        let units = crate::unit_rules().expect("unit rules");
        let combat = crate::combat_rules().expect("combat rules");
        let scout = crate::scout_rules().expect("scout rules");
        let map = WorldMap::new(
            world.seed as u64,
            config.radius,
            crate::map_rules().unwrap(),
        );
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
            &crate::culture_rules().unwrap(),
            &crate::loyalty_rules().unwrap(),
            &crate::ranking_rules().unwrap(),
            &map,
            GameSpeed::new(1.0).unwrap(),
            world.seed as u64,
            arrive,
            100,
            (3, 6, 10),
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
    #[sqlx::test(migrations = "../../migrations")]
    async fn scout_apply_and_reports(pool: PgPool) {
        let Setup { repo, template, .. } = setup(pool.clone()).await;
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
    #[sqlx::test(migrations = "../../migrations")]
    async fn process_due_scouts_resolves_a_mission(pool: PgPool) {
        let Setup {
            repo,
            econ,
            world,
            config,
            template,
        } = setup(pool.clone()).await;
        let units = crate::unit_rules().expect("unit rules");
        let scout = crate::scout_rules().expect("scout rules");
        let map = WorldMap::new(
            world.seed as u64,
            config.radius,
            crate::map_rules().unwrap(),
        );
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
    #[sqlx::test(migrations = "../../migrations")]
    async fn combat_crash_resume_resolves_once(pool: PgPool) {
        let Setup {
            repo,
            econ,
            world,
            config,
            template,
        } = setup(pool.clone()).await;
        let units = crate::unit_rules().expect("unit rules");
        let combat = crate::combat_rules().expect("combat rules");
        let scout = crate::scout_rules().expect("scout rules");
        let map = WorldMap::new(
            world.seed as u64,
            config.radius,
            crate::map_rules().unwrap(),
        );
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
                &crate::culture_rules().unwrap(),
                &crate::loyalty_rules().unwrap(),
                &crate::ranking_rules().unwrap(),
                &map,
                GameSpeed::new(1.0).unwrap(),
                world.seed as u64,
                arrive,
                100,
                (3, 6, 10),
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

    #[sqlx::test(migrations = "../../migrations")]
    async fn create_account_persists_user_and_one_village(pool: PgPool) {
        let Setup { repo, .. } = setup(pool.clone()).await;
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

    /// 037: registration creates a per-world player profile, and the resolution layer finds it. The
    /// reuse-UUID invariant holds — `player_in_world` equals the user id, which equals the village owner.
    #[sqlx::test(migrations = "../../migrations")]
    async fn registration_creates_player_and_resolves_in_world(pool: PgPool) {
        let Setup { repo, world, .. } = setup(pool.clone()).await;
        let template = crate::starting_village().expect("template");
        let user = make_account(&repo, &template, "split").await;

        // The account now has exactly one player, in the one world, resolving to the same id (AC2/AC3/AC4).
        let player = repo
            .player_in_world(user, world.id)
            .await
            .expect("resolve")
            .expect("a player exists in the world");
        assert_eq!(player, user, "reuse-UUID invariant: player id == user id");

        // The village owner is that player (the seam that lets owner_id key on the player, AC5).
        let villages = repo.villages_of(user).await.expect("villages");
        assert_eq!(villages[0].owner, player);

        // `worlds_of_user` lists exactly the one world, with the chosen tribe.
        let worlds = repo.worlds_of_user(user).await.expect("worlds");
        assert_eq!(worlds.len(), 1);
        assert_eq!(worlds[0].player, user);
        assert_eq!(worlds[0].world, world.id);
        assert_eq!(worlds[0].tribe, Tribe::Gauls);

        // An account with no player in a *different* world resolves to None (the join gate, future).
        let other_world = WorldId(0xDEAD_BEEF);
        assert!(
            repo.player_in_world(user, other_world)
                .await
                .expect("resolve other")
                .is_none()
        );
    }

    /// 037 AC2: the migration backfill gives every **pre-existing** user (one that predates the players
    /// table) exactly one player with `id = user_id`, and is idempotent. Exercised against a directly
    /// inserted user — the path the migration runs against real data, which `create_account` does not cover.
    #[sqlx::test(migrations = "../../migrations")]
    async fn backfill_creates_one_player_per_existing_user(pool: PgPool) {
        let Setup { world, .. } = setup(pool.clone()).await;
        let world_uuid = Uuid::from_u128(world.id.0);

        // A user that exists with no player row (as if it predated migration 0043).
        let user_uuid = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO users (id, username, email, password_hash, email_confirmed, tribe) \
             VALUES ($1, $2, $3, 'h', true, 'teutons')",
        )
        .bind(user_uuid)
        .bind(format!("legacy_{}", user_uuid.simple()))
        .bind(format!("legacy_{}@example.com", user_uuid.simple()))
        .execute(&pool)
        .await
        .expect("insert legacy user");

        // The backfill statement from migration 0043.
        let backfill = "INSERT INTO players (id, user_id, world_id, tribe) \
             SELECT u.id, u.id, w.id, u.tribe \
             FROM users u CROSS JOIN (SELECT id FROM worlds LIMIT 1) w \
             ON CONFLICT (user_id, world_id) DO NOTHING";
        sqlx::query(backfill)
            .execute(&pool)
            .await
            .expect("backfill");

        // Exactly one player: id = user_id, the world, the user's tribe.
        let rows: Vec<(Uuid, Uuid, String)> =
            sqlx::query_as("SELECT id, world_id, tribe FROM players WHERE user_id = $1")
                .bind(user_uuid)
                .fetch_all(&pool)
                .await
                .expect("players");
        assert_eq!(rows.len(), 1, "exactly one player per existing user");
        assert_eq!(rows[0].0, user_uuid, "player id == user id (reuse-UUID)");
        assert_eq!(rows[0].1, world_uuid, "player in the single world");
        assert_eq!(rows[0].2, "teutons", "tribe copied from the user");

        // Idempotent — re-running the backfill adds nothing.
        sqlx::query(backfill)
            .execute(&pool)
            .await
            .expect("backfill again");
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM players WHERE user_id = $1")
            .bind(user_uuid)
            .fetch_one(&pool)
            .await
            .expect("count");
        assert_eq!(count, 1, "backfill is idempotent");
    }

    /// 006 AC6 migration-boundary guard: the world `seed` is backfilled NOT NULL with the
    /// deterministic per-world value, and adding it does not move a pre-existing village or change
    /// its fields. (The NOT NULL is guaranteed by 0009's own `SET NOT NULL`, which aborts on any
    /// row left NULL — like the 0005 tribe backfill — so only the determinism + village-stability
    /// halves need a data-level test.)
    #[sqlx::test(migrations = "../../migrations")]
    async fn world_seed_is_backfilled_and_villages_are_unmoved(pool: PgPool) {
        let Setup { repo, world, .. } = setup(pool.clone()).await;
        // The seed is non-null and equals the deterministic per-world backfill value.
        let expected: i64 =
            sqlx::query_scalar("SELECT hashtextextended(id::text, 0) FROM worlds WHERE id = $1")
                .bind(Uuid::from_u128(world.id.0))
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(world.seed, expected);

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
    #[sqlx::test(migrations = "../../migrations")]
    async fn villages_are_placed_on_valleys_with_tile_fields(pool: PgPool) {
        let Setup {
            repo,
            world,
            config,
            ..
        } = setup(pool.clone()).await;
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
    #[sqlx::test(migrations = "../../migrations")]
    async fn backfill_repairs_legacy_village_without_resources(pool: PgPool) {
        let Setup { repo, .. } = setup(pool.clone()).await;

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
    #[sqlx::test(migrations = "../../migrations")]
    async fn tribe_backfill_repairs_pre_004_village(pool: PgPool) {
        let Setup { repo, .. } = setup(pool.clone()).await;

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

    #[sqlx::test(migrations = "../../migrations")]
    async fn build_order_lifecycle(pool: PgPool) {
        let Setup { repo, .. } = setup(pool.clone()).await;

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

    /// 039 AC3: a repo scoped to one world claims/requeues only its own world's due work — a due build in
    /// another world is invisible to it. This is the isolation per-world schedulers (040) rely on.
    #[sqlx::test(migrations = "../../migrations")]
    async fn due_claims_are_world_scoped(pool: PgPool) {
        let Setup { repo, template, .. } = setup(pool.clone()).await;
        // A village in world A, owned by a real account.
        let user = make_account(&repo, &template, "wsa").await;
        let village_a = repo.villages_of(user).await.unwrap()[0].id;

        // A second world B, with a village owned by the *same* account (one account, many worlds — 037).
        let world_b = Uuid::new_v4();
        sqlx::query("INSERT INTO worlds (id, speed, radius, seed) VALUES ($1, 1.0, 50, 1)")
            .bind(world_b)
            .execute(&pool)
            .await
            .unwrap();
        let village_b = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO villages (id, world_id, owner_id, x, y, tribe) \
             VALUES ($1, $2, $3, 999, 999, 'gauls')",
        )
        .bind(village_b)
        .bind(world_b)
        .bind(Uuid::from_u128(user.0))
        .execute(&pool)
        .await
        .unwrap();

        // A due (past) build in each world.
        let past = (crate::now().0 - 60_000) as f64;
        for vid in [Uuid::from_u128(village_a.0), village_b] {
            sqlx::query(
                "INSERT INTO build_orders \
                 (id, village_id, target_table, slot, building_type, target_level, complete_at, status, lane) \
                 VALUES ($1, $2, 'building', 1, 'warehouse', 1, \
                         to_timestamp($3::double precision / 1000.0), 'pending', 'all')",
            )
            .bind(Uuid::new_v4())
            .bind(vid)
            .bind(past)
            .execute(&pool)
            .await
            .unwrap();
        }

        // The world-A repo claims only world A's build.
        let due = repo
            .claim_due_builds(Timestamp(crate::now().0), 100)
            .await
            .unwrap();
        assert_eq!(due.len(), 1, "claims only world A's due build");
        assert_eq!(due[0].village, village_a);

        // World B's build is untouched — still pending.
        let b_pending: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM build_orders WHERE village_id = $1 AND status = 'pending'",
        )
        .bind(village_b)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(b_pending, 1, "world B's build untouched by world A's claim");

        // Requeue is likewise world-scoped: force B's build to 'processing', then A's requeue leaves it.
        sqlx::query("UPDATE build_orders SET status = 'processing' WHERE village_id = $1")
            .bind(village_b)
            .execute(&pool)
            .await
            .unwrap();
        // A's claimed build is now 'processing' too; A's requeue should reset only it.
        assert_eq!(
            repo.requeue_orphaned_builds().await.unwrap(),
            1,
            "requeues only world A's processing build"
        );
        let b_processing: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM build_orders WHERE village_id = $1 AND status = 'processing'",
        )
        .bind(village_b)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            b_processing, 1,
            "world B's processing build untouched by world A's requeue"
        );

        // The `home_village` path (movements/trades) is world-scoped the same way: a due reinforce in
        // world B is invisible to world A's movement claim + requeue.
        let arrive_past = (crate::now().0 - 60_000) as f64;
        sqlx::query(
            "INSERT INTO troop_movements \
             (id, owner_id, kind, home_village, deliver_village, origin_x, origin_y, dest_x, dest_y, \
              depart_at, arrive_at, status) \
             VALUES ($1, $2, 'reinforce', $3, $3, 999, 999, 999, 999, \
                     to_timestamp($4::double precision / 1000.0), \
                     to_timestamp($4::double precision / 1000.0), 'in_transit')",
        )
        .bind(Uuid::new_v4())
        .bind(Uuid::from_u128(user.0))
        .bind(village_b)
        .bind(arrive_past)
        .execute(&pool)
        .await
        .unwrap();

        // World A's movement claim takes nothing (it has no due movements of its own).
        assert!(
            repo.claim_due_movements(Timestamp(crate::now().0), 100)
                .await
                .unwrap()
                .is_empty(),
            "world A claims none of world B's movements"
        );
        // World B's reinforce is still in_transit — untouched by A's claim.
        let b_in_transit: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM troop_movements WHERE home_village = $1 AND status = 'in_transit'",
        )
        .bind(village_b)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            b_in_transit, 1,
            "world B's movement untouched by world A's claim"
        );

        // Force B's movement to 'processing'; world A's requeue must leave it.
        sqlx::query("UPDATE troop_movements SET status = 'processing' WHERE home_village = $1")
            .bind(village_b)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            repo.requeue_orphaned_movements().await.unwrap(),
            0,
            "world A requeues none of world B's movements"
        );
        let b_still_processing: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM troop_movements WHERE home_village = $1 AND status = 'processing'",
        )
        .bind(village_b)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            b_still_processing, 1,
            "world B's processing movement untouched by world A's requeue"
        );
    }

    /// 040 AC1: `all_worlds` loads every world with its own speed + radius (the registry spawns a
    /// scheduler per row).
    #[sqlx::test(migrations = "../../migrations")]
    async fn all_worlds_loads_each_with_its_config(pool: PgPool) {
        let Setup { world, .. } = setup(pool.clone()).await; // the home world (speed 1.0, radius 50)
        // A second world with a distinct speed + radius.
        let world_b = Uuid::new_v4();
        sqlx::query("INSERT INTO worlds (id, speed, radius, seed) VALUES ($1, 5.0, 33, 7)")
            .bind(world_b)
            .execute(&pool)
            .await
            .unwrap();

        let worlds = crate::all_worlds(&pool).await.unwrap();
        assert_eq!(worlds.len(), 2);
        let home = worlds.iter().find(|w| w.id == world.id).unwrap();
        assert!((home.speed - 1.0).abs() < f64::EPSILON);
        assert_eq!(home.radius, 50);
        let b = worlds.iter().find(|w| w.id.0 == world_b.as_u128()).unwrap();
        assert!((b.speed - 5.0).abs() < f64::EPSILON);
        assert_eq!(b.radius, 33);
        // 049: every world defaults to the `classic` rule preset.
        assert_eq!(home.rule_preset, "classic");
        assert_eq!(b.rule_preset, "classic");
    }

    /// 040 AC5: a repo built per world processes only its own world's due work — the registry premise.
    /// Two repos over two worlds each claim only their world's due build.
    #[sqlx::test(migrations = "../../migrations")]
    async fn per_world_repos_claim_independently(pool: PgPool) {
        let Setup {
            repo: repo_a,
            econ,
            template,
            ..
        } = setup(pool.clone()).await;
        let beginner = crate::lifecycle_rules().unwrap().beginner_protection_secs;

        // World A: a real account + a due build.
        let user = make_account(&repo_a, &template, "pw").await;
        let village_a = repo_a.villages_of(user).await.unwrap()[0].id;

        // World B: its own row + repo + a village (same account, second world) + a due build.
        let world_b = Uuid::new_v4();
        sqlx::query("INSERT INTO worlds (id, speed, radius, seed) VALUES ($1, 1.0, 50, 2)")
            .bind(world_b)
            .execute(&pool)
            .await
            .unwrap();
        let repo_b = PgAccountRepository::new(
            pool.clone(),
            WorldId(world_b.as_u128()),
            2,
            50,
            econ.starting_amounts,
            beginner,
            GameSpeed::new(1.0).unwrap(),
        );
        let village_b = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO villages (id, world_id, owner_id, x, y, tribe) \
             VALUES ($1, $2, $3, 998, 998, 'gauls')",
        )
        .bind(village_b)
        .bind(world_b)
        .bind(Uuid::from_u128(user.0))
        .execute(&pool)
        .await
        .unwrap();

        let past = (crate::now().0 - 60_000) as f64;
        for vid in [Uuid::from_u128(village_a.0), village_b] {
            sqlx::query(
                "INSERT INTO build_orders \
                 (id, village_id, target_table, slot, building_type, target_level, complete_at, status, lane) \
                 VALUES ($1, $2, 'building', 1, 'warehouse', 1, \
                         to_timestamp($3::double precision / 1000.0), 'pending', 'all')",
            )
            .bind(Uuid::new_v4())
            .bind(vid)
            .bind(past)
            .execute(&pool)
            .await
            .unwrap();
        }

        // Each world's repo claims only its own world's build.
        let due_a = repo_a
            .claim_due_builds(Timestamp(crate::now().0), 100)
            .await
            .unwrap();
        assert_eq!(due_a.len(), 1);
        assert_eq!(due_a[0].village, village_a);

        let due_b = repo_b
            .claim_due_builds(Timestamp(crate::now().0), 100)
            .await
            .unwrap();
        assert_eq!(due_b.len(), 1);
        assert_eq!(due_b[0].village.0, village_b.as_u128());
    }

    /// 042 AC3: an existing account joins a second world — a fresh player (id ≠ user id) + a starting
    /// village are created there; re-joining is rejected. `create_account` (the home world) is unchanged.
    #[sqlx::test(migrations = "../../migrations")]
    async fn account_joins_a_second_world(pool: PgPool) {
        let Setup {
            repo: home,
            econ,
            template,
            ..
        } = setup(pool.clone()).await;
        let beginner = crate::lifecycle_rules().unwrap().beginner_protection_secs;
        let user = make_account(&home, &template, "join").await;

        // A second world + its repo (its own map for placement).
        let world_b = Uuid::new_v4();
        sqlx::query("INSERT INTO worlds (id, speed, radius, seed) VALUES ($1, 3.0, 40, 9)")
            .bind(world_b)
            .execute(&pool)
            .await
            .unwrap();
        let repo_b = PgAccountRepository::new(
            pool.clone(),
            WorldId(world_b.as_u128()),
            9,
            40,
            econ.starting_amounts,
            beginner,
            GameSpeed::new(3.0).unwrap(),
        );

        // Join: a fresh player id (not the user id) + a starting village in world B.
        let player_b = repo_b
            .create_player_in_world(user, Tribe::Teutons, &template)
            .await
            .expect("join");
        assert_ne!(player_b, user, "a second world's player gets a fresh id");

        // Resolution finds the new player; the account now participates in two worlds.
        assert_eq!(
            repo_b
                .player_in_world(user, WorldId(world_b.as_u128()))
                .await
                .unwrap(),
            Some(player_b)
        );
        let worlds = home.worlds_of_user(user).await.unwrap();
        assert_eq!(worlds.len(), 2, "home world + the joined world");

        // The starting village exists in world B, owned by the new player, with fields.
        let villages = repo_b.villages_of(player_b).await.unwrap();
        assert_eq!(villages.len(), 1);
        assert_eq!(villages[0].fields.len(), 18);
        let vworld: Uuid = sqlx::query_scalar("SELECT world_id FROM villages WHERE owner_id = $1")
            .bind(Uuid::from_u128(player_b.0))
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(vworld, world_b, "the village is in the joined world");

        // Re-joining the same world is rejected.
        assert!(matches!(
            repo_b
                .create_player_in_world(user, Tribe::Teutons, &template)
                .await,
            Err(RepoError::Duplicate)
        ));
    }

    /// 045 AC1: a **second-world** player's name resolves through `players`. The player's id ≠ the user id,
    /// so the old `JOIN users ON u.id = owner_id` found nothing; the re-pointed `owner → players → users`
    /// join resolves the account's username on the map owner read.
    #[sqlx::test(migrations = "../../migrations")]
    async fn second_world_player_name_resolves_through_players(pool: PgPool) {
        let Setup {
            repo: home,
            econ,
            template,
            ..
        } = setup(pool.clone()).await;
        let beginner = crate::lifecycle_rules().unwrap().beginner_protection_secs;
        let user = make_account(&home, &template, "name").await;
        let username: String = sqlx::query_scalar("SELECT username FROM users WHERE id = $1")
            .bind(Uuid::from_u128(user.0))
            .fetch_one(&pool)
            .await
            .unwrap();

        // A second world + the account's player (fresh id ≠ user id) + starting village in it.
        let world_b = Uuid::new_v4();
        sqlx::query("INSERT INTO worlds (id, speed, radius, seed) VALUES ($1, 2.0, 40, 77)")
            .bind(world_b)
            .execute(&pool)
            .await
            .unwrap();
        let repo_b = PgAccountRepository::new(
            pool.clone(),
            WorldId(world_b.as_u128()),
            77,
            40,
            econ.starting_amounts,
            beginner,
            GameSpeed::new(2.0).unwrap(),
        );
        let player_b = repo_b
            .create_player_in_world(user, Tribe::Teutons, &template)
            .await
            .expect("join");
        assert_ne!(player_b, user, "the second-world player has a fresh id");

        // The map owner read for that village resolves the account's username through `players`.
        let villages = repo_b.villages_of(player_b).await.unwrap();
        let coord = villages[0].coordinate;
        let markers = repo_b.villages_at(&[coord]).await.unwrap();
        assert_eq!(markers.len(), 1, "the second-world village is on the map");
        assert_eq!(
            markers[0].owner_name, username,
            "the second-world player's name resolves via owner → players → users"
        );
    }

    /// 042 AC2: the single global Natar NPC player (id = NPC user id) is created idempotently and is
    /// collision-safe when more than one world reaches its end-game — the exact statement the artifact /
    /// Wonder release runs, applied for two worlds, inserts exactly one NPC player without erroring.
    #[sqlx::test(migrations = "../../migrations")]
    async fn npc_player_is_collision_safe_across_worlds(pool: PgPool) {
        let Setup { world, .. } = setup(pool.clone()).await;
        let world_b = Uuid::new_v4();
        sqlx::query("INSERT INTO worlds (id, speed, radius, seed) VALUES ($1, 1.0, 20, 1)")
            .bind(world_b)
            .execute(&pool)
            .await
            .unwrap();
        let npc = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO users (id, username, email, password_hash, email_confirmed, tribe, is_npc) \
             VALUES ($1, 'Natars', 'natars@system.local', '!', true, 'romans', true)",
        )
        .bind(npc)
        .execute(&pool)
        .await
        .unwrap();

        // The release's NPC-player statement, run for both worlds.
        for w in [Uuid::from_u128(world.id.0), world_b] {
            sqlx::query(
                "INSERT INTO players (id, user_id, world_id, tribe) VALUES ($1, $1, $2, 'romans') \
                 ON CONFLICT (id) DO NOTHING",
            )
            .bind(npc)
            .bind(w)
            .execute(&pool)
            .await
            .expect("npc player insert is collision-safe");
        }
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM players WHERE user_id = $1")
            .bind(npc)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1, "exactly one NPC player across worlds");
    }

    /// 004 AC13: a Roman village holds one field and one building order concurrently (separate
    /// lanes), but never two of the same lane; a non-Roman village is limited to one in total
    /// (single 'all' lane) — both DB-enforced under races by the partial unique index.
    #[sqlx::test(migrations = "../../migrations")]
    async fn roman_lanes_allow_field_and_building_in_parallel(pool: PgPool) {
        let Setup { repo, .. } = setup(pool.clone()).await;

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
    #[sqlx::test(migrations = "../../migrations")]
    async fn unit_order_lifecycle(pool: PgPool) {
        let Setup { repo, .. } = setup(pool.clone()).await;

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
    #[sqlx::test(migrations = "../../migrations")]
    async fn training_batch_lifecycle(pool: PgPool) {
        let Setup {
            repo, econ: rules, ..
        } = setup(pool.clone()).await;

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
    #[sqlx::test(migrations = "../../migrations")]
    async fn starvation_check_lifecycle(pool: PgPool) {
        let Setup { repo, .. } = setup(pool.clone()).await;

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

    #[sqlx::test(migrations = "../../migrations")]
    async fn process_due_builds_applies_due_orders(pool: PgPool) {
        let Setup { repo, .. } = setup(pool.clone()).await;

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
        eperica_application::process_due_builds(
            &repo,
            &repo,
            &repo,
            &crate::culture_rules().unwrap(),
            now,
            1000,
        )
        .await
        .expect("process due builds");
        let fields = repo.villages_of(user.id).await.unwrap()[0].fields.clone();
        assert_eq!(fields[1].level, 1);
    }

    /// AC5 (building path): constructing a new building in an empty center slot exercises the
    /// `apply_build` Building arm — the `INSERT ... ON CONFLICT` upsert taking its INSERT branch.
    /// The starting village has only Main Building (slot 0) + Rally Point (slot 1), so building a
    /// Warehouse at slot 2 creates a brand-new row (vs. the Field path, which only ever UPDATEs).
    #[sqlx::test(migrations = "../../migrations")]
    async fn build_constructs_new_building_in_empty_slot(pool: PgPool) {
        let Setup { repo, .. } = setup(pool.clone()).await;

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
    #[sqlx::test(migrations = "../../migrations")]
    async fn oasis_clear_and_occupy_lifecycle(pool: PgPool) {
        let Setup {
            repo,
            world,
            config,
            template,
            ..
        } = setup(pool.clone()).await;

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
            report: NewOasisReport {
                attacker_player: attacker.id,
                attacker_village: v.id,
                defender_player: None,
                defender_village: None,
                oasis,
                label: "Oasis".to_owned(),
                attacker_won: true,
                luck: 1.0,
                morale: 1.0,
                attacker_forces: vec![(UnitId("phalanx".into()), 30)],
                attacker_losses: vec![(UnitId("phalanx".into()), 5)],
                defender_forces: seeded.clone(),
                defender_losses: seeded.clone(),
            },
            regrow_at: None,
        })
        .await
        .expect("apply oasis battle");

        // AC11: the oasis battle report is readable in the attacker's inbox, on the 009 rails.
        let inbox = repo.reports_for(attacker.id, 50).await.unwrap();
        let report = inbox
            .iter()
            .find(|r| r.kind == MovementKind::OasisAttack && r.defender_coord == oasis)
            .expect("oasis report in the inbox");
        assert_eq!(report.attacker_player, attacker.id);
        assert_eq!(report.defender_player, None, "wild animals have no player");
        assert_eq!(report.defender_name, "Oasis");
        assert!(report.attacker_won);

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
    #[sqlx::test(migrations = "../../migrations")]
    async fn oasis_clear_without_capacity_stays_unoccupied(pool: PgPool) {
        let Setup {
            repo,
            world,
            config,
            template,
            ..
        } = setup(pool.clone()).await;

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
            report: NewOasisReport {
                attacker_player: attacker.id,
                attacker_village: v.id,
                defender_player: None,
                defender_village: None,
                oasis,
                label: "Oasis".to_owned(),
                attacker_won: true,
                luck: 1.0,
                morale: 1.0,
                attacker_forces: Vec::new(),
                attacker_losses: Vec::new(),
                defender_forces: Vec::new(),
                defender_losses: Vec::new(),
            },
            regrow_at: None,
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

    // 012 AC8: the bonus of the oases a village occupies is folded into its read and stacks across
    // multiple oases, lifting its production.
    #[sqlx::test(migrations = "../../migrations")]
    async fn occupied_oasis_bonus_stacks_into_village_read(pool: PgPool) {
        let Setup {
            repo,
            econ: rules,
            world,
            config,
            template,
        } = setup(pool.clone()).await;

        let uname = format!("oasisbonus_{}", Uuid::new_v4().simple());
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
        assert_eq!(
            v.oasis_bonus,
            OasisBonus::default(),
            "a fresh village holds no oasis"
        );

        // Two oasis tiles, occupied by this village.
        let map = WorldMap::new(
            world.seed as u64,
            config.radius,
            crate::map_rules().unwrap(),
        );
        let oases: Vec<Coordinate> = coordinates_within(config.radius)
            .filter(|c| map.oasis_bonus_at(*c).is_some())
            .take(2)
            .collect();
        assert_eq!(oases.len(), 2, "need two oases on the seeded map");
        for c in &oases {
            sqlx::query(
                "INSERT INTO oases (world_id, x, y, owner_village, materialised) \
                 VALUES ($1, $2, $3, $4, true) \
                 ON CONFLICT (world_id, x, y) \
                 DO UPDATE SET owner_village = EXCLUDED.owner_village, materialised = true",
            )
            .bind(Uuid::from_u128(world.id.0))
            .bind(c.x)
            .bind(c.y)
            .bind(Uuid::from_u128(v.id.0))
            .execute(&pool)
            .await
            .unwrap();
        }

        // The summed bonus stacks the two tiles' per-resource bonuses (saturating at u8).
        let sum = |pick: fn(&OasisBonus) -> u8| -> u8 {
            oases
                .iter()
                .map(|c| u32::from(pick(&map.oasis_bonus_at(*c).unwrap())))
                .sum::<u32>()
                .min(u32::from(u8::MAX)) as u8
        };
        let expected = OasisBonus {
            wood: sum(|b| b.wood),
            clay: sum(|b| b.clay),
            iron: sum(|b| b.iron),
            crop: sum(|b| b.crop),
        };
        assert_eq!(repo.village_oasis_bonus(v.id).await.unwrap(), expected);

        // It is folded into the village read, so every economy computation sees it.
        let read = repo.village_by_id(v.id).await.unwrap().expect("village");
        assert_eq!(read.oasis_bonus, expected);
        assert_eq!(
            repo.villages_of(user.id).await.unwrap()[0].oasis_bonus,
            expected
        );

        // A holding village's production is at least as high as without the bonus (strictly higher
        // when any tile grants a bonus, which the shipped balance always does).
        use eperica_domain::production_rates;
        let speed = GameSpeed::new(1.0).unwrap();
        let base = production_rates(
            &read.fields,
            &read.buildings,
            0,
            &rules,
            speed,
            OasisBonus::default(),
        );
        let boosted = production_rates(
            &read.fields,
            &read.buildings,
            0,
            &rules,
            speed,
            read.oasis_bonus,
        );
        assert!(boosted.wood >= base.wood && boosted.clay >= base.clay);
        assert!(
            boosted.wood + boosted.clay + boosted.iron > base.wood + base.clay + base.iron,
            "an occupied oasis lifts production"
        );
    }

    // 012 AC7/AC5: reinforcing an owned oasis stations defenders that read back and can be recalled;
    // a stronger attacker then beats the stationed defenders and (with Outpost capacity) takes it.
    #[sqlx::test(migrations = "../../migrations")]
    async fn oasis_reinforce_defend_recall_and_take(pool: PgPool) {
        let Setup {
            repo,
            econ,
            world,
            config,
            template,
        } = setup(pool.clone()).await;
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
            (user, v)
        };
        let (defender, dv) = account("oasisdef").await;
        let (attacker, av) = account("oasisatk").await;

        let map = WorldMap::new(
            world.seed as u64,
            config.radius,
            crate::map_rules().unwrap(),
        );
        let oasis = coordinates_within(config.radius)
            .find(|c| {
                map.oasis_bonus_at(*c).is_some() && *c != dv.coordinate && *c != av.coordinate
            })
            .expect("an oasis exists");
        let units = crate::unit_rules().expect("unit rules");
        let orules = crate::oasis_rules().expect("oasis rules");
        let animals = units.wild_animal_roster();

        // The defender owns the (cleared) oasis.
        sqlx::query(
            "INSERT INTO oases (world_id, x, y, owner_village, materialised) VALUES ($1, $2, $3, $4, true) \
             ON CONFLICT (world_id, x, y) DO UPDATE SET owner_village = EXCLUDED.owner_village, materialised = true",
        )
        .bind(Uuid::from_u128(world.id.0))
        .bind(oasis.x)
        .bind(oasis.y)
        .bind(Uuid::from_u128(dv.id.0))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("DELETE FROM oasis_garrison WHERE world_id = $1 AND x = $2 AND y = $3")
            .bind(Uuid::from_u128(world.id.0))
            .bind(oasis.x)
            .bind(oasis.y)
            .execute(&pool)
            .await
            .unwrap();

        // The defender reinforces the oasis with 20 phalanx.
        sqlx::query(
            "INSERT INTO village_units (village_id, unit_id, count) VALUES ($1, 'phalanx', 60)",
        )
        .bind(Uuid::from_u128(dv.id.0))
        .execute(&pool)
        .await
        .unwrap();
        let now = Timestamp(3_000_000_000_000);
        let arrive = Timestamp(now.0 + 1000);
        repo.start_oasis_reinforce(
            dv.id,
            defender.id,
            dv.coordinate,
            oasis,
            now,
            arrive,
            &[(UnitId("phalanx".into()), 20)],
        )
        .await
        .expect("reinforce");
        let due = repo
            .claim_due_oasis_reinforcements(arrive, 10)
            .await
            .unwrap();
        let mine = due.iter().find(|d| d.home_village == dv.id).expect("due");
        repo.apply_oasis_reinforce(mine, OasisReinforceOutcome::Station)
            .await
            .expect("station");
        // AC7: the stationed troops are the oasis's defenders.
        assert_eq!(
            repo.oasis_defenders(oasis, animals, &orules).await.unwrap(),
            vec![(UnitId("phalanx".into()), 20)]
        );

        // AC7: recall pulls them home; the oasis is left owned but undefended.
        let recall_arrive = Timestamp(arrive.0 + 1000);
        let recalled = repo
            .start_oasis_recall(
                oasis,
                dv.id,
                defender.id,
                dv.coordinate,
                arrive,
                recall_arrive,
            )
            .await
            .expect("recall");
        assert_eq!(recalled, vec![(UnitId("phalanx".into()), 20)]);
        assert!(
            repo.oasis_defenders(oasis, animals, &orules)
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            repo.oasis_at(oasis).await.unwrap().unwrap().owner,
            Some(dv.id),
            "recall leaves ownership unchanged"
        );

        // Re-station 20 phalanx, then a stronger attacker takes the oasis.
        repo.start_oasis_reinforce(
            dv.id,
            defender.id,
            dv.coordinate,
            oasis,
            now,
            arrive,
            &[(UnitId("phalanx".into()), 20)],
        )
        .await
        .expect("reinforce 2");
        let due = repo
            .claim_due_oasis_reinforcements(arrive, 10)
            .await
            .unwrap();
        let mine = due.iter().find(|d| d.home_village == dv.id).expect("due 2");
        repo.apply_oasis_reinforce(mine, OasisReinforceOutcome::Station)
            .await
            .expect("station 2");

        // The attacker has an Outpost (capacity ≥ 1) and a large army.
        sqlx::query(
            "INSERT INTO village_buildings (village_id, slot, building_type, level) VALUES ($1, 20, 'outpost', 3) \
             ON CONFLICT (village_id, slot) DO UPDATE SET building_type = EXCLUDED.building_type, level = EXCLUDED.level",
        )
        .bind(Uuid::from_u128(av.id.0))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO village_units (village_id, unit_id, count) VALUES ($1, 'phalanx', 600)",
        )
        .bind(Uuid::from_u128(av.id.0))
        .execute(&pool)
        .await
        .unwrap();
        let atk_now = Timestamp(now.0 + 10_000);
        let atk_arrive = Timestamp(atk_now.0 + 1000);
        repo.start_oasis_attack(
            av.id,
            attacker.id,
            av.coordinate,
            oasis,
            atk_now,
            atk_arrive,
            &[(UnitId("phalanx".into()), 500)],
        )
        .await
        .expect("attack");

        eperica_application::process_due_oasis_combat(
            &repo,
            &repo,
            &repo,
            &econ,
            &units,
            &crate::combat_rules().unwrap(),
            &orules,
            &map,
            GameSpeed::new(1.0).unwrap(),
            world.seed as u64,
            atk_arrive,
            10,
        )
        .await
        .expect("resolve oasis combat");

        // AC5: the stronger attacker beat the stationed defenders and took the oasis.
        assert_eq!(
            repo.oasis_at(oasis).await.unwrap().unwrap().owner,
            Some(av.id),
            "the attacker took the oasis"
        );
        assert!(
            repo.oasis_defenders(oasis, animals, &orules)
                .await
                .unwrap()
                .is_empty(),
            "the taken oasis is cleared of the old defenders"
        );
        // The previous owner no longer holds it; the new owner does.
        assert!(repo.occupied_oases(dv.id).await.unwrap().is_empty());
        assert_eq!(repo.occupied_oases(av.id).await.unwrap().len(), 1);

        // AC11: the oasis battle report reaches **both** parties — the attacker and the previous
        // owner (the defending owner, since the oasis was occupied when attacked). These are fresh
        // accounts, so each party has exactly this one oasis-attack report. (For an *occupied* oasis
        // the report's defender is the owner's village, so `defender_coord` is the village tile.)
        let oasis_report = |reports: Vec<BattleReportView>| {
            reports
                .into_iter()
                .find(|r| r.kind == MovementKind::OasisAttack)
        };
        let atk_report = oasis_report(repo.reports_for(attacker.id, 50).await.unwrap())
            .expect("attacker sees the oasis report");
        assert!(atk_report.attacker_won);
        assert_eq!(atk_report.defender_player, Some(defender.id));
        let def_report = oasis_report(repo.reports_for(defender.id, 50).await.unwrap())
            .expect("the previous owner sees the oasis report");
        assert_eq!(def_report.attacker_player, attacker.id);
    }

    // 012 AC9: a cleared, unoccupied oasis regrows its animals toward the seeded strength over due
    // ticks, then stops (the regrow is cleared once full).
    #[sqlx::test(migrations = "../../migrations")]
    async fn oasis_regrows_when_cleared(pool: PgPool) {
        let Setup {
            repo,
            world,
            config,
            ..
        } = setup(pool.clone()).await;
        let map = WorldMap::new(
            world.seed as u64,
            config.radius,
            crate::map_rules().unwrap(),
        );
        let units = crate::unit_rules().expect("unit rules");
        let orules = crate::oasis_rules().expect("oasis rules");
        let animals = units.wild_animal_roster();
        let oasis = coordinates_within(config.radius)
            .find(|c| map.oasis_bonus_at(*c).is_some())
            .expect("an oasis");
        let seeded = oasis_garrison(world.seed as u64, oasis, animals, &orules);
        let seeded_total: u32 = seeded.iter().map(|(_, n)| *n).sum();
        assert!(seeded_total > 0);

        // Materialise the oasis as cleared + unoccupied with a regrow already due (empty garrison).
        let now = Timestamp(3_000_000_000_000);
        sqlx::query("DELETE FROM oases WHERE world_id = $1 AND x = $2 AND y = $3")
            .bind(Uuid::from_u128(world.id.0))
            .bind(oasis.x)
            .bind(oasis.y)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO oases (world_id, x, y, owner_village, materialised, regrow_at) \
             VALUES ($1, $2, $3, NULL, true, to_timestamp($4::double precision / 1000.0))",
        )
        .bind(Uuid::from_u128(world.id.0))
        .bind(oasis.x)
        .bind(oasis.y)
        .bind(now.0 as f64)
        .execute(&pool)
        .await
        .unwrap();
        assert!(
            repo.oasis_defenders(oasis, animals, &orules)
                .await
                .unwrap()
                .is_empty(),
            "starts cleared"
        );

        // Run regrow ticks until the oasis stops being due (full strength). The clock advances one
        // regrow interval each tick (a regrow reschedules `regrow_secs` ahead).
        let step_ms = orules.regrow_secs * 1000; // speed 1.0
        let mut clock = Timestamp(now.0 + 1);
        let mut prev_total = 0u32;
        let mut ticks = 0;
        loop {
            let due = repo.claim_due_oasis_regrows(clock, 10).await.unwrap();
            if !due.iter().any(|d| d.oasis == oasis) {
                break;
            }
            eperica_application::process_due_oasis_regrow(
                &repo,
                &units,
                &orules,
                world.seed as u64,
                GameSpeed::new(1.0).unwrap(),
                clock,
                10,
            )
            .await
            .expect("regrow");
            let total: u32 = repo
                .oasis_defenders(oasis, animals, &orules)
                .await
                .unwrap()
                .iter()
                .map(|(_, n)| *n)
                .sum();
            assert!(total > prev_total, "each tick regrows more animals");
            assert!(total <= seeded_total, "never exceeds the seeded strength");
            prev_total = total;
            clock = Timestamp(clock.0 + step_ms);
            ticks += 1;
            assert!(ticks < 100, "regrow should converge");
        }
        // It reached full strength and the regrow was cleared.
        assert_eq!(
            prev_total, seeded_total,
            "regrew back to the seeded strength"
        );
        let state_row: Option<i64> = sqlx::query_scalar(
            "SELECT (EXTRACT(EPOCH FROM regrow_at) * 1000)::bigint FROM oases \
             WHERE world_id = $1 AND x = $2 AND y = $3",
        )
        .bind(Uuid::from_u128(world.id.0))
        .bind(oasis.x)
        .bind(oasis.y)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(state_row.is_none(), "regrow_at cleared once full");
    }

    // 013 AC9: completing a Palace makes the village the player's capital; building another Palace
    // elsewhere relocates it (exactly one capital per player).
    #[sqlx::test(migrations = "../../migrations")]
    async fn palace_sets_and_relocates_capital(pool: PgPool) {
        let Setup {
            repo,
            world,
            template,
            ..
        } = setup(pool.clone()).await;
        let uname = format!("capital_{}", Uuid::new_v4().simple());
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
        let v1 = repo.villages_of(user.id).await.unwrap()[0].clone();
        assert!(!v1.is_capital, "a fresh village is not capital");

        // Complete a Palace in v1 → it becomes the capital.
        let palace = |village: VillageId| DueBuild {
            id: Uuid::new_v4().as_u128(),
            village,
            target: BuildTarget::Building {
                slot: 15,
                kind: BuildingKind::Palace,
            },
            target_level: 1,
            complete_at: Timestamp(0),
        };
        repo.apply_build(palace(v1.id)).await.expect("build palace");
        assert!(
            repo.village_by_id(v1.id).await.unwrap().unwrap().is_capital,
            "the Palace village is now the capital"
        );
        assert!(
            repo.village_by_id(v1.id)
                .await
                .unwrap()
                .unwrap()
                .buildings
                .iter()
                .any(|b| b.kind == BuildingKind::Palace),
            "v1 has a Palace building"
        );

        // A second village for the same player (founded directly for the test). The shared dev DB is
        // saturated near the origin, so scan from the frontier for a free tile.
        let world_uuid = Uuid::from_u128(world.id.0);
        let (mut vx, mut vy) = (49i32, 49i32);
        loop {
            let taken: bool = sqlx::query_scalar(
                "SELECT true FROM villages WHERE world_id = $1 AND x = $2 AND y = $3",
            )
            .bind(world_uuid)
            .bind(vx)
            .bind(vy)
            .fetch_optional(&pool)
            .await
            .unwrap()
            .unwrap_or(false);
            if !taken {
                break;
            }
            vx -= 1;
            if vx < -49 {
                vx = 49;
                vy -= 1;
            }
        }
        let v2_id = Uuid::new_v4();
        sqlx::query("INSERT INTO villages (id, world_id, owner_id, x, y, tribe) VALUES ($1, $2, $3, $4, $5, 'gauls')")
            .bind(v2_id)
            .bind(world_uuid)
            .bind(Uuid::from_u128(user.id.0))
            .bind(vx)
            .bind(vy)
            .execute(&pool)
            .await
            .unwrap();
        let v2 = VillageId(v2_id.as_u128());

        // Building a Palace in v2 relocates the capital: v2 becomes it, v1's flag AND its Palace
        // building are cleared (at most one Palace per player — the old one cannot remain, AC9).
        repo.apply_build(palace(v2)).await.expect("build palace v2");
        assert!(repo.village_by_id(v2).await.unwrap().unwrap().is_capital);
        let v1_after = repo.village_by_id(v1.id).await.unwrap().unwrap();
        assert!(
            !v1_after.is_capital,
            "the previous capital is cleared — one per player"
        );
        assert!(
            !v1_after
                .buildings
                .iter()
                .any(|b| b.kind == BuildingKind::Palace),
            "the previous Palace building is demolished on relocation"
        );
        assert!(
            repo.village_by_id(v2)
                .await
                .unwrap()
                .unwrap()
                .buildings
                .iter()
                .any(|b| b.kind == BuildingKind::Palace),
            "the new capital keeps its Palace"
        );
    }

    // 013 AC1/AC2: the per-player culture accumulator is seeded at registration, accrues at the live
    // rate (base per village), and a Town Hall raises that rate (re-anchored exactly).
    #[sqlx::test(migrations = "../../migrations")]
    async fn culture_accrues_and_a_town_hall_raises_the_rate(pool: PgPool) {
        let Setup { repo, template, .. } = setup(pool.clone()).await;
        let crules = crate::culture_rules().expect("culture rules");
        let uname = format!("culture_{}", Uuid::new_v4().simple());
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

        // AC1: registration seeded the accumulator (value 0); the village has no Town Hall yet.
        assert_eq!(repo.player_culture(user.id).await.unwrap().0, 0);
        assert_eq!(
            repo.village_town_hall_levels(user.id).await.unwrap(),
            vec![0]
        );

        // Baseline at a controlled instant, then accrue one hour at the no-Town-Hall rate.
        let t0 = Timestamp(3_000_000_000_000);
        repo.settle_culture(user.id, 0, t0).await.unwrap();
        use eperica_domain::culture_rate;
        let base_rate = culture_rate(&[0], &crules); // one village, no Town Hall
        eperica_application::reanchor_culture(&repo, &crules, Timestamp(t0.0 + 3_600_000), user.id)
            .await
            .unwrap();
        let (after_1h, _) = repo.player_culture(user.id).await.unwrap();
        assert_eq!(after_1h, base_rate, "one hour at the base rate");

        // Build a Town Hall (level 3); the rate rises.
        sqlx::query(
            "INSERT INTO village_buildings (village_id, slot, building_type, level) \
             VALUES ($1, 14, 'town_hall', 3) \
             ON CONFLICT (village_id, slot) DO UPDATE SET building_type = EXCLUDED.building_type, level = EXCLUDED.level",
        )
        .bind(Uuid::from_u128(v.id.0))
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(
            repo.village_town_hall_levels(user.id).await.unwrap(),
            vec![3]
        );

        // AC2: another hour now accrues at the higher Town-Hall rate.
        let th_rate = culture_rate(&[3], &crules);
        assert!(th_rate > base_rate, "a Town Hall raises the rate");
        eperica_application::reanchor_culture(&repo, &crules, Timestamp(t0.0 + 7_200_000), user.id)
            .await
            .unwrap();
        let (after_2h, _) = repo.player_culture(user.id).await.unwrap();
        assert_eq!(
            after_2h,
            base_rate + th_rate,
            "first hour at base, second hour with the Town Hall"
        );
    }

    // 013 AC1 (migration boundary): a player whose account predates the culture accumulator (no
    // `player_culture` row) must NOT be handed a CP windfall. The read anchors a missing row at *now*
    // (zero CP), never the epoch — anchoring at the epoch would settle rate x decades of culture on the
    // first read and vault the player past the expansion thresholds. The 0021 backfill seeds the row.
    #[sqlx::test(migrations = "../../migrations")]
    async fn pre_013_account_without_a_culture_row_gets_no_windfall(pool: PgPool) {
        let Setup { repo, template, .. } = setup(pool.clone()).await;
        let crules = crate::culture_rules().expect("culture rules");
        let uname = format!("legacy_{}", Uuid::new_v4().simple());
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

        // Reproduce the legacy state: drop the seeded row so the account looks pre-013.
        sqlx::query("DELETE FROM player_culture WHERE player_id = $1")
            .bind(Uuid::from_u128(user.id.0))
            .execute(&pool)
            .await
            .unwrap();

        // The read anchors at *now* (a recent ms timestamp), not the epoch, and yields zero CP.
        let (value, anchor) = repo.player_culture(user.id).await.unwrap();
        assert_eq!(value, 0, "a missing row reads as zero CP");
        assert!(
            anchor.0 > 1_500_000_000_000,
            "a missing row anchors at now, not the epoch (got {})",
            anchor.0
        );
        let view = eperica_application::load_culture(&repo, &repo, &crules, crate::now(), user.id)
            .await
            .unwrap();
        assert_eq!(
            view.cp, 0,
            "no windfall — CP starts at zero, not rate x decades"
        );

        // The 0021 backfill (idempotently) seeds the row anchored at now.
        sqlx::query(
            "INSERT INTO player_culture (player_id, value, updated_at) \
             SELECT id, 0, now() FROM users WHERE id = $1 ON CONFLICT (player_id) DO NOTHING",
        )
        .bind(Uuid::from_u128(user.id.0))
        .execute(&pool)
        .await
        .unwrap();
        let (value, anchor) = repo.player_culture(user.id).await.unwrap();
        assert_eq!(value, 0);
        assert!(
            anchor.0 > 1_500_000_000_000,
            "backfilled row anchored at now"
        );
    }

    // 014 AC1/AC9: a village's loyalty starts at the maximum, reads back, re-anchors on a strike, and
    // regenerates toward the maximum (clamping there) over time.
    #[sqlx::test(migrations = "../../migrations")]
    async fn loyalty_reads_back_and_regenerates(pool: PgPool) {
        let Setup { repo, template, .. } = setup(pool.clone()).await;
        let rules = crate::loyalty_rules().expect("loyalty rules");
        let uname = format!("loyal_{}", Uuid::new_v4().simple());
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
        let v = repo.villages_of(user.id).await.unwrap()[0].id;

        // AC1: a fresh village starts fully loyal.
        let (loyalty, _) = repo.village_loyalty(v).await.unwrap().unwrap();
        assert_eq!(loyalty, eperica_domain::MAX_LOYALTY);

        // A strike lowers loyalty, anchored at a controlled instant; it reads back exactly.
        let t0 = Timestamp(3_000_000_000_000);
        repo.set_loyalty(v, 40, t0).await.unwrap();
        let (loyalty, at) = repo.village_loyalty(v).await.unwrap().unwrap();
        assert_eq!((loyalty, at), (40, t0));

        // AC9: it regenerates toward the maximum and clamps there.
        let speed = GameSpeed::new(1.0).unwrap();
        let one_hour = eperica_domain::regenerate_loyalty(loyalty, 3600, &rules, speed);
        assert!(
            one_hour > 40 && one_hour <= eperica_domain::MAX_LOYALTY,
            "regenerates upward (got {one_hour})"
        );
        let far = eperica_domain::regenerate_loyalty(loyalty, 10_000_000, &rules, speed);
        assert_eq!(far, eperica_domain::MAX_LOYALTY, "clamps at the max");
    }

    // 015 AC2/AC3/AC4/AC5/AC12: the alliance membership lifecycle over the real repository — found,
    // invite/accept, the one-alliance-per-player + cap guards, expel, and the disband cascade.
    #[sqlx::test(migrations = "../../migrations")]
    async fn alliance_membership_lifecycle(pool: PgPool) {
        let Setup { repo, template, .. } = setup(pool.clone()).await;
        let rules = crate::alliance_rules().expect("alliance rules");

        let make = |label: &'static str| {
            let repo = &repo;
            let template = &template;
            async move {
                let uname = format!("{label}_{}", Uuid::new_v4().simple());
                let user = repo
                    .create_account(
                        NewUser {
                            username: uname.clone(),
                            email: format!("{uname}@example.com"),
                            password_hash: "h".to_owned(),
                            email_confirmed: true,
                            tribe: Tribe::Gauls,
                        },
                        template,
                    )
                    .await
                    .expect("create account");
                let v = repo.villages_of(user.id).await.unwrap()[0].id;
                (user.id, v)
            }
        };
        let (founder, fv) = make("ally_f").await;
        let (m2, _) = make("ally_2").await;
        let (m3, _) = make("ally_3").await;
        let (m4, _) = make("ally_4").await;

        // Give the founder an Embassy ≥ 3 and the others ≥ 1 (insert the building rows directly).
        let set_embassy = |village: VillageId, level: i16| {
            let pool = &pool;
            async move {
                sqlx::query(
                    "INSERT INTO village_buildings (village_id, slot, building_type, level) \
                     VALUES ($1, 16, 'embassy', $2) \
                     ON CONFLICT (village_id, slot) DO UPDATE SET level = EXCLUDED.level",
                )
                .bind(Uuid::from_u128(village.0))
                .bind(level)
                .execute(pool)
                .await
                .unwrap();
            }
        };
        set_embassy(fv, 3).await;
        for (_, v) in [(m2, repo.villages_of(m2).await.unwrap()[0].id)] {
            set_embassy(v, 1).await;
        }
        {
            let v = repo.villages_of(m3).await.unwrap()[0].id;
            set_embassy(v, 1).await;
        }
        {
            // m4 is Embassy ≥ 3 so it can attempt a (rejected, duplicate-name) founding.
            let v = repo.villages_of(m4).await.unwrap()[0].id;
            set_embassy(v, 3).await;
        }

        // AC1/AC2: found needs Embassy ≥ 3; the highest-Embassy read sees it. (Unique name/tag per run
        // — the test DB is shared and not reset between runs.)
        assert_eq!(repo.max_embassy_level(founder).await.unwrap(), 3);
        let suffix = Uuid::new_v4().simple().to_string();
        let aname = format!("Templars_{suffix}");
        let atag = format!("T{}", &suffix[..6]);
        let aid = eperica_application::found_alliance(&repo, &rules, founder, &aname, &atag)
            .await
            .expect("found");
        assert_eq!(
            repo.alliance_of(founder).await.unwrap().unwrap().role,
            AllianceRole::Founder
        );
        // AC2: a duplicate name is rejected (m4 is eligible but the name is taken).
        assert!(matches!(
            eperica_application::found_alliance(
                &repo,
                &rules,
                m4,
                &aname,
                &format!("Z{}", &suffix[..6])
            )
            .await,
            Err(eperica_application::AllianceError::NameOrTagTaken)
        ));

        // AC3: invite + accept; the roster now lists two.
        eperica_application::invite_player(&repo, founder, m2)
            .await
            .unwrap();
        eperica_application::respond_invite(&repo, &rules, m2, aid, true)
            .await
            .unwrap();
        assert_eq!(repo.member_count(aid).await.unwrap(), 2);
        assert!(
            repo.roster(aid)
                .await
                .unwrap()
                .iter()
                .any(|e| e.player == m2)
        );

        // AC12: one alliance per player — adding the founder again is a Duplicate; the cap guard rejects
        // a join once the alliance is full (cap forced to 2 here).
        assert!(matches!(
            repo.add_member(aid, founder, AllianceRole::Member, RightSet::empty(), 60)
                .await,
            Err(RepoError::Duplicate)
        ));
        assert!(matches!(
            repo.add_member(aid, m3, AllianceRole::Member, RightSet::empty(), 2)
                .await,
            Err(RepoError::Conflict)
        ));

        // AC5: the founder expels m2; the roster is back to one.
        eperica_application::expel_member(&repo, founder, m2)
            .await
            .unwrap();
        assert_eq!(repo.member_count(aid).await.unwrap(), 1);
        assert!(repo.alliance_of(m2).await.unwrap().is_none());

        // AC5/AC12: disband cascades — invite m3, then disband; the alliance, members, and invitations
        // are all gone.
        eperica_application::invite_player(&repo, founder, m3)
            .await
            .unwrap();
        eperica_application::respond_invite(&repo, &rules, m3, aid, true)
            .await
            .unwrap();
        eperica_application::disband_alliance(&repo, founder)
            .await
            .unwrap();
        assert!(repo.alliance_of(founder).await.unwrap().is_none());
        assert!(repo.alliance_of(m3).await.unwrap().is_none());
        let invites: i64 =
            sqlx::query_scalar("SELECT count(*) FROM alliance_invitations WHERE alliance_id = $1")
                .bind(Uuid::from_u128(aid.0))
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(invites, 0, "disband cascades to invitations");
        let exists: Option<i32> = sqlx::query_scalar("SELECT 1 FROM alliances WHERE id = $1")
            .bind(Uuid::from_u128(aid.0))
            .fetch_optional(&pool)
            .await
            .unwrap();
        assert!(exists.is_none(), "the alliance row is gone");
    }

    // 015 AC7/AC12: diplomacy over the real repository — war is unilateral + mutual, a confederation is
    // propose→accept, declaring war clears the confederation, and the normalised pair (lo<hi PK) makes
    // a single canonical row regardless of who acts.
    #[sqlx::test(migrations = "../../migrations")]
    async fn alliance_diplomacy_lifecycle(pool: PgPool) {
        let Setup { repo, template, .. } = setup(pool.clone()).await;
        let rules = crate::alliance_rules().expect("alliance rules");

        let make = |label: &'static str| {
            let repo = &repo;
            let pool = &pool;
            let template = &template;
            async move {
                let uname = format!("{label}_{}", Uuid::new_v4().simple());
                let user = repo
                    .create_account(
                        NewUser {
                            username: uname.clone(),
                            email: format!("{uname}@example.com"),
                            password_hash: "h".to_owned(),
                            email_confirmed: true,
                            tribe: Tribe::Gauls,
                        },
                        template,
                    )
                    .await
                    .expect("create account");
                let v = repo.villages_of(user.id).await.unwrap()[0].id;
                sqlx::query(
                    "INSERT INTO village_buildings (village_id, slot, building_type, level) \
                     VALUES ($1, 16, 'embassy', 3) \
                     ON CONFLICT (village_id, slot) DO UPDATE SET level = EXCLUDED.level",
                )
                .bind(Uuid::from_u128(v.0))
                .execute(pool)
                .await
                .unwrap();
                user.id
            }
        };
        let f1 = make("dip_a").await;
        let f2 = make("dip_b").await;
        let suffix = Uuid::new_v4().simple().to_string();
        let a = eperica_application::found_alliance(
            &repo,
            &rules,
            f1,
            &format!("A_{suffix}"),
            &format!("A{}", &suffix[..5]),
        )
        .await
        .unwrap();
        let b = eperica_application::found_alliance(
            &repo,
            &rules,
            f2,
            &format!("B_{suffix}"),
            &format!("B{}", &suffix[..5]),
        )
        .await
        .unwrap();

        use eperica_application::DiplomacyCommand;
        // Propose → accept builds a single canonical row, active both ways.
        eperica_application::set_diplomacy(&repo, f1, b, DiplomacyCommand::ProposeConfederation)
            .await
            .unwrap();
        eperica_application::set_diplomacy(&repo, f2, a, DiplomacyCommand::AcceptConfederation)
            .await
            .unwrap();
        assert_eq!(repo.confederate_alliances(a).await.unwrap(), vec![b]);
        assert_eq!(repo.confederate_alliances(b).await.unwrap(), vec![a]);
        // Exactly one diplomacy row for the pair, regardless of action order.
        let rows: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM alliance_diplomacy \
             WHERE (alliance_lo = $1 AND alliance_hi = $2) OR (alliance_lo = $2 AND alliance_hi = $1)",
        )
        .bind(Uuid::from_u128(a.0))
        .bind(Uuid::from_u128(b.0))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(rows, 1, "the normalised pair has a single row");

        // Declaring war (from the other side) overrides the confederation and is mutual.
        eperica_application::set_diplomacy(&repo, f2, a, DiplomacyCommand::DeclareWar)
            .await
            .unwrap();
        assert!(repo.confederate_alliances(a).await.unwrap().is_empty());
        let (stance, status, _) = repo.diplomacy_state(a, b).await.unwrap().unwrap();
        assert_eq!(
            (stance, status),
            (DiplomacyStance::War, DiplomacyStatus::Active)
        );

        // Cancel ⇒ neutral.
        eperica_application::set_diplomacy(&repo, f1, b, DiplomacyCommand::Cancel)
            .await
            .unwrap();
        assert!(repo.diplomacy_state(a, b).await.unwrap().is_none());
    }

    // 015 AC8/AC9: the alliance view spans members + one-hop confederates, the incoming-defence list
    // surfaces hostile movements against any allied village (target + ETA only, never troops), and a
    // non-allied target is excluded; a non-member has no alliance view.
    #[sqlx::test(migrations = "../../migrations")]
    async fn alliance_shared_visibility_and_incoming(pool: PgPool) {
        let Setup { repo, template, .. } = setup(pool.clone()).await;
        let rules = crate::alliance_rules().expect("alliance rules");
        let make = |label: &'static str, embassy: i16| {
            let repo = &repo;
            let pool = &pool;
            let template = &template;
            async move {
                let uname = format!("{label}_{}", Uuid::new_v4().simple());
                let user = repo
                    .create_account(
                        NewUser {
                            username: uname.clone(),
                            email: format!("{uname}@example.com"),
                            password_hash: "h".to_owned(),
                            email_confirmed: true,
                            tribe: Tribe::Gauls,
                        },
                        template,
                    )
                    .await
                    .expect("create account");
                let v = repo.villages_of(user.id).await.unwrap()[0].clone();
                if embassy > 0 {
                    sqlx::query(
                        "INSERT INTO village_buildings (village_id, slot, building_type, level) \
                         VALUES ($1, 16, 'embassy', $2) \
                         ON CONFLICT (village_id, slot) DO UPDATE SET level = EXCLUDED.level",
                    )
                    .bind(Uuid::from_u128(v.id.0))
                    .bind(embassy)
                    .execute(pool)
                    .await
                    .unwrap();
                }
                (user.id, v)
            }
        };
        let (f1, _v1) = make("vis_a", 3).await;
        let (mem, _vm) = make("vis_m", 1).await;
        let (f2, v2) = make("vis_b", 3).await; // confederate village (the threatened one)
        let (enemy, ev) = make("vis_e", 0).await;

        let suffix = Uuid::new_v4().simple().to_string();
        let a = eperica_application::found_alliance(
            &repo,
            &rules,
            f1,
            &format!("VA_{suffix}"),
            &format!("VA{}", &suffix[..4]),
        )
        .await
        .unwrap();
        let b = eperica_application::found_alliance(
            &repo,
            &rules,
            f2,
            &format!("VB_{suffix}"),
            &format!("VB{}", &suffix[..4]),
        )
        .await
        .unwrap();
        eperica_application::invite_player(&repo, f1, mem)
            .await
            .unwrap();
        eperica_application::respond_invite(&repo, &rules, mem, a, true)
            .await
            .unwrap();
        // Confederate A and B.
        use eperica_application::DiplomacyCommand;
        eperica_application::set_diplomacy(&repo, f1, b, DiplomacyCommand::ProposeConfederation)
            .await
            .unwrap();
        eperica_application::set_diplomacy(&repo, f2, a, DiplomacyCommand::AcceptConfederation)
            .await
            .unwrap();

        // Two attacks: the enemy hits the confederate's village (visible to A); f2 attacks the enemy's
        // village (NOT allied to A — excluded from A's incoming).
        let insert_attack = "INSERT INTO troop_movements \
             (id, owner_id, kind, home_village, deliver_village, origin_x, origin_y, dest_x, dest_y, \
              depart_at, arrive_at, status) \
             VALUES ($1, $2, 'attack', $3, $4, 0, 0, $5, $6, now(), \
                     to_timestamp($7::double precision / 1000.0), 'in_transit')";
        sqlx::query(insert_attack)
            .bind(Uuid::new_v4())
            .bind(Uuid::from_u128(enemy.0))
            .bind(Uuid::from_u128(ev.id.0))
            .bind(Uuid::from_u128(v2.id.0))
            .bind(v2.coordinate.x)
            .bind(v2.coordinate.y)
            .bind(5_000_000_000_000_f64)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(insert_attack)
            .bind(Uuid::new_v4())
            .bind(Uuid::from_u128(f2.0))
            .bind(Uuid::from_u128(v2.id.0))
            .bind(Uuid::from_u128(ev.id.0))
            .bind(ev.coordinate.x)
            .bind(ev.coordinate.y)
            .bind(6_000_000_000_000_f64)
            .execute(&pool)
            .await
            .unwrap();
        // A *resolved* attack on the allied village must NOT show (only in-transit force is incoming).
        sqlx::query(
            "INSERT INTO troop_movements \
             (id, owner_id, kind, home_village, deliver_village, origin_x, origin_y, dest_x, dest_y, \
              depart_at, arrive_at, status) \
             VALUES ($1, $2, 'attack', $3, $4, 0, 0, $5, $6, now(), \
                     to_timestamp($7::double precision / 1000.0), 'done')",
        )
        .bind(Uuid::new_v4())
        .bind(Uuid::from_u128(enemy.0))
        .bind(Uuid::from_u128(ev.id.0))
        .bind(Uuid::from_u128(v2.id.0))
        .bind(v2.coordinate.x)
        .bind(v2.coordinate.y)
        .bind(4_000_000_000_000_f64)
        .execute(&pool)
        .await
        .unwrap();

        // The founder's alliance view spans members + the confederate's villages, and the incoming list
        // holds exactly the attack on the confederate village (target + ETA only).
        let view = eperica_application::alliance_view(&repo, f1)
            .await
            .unwrap()
            .expect("founder has a view");
        assert!(
            view.allied_villages.iter().any(|av| av.player == mem),
            "fellow member's village is visible"
        );
        assert!(
            view.allied_villages.iter().any(|av| av.village == v2.id),
            "confederate's village is visible"
        );
        assert_eq!(view.incoming.len(), 1, "only the allied-target attack");
        assert_eq!(view.incoming[0].target, v2.id);
        assert_eq!(view.incoming[0].arrive_at, Timestamp(5_000_000_000_000));

        // A non-member (the enemy) has no alliance view.
        assert!(
            eperica_application::alliance_view(&repo, enemy)
                .await
                .unwrap()
                .is_none()
        );
    }

    // 014 AC7/AC8/AC12: a conquest transfers ownership in the battle transaction — the village keeps its
    // resources, its garrison is emptied, a third-party reinforcement is sent home, the loser's pending
    // orders are cancelled, both cultures are re-anchored, and the capital flag is cleared; re-applying
    // is rejected (the ownership guard prevents a double-transfer).
    #[sqlx::test(migrations = "../../migrations")]
    async fn conquest_transfers_ownership_and_re_anchors(pool: PgPool) {
        let Setup { repo, template, .. } = setup(pool.clone()).await;
        let account = |p: &str| {
            let repo = &repo;
            let template = &template;
            let p = p.to_owned();
            async move {
                let uname = format!("{p}_{}", Uuid::new_v4().simple());
                let user = repo
                    .create_account(
                        NewUser {
                            username: uname.clone(),
                            email: format!("{uname}@example.com"),
                            password_hash: "h".to_owned(),
                            email_confirmed: true,
                            tribe: Tribe::Gauls,
                        },
                        template,
                    )
                    .await
                    .expect("create account");
                (user.id, repo.villages_of(user.id).await.unwrap()[0].clone())
            }
        };
        let (attacker, a) = account("conq_atk").await;
        let (defender, d) = account("conq_def").await;
        let (ally, al) = account("conq_ally").await;

        // The defender's village holds a garrison, a pending build, and a third-party reinforcement.
        sqlx::query(
            "INSERT INTO village_units (village_id, unit_id, count) VALUES ($1, 'phalanx', 9)",
        )
        .bind(Uuid::from_u128(d.id.0))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO reinforcements (host_village, home_village, unit_id, count) \
             VALUES ($1, $2, 'phalanx', 3)",
        )
        .bind(Uuid::from_u128(d.id.0))
        .bind(Uuid::from_u128(al.id.0))
        .execute(&pool)
        .await
        .unwrap();
        // The defender's OWN troops are stationed abroad (home = the soon-conquered village, hosted at
        // the ally's village) and a column of the defender's troops is RETURNING to the village.
        sqlx::query(
            "INSERT INTO reinforcements (host_village, home_village, unit_id, count) \
             VALUES ($1, $2, 'phalanx', 5)",
        )
        .bind(Uuid::from_u128(al.id.0))
        .bind(Uuid::from_u128(d.id.0))
        .execute(&pool)
        .await
        .unwrap();
        let inbound_return = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO troop_movements \
             (id, owner_id, kind, home_village, deliver_village, origin_x, origin_y, dest_x, dest_y, \
              depart_at, arrive_at, status) \
             VALUES ($1, $2, 'return', $3, $3, $4, $5, $6, $7, now(), now() + interval '1 hour', \
                     'in_transit')",
        )
        .bind(inbound_return)
        .bind(Uuid::from_u128(defender.0))
        .bind(Uuid::from_u128(d.id.0))
        .bind(a.coordinate.x)
        .bind(a.coordinate.y)
        .bind(d.coordinate.x)
        .bind(d.coordinate.y)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO build_orders \
             (id, village_id, target_table, slot, building_type, target_level, complete_at, status, lane) \
             VALUES ($1, $2, 'building', 2, 'warehouse', 2, now() + interval '1 hour', 'pending', 'all')",
        )
        .bind(Uuid::new_v4())
        .bind(Uuid::from_u128(d.id.0))
        .execute(&pool)
        .await
        .unwrap();
        let wood_before: i64 =
            sqlx::query_scalar("SELECT wood FROM village_resources WHERE village_id = $1")
                .bind(Uuid::from_u128(d.id.0))
                .fetch_one(&pool)
                .await
                .unwrap();

        // Apply a winning admin-attack that conquers the village.
        let battle_at = Timestamp(3_000_000_000_000);
        let return_arrive = Timestamp(battle_at.0 + 100_000);
        let transfer = ConquestTransfer {
            new_owner: attacker,
            loser: defender,
            post_conquest_loyalty: 25,
            loser_culture_value: 111,
            gainer_culture_value: 222,
            reinforcement_returns: vec![ReinforcementReturn {
                home_village: al.id,
                owner: ally,
                home_coord: al.coordinate,
                troops: vec![(UnitId("phalanx".into()), 3)],
                arrive_at: return_arrive,
            }],
        };
        let apply = BattleApply {
            movement_id: Uuid::new_v4().as_u128(),
            owner: attacker,
            attacker_home: a.id,
            attacker_origin: a.coordinate,
            target: d.id,
            target_coord: d.coordinate,
            defender_losses: vec![(UnitId("phalanx".into()), 9)],
            reinforcement_losses: Vec::new(),
            survivors: vec![(UnitId("senator".into()), 1)],
            battle_at,
            return_arrive,
            report: NewBattleReport {
                kind: MovementKind::Attack,
                attacker_player: attacker,
                attacker_village: a.id,
                defender_player: defender,
                defender_village: d.id,
                attacker_won: true,
                luck: 1.0,
                morale: 1.0,
                wall_before: 0,
                wall_after: 0,
                attacker_forces: vec![(UnitId("senator".into()), 1)],
                attacker_losses: Vec::new(),
                defender_forces: vec![(UnitId("phalanx".into()), 9)],
                defender_losses: vec![(UnitId("phalanx".into()), 9)],
                loot: ResourceAmounts::default(),
                razed: None,
                loyalty_before: Some(20),
                loyalty_after: Some(0),
                conquered: true,
            },
            scouted: false,
            scout_target: None,
            scout_report: None,
            loot: ResourceAmounts::default(),
            target_debit: None,
            razed: None,
            loyalty: Some(LoyaltyApply::Conquered(transfer.clone())),
            attack_points: 0,
            defender_contributions: Vec::new(),
            artifact_capture: None,
            plan_capture: None,
        };
        repo.apply_battle(apply.clone()).await.expect("conquest");

        // AC7/AC8: ownership transferred, loyalty reset, capital flag cleared, resources kept.
        let taken = repo.village_by_id(d.id).await.unwrap().unwrap();
        assert_eq!(taken.owner, attacker, "ownership transferred");
        assert!(!taken.is_capital, "a conquered village is not a capital");
        let (loy, at) = repo.village_loyalty(d.id).await.unwrap().unwrap();
        assert_eq!(
            (loy, at),
            (25, battle_at),
            "loyalty reset, anchored at the battle"
        );
        assert_eq!(taken.coordinate, d.coordinate, "the tile is unchanged");
        let wood_after: i64 =
            sqlx::query_scalar("SELECT wood FROM village_resources WHERE village_id = $1")
                .bind(Uuid::from_u128(d.id.0))
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(wood_after, wood_before, "the village keeps its resources");
        // The village now appears under the new owner, and no longer under the loser.
        assert!(
            repo.villages_of(attacker)
                .await
                .unwrap()
                .iter()
                .any(|v| v.id == d.id)
        );
        assert!(
            !repo
                .villages_of(defender)
                .await
                .unwrap()
                .iter()
                .any(|v| v.id == d.id)
        );

        // Garrison emptied; the pending build cancelled.
        assert!(
            repo.garrison(d.id).await.unwrap().is_empty(),
            "garrison emptied"
        );
        let builds: i64 =
            sqlx::query_scalar("SELECT count(*) FROM build_orders WHERE village_id = $1")
                .bind(Uuid::from_u128(d.id.0))
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(builds, 0, "the loser's pending build was cancelled");

        // The third-party reinforcement was sent home (a return movement to the ally) and cleared.
        assert!(repo.reinforcements_at(d.id).await.unwrap().is_empty());
        let returning = repo.active_movements(ally).await.unwrap();
        assert!(
            returning.iter().any(|m| m.kind == MovementKind::Return),
            "the ally's reinforcement returns home"
        );

        // AC7 enumeration: the village's OWN troops stationed abroad pass to the new owner with the
        // village (their home is now the attacker's), and off the loser's books.
        assert!(
            repo.reinforcements_of(attacker)
                .await
                .unwrap()
                .iter()
                .any(|g| g.home_village == d.id),
            "the conquered village's stationed army follows it to the new owner"
        );
        assert!(
            repo.reinforcements_of(defender)
                .await
                .unwrap()
                .iter()
                .all(|g| g.home_village != d.id),
            "the loser no longer owns the conquered village's stationed army"
        );
        // AC7: a column returning to the now-lost village is forfeited (no loyal home to arrive at).
        let inbound_status: String =
            sqlx::query_scalar("SELECT status FROM troop_movements WHERE id = $1")
                .bind(inbound_return)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            inbound_status, "done",
            "a return inbound to the conquered village is forfeited"
        );

        // AC1/AC7: both players' culture was re-anchored at the battle instant.
        assert_eq!(
            repo.player_culture(defender).await.unwrap(),
            (111, battle_at)
        );
        assert_eq!(
            repo.player_culture(attacker).await.unwrap(),
            (222, battle_at)
        );

        // AC10: the report records the loyalty change + the capture, visible to both.
        let reports = repo.reports_for(attacker, 10).await.unwrap();
        let r = reports.first().expect("a report");
        assert!(r.conquered);
        assert_eq!((r.loyalty_before, r.loyalty_after), (Some(20), Some(0)));

        // AC12: re-applying the same conquest is rejected — the ownership guard prevents a double take.
        let err = repo.apply_battle(apply).await;
        assert!(
            matches!(err, Err(RepoError::Conflict)),
            "guarded once: {err:?}"
        );
    }

    // 013 AC4/AC6/AC7/AC8/AC12: a settle founds a new, independent village on a free valley with a free
    // slot; a settle whose target is taken in flight bounces the settlers home.
    #[sqlx::test(migrations = "../../migrations")]
    async fn settle_founds_a_village_then_bounces_when_taken(pool: PgPool) {
        let Setup {
            repo,
            econ,
            world,
            config,
            template,
        } = setup(pool.clone()).await;
        let units = crate::unit_rules().expect("unit rules");
        let crules = crate::culture_rules().expect("culture rules");
        let speed = GameSpeed::new(1.0).unwrap();

        let uname = format!("settle_{}", Uuid::new_v4().simple());
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
        let v1 = repo.villages_of(user.id).await.unwrap()[0].clone();
        let world_uuid = Uuid::from_u128(world.id.0);

        // A Residence (slot capacity) + ample culture points so the player may found a 2nd village.
        sqlx::query("INSERT INTO village_buildings (village_id, slot, building_type, level) VALUES ($1, 9, 'residence', 5) ON CONFLICT (village_id, slot) DO UPDATE SET building_type = EXCLUDED.building_type, level = EXCLUDED.level")
            .bind(Uuid::from_u128(v1.id.0))
            .execute(&pool)
            .await
            .unwrap();
        let now = Timestamp(3_000_000_000_000);
        repo.settle_culture(user.id, 100_000, now).await.unwrap();
        // Settlers in the home garrison (the settler group + spares for a second attempt).
        let settler = crules.settler_id.clone();
        sqlx::query("INSERT INTO village_units (village_id, unit_id, count) VALUES ($1, $2, $3)")
            .bind(Uuid::from_u128(v1.id.0))
            .bind(&settler)
            .bind(i32::try_from(crules.settlers_per_village * 2).unwrap())
            .execute(&pool)
            .await
            .unwrap();

        // Two free valleys (the dev DB is saturated, so check no village sits there).
        let map = WorldMap::new(
            world.seed as u64,
            config.radius,
            crate::map_rules().unwrap(),
        );
        let occupied: std::collections::HashSet<(i32, i32)> =
            sqlx::query_as::<_, (i32, i32)>("SELECT x, y FROM villages WHERE world_id = $1")
                .bind(world_uuid)
                .fetch_all(&pool)
                .await
                .unwrap()
                .into_iter()
                .collect();
        let valleys: Vec<Coordinate> = coordinates_within(config.radius)
            .filter(|c| {
                matches!(map.tile_at(*c), Some(TileKind::Valley(_)))
                    && *c != v1.coordinate
                    && !occupied.contains(&(c.x, c.y))
            })
            .take(2)
            .collect();
        assert_eq!(valleys.len(), 2, "need two free valleys");
        let (target, taken_target) = (valleys[0], valleys[1]);

        // AC6: order a settle at a free valley and resolve it — a new village is founded.
        eperica_application::order_settle(
            &repo, &repo, &repo, &repo, &econ, &units, &crules, &map, speed, now, user.id, None,
            target,
        )
        .await
        .expect("order settle");
        // The settler group left the garrison.
        let after_dispatch: i32 = sqlx::query_scalar(
            "SELECT count FROM village_units WHERE village_id = $1 AND unit_id = $2",
        )
        .bind(Uuid::from_u128(v1.id.0))
        .bind(&settler)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            after_dispatch,
            i32::try_from(crules.settlers_per_village).unwrap()
        );

        let arrive = Timestamp(now.0 + 100_000_000);
        eperica_application::process_due_settles(
            &repo, &repo, &repo, &crules, &units, &template, &map, speed, arrive, 100,
        )
        .await
        .expect("process settles");

        // AC6/AC8: the player now has a 2nd, independent village at the target with its own resources.
        let villages = repo.villages_of(user.id).await.unwrap();
        assert_eq!(villages.len(), 2, "a new village was founded");
        let founded = villages
            .iter()
            .find(|v| v.coordinate == target)
            .expect("founded village at the target");
        assert!(
            !founded.fields.is_empty(),
            "the new village has its own fields"
        );
        assert!(
            repo.stored_resources(founded.id).await.unwrap().is_some(),
            "the new village has its own resources"
        );

        // AC7: a settle whose target is taken in flight bounces the settlers home.
        eperica_application::order_settle(
            &repo,
            &repo,
            &repo,
            &repo,
            &econ,
            &units,
            &crules,
            &map,
            speed,
            now,
            user.id,
            None,
            taken_target,
        )
        .await
        .expect("order second settle");
        // Someone else founds on that tile before the settlers arrive.
        sqlx::query("INSERT INTO villages (id, world_id, owner_id, x, y, tribe) VALUES ($1, $2, $3, $4, $5, 'gauls')")
            .bind(Uuid::new_v4())
            .bind(world_uuid)
            .bind(Uuid::from_u128(user.id.0))
            .bind(taken_target.x)
            .bind(taken_target.y)
            .execute(&pool)
            .await
            .unwrap();
        let villages_before = repo.villages_of(user.id).await.unwrap().len();
        eperica_application::process_due_settles(
            &repo, &repo, &repo, &crules, &units, &template, &map, speed, arrive, 100,
        )
        .await
        .expect("process second settle");
        // No founding beyond the squatter; a survivor return is in flight carrying the settlers.
        assert_eq!(
            repo.villages_of(user.id).await.unwrap().len(),
            villages_before
        );
        let returns: Vec<_> = repo
            .active_movements(user.id)
            .await
            .unwrap()
            .into_iter()
            .filter(|m| m.kind == MovementKind::Return)
            .collect();
        assert!(
            returns.iter().any(|m| m
                .troops
                .iter()
                .any(|(u, n)| u.0 == settler && *n == crules.settlers_per_village)),
            "the settlers bounced home"
        );
    }

    /// 016 AC1/AC2/AC5/AC6/AC8/AC9/AC10/AC12: the ranking read paths — population / attack / defense /
    /// raider boards, alliance aggregates, player & alliance stat pages, the reinforcer inbox, and the
    /// quadrant scope — all derived from persisted facts.
    #[sqlx::test(migrations = "../../migrations")]
    async fn ranking_boards_and_stats(pool: PgPool) {
        let Setup {
            repo,
            econ,
            template,
            ..
        } = setup(pool.clone()).await;
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
        let (attacker, a) = account("rkatk").await;
        let (defender, d) = account("rkdef").await;
        let (ally, al) = account("rkally").await;

        // Seed one resolved battle directly: attacker scores 50 attack points and loots; the defender
        // (30) and the ally reinforcer (20) split the defense points.
        let battle_at = Timestamp(3_000_000_000_000);
        repo.apply_battle(BattleApply {
            movement_id: Uuid::new_v4().as_u128(),
            owner: attacker,
            attacker_home: a.id,
            attacker_origin: a.coordinate,
            target: d.id,
            target_coord: d.coordinate,
            defender_losses: Vec::new(),
            reinforcement_losses: Vec::new(),
            survivors: Vec::new(),
            battle_at,
            return_arrive: battle_at,
            report: NewBattleReport {
                kind: MovementKind::Raid,
                attacker_player: attacker,
                attacker_village: a.id,
                defender_player: defender,
                defender_village: d.id,
                attacker_won: true,
                luck: 1.0,
                morale: 1.0,
                wall_before: 0,
                wall_after: 0,
                attacker_forces: vec![(UnitId("swordsman".into()), 10)],
                attacker_losses: Vec::new(),
                defender_forces: Vec::new(),
                defender_losses: Vec::new(),
                loot: ResourceAmounts {
                    wood: 100,
                    clay: 50,
                    iron: 0,
                    crop: 0,
                },
                razed: None,
                loyalty_before: None,
                loyalty_after: None,
                conquered: false,
            },
            scouted: false,
            scout_target: None,
            scout_report: None,
            loot: ResourceAmounts {
                wood: 100,
                clay: 50,
                iron: 0,
                crop: 0,
            },
            target_debit: None,
            razed: None,
            loyalty: None,
            attack_points: 50,
            defender_contributions: vec![
                DefenderContribution {
                    player: defender,
                    village: d.id,
                    is_owner: true,
                    forces: Vec::new(),
                    losses: Vec::new(),
                    defense_value: 30,
                    defense_points: 30,
                },
                DefenderContribution {
                    player: ally,
                    village: al.id,
                    is_owner: false,
                    forces: Vec::new(),
                    losses: Vec::new(),
                    defense_value: 20,
                    defense_points: 20,
                },
            ],
            artifact_capture: None,
            plan_capture: None,
        })
        .await
        .expect("seed battle");

        // AC1/AC2: population board lists all three (equal starting pop > 0), bounded + ranked.
        let pop = repo
            .population_board(&econ, BoardScope::World, 100)
            .await
            .unwrap();
        assert!(pop.len() >= 3);
        assert!(pop.iter().all(|r| r.value > 0));
        assert!(pop.iter().any(|r| r.player == attacker));

        // AC5: attack board — the attacker has 50; the defender (zero) is omitted.
        let atk = repo
            .conflict_board(ConflictMetric::Attack, BoardScope::World, None, None, 100)
            .await
            .unwrap();
        assert_eq!(
            atk.iter().find(|r| r.player == attacker).map(|r| r.value),
            Some(50)
        );
        assert!(atk.iter().all(|r| r.player != defender)); // omitted (zero activity)

        // AC5: defense board — defender 30, ally 20 (the split).
        let def = repo
            .conflict_board(ConflictMetric::Defense, BoardScope::World, None, None, 100)
            .await
            .unwrap();
        assert_eq!(
            def.iter().find(|r| r.player == defender).map(|r| r.value),
            Some(30)
        );
        assert_eq!(
            def.iter().find(|r| r.player == ally).map(|r| r.value),
            Some(20)
        );

        // AC6: raider board — the attacker's looted 150 (100 + 50).
        let raid = repo
            .conflict_board(ConflictMetric::Raided, BoardScope::World, None, None, 100)
            .await
            .unwrap();
        assert_eq!(
            raid.iter().find(|r| r.player == attacker).map(|r| r.value),
            Some(150)
        );

        // AC9: player stats — public metrics, never empty population.
        let s = repo.player_stats(&econ, attacker).await.unwrap().unwrap();
        assert_eq!(s.attack_points, 50);
        assert_eq!(s.loot_total, 150);
        assert!(s.population > 0 && !s.villages.is_empty());
        let sd = repo.player_stats(&econ, defender).await.unwrap().unwrap();
        assert_eq!(sd.defense_points, 30);
        assert!(
            repo.player_stats(&econ, PlayerId(0))
                .await
                .unwrap()
                .is_none()
        );

        // AC3/AC12: the ally reinforcer reads their own defender report (20 points, not the owner).
        let inbox = repo.defender_reports_for(ally, 100).await.unwrap();
        assert_eq!(inbox.len(), 1);
        assert!(!inbox[0].is_owner);
        assert_eq!(inbox[0].defense_points, 20);

        // AC8/AC10: an alliance of {attacker, defender} aggregates their stats.
        let aid = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO alliances (id, name, tag, founder_id) VALUES ($1, 'Rankers', 'RK', $2)",
        )
        .bind(aid)
        .bind(Uuid::from_u128(attacker.0))
        .execute(&pool)
        .await
        .unwrap();
        for p in [attacker, defender] {
            sqlx::query(
                "INSERT INTO alliance_members (player_id, alliance_id, role, rights) \
                 VALUES ($1, $2, 'member', 0)",
            )
            .bind(Uuid::from_u128(p.0))
            .bind(aid)
            .execute(&pool)
            .await
            .unwrap();
        }
        let apop = repo
            .alliance_population_board(&econ, BoardScope::World, 100)
            .await
            .unwrap();
        assert!(apop.iter().any(|r| r.tag == "RK" && r.value > 0));
        let aatk = repo
            .alliance_conflict_board(ConflictMetric::Attack, BoardScope::World, None, None, 100)
            .await
            .unwrap();
        assert_eq!(
            aatk.iter().find(|r| r.tag == "RK").map(|r| r.value),
            Some(50)
        );
        let ast = repo
            .alliance_stats(&econ, AllianceId(aid.as_u128()))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(ast.attack_points, 50);
        assert_eq!(ast.defense_points, 30); // the defender is a member; the ally is not
        assert_eq!(ast.members.len(), 2);

        // AC7: with the attacker's village flagged capital, a quadrant-scoped board includes them.
        sqlx::query("UPDATE villages SET is_capital = true WHERE id = $1")
            .bind(Uuid::from_u128(a.id.0))
            .execute(&pool)
            .await
            .unwrap();
        let q = eperica_domain::quadrant(a.coordinate);
        let qpop = repo
            .population_board(&econ, BoardScope::Quadrant(q), 100)
            .await
            .unwrap();
        assert!(qpop.iter().any(|r| r.player == attacker));

        // AC7 exclusion: move the defender's capital to the SW quadrant — an NE-scoped board excludes
        // them, a SW-scoped board includes them.
        sqlx::query("UPDATE villages SET x = -5, y = -5, is_capital = true WHERE id = $1")
            .bind(Uuid::from_u128(d.id.0))
            .execute(&pool)
            .await
            .unwrap();
        let ne = repo
            .population_board(&econ, BoardScope::Quadrant(Quadrant::Ne), 100)
            .await
            .unwrap();
        assert!(
            ne.iter().all(|r| r.player != defender),
            "an SW-capital player is excluded from the NE board"
        );
        let sw = repo
            .population_board(&econ, BoardScope::Quadrant(Quadrant::Sw), 100)
            .await
            .unwrap();
        assert!(sw.iter().any(|r| r.player == defender));

        // AC5: a rolling window that predates the battle excludes it; all-time still includes it.
        for t in ["battle_reports", "battle_defenders"] {
            sqlx::query(&format!(
                "UPDATE {t} SET occurred_at = now() - interval '40 days'"
            ))
            .execute(&pool)
            .await
            .unwrap();
        }
        let since_30d = Some(Timestamp(crate::now().0 - 30 * 86_400 * 1000));
        let recent = repo
            .conflict_board(
                ConflictMetric::Attack,
                BoardScope::World,
                since_30d,
                None,
                100,
            )
            .await
            .unwrap();
        assert!(
            recent.iter().all(|r| r.player != attacker),
            "a 40-day-old battle is outside the 30-day window"
        );
        let all_time = repo
            .conflict_board(ConflictMetric::Attack, BoardScope::World, None, None, 100)
            .await
            .unwrap();
        assert_eq!(
            all_time
                .iter()
                .find(|r| r.player == attacker)
                .map(|r| r.value),
            Some(50),
            "all-time still includes the old battle"
        );
    }

    /// 016 AC4: defence points from a real `resolve_one` are split across the owner and a reinforcer
    /// by contributed defensive value, summing to the valued attacker losses.
    #[sqlx::test(migrations = "../../migrations")]
    async fn defense_points_split_across_reinforcers(pool: PgPool) {
        let Setup {
            repo,
            econ,
            template,
            world,
            config,
        } = setup(pool.clone()).await;
        let units = crate::unit_rules().expect("unit rules");
        let combat = crate::combat_rules().expect("combat rules");
        let scout = crate::scout_rules().expect("scout rules");
        let map = WorldMap::new(
            world.seed as u64,
            config.radius,
            crate::map_rules().unwrap(),
        );
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
        let (attacker, a) = account("dsatk").await;
        let (_defender, d) = account("dsdef").await;
        let (_ally, al) = account("dsally").await;

        // A weak attacker (5 swordsmen) is wiped by a strong defence: the owner's 50 phalanx and the
        // ally's 25 reinforcing phalanx (a 2:1 defence split).
        for (v, unit, n) in [(a.id, "swordsman", 5), (d.id, "phalanx", 50)] {
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
        sqlx::query(
            "INSERT INTO reinforcements (host_village, home_village, unit_id, count) \
             VALUES ($1, $2, 'phalanx', 25)",
        )
        .bind(Uuid::from_u128(d.id.0))
        .bind(Uuid::from_u128(al.id.0))
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
            MovementKind::Attack,
            &[(UnitId("swordsman".into()), 5)],
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
            &crate::culture_rules().unwrap(),
            &crate::loyalty_rules().unwrap(),
            &crate::ranking_rules().unwrap(),
            &map,
            GameSpeed::new(1.0).unwrap(),
            world.seed as u64,
            arrive,
            100,
            (3, 6, 10),
        )
        .await
        .expect("resolve");

        // The 5 swordsmen (point value 1 each) are destroyed → 5 defence points, split owner ≥ ally
        // (the owner brought 2× the defence) and summing exactly to 5.
        let defs: Vec<(bool, i64)> = sqlx::query_as(
            "SELECT is_owner, defense_points FROM battle_defenders ORDER BY is_owner DESC",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(defs.len(), 2, "owner + reinforcer each get a row");
        assert_eq!(
            defs.iter().map(|(_, p)| p).sum::<i64>(),
            5,
            "defence points sum to the valued attacker losses"
        );
        assert!(
            defs[0].1 > 0 && defs[1].1 > 0,
            "both defenders earned points"
        );
        assert!(defs[0].1 >= defs[1].1, "the owner contributed more defence");
    }

    /// 017 AC2/AC5/AC6: population snapshots and medal awards persist and are idempotent per period.
    #[sqlx::test(migrations = "../../migrations")]
    async fn medal_snapshot_and_award_persistence(pool: PgPool) {
        let Setup {
            repo,
            econ,
            template,
            ..
        } = setup(pool.clone()).await;
        let account = async |tag: &str| {
            let uname = format!("{tag}_{}", Uuid::new_v4().simple());
            repo.create_account(
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
            .expect("create account")
            .id
        };
        let a = account("msa").await;
        let b = account("msb").await;

        // AC2: one snapshot per player for period 0; re-running writes none (idempotent).
        assert_eq!(repo.latest_settled_period().await.unwrap(), None);
        repo.snapshot_population(&econ, 0).await.unwrap();
        repo.snapshot_population(&econ, 0).await.unwrap();
        let snap_count: i64 = sqlx::query_scalar("SELECT count(*) FROM population_snapshots")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(snap_count, 2);
        assert_eq!(repo.latest_settled_period().await.unwrap(), Some(0));
        let hist = repo.population_history(a).await.unwrap();
        assert_eq!(hist.len(), 1);
        assert_eq!(hist[0].0, 0);
        assert!(hist[0].1 > 0, "starting village has population");

        // AC3/AC5/AC6: award two attacker medals; re-running the same awards is a no-op.
        let awards = vec![
            MedalAward {
                category: MedalCategory::Attacker,
                rank: 1,
                subject_kind: MedalSubjectKind::Player,
                subject_id: a.0,
            },
            MedalAward {
                category: MedalCategory::Attacker,
                rank: 2,
                subject_kind: MedalSubjectKind::Player,
                subject_id: b.0,
            },
        ];
        repo.award_medals(0, &awards).await.unwrap();
        repo.award_medals(0, &awards).await.unwrap();
        let medal_count: i64 = sqlx::query_scalar("SELECT count(*) FROM medals")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            medal_count, 2,
            "per-period (category, rank) uniqueness holds"
        );
        let a_medals = repo
            .medals_for(MedalSubjectKind::Player, a.0)
            .await
            .unwrap();
        assert_eq!(a_medals.len(), 1);
        assert_eq!(a_medals[0].category, MedalCategory::Attacker);
        assert_eq!(a_medals[0].rank, 1);
        assert_eq!(a_medals[0].period, 0);

        // AC4: climber board over [0,1] — grow player a's snapshot, then the delta ranks them.
        repo.snapshot_population(&econ, 1).await.unwrap();
        sqlx::query("UPDATE population_snapshots SET population = population + 500 WHERE player_id = $1 AND period = 1")
            .bind(Uuid::from_u128(a.0))
            .execute(&pool)
            .await
            .unwrap();
        let climbers = repo
            .climber_board(1, 0, BoardScope::World, 100)
            .await
            .unwrap();
        assert_eq!(climbers.len(), 1, "only the grower is a positive climber");
        assert_eq!(climbers[0].player, a);
        assert_eq!(climbers[0].value, 500);
    }

    /// 017 AC1/AC3/AC6: the weekly settlement settles each complete period (snapshot + award the
    /// period's attacker medal from battles in that window) and is idempotent.
    #[sqlx::test(migrations = "../../migrations")]
    async fn weekly_settlement_snapshots_and_awards(pool: PgPool) {
        let Setup {
            repo,
            econ,
            template,
            ..
        } = setup(pool.clone()).await;
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
        let (attacker, a) = account("wsatk").await;
        let (defender, d) = account("wsdef").await;

        // Seed a battle giving the attacker 50 attack points, then date it into period 0's window.
        let battle_at = Timestamp(4_000_000_000_000);
        repo.apply_battle(BattleApply {
            movement_id: Uuid::new_v4().as_u128(),
            owner: attacker,
            attacker_home: a.id,
            attacker_origin: a.coordinate,
            target: d.id,
            target_coord: d.coordinate,
            defender_losses: Vec::new(),
            reinforcement_losses: Vec::new(),
            survivors: Vec::new(),
            battle_at,
            return_arrive: battle_at,
            report: NewBattleReport {
                kind: MovementKind::Raid,
                attacker_player: attacker,
                attacker_village: a.id,
                defender_player: defender,
                defender_village: d.id,
                attacker_won: true,
                luck: 1.0,
                morale: 1.0,
                wall_before: 0,
                wall_after: 0,
                attacker_forces: Vec::new(),
                attacker_losses: Vec::new(),
                defender_forces: Vec::new(),
                defender_losses: Vec::new(),
                loot: ResourceAmounts::default(),
                razed: None,
                loyalty_before: None,
                loyalty_after: None,
                conquered: false,
            },
            scouted: false,
            scout_target: None,
            scout_report: None,
            loot: ResourceAmounts::default(),
            target_debit: None,
            razed: None,
            loyalty: None,
            attack_points: 50,
            defender_contributions: Vec::new(),
            artifact_capture: None,
            plan_capture: None,
        })
        .await
        .expect("seed battle");

        // A short real-time period; the battle sits inside period 0; `now` is in period 2 → settle 0 and 1.
        let world_start = Timestamp(4_000_000_000_000 - 50_000); // period 0 starts 50s before the battle
        let rules = eperica_domain::MedalRules {
            period_secs: 100,
            per_category: 3,
            categories: vec![
                eperica_domain::MedalCategory::Attacker,
                eperica_domain::MedalCategory::Climber,
            ],
        };
        sqlx::query(
            "UPDATE battle_reports SET occurred_at = to_timestamp($1::double precision / 1000.0)",
        )
        .bind(battle_at.0 as f64)
        .execute(&pool)
        .await
        .unwrap();
        let now = Timestamp(world_start.0 + 250_000); // 2.5 periods elapsed

        let settled = eperica_application::process_due_medal_settlement(
            &repo,
            &econ,
            &rules,
            world_start,
            now,
        )
        .await
        .expect("settle");
        assert_eq!(settled, vec![0, 1], "periods 0 and 1 are complete");

        // AC2: a snapshot per player per settled period (2 players × 2 periods).
        let snaps: i64 = sqlx::query_scalar("SELECT count(*) FROM population_snapshots")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(snaps, 4);

        // AC3: the attacker wins period 0's attacker medal (rank 1); period 1 has no battles → none.
        let medals = repo
            .medals_for(MedalSubjectKind::Player, attacker.0)
            .await
            .unwrap();
        assert_eq!(medals.len(), 1);
        assert_eq!(medals[0].category, MedalCategory::Attacker);
        assert_eq!(medals[0].period, 0);
        assert_eq!(medals[0].rank, 1);

        // AC6: re-running settles nothing more and adds no duplicate medals/snapshots.
        let again = eperica_application::process_due_medal_settlement(
            &repo,
            &econ,
            &rules,
            world_start,
            now,
        )
        .await
        .expect("settle again");
        assert!(again.is_empty());
        let medals2: i64 = sqlx::query_scalar("SELECT count(*) FROM medals")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(medals2, 1);
    }

    /// 017 AC8/AC9/AC10: achievements grant once at the milestone with their reward; re-evaluation is a
    /// no-op; the resource reward credits the capital (capped) exactly once.
    #[sqlx::test(migrations = "../../migrations")]
    async fn achievement_grant_and_rewards(pool: PgPool) {
        let Setup {
            repo,
            econ,
            template,
            world,
            ..
        } = setup(pool.clone()).await;
        let units = crate::unit_rules().expect("unit rules");
        let catalogue = crate::achievement_catalogue().expect("catalogue");
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
        let (player, v) = account("ach").await;

        // AC10: a 2nd village ⇒ the `second_village` achievement, with its 50 CP reward (AC9).
        sqlx::query("INSERT INTO villages (id, world_id, owner_id, x, y, tribe) VALUES ($1, $2, $3, 99, 99, 'gauls')")
            .bind(Uuid::new_v4())
            .bind(Uuid::from_u128(world.id.0))
            .bind(Uuid::from_u128(player.0))
            .execute(&pool)
            .await
            .unwrap();
        let granted =
            eperica_application::evaluate_achievements(&repo, &econ, &units, &catalogue, player)
                .await
                .unwrap();
        assert!(granted.iter().any(|id| id.0 == "second_village"));
        let cp: i64 = sqlx::query_scalar("SELECT value FROM player_culture WHERE player_id = $1")
            .bind(Uuid::from_u128(player.0))
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(cp, 50, "the second-village CP reward applied once");

        // AC8: re-evaluation grants nothing new and does not re-apply the reward.
        let again =
            eperica_application::evaluate_achievements(&repo, &econ, &units, &catalogue, player)
                .await
                .unwrap();
        assert!(!again.iter().any(|id| id.0 == "second_village"));
        let cp2: i64 = sqlx::query_scalar("SELECT value FROM player_culture WHERE player_id = $1")
            .bind(Uuid::from_u128(player.0))
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(cp2, 50);

        // AC10: occupying an oasis ⇒ `first_oasis`.
        sqlx::query("INSERT INTO oases (world_id, x, y, owner_village) VALUES ($1, 5, 5, $2)")
            .bind(Uuid::from_u128(world.id.0))
            .bind(Uuid::from_u128(v.id.0))
            .execute(&pool)
            .await
            .unwrap();
        let g2 =
            eperica_application::evaluate_achievements(&repo, &econ, &units, &catalogue, player)
                .await
                .unwrap();
        assert!(g2.iter().any(|id| id.0 == "first_oasis"));

        // AC9: a resource reward credits the capital (capped), exactly once.
        let before: i64 =
            sqlx::query_scalar("SELECT wood FROM village_resources WHERE village_id = $1")
                .bind(Uuid::from_u128(v.id.0))
                .fetch_one(&pool)
                .await
                .unwrap();
        let res_def = AchievementDef {
            id: AchievementId("res_test".into()),
            kind: eperica_domain::AchievementKind::Population,
            threshold: 0,
            reward: Reward::Resources(ResourceAmounts {
                wood: 100,
                clay: 0,
                iron: 0,
                crop: 0,
            }),
        };
        assert!(
            repo.grant_achievement(&econ, player, &res_def)
                .await
                .unwrap()
        );
        let after: i64 =
            sqlx::query_scalar("SELECT wood FROM village_resources WHERE village_id = $1")
                .bind(Uuid::from_u128(v.id.0))
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(after > before, "the resource reward credited the capital");
        // Re-granting is a no-op (already held) — no further credit.
        assert!(
            !repo
                .grant_achievement(&econ, player, &res_def)
                .await
                .unwrap()
        );
        let after2: i64 =
            sqlx::query_scalar("SELECT wood FROM village_resources WHERE village_id = $1")
                .bind(Uuid::from_u128(v.id.0))
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(after2, after, "no double reward");
    }

    /// 018 AC3/AC4: `quest_progress` reflects persisted state (garrison, raid, field level,
    /// population); `complete_quest` applies its reward (resources capped to the capital, culture,
    /// troops to the garrison) exactly once per `(player, quest)`.
    #[sqlx::test(migrations = "../../migrations")]
    async fn quest_progress_and_completion(pool: PgPool) {
        let Setup {
            repo,
            econ,
            template,
            ..
        } = setup(pool.clone()).await;
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
        let (player, v) = account("quest").await;

        // A fresh village: no troops, no raids, but a starting population.
        let p0 = repo.quest_progress(&econ, player).await.unwrap();
        assert!(!p0.has_troops, "a new village has no garrison");
        assert!(!p0.has_raided, "a new player has launched no raid");
        assert!(p0.population > 0, "the starting village has population");

        // Garrison a unit ⇒ has_troops; an upgraded field raises max_field_level.
        sqlx::query(
            "INSERT INTO village_units (village_id, unit_id, count) VALUES ($1, 'phalanx', 5)",
        )
        .bind(Uuid::from_u128(v.id.0))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("UPDATE village_fields SET level = 3 WHERE village_id = $1 AND slot = 0")
            .bind(Uuid::from_u128(v.id.0))
            .execute(&pool)
            .await
            .unwrap();
        let p1 = repo.quest_progress(&econ, player).await.unwrap();
        assert!(p1.has_troops);
        assert!(p1.max_field_level >= 3);

        // A launched raid ⇒ has_raided.
        let (foe, foe_v) = account("foe").await;
        sqlx::query(
            "INSERT INTO battle_reports (id, kind, attacker_player, attacker_village, \
             defender_player, defender_village, attacker_won, luck, morale, wall_before, \
             wall_after, attacker_forces, attacker_losses, defender_forces, defender_losses) \
             VALUES ($1,'raid',$2,$3,$4,$5,true,0,0,0,0,'{}','{}','{}','{}')",
        )
        .bind(Uuid::new_v4())
        .bind(Uuid::from_u128(player.0))
        .bind(Uuid::from_u128(v.id.0))
        .bind(Uuid::from_u128(foe.0))
        .bind(Uuid::from_u128(foe_v.id.0))
        .execute(&pool)
        .await
        .unwrap();
        assert!(repo.quest_progress(&econ, player).await.unwrap().has_raided);

        // Complete a quest whose reward covers all three kinds; each applies exactly once.
        let wood_before: i64 =
            sqlx::query_scalar("SELECT wood FROM village_resources WHERE village_id = $1")
                .bind(Uuid::from_u128(v.id.0))
                .fetch_one(&pool)
                .await
                .unwrap();
        let def = QuestDef {
            id: QuestId("q_test".into()),
            description: "test".into(),
            condition: eperica_domain::QuestCondition::SendRaid,
            reward: eperica_domain::QuestReward {
                resources: ResourceAmounts {
                    wood: 50,
                    clay: 0,
                    iron: 0,
                    crop: 0,
                },
                culture: 30,
                troops: Some((UnitId("phalanx".into()), 2)),
            },
        };
        assert!(repo.complete_quest(&econ, player, &def).await.unwrap());
        let cp: i64 = sqlx::query_scalar("SELECT value FROM player_culture WHERE player_id = $1")
            .bind(Uuid::from_u128(player.0))
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(cp, 30, "the culture reward applied");
        let wood_after: i64 =
            sqlx::query_scalar("SELECT wood FROM village_resources WHERE village_id = $1")
                .bind(Uuid::from_u128(v.id.0))
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(
            wood_after > wood_before,
            "the resource reward credited the capital"
        );
        let troops: i32 = sqlx::query_scalar(
            "SELECT count FROM village_units WHERE village_id = $1 AND unit_id = 'phalanx'",
        )
        .bind(Uuid::from_u128(v.id.0))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(troops, 7, "the troop reward joined the garrison (5 + 2)");

        let completed = repo.completed_quests(player).await.unwrap();
        assert!(completed.contains(&QuestId("q_test".into())));

        // Re-completion is a no-op — no double reward.
        assert!(!repo.complete_quest(&econ, player, &def).await.unwrap());
        let cp2: i64 = sqlx::query_scalar("SELECT value FROM player_culture WHERE player_id = $1")
            .bind(Uuid::from_u128(player.0))
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(cp2, 30, "culture not re-credited");
        let troops2: i32 = sqlx::query_scalar(
            "SELECT count FROM village_units WHERE village_id = $1 AND unit_id = 'phalanx'",
        )
        .bind(Uuid::from_u128(v.id.0))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(troops2, 7, "troops not re-added");
    }

    /// 018 AC5/AC6: `evaluate_quests` over the seed chain completes each quest only at its
    /// triggering action (the stage-gate holds the rest), a resumable prefix completes in order in
    /// one pass, and the finished chain short-circuits to nothing.
    #[sqlx::test(migrations = "../../migrations")]
    async fn evaluate_quests_gates_and_cascades(pool: PgPool) {
        let Setup {
            repo,
            econ,
            template,
            ..
        } = setup(pool.clone()).await;
        let chain = crate::quest_chain().expect("quest chain");
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
        let run = async |player| {
            eperica_application::evaluate_quests(&repo, &econ, &chain, player)
                .await
                .unwrap()
                .into_iter()
                .map(|q| q.0)
                .collect::<Vec<_>>()
        };

        let (player, v) = account("q").await;
        let vid = Uuid::from_u128(v.id.0);

        // Fresh village (fields lvl 0, no warehouse, no troops, no raid, pop 3): nothing is met.
        assert!(run(player).await.is_empty());

        // Each quest completes only at its action — the gate holds the rest from completing early.
        sqlx::query("UPDATE village_fields SET level = 2 WHERE village_id = $1 AND slot = 0")
            .bind(vid)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(run(player).await.as_slice(), ["upgrade_field"]);

        sqlx::query(
            "INSERT INTO village_buildings (village_id, slot, building_type, level) \
             VALUES ($1, 20, 'warehouse', 1)",
        )
        .bind(vid)
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(run(player).await.as_slice(), ["build_warehouse"]);

        sqlx::query(
            "INSERT INTO village_units (village_id, unit_id, count) VALUES ($1, 'phalanx', 1)",
        )
        .bind(vid)
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(run(player).await.as_slice(), ["train_troops"]);
        let cp: i64 = sqlx::query_scalar("SELECT value FROM player_culture WHERE player_id = $1")
            .bind(Uuid::from_u128(player.0))
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(cp, 50, "train_troops' 50 CP reward landed");

        let (foe, foe_v) = account("foe").await;
        sqlx::query(
            "INSERT INTO battle_reports (id, kind, attacker_player, attacker_village, \
             defender_player, defender_village, attacker_won, luck, morale, wall_before, \
             wall_after, attacker_forces, attacker_losses, defender_forces, defender_losses) \
             VALUES ($1,'raid',$2,$3,$4,$5,true,0,0,0,0,'{}','{}','{}','{}')",
        )
        .bind(Uuid::new_v4())
        .bind(Uuid::from_u128(player.0))
        .bind(vid)
        .bind(Uuid::from_u128(foe.0))
        .bind(Uuid::from_u128(foe_v.id.0))
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(run(player).await.as_slice(), ["send_raid"]);

        // Raise all fields to push population past the 50 threshold (lvl 10 ⇒ 11 pop each).
        sqlx::query("UPDATE village_fields SET level = 10 WHERE village_id = $1")
            .bind(vid)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(run(player).await.as_slice(), ["grow_population"]);
        let cp2: i64 = sqlx::query_scalar("SELECT value FROM player_culture WHERE player_id = $1")
            .bind(Uuid::from_u128(player.0))
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(cp2, 150, "grow_population added its 100 CP (50 + 100)");

        // The finished chain short-circuits to nothing.
        assert!(run(player).await.is_empty());

        // AC6: a resumable prefix completes in order in one pass, stopping at the first unmet quest.
        let (p2, v2) = account("p2").await;
        let v2id = Uuid::from_u128(v2.id.0);
        sqlx::query("UPDATE village_fields SET level = 2 WHERE village_id = $1 AND slot = 0")
            .bind(v2id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO village_buildings (village_id, slot, building_type, level) \
             VALUES ($1, 20, 'warehouse', 1)",
        )
        .bind(v2id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO village_units (village_id, unit_id, count) VALUES ($1, 'phalanx', 1)",
        )
        .bind(v2id)
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(
            run(p2).await.as_slice(),
            ["upgrade_field", "build_warehouse", "train_troops"],
            "the satisfied prefix completes in order, then stops at the unmet send_raid",
        );
    }

    /// 017 AC10: the count-based predicates — `defensive_wins` (100 lost-defence battles) and
    /// `research_all_units` (every researchable unit of the tribe) — grant at their crossing.
    #[sqlx::test(migrations = "../../migrations")]
    async fn achievement_count_predicates(pool: PgPool) {
        let Setup {
            repo,
            econ,
            template,
            ..
        } = setup(pool.clone()).await;
        let units = crate::unit_rules().expect("unit rules");
        let catalogue = crate::achievement_catalogue().expect("catalogue");
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
        let (defender, dv) = account("cpdef").await;
        let (attacker, av) = account("cpatk").await;

        // Before crossing: neither count achievement is met.
        let pre =
            eperica_application::evaluate_achievements(&repo, &econ, &units, &catalogue, defender)
                .await
                .unwrap();
        assert!(!pre.iter().any(|id| id.0 == "defender_100"));

        // 100 battles the defender defended and the attacker lost (a defender_100 worth of wins).
        sqlx::query(
            "WITH ins AS ( \
               INSERT INTO battle_reports \
                 (id, kind, attacker_player, attacker_village, defender_player, defender_village, \
                  attacker_won, luck, morale, wall_before, wall_after, \
                  attacker_forces, attacker_losses, defender_forces, defender_losses) \
               SELECT gen_random_uuid(), 'attack', $1, $2, $3, $4, false, 1, 1, 0, 0, \
                      '[]'::jsonb, '[]'::jsonb, '[]'::jsonb, '[]'::jsonb \
               FROM generate_series(1, 100) RETURNING id) \
             INSERT INTO battle_defenders \
               (id, battle_id, player_id, village_id, is_owner, forces, losses, defense_value, defense_points) \
             SELECT gen_random_uuid(), ins.id, $3, $4, true, '[]'::jsonb, '[]'::jsonb, 0, 0 FROM ins",
        )
        .bind(Uuid::from_u128(attacker.0))
        .bind(Uuid::from_u128(av.id.0))
        .bind(Uuid::from_u128(defender.0))
        .bind(Uuid::from_u128(dv.id.0))
        .execute(&pool)
        .await
        .unwrap();

        // Research every **researchable** gaul unit (the tier-1 phalanx is researched by default).
        for unit in [
            "swordsman",
            "pathfinder",
            "theutates_thunder",
            "druidrider",
            "haeduan",
            "ram",
            "trebuchet",
            "chieftain",
            "settler",
        ] {
            sqlx::query("INSERT INTO village_research (village_id, unit_id) VALUES ($1, $2)")
                .bind(Uuid::from_u128(dv.id.0))
                .bind(unit)
                .execute(&pool)
                .await
                .unwrap();
        }

        let granted =
            eperica_application::evaluate_achievements(&repo, &econ, &units, &catalogue, defender)
                .await
                .unwrap();
        assert!(
            granted.iter().any(|id| id.0 == "defender_100"),
            "100 defensive wins granted defender_100"
        );
        assert!(
            granted.iter().any(|id| id.0 == "research_all_units"),
            "all researchable units granted research_all_units"
        );
    }

    /// 017 AC4: the settlement awards a **climber** medal — a player whose population grows between two
    /// periods tops the climber category (exercises the in-transaction climber computation).
    #[sqlx::test(migrations = "../../migrations")]
    async fn settlement_awards_climber_medal(pool: PgPool) {
        let Setup {
            repo,
            econ,
            template,
            ..
        } = setup(pool.clone()).await;
        let uname = format!("clm_{}", Uuid::new_v4().simple());
        let player = repo
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
            .expect("create account")
            .id;
        let v = repo.villages_of(player).await.unwrap()[0].clone();
        let rules = eperica_domain::MedalRules {
            period_secs: 100,
            per_category: 3,
            categories: vec![eperica_domain::MedalCategory::Climber],
        };
        let world_start = Timestamp(5_000_000_000_000);

        // Settle period 0 (baseline snapshot) — no climber medal (no prior snapshot).
        let s0 = eperica_application::process_due_medal_settlement(
            &repo,
            &econ,
            &rules,
            world_start,
            Timestamp(world_start.0 + 150_000),
        )
        .await
        .unwrap();
        assert_eq!(s0, vec![0]);
        assert!(
            repo.medals_for(MedalSubjectKind::Player, player.0)
                .await
                .unwrap()
                .is_empty()
        );

        // Grow the player's population, then settle period 1 → they top the climber category.
        sqlx::query("UPDATE village_fields SET level = level + 1 WHERE village_id = $1")
            .bind(Uuid::from_u128(v.id.0))
            .execute(&pool)
            .await
            .unwrap();
        let s1 = eperica_application::process_due_medal_settlement(
            &repo,
            &econ,
            &rules,
            world_start,
            Timestamp(world_start.0 + 250_000),
        )
        .await
        .unwrap();
        assert_eq!(s1, vec![1]);
        let medals = repo
            .medals_for(MedalSubjectKind::Player, player.0)
            .await
            .unwrap();
        assert_eq!(medals.len(), 1);
        assert_eq!(medals[0].category, MedalCategory::Climber);
        assert_eq!(medals[0].period, 1);
        assert_eq!(medals[0].rank, 1);
    }
}
