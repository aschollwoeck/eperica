//! The world registry (041) — the live runtime that holds the shared, world-agnostic scheduler rules and
//! can **spawn a per-world scheduler on demand**, so an admin can create a world that starts running with
//! no process restart (040 spawns at startup; this lets `main.rs` and the admin handler share one path).

use eperica_domain::{GameSpeed, WorldId, WorldMap};
use eperica_infrastructure::{
    PgAccountRepository, PgEventStore, PgPool, Scheduler, WorldRules, load_world_rules, world_by_id,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::watch;
use tokio::task::JoinHandle;

/// A running world's immutable config (043) — cached so a game request can build its world-scoped repo +
/// map without a DB round-trip (the map is generate-on-read, so cheap to construct). Carries the world's
/// resolved rule bundle (050), so the per-world preset is served without re-loading balance per request.
#[derive(Debug, Clone)]
pub struct WorldMeta {
    pub seed: i64,
    pub radius: u32,
    pub speed: GameSpeed,
    /// The world's resolved rule bundle (050) — the `rule_preset` (049) loaded once and shared.
    pub rules: Arc<WorldRules>,
}

/// Holds the shared scheduler rules + the machinery to start a scheduler for any world.
pub struct WorldRegistry {
    pool: PgPool,
    shutdown_rx: watch::Receiver<bool>,
    beginner_secs: i64,
    /// Resolved rule bundles keyed by preset name (049/050) — every preset's `WorldRules` loaded once and
    /// shared across that preset's worlds (the map rules + starting amounts live inside each bundle). Seeded
    /// with the boot bundle; a world's preset is resolved + cached on first access via [`Self::rules_for`].
    presets: Mutex<HashMap<String, Arc<WorldRules>>>,
    /// Per-world config (043), recorded when a world is started; drives `context_for`.
    meta: Mutex<HashMap<WorldId, WorldMeta>>,
    /// The worlds whose scheduler is **claimed** (starting or running) — the key's presence is the
    /// atomic idempotency guard; the value is the join handle once spawned (`None` while starting).
    running: Mutex<HashMap<WorldId, Option<JoinHandle<()>>>>,
}

impl WorldRegistry {
    /// Build the registry, seeding the preset cache with the boot world's already-loaded bundle (050) so the
    /// common `classic` case never re-loads balance. Other presets are loaded lazily on first access.
    pub fn new(
        pool: PgPool,
        shutdown_rx: watch::Receiver<bool>,
        beginner_secs: i64,
        boot_preset: String,
        boot_rules: Arc<WorldRules>,
    ) -> Self {
        let mut presets = HashMap::new();
        presets.insert(boot_preset, boot_rules);
        Self {
            pool,
            shutdown_rx,
            beginner_secs,
            presets: Mutex::new(presets),
            meta: Mutex::new(HashMap::new()),
            running: Mutex::new(HashMap::new()),
        }
    }

    /// Resolve a preset name (049) to its rule bundle (050), loading + caching on first use. `None` if the
    /// preset is unknown or its balance fails to load — logged, never a panic (P4); the caller falls back to
    /// the home world (request) or refuses to start the world (scheduler).
    fn rules_for(&self, preset: &str) -> Option<Arc<WorldRules>> {
        if let Some(rules) = self.presets.lock().unwrap().get(preset) {
            return Some(Arc::clone(rules));
        }
        match load_world_rules(preset) {
            Ok(rules) => {
                let arc = Arc::new(rules);
                self.presets
                    .lock()
                    .unwrap()
                    .insert(preset.to_owned(), Arc::clone(&arc));
                Some(arc)
            }
            Err(e) => {
                tracing::error!(preset, error = %e, "failed to load world rule preset");
                None
            }
        }
    }

    /// The selected world's game runtime (043): a freshly-built world-scoped `PgAccountRepository` + its
    /// `WorldMap` + speed + radius, from the cached meta. The map is generate-on-read, so building the
    /// runtime is cheap; the cache is populated on first access from the world row (one DB lookup), then
    /// reused. `None` if the world does not exist or its speed is invalid.
    pub async fn context_for(
        &self,
        world_id: WorldId,
    ) -> Option<(
        PgAccountRepository,
        Arc<WorldMap>,
        GameSpeed,
        u32,
        Arc<WorldRules>,
    )> {
        let cached = self.meta.lock().unwrap().get(&world_id).cloned();
        let meta = match cached {
            Some(m) => m,
            None => {
                let world = world_by_id(&self.pool, world_id).await.ok()??;
                let m = WorldMeta {
                    seed: world.seed,
                    radius: world.radius,
                    speed: GameSpeed::new(world.speed).ok()?,
                    rules: self.rules_for(&world.rule_preset)?,
                };
                self.meta.lock().unwrap().insert(world_id, m.clone());
                m
            }
        };
        let map = Arc::new(WorldMap::new(
            meta.seed as u64,
            meta.radius,
            meta.rules.map_rules.clone(),
        ));
        let repo = PgAccountRepository::new(
            self.pool.clone(),
            world_id,
            meta.seed,
            meta.radius,
            meta.rules.economy.starting_amounts,
            self.beginner_secs,
            meta.speed,
        );
        Some((repo, map, meta.speed, meta.radius, Arc::clone(&meta.rules)))
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
        // Resolve the world's preset (049) to its bundle (050) — the world runs entirely under its own rules.
        let rules = self
            .rules_for(&world.rule_preset)
            .ok_or_else(|| format!("unknown or invalid rule preset: {}", world.rule_preset))?;
        // Cache the world's config (043) so game requests resolve their world-scoped repo without a DB hit.
        self.meta.lock().unwrap().insert(
            world_id,
            WorldMeta {
                seed: world.seed,
                radius: world.radius,
                speed,
                rules: Arc::clone(&rules),
            },
        );
        let map = Arc::new(WorldMap::new(
            world.seed as u64,
            world.radius,
            rules.map_rules.clone(),
        ));
        let accounts = PgAccountRepository::new(
            self.pool.clone(),
            world.id,
            world.seed,
            world.radius,
            rules.economy.starting_amounts,
            self.beginner_secs,
            speed,
        );
        // Derive the scheduler's per-rule `Arc`s from the world's resolved bundle (050). Per spawn (boot /
        // admin create), not per request — cheap.
        let r = &rules;
        let scheduler = Scheduler::new(
            PgEventStore::new(self.pool.clone(), world.id),
            accounts,
            Arc::new(r.economy.clone()),
            Arc::new(r.units.clone()),
            Arc::new(r.merchant.clone()),
            Arc::new(r.combat.clone()),
            Arc::new(r.scout.clone()),
            Arc::new(r.oasis),
            Arc::new(r.culture.clone()),
            Arc::new(r.loyalty.clone()),
            Arc::new(r.ranking.clone()),
            Arc::new(r.medals.clone()),
            Arc::new(r.lifecycle.clone()),
            Arc::new(r.artifacts.clone()),
            Arc::new(r.starting_village.clone()),
            Arc::clone(&map),
            speed,
            world.seed as u64,
            world.created_at,
            world.artifact_release_at,
            Arc::new(r.wonder.clone()),
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

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;

    /// `rules_for` does no I/O, so a lazy (never-connecting) pool lets us test it without a database.
    fn test_registry() -> WorldRegistry {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://user:pass@localhost/none")
            .expect("build a lazy pool");
        let (_tx, rx) = watch::channel(false);
        let boot = Arc::new(load_world_rules("classic").expect("classic bundle loads"));
        WorldRegistry::new(pool, rx, 0, "classic".to_owned(), boot)
    }

    #[tokio::test]
    async fn rules_for_caches_classic_and_rejects_unknown() {
        let reg = test_registry();
        // The seeded boot bundle is served, and a second call returns the same cached `Arc` (no reload).
        let a = reg.rules_for("classic").expect("classic resolves");
        let b = reg.rules_for("classic").expect("classic stays cached");
        assert!(
            Arc::ptr_eq(&a, &b),
            "the classic bundle is cached, not reloaded"
        );
        // A known second preset (052) resolves and caches too.
        let s = reg.rules_for("speed").expect("speed resolves");
        assert!(Arc::ptr_eq(
            &s,
            &reg.rules_for("speed").expect("speed cached")
        ));
        // An unknown preset is a serviceability failure (None), never a panic (P4).
        assert!(reg.rules_for("nonesuch").is_none());
    }
}
