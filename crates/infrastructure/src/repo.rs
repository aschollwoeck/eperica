//! PostgreSQL adapter for the application's [`AccountRepository`] port.

use async_trait::async_trait;
use eperica_application::{AccountRepository, NewUser, RepoError, UserRecord};
use eperica_domain::{
    BuildingKind, BuildingSlot, Coordinate, PlayerId, ResourceAmounts, ResourceField, ResourceKind,
    StartingVillage, Timestamp, Tribe, Village, VillageId, WorldId, coordinates_within,
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
        let village_rows = sqlx::query("SELECT id, x, y, tribe FROM villages WHERE owner_id = $1")
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
}
