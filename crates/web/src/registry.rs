//! The world registry (041) — the live runtime that holds the shared, world-agnostic scheduler rules and
//! can **spawn a per-world scheduler on demand**, so an admin can create a world that starts running with
//! no process restart (040 spawns at startup; this lets `main.rs` and the admin handler share one path).

use eperica_domain::{
    CombatRules, CultureRules, EconomyRules, GameSpeed, LifecycleRules, LoyaltyRules, MedalRules,
    MerchantRules, OasisRules, RankingRules, ScoutRules, StartingVillage, UnitRules, WonderRules,
    WorldId, WorldMap,
};
use eperica_infrastructure::{
    ArtifactCatalogue, PgAccountRepository, PgEventStore, PgPool, Scheduler, map_rules, world_by_id,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::watch;
use tokio::task::JoinHandle;

/// Holds the shared scheduler rules + the machinery to start a scheduler for any world.
pub struct WorldRegistry {
    pool: PgPool,
    shutdown_rx: watch::Receiver<bool>,
    beginner_secs: i64,
    economy: Arc<EconomyRules>,
    units: Arc<UnitRules>,
    merchants: Arc<MerchantRules>,
    combat: Arc<CombatRules>,
    scout: Arc<ScoutRules>,
    oases: Arc<OasisRules>,
    culture: Arc<CultureRules>,
    loyalty: Arc<LoyaltyRules>,
    ranking: Arc<RankingRules>,
    medals: Arc<MedalRules>,
    lifecycle: Arc<LifecycleRules>,
    artifacts: Arc<ArtifactCatalogue>,
    template: Arc<StartingVillage>,
    wonder: Arc<WonderRules>,
    /// The worlds whose scheduler is running (for idempotency + graceful shutdown).
    running: Mutex<HashMap<WorldId, JoinHandle<()>>>,
}

impl WorldRegistry {
    /// Build the registry from the shared rules + the process shutdown signal.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        pool: PgPool,
        shutdown_rx: watch::Receiver<bool>,
        beginner_secs: i64,
        economy: Arc<EconomyRules>,
        units: Arc<UnitRules>,
        merchants: Arc<MerchantRules>,
        combat: Arc<CombatRules>,
        scout: Arc<ScoutRules>,
        oases: Arc<OasisRules>,
        culture: Arc<CultureRules>,
        loyalty: Arc<LoyaltyRules>,
        ranking: Arc<RankingRules>,
        medals: Arc<MedalRules>,
        lifecycle: Arc<LifecycleRules>,
        artifacts: Arc<ArtifactCatalogue>,
        template: Arc<StartingVillage>,
        wonder: Arc<WonderRules>,
    ) -> Self {
        Self {
            pool,
            shutdown_rx,
            beginner_secs,
            economy,
            units,
            merchants,
            combat,
            scout,
            oases,
            culture,
            loyalty,
            ranking,
            medals,
            lifecycle,
            artifacts,
            template,
            wonder,
            running: Mutex::new(HashMap::new()),
        }
    }

    /// Start the scheduler for `world_id`, building its world-scoped runtime (map/repo/event-store from
    /// the world row — 038/039) on the spot. **Idempotent**: a world already running is a no-op. Used at
    /// startup for every world (040) and live by the admin create-world handler (041 AC2).
    ///
    /// # Errors
    /// Returns a message when the world does not exist or its rules/speed are invalid.
    pub async fn start_world(&self, world_id: WorldId) -> Result<(), String> {
        if self.running.lock().unwrap().contains_key(&world_id) {
            return Ok(()); // already running
        }
        let world = world_by_id(&self.pool, world_id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "world not found".to_owned())?;
        let speed = GameSpeed::new(world.speed).map_err(|e| e.to_string())?;
        let map = Arc::new(WorldMap::new(
            world.seed as u64,
            world.radius,
            map_rules().map_err(|e| e.to_string())?,
        ));
        let accounts = PgAccountRepository::new(
            self.pool.clone(),
            world.id,
            world.seed,
            world.radius,
            self.economy.starting_amounts,
            self.beginner_secs,
            speed,
        );
        let scheduler = Scheduler::new(
            PgEventStore::new(self.pool.clone(), world.id),
            accounts,
            Arc::clone(&self.economy),
            Arc::clone(&self.units),
            Arc::clone(&self.merchants),
            Arc::clone(&self.combat),
            Arc::clone(&self.scout),
            Arc::clone(&self.oases),
            Arc::clone(&self.culture),
            Arc::clone(&self.loyalty),
            Arc::clone(&self.ranking),
            Arc::clone(&self.medals),
            Arc::clone(&self.lifecycle),
            Arc::clone(&self.artifacts),
            Arc::clone(&self.template),
            Arc::clone(&map),
            speed,
            world.seed as u64,
            world.created_at,
            world.artifact_release_at,
            Arc::clone(&self.wonder),
            world.wonder_release_at,
        );
        let handle = tokio::spawn(scheduler.run(self.shutdown_rx.clone()));
        // Last-writer-wins on the rare concurrent start (startup + create are not concurrent in practice).
        self.running.lock().unwrap().insert(world_id, handle);
        tracing::info!(world = world_id.0, "registry started scheduler for world");
        Ok(())
    }

    /// Await every running world's scheduler — called on graceful shutdown after the signal is sent.
    pub async fn join_all(&self) {
        let handles: Vec<JoinHandle<()>> = {
            let mut g = self.running.lock().unwrap();
            g.drain().map(|(_, h)| h).collect()
        };
        for h in handles {
            let _ = h.await;
        }
    }
}
