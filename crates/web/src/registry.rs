//! The world registry (041) — the live runtime that holds the shared, world-agnostic scheduler rules and
//! can **spawn a per-world scheduler on demand**, so an admin can create a world that starts running with
//! no process restart (040 spawns at startup; this lets `main.rs` and the admin handler share one path).

use eperica_domain::{
    CombatRules, CultureRules, EconomyRules, GameSpeed, LifecycleRules, LoyaltyRules, MapRules,
    MedalRules, MerchantRules, OasisRules, RankingRules, ScoutRules, StartingVillage, UnitRules,
    WonderRules, WorldId, WorldMap,
};
use eperica_infrastructure::{
    ArtifactCatalogue, PgAccountRepository, PgEventStore, PgPool, Scheduler, world_by_id,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::watch;
use tokio::task::JoinHandle;

/// A running world's immutable config (043) — cached so a game request can build its world-scoped repo +
/// map without a DB round-trip (the map is generate-on-read, so cheap to construct).
#[derive(Debug, Clone, Copy)]
pub struct WorldMeta {
    pub seed: i64,
    pub radius: u32,
    pub speed: GameSpeed,
}

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
    /// The map balance rules, loaded once — every world's `WorldMap` shares them.
    map_rules: MapRules,
    /// Per-world config (043), recorded when a world is started; drives `context_for`.
    meta: Mutex<HashMap<WorldId, WorldMeta>>,
    /// The worlds whose scheduler is **claimed** (starting or running) — the key's presence is the
    /// atomic idempotency guard; the value is the join handle once spawned (`None` while starting).
    running: Mutex<HashMap<WorldId, Option<JoinHandle<()>>>>,
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
        map_rules: MapRules,
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
            map_rules,
            meta: Mutex::new(HashMap::new()),
            running: Mutex::new(HashMap::new()),
        }
    }

    /// The selected world's game runtime (043): a freshly-built world-scoped `PgAccountRepository` + its
    /// `WorldMap` + speed + radius, from the cached meta. The map is generate-on-read, so building the
    /// runtime is cheap; the cache is populated on first access from the world row (one DB lookup), then
    /// reused. `None` if the world does not exist or its speed is invalid.
    pub async fn context_for(
        &self,
        world_id: WorldId,
    ) -> Option<(PgAccountRepository, Arc<WorldMap>, GameSpeed, u32)> {
        let cached = self.meta.lock().unwrap().get(&world_id).copied();
        let meta = match cached {
            Some(m) => m,
            None => {
                let world = world_by_id(&self.pool, world_id).await.ok()??;
                let m = WorldMeta {
                    seed: world.seed,
                    radius: world.radius,
                    speed: GameSpeed::new(world.speed).ok()?,
                };
                self.meta.lock().unwrap().insert(world_id, m);
                m
            }
        };
        let map = Arc::new(WorldMap::new(
            meta.seed as u64,
            meta.radius,
            self.map_rules.clone(),
        ));
        let repo = PgAccountRepository::new(
            self.pool.clone(),
            world_id,
            meta.seed,
            meta.radius,
            self.economy.starting_amounts,
            self.beginner_secs,
            meta.speed,
        );
        Some((repo, map, meta.speed, meta.radius))
    }

    /// Start the scheduler for `world_id`, building its world-scoped runtime (map/repo/event-store from
    /// the world row — 038/039) on the spot. **Idempotent**: a world already running is a no-op. Used at
    /// startup for every world (040) and live by the admin create-world handler (041 AC2).
    ///
    /// # Errors
    /// Returns a message when the world does not exist or its rules/speed are invalid.
    pub async fn start_world(&self, world_id: WorldId) -> Result<(), String> {
        // Atomically claim the slot before the await: a concurrent `start_world(same id)` that already
        // claimed it returns here, so a world is never spawned twice (idempotency, not "in practice").
        {
            let mut g = self.running.lock().unwrap();
            if g.contains_key(&world_id) {
                return Ok(());
            }
            g.insert(world_id, None);
        }
        // On any failure, release the claim so a later retry/restart can start the world.
        match self.build_and_spawn(world_id).await {
            Ok(handle) => {
                self.running.lock().unwrap().insert(world_id, Some(handle));
                tracing::info!(world = world_id.0, "registry started scheduler for world");
                Ok(())
            }
            Err(e) => {
                self.running.lock().unwrap().remove(&world_id);
                Err(e)
            }
        }
    }

    /// Build the world's scoped runtime (map/repo/event-store from the world row) and spawn its
    /// scheduler. The caller owns the `running` claim/release around this.
    async fn build_and_spawn(&self, world_id: WorldId) -> Result<JoinHandle<()>, String> {
        let world = world_by_id(&self.pool, world_id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "world not found".to_owned())?;
        let speed = GameSpeed::new(world.speed).map_err(|e| e.to_string())?;
        // Cache the world's config (043) so game requests resolve their world-scoped repo without a DB hit.
        self.meta.lock().unwrap().insert(
            world_id,
            WorldMeta {
                seed: world.seed,
                radius: world.radius,
                speed,
            },
        );
        let map = Arc::new(WorldMap::new(
            world.seed as u64,
            world.radius,
            self.map_rules.clone(),
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
        Ok(tokio::spawn(scheduler.run(self.shutdown_rx.clone())))
    }

    /// Await every running world's scheduler — called on graceful shutdown after the signal is sent.
    pub async fn join_all(&self) {
        let handles: Vec<JoinHandle<()>> = {
            let mut g = self.running.lock().unwrap();
            g.drain().filter_map(|(_, h)| h).collect()
        };
        for h in handles {
            let _ = h.await;
        }
    }
}
