//! PostgreSQL event store and the background scheduler (P1 due-event engine).

use crate::repo::PgAccountRepository;
use async_trait::async_trait;
use eperica_application::{
    DueEvent, EventStore, RepoError, process_due, process_due_builds, process_due_combat,
    process_due_movements, process_due_oasis_combat, process_due_oasis_regrow,
    process_due_oasis_reinforce, process_due_scouts, process_due_starvation, process_due_trades,
    process_due_training, process_due_unit_orders, sync_starvation_checks,
};
use eperica_domain::{
    CombatRules, CultureRules, EconomyRules, EventKind, GameSpeed, MerchantRules, OasisRules,
    ScoutRules, Timestamp, UnitRules, WorldMap,
};
use sqlx::{PgPool, Row};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// The current wall-clock time as a domain [`Timestamp`] (Unix-ms, UTC).
pub fn now() -> Timestamp {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0);
    Timestamp(ms)
}

fn backend(e: sqlx::Error) -> RepoError {
    RepoError::Backend(e.to_string())
}

fn kind_str(kind: EventKind) -> &'static str {
    match kind {
        EventKind::Heartbeat => "heartbeat",
    }
}

fn parse_kind(s: &str) -> Result<EventKind, RepoError> {
    match s {
        "heartbeat" => Ok(EventKind::Heartbeat),
        other => Err(RepoError::Backend(format!("unknown event kind: {other}"))),
    }
}

/// SQLx-backed [`EventStore`].
#[derive(Debug, Clone)]
pub struct PgEventStore {
    pool: PgPool,
}

impl PgEventStore {
    /// Create an event store over the given pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Reset events stuck in `processing` (e.g. left by a crashed worker) back to `pending` so they
    /// are reprocessed. Returns how many were requeued. (Once real, effectful events exist, handlers
    /// must be idempotent or processed within a single transaction — see docs/architecture/0002.)
    pub async fn requeue_orphaned(&self) -> Result<u64, RepoError> {
        let result = sqlx::query(
            "UPDATE scheduled_events SET status = 'pending' WHERE status = 'processing'",
        )
        .execute(&self.pool)
        .await
        .map_err(backend)?;
        Ok(result.rows_affected())
    }
}

#[async_trait]
impl EventStore for PgEventStore {
    async fn schedule(&self, kind: EventKind, due_at: Timestamp) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT INTO scheduled_events (id, kind, due_at, status) \
             VALUES ($1, $2, to_timestamp($3::double precision / 1000.0), 'pending')",
        )
        .bind(Uuid::new_v4())
        .bind(kind_str(kind))
        .bind(due_at.0 as f64)
        .execute(&self.pool)
        .await
        .map_err(backend)?;
        Ok(())
    }

    async fn claim_due(&self, now: Timestamp, limit: i64) -> Result<Vec<DueEvent>, RepoError> {
        let rows = sqlx::query(
            "UPDATE scheduled_events SET status = 'processing' \
             WHERE id IN ( \
                 SELECT id FROM scheduled_events \
                 WHERE status = 'pending' AND due_at <= to_timestamp($1::double precision / 1000.0) \
                 ORDER BY due_at, seq \
                 LIMIT $2 \
                 FOR UPDATE SKIP LOCKED \
             ) \
             RETURNING id, kind, (EXTRACT(EPOCH FROM due_at) * 1000)::bigint AS due_ms",
        )
        .bind(now.0 as f64)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(backend)?;

        let mut events = Vec::with_capacity(rows.len());
        for r in &rows {
            let id: Uuid = r.try_get("id").map_err(backend)?;
            let kind: String = r.try_get("kind").map_err(backend)?;
            let due_ms: i64 = r.try_get("due_ms").map_err(backend)?;
            events.push(DueEvent {
                id: id.as_u128(),
                kind: parse_kind(&kind)?,
                due_at: Timestamp(due_ms),
            });
        }
        Ok(events)
    }

    async fn mark_done(&self, id: u128) -> Result<(), RepoError> {
        sqlx::query("UPDATE scheduled_events SET status = 'done' WHERE id = $1")
            .bind(Uuid::from_u128(id))
            .execute(&self.pool)
            .await
            .map_err(backend)?;
        Ok(())
    }
}

/// Background scheduler that processes due events. Slice 001 polls at a short interval; sleeping
/// precisely until the next due event (and `LISTEN/NOTIFY` wake-ups) is a later refinement.
///
/// Deployment assumption: **one active scheduler instance** — the startup orphan requeues would
/// re-activate rows another live instance has claimed. Claiming itself (`SKIP LOCKED`) and every
/// state mutation (snapshot-guarded settles, single-transaction applies) are already safe under
/// concurrent workers; only the requeues need coordination before scaling out (P5 note).
#[derive(Clone)]
pub struct Scheduler {
    store: PgEventStore,
    builds: PgAccountRepository,
    economy_rules: Arc<EconomyRules>,
    unit_rules: Arc<UnitRules>,
    merchant_rules: Arc<MerchantRules>,
    combat_rules: Arc<CombatRules>,
    scout_rules: Arc<ScoutRules>,
    oasis_rules: Arc<OasisRules>,
    culture_rules: Arc<CultureRules>,
    map: Arc<WorldMap>,
    speed: GameSpeed,
    world_seed: u64,
    poll_interval: Duration,
}

impl Scheduler {
    /// Create a scheduler over the event store and repositories with a default poll interval.
    /// The rules + speed drive starvation re-validation (005 AC7), trade delivery (008), and combat
    /// resolution (009); the world seed seeds battle luck (P6).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        store: PgEventStore,
        builds: PgAccountRepository,
        economy_rules: Arc<EconomyRules>,
        unit_rules: Arc<UnitRules>,
        merchant_rules: Arc<MerchantRules>,
        combat_rules: Arc<CombatRules>,
        scout_rules: Arc<ScoutRules>,
        oasis_rules: Arc<OasisRules>,
        culture_rules: Arc<CultureRules>,
        map: Arc<WorldMap>,
        speed: GameSpeed,
        world_seed: u64,
    ) -> Self {
        Self {
            store,
            builds,
            economy_rules,
            unit_rules,
            merchant_rules,
            combat_rules,
            scout_rules,
            oasis_rules,
            culture_rules,
            map,
            speed,
            world_seed,
            poll_interval: Duration::from_millis(200),
        }
    }

    /// Run until `shutdown` flips to `true`, processing due events, builds, unit orders, training
    /// completions, and starvation checks each tick.
    pub async fn run(self, mut shutdown: tokio::sync::watch::Receiver<bool>) {
        // Recover anything left mid-flight by a previous crash before starting the loop.
        match self.store.requeue_orphaned().await {
            Ok(n) if n > 0 => tracing::warn!(requeued = n, "requeued orphaned events at startup"),
            Ok(_) => {}
            Err(e) => tracing::error!(error = %e, "failed to requeue orphaned events"),
        }
        match self.builds.requeue_orphaned_builds().await {
            Ok(n) if n > 0 => tracing::warn!(requeued = n, "requeued orphaned builds at startup"),
            Ok(_) => {}
            Err(e) => tracing::error!(error = %e, "failed to requeue orphaned builds"),
        }
        match self.builds.requeue_orphaned_unit_orders().await {
            Ok(n) if n > 0 => {
                tracing::warn!(requeued = n, "requeued orphaned unit orders at startup");
            }
            Ok(_) => {}
            Err(e) => tracing::error!(error = %e, "failed to requeue orphaned unit orders"),
        }
        match self.builds.requeue_orphaned_training().await {
            Ok(n) if n > 0 => {
                tracing::warn!(requeued = n, "requeued orphaned training at startup");
            }
            Ok(_) => {}
            Err(e) => tracing::error!(error = %e, "failed to requeue orphaned training"),
        }
        match self.builds.requeue_orphaned_starvation().await {
            Ok(n) if n > 0 => {
                tracing::warn!(
                    requeued = n,
                    "requeued orphaned starvation checks at startup"
                );
            }
            Ok(_) => {}
            Err(e) => tracing::error!(error = %e, "failed to requeue orphaned starvation checks"),
        }
        match self.builds.requeue_orphaned_movements().await {
            Ok(n) if n > 0 => {
                tracing::warn!(requeued = n, "requeued orphaned movements at startup");
            }
            Ok(_) => {}
            Err(e) => tracing::error!(error = %e, "failed to requeue orphaned movements"),
        }
        match self.builds.requeue_orphaned_trades().await {
            Ok(n) if n > 0 => {
                tracing::warn!(requeued = n, "requeued orphaned trades at startup");
            }
            Ok(_) => {}
            Err(e) => tracing::error!(error = %e, "failed to requeue orphaned trades"),
        }
        loop {
            if *shutdown.borrow() {
                break;
            }
            match process_due(&self.store, now(), 100).await {
                Ok(n) if n > 0 => tracing::debug!(processed = n, "scheduler processed due events"),
                Ok(_) => {}
                Err(e) => tracing::error!(error = %e, "scheduler tick failed"),
            }
            match process_due_builds(
                &self.builds,
                &self.builds,
                &self.builds,
                &self.culture_rules,
                now(),
                100,
            )
            .await
            {
                Ok(villages) if !villages.is_empty() => {
                    tracing::debug!(applied = villages.len(), "scheduler applied due builds");
                    // Population moved — re-sync the affected depletion checks (005 AC7).
                    self.resync(&villages).await;
                }
                Ok(_) => {}
                Err(e) => tracing::error!(error = %e, "scheduler build tick failed"),
            }
            match process_due_unit_orders(&self.builds, now(), 100).await {
                Ok(n) if n > 0 => {
                    tracing::debug!(applied = n, "scheduler applied due unit orders");
                }
                Ok(_) => {}
                Err(e) => tracing::error!(error = %e, "scheduler unit tick failed"),
            }
            match process_due_training(
                &self.builds,
                &self.builds,
                &self.economy_rules,
                &self.unit_rules,
                self.speed,
                now(),
                100,
            )
            .await
            {
                Ok(villages) if !villages.is_empty() => {
                    tracing::debug!(
                        villages = villages.len(),
                        "scheduler delivered trained units"
                    );
                    // Upkeep rose — re-sync the affected depletion checks (005 AC7).
                    self.resync(&villages).await;
                }
                Ok(_) => {}
                Err(e) => tracing::error!(error = %e, "scheduler training tick failed"),
            }
            match process_due_movements(
                &self.builds,
                &self.builds,
                &self.economy_rules,
                &self.unit_rules,
                self.speed,
                now(),
                100,
            )
            .await
            {
                Ok(homes) if !homes.is_empty() => {
                    tracing::debug!(returned = homes.len(), "scheduler delivered movements");
                    // Returning troops rejoined a garrison — re-sync those depletion checks (AC5).
                    self.resync(&homes).await;
                }
                Ok(_) => {}
                Err(e) => tracing::error!(error = %e, "scheduler movement tick failed"),
            }
            match process_due_trades(
                &self.builds,
                &self.builds,
                &self.economy_rules,
                &self.unit_rules,
                &self.merchant_rules,
                &self.map,
                self.speed,
                now(),
                100,
            )
            .await
            {
                Ok(targets) if !targets.is_empty() => {
                    tracing::debug!(delivered = targets.len(), "scheduler delivered trades");
                    // Credited targets gained crop — re-sync those depletion checks (005 AC7).
                    self.resync(&targets).await;
                }
                Ok(_) => {}
                Err(e) => tracing::error!(error = %e, "scheduler trade tick failed"),
            }
            match process_due_combat(
                &self.builds,
                &self.builds,
                &self.builds,
                &self.builds,
                &self.economy_rules,
                &self.unit_rules,
                &self.combat_rules,
                &self.scout_rules,
                &self.map,
                self.speed,
                self.world_seed,
                now(),
                100,
            )
            .await
            {
                Ok(targets) if !targets.is_empty() => {
                    tracing::debug!(resolved = targets.len(), "scheduler resolved battles");
                    // Defender garrisons shrank — re-sync those depletion checks (005 AC7).
                    self.resync(&targets).await;
                }
                Ok(_) => {}
                Err(e) => tracing::error!(error = %e, "scheduler combat tick failed"),
            }
            // Standalone scout missions (010): no village garrison changes at resolution, so there is
            // nothing to re-sync here (surviving scouts re-sync home when their return arrives).
            if let Err(e) = process_due_scouts(
                &self.builds,
                &self.builds,
                &self.builds,
                &self.economy_rules,
                &self.unit_rules,
                &self.scout_rules,
                &self.map,
                self.speed,
                now(),
                100,
            )
            .await
            {
                tracing::error!(error = %e, "scheduler scout tick failed");
            }
            // Oases (012): clearing/occupying, reinforcing, and animal regrowth. None of these change
            // a village garrison at resolution (survivors return via the movement tick), so there is
            // nothing to re-sync here.
            if let Err(e) = process_due_oasis_combat(
                &self.builds,
                &self.builds,
                &self.builds,
                &self.economy_rules,
                &self.unit_rules,
                &self.combat_rules,
                &self.oasis_rules,
                &self.map,
                self.speed,
                self.world_seed,
                now(),
                100,
            )
            .await
            {
                tracing::error!(error = %e, "scheduler oasis combat tick failed");
            }
            if let Err(e) = process_due_oasis_reinforce(
                &self.builds,
                &self.builds,
                &self.unit_rules,
                &self.map,
                self.speed,
                now(),
                100,
            )
            .await
            {
                tracing::error!(error = %e, "scheduler oasis reinforce tick failed");
            }
            if let Err(e) = process_due_oasis_regrow(
                &self.builds,
                &self.unit_rules,
                &self.oasis_rules,
                self.world_seed,
                self.speed,
                now(),
                100,
            )
            .await
            {
                tracing::error!(error = %e, "scheduler oasis regrow tick failed");
            }
            match process_due_starvation(
                &self.builds,
                &self.builds,
                &self.economy_rules,
                &self.unit_rules,
                self.speed,
                now(),
                100,
            )
            .await
            {
                Ok(n) if n > 0 => tracing::info!(culled = n, "scheduler starved garrisons"),
                Ok(_) => {}
                Err(e) => tracing::error!(error = %e, "scheduler starvation tick failed"),
            }
            tokio::select! {
                () = tokio::time::sleep(self.poll_interval) => {}
                _ = shutdown.changed() => {}
            }
        }
    }

    async fn resync(&self, villages: &[eperica_domain::VillageId]) {
        if let Err(e) = sync_starvation_checks(
            &self.builds,
            &self.builds,
            &self.economy_rules,
            &self.unit_rules,
            self.speed,
            now(),
            villages,
        )
        .await
        {
            tracing::error!(error = %e, "starvation re-sync failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn processes_due_events_once_and_leaves_future_pending() {
        let _ = dotenvy::dotenv();
        let Ok(url) = std::env::var("DATABASE_URL") else {
            eprintln!("skipping scheduler test: DATABASE_URL not set");
            return;
        };
        let pool = crate::create_pool(&url).await.expect("connect");
        crate::run_migrations(&pool).await.expect("migrate");
        sqlx::query("TRUNCATE scheduled_events")
            .execute(&pool)
            .await
            .expect("truncate");

        let store = PgEventStore::new(pool.clone());
        // Wide margins so a jittery dev/container clock can't flip "past"/"future".
        let due_past = Timestamp(now().0 - 600_000);
        let due_future = Timestamp(now().0 + 3_600_000);
        store
            .schedule(EventKind::Heartbeat, due_past)
            .await
            .unwrap();
        store
            .schedule(EventKind::Heartbeat, due_future)
            .await
            .unwrap();

        // AC6: the past event is processed exactly once; a second pass processes nothing more.
        assert_eq!(process_due(&store, now(), 100).await.unwrap(), 1);
        assert_eq!(process_due(&store, now(), 100).await.unwrap(), 0);

        // The future event remains pending (persisted) — survives restarts (AC8 at the store level).
        let pending: i64 =
            sqlx::query_scalar("SELECT count(*) FROM scheduled_events WHERE status = 'pending'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(pending, 1);
    }
}
