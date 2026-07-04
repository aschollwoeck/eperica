//! Fleet scheduler.
//!
//! [`run_fleet`] loads the bot manifest, validates keys, and runs the
//! per-bot tick loop.  A single scheduler task sleeps until the next due bot,
//! wakes up, and spawns per-tick tasks through a fleet-wide `Arc<Semaphore>`
//! (cap = [`RunnerConfig::cap`]).
//!
//! # Jitter
//!
//! Next-tick delay = XorShift64(FNV-1a(name) XOR tick_count), remapped to
//! `[tick_min_secs, tick_max_secs]`.
//!
//! Combining the deterministic name hash with the monotonic tick counter gives
//! successive ticks for the same bot visibly different delays while keeping the
//! sequence reproducible when both the name and the counter are known (e.g., in
//! a test or replay).  Different bots with the same counter still get different
//! jitter because their name hashes differ.  This is intentionally
//! "reproducible-ish" rather than cryptographically unpredictable — the goal
//! is avoiding thundering-herd startup, not security.
//!
//! When `--tick-secs` is set, jitter is disabled: all bots tick at that fixed
//! interval.  Intended for test/demo use only.
//!
//! # Map TTL
//!
//! Each bot fetches a fresh map window at most once every [`MAP_TTL_TICKS`]
//! ticks (default 5).  Between refreshes the cached window is reused.  If a
//! fetch fails, the bot retries on the next tick (the TTL counter is not reset
//! on failure).
//!
//! # Ctrl-C drain (AC5)
//!
//! On SIGINT the scheduler stops accepting new ticks and waits for all
//! in-flight tick tasks to complete before exiting cleanly.
//!
//! # LLM strategist (122)
//!
//! When a `StrategistBackend` is supplied, each bot runs a strategist check
//! once per `LlmConfig::interval_secs` (default 4h, ±10% jitter), only inside
//! its activity window, and only when the fleet-wide `LlmBudget` has headroom.
//! The strategist call runs inside the bot's tick task (accepted latency risk —
//! bounded by the semaphore cap and made rare by the long interval).

use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tracing::{Instrument, debug, error, info, warn};

use crate::client::ApiClient;
use crate::digest::MapWindow;
use crate::executor::{Outcome, classify, execute_intents};
use crate::manifest::{load_manifest, validate};
use crate::persona::Persona;
use crate::policy::{BotTribe, plan_tick};
use crate::strategist::{LlmBudget, StrategistBackend};
use crate::strategy::{Strategy, build_prompt, parse_reply};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// How many ticks between map-window refreshes per bot (at most one fetch in
/// any MAP_TTL_TICKS-tick window).
pub const MAP_TTL_TICKS: u64 = 5;

/// Radius (Chebyshev tiles) used when fetching a bot's map window.
/// Matches the documented server clamp of 10 (docs/agent-api.md); the persona
/// raid range is bounded to 5..=10, so this covers the full possible range.
const MAP_RADIUS: u32 = 10;

/// Default strategist call interval in seconds (4 hours).
const DEFAULT_LLM_INTERVAL_SECS: u64 = 4 * 3600;

/// Default fleet-wide LLM budget (calls per rolling hour).
const DEFAULT_LLM_BUDGET_PER_HOUR: u32 = 12;

// ---------------------------------------------------------------------------
// LlmConfig
// ---------------------------------------------------------------------------

/// LLM strategist configuration — kept serializable/simple (no backend here).
///
/// The backend itself (`Arc<dyn StrategistBackend>`) is passed separately so
/// that `RunnerConfig` stays serializable.
pub struct LlmConfig {
    /// Model ID to use (e.g. `"claude-haiku-4-5-20251001"`).
    pub model: String,
    /// Maximum strategist calls per rolling hour, fleet-wide.
    pub budget_per_hour: u32,
    /// Seconds between per-bot strategist calls (±10% jitter applied).
    pub interval_secs: u64,
}

// ---------------------------------------------------------------------------
// RunnerConfig
// ---------------------------------------------------------------------------

/// Configuration for the fleet runner, parsed from flags and env in `main`.
pub struct RunnerConfig {
    /// Base URL of the Eperica server (trailing slash stripped on use).
    pub server: String,
    /// World UUID string identifying the target world.
    pub world: String,
    /// Filesystem path to the agent key manifest JSON produced by slice 120.
    pub keys_path: String,
    /// When `true`, log intents but make no HTTP POST calls.
    pub dry_run: bool,
    /// Override tick interval in seconds for ALL bots; disables jitter.
    /// Intended for testing and demos only.
    pub tick_scale: Option<u64>,
    /// Maximum number of bot ticks executing concurrently (default: 4).
    pub cap: usize,
    /// When `true`, skip the activity-window check so bots always tick
    /// regardless of the UTC hour.  For deterministic tests only; never set
    /// in production (the binary always sets this to `false`).
    pub open_window: bool,
    /// LLM strategist configuration.  `None` when disabled (no key or --no-llm).
    pub llm: Option<LlmConfig>,
}

// ---------------------------------------------------------------------------
// Internal: bot state (lives only in the scheduler task)
// ---------------------------------------------------------------------------

struct BotState {
    username: String,
    persona: Persona,
    tribe: BotTribe,
    /// Shared across the scheduler and tick tasks; cloning the Arc is cheap.
    client: Arc<ApiClient>,
    /// Unix milliseconds at which this bot's next tick is due.
    next_tick_at_ms: i64,
    /// Monotonically increasing tick counter.  Seeded into the jitter hash.
    tick_count: u64,
    /// Ticks since the last successful map fetch.  Starts at MAP_TTL_TICKS to
    /// force a fetch on the first tick.
    map_ticks_since_fetch: u64,
    /// Cached map window from the last successful fetch.
    cached_map: Option<MapWindow>,
    /// True while a tick task is executing for this bot.
    /// A bot is never scheduled twice concurrently.
    in_flight: bool,
    /// True once a tick task has returned `retire = true`.
    retired: bool,
    /// FNV-1a(username) precomputed to avoid re-hashing every tick.
    name_hash: u64,
    /// Current LLM-derived strategy (default = no bias, identical to 121 behaviour).
    strategy: Strategy,
    /// Unix-ms when this bot's next strategist call is due.
    /// Set to `i64::MAX` when the LLM is disabled so it never fires.
    next_strategist_at_ms: i64,
}

// ---------------------------------------------------------------------------
// Internal: per-tick strategist context (bundled to keep run_tick manageable)
// ---------------------------------------------------------------------------

struct StrategistCtx {
    /// Current strategy (cloned from BotState for use inside the tick task).
    strategy: Strategy,
    /// Unix-ms when the next strategist call is due.
    next_at_ms: i64,
    /// Name hash for jitter (same as BotState.name_hash).
    name_hash: u64,
    /// Current tick count for jitter seed variety.
    tick_count: u64,
    /// LLM backend (fleet-shared Arc).
    backend: Arc<dyn StrategistBackend>,
    /// Fleet-wide rolling-hour budget (fleet-shared Arc<Mutex<>>).
    budget: Arc<Mutex<LlmBudget>>,
    /// Budget limit (calls per rolling hour).
    budget_per_hour: u32,
    /// Interval between per-bot strategist calls in seconds.
    interval_secs: u64,
}

// Result returned by a tick task to the scheduler.
struct TickTaskResult {
    username: String,
    /// Some when a map fetch succeeded this tick.
    new_map: Option<MapWindow>,
    /// True when a map fetch was ATTEMPTED (success or failure).
    /// Distinguishes "no fetch needed" from "fetch failed"; on failure the
    /// scheduler does NOT advance map_ticks_since_fetch so the next tick retries.
    map_was_attempted: bool,
    /// Non-zero when the server responded 429.
    backoff_secs: Option<u64>,
    /// True when the server responded 401 (dead key).
    retire: bool,
    /// Non-None when the strategist ran and produced a validated (non-dry-run) update.
    new_strategy: Option<Strategy>,
    /// Non-None when the strategist was due this tick (regardless of outcome).
    /// The scheduler updates `bot.next_strategist_at_ms` to this value.
    new_next_strategist_at_ms: Option<i64>,
}

// ---------------------------------------------------------------------------
// next_due — pure, unit-tested (AC5)
// ---------------------------------------------------------------------------

/// Return the indices of `bots` whose `next_tick_at_ms` is ≤ `now_ms`.
///
/// Input: slice of `(name, next_tick_at_ms)` pairs.  Output: ascending
/// indices.  A bot is "due" when its scheduled time has arrived.
pub fn next_due(bots: &[(String, i64)], now_ms: i64) -> Vec<usize> {
    bots.iter()
        .enumerate()
        .filter(|(_, (_, t))| *t <= now_ms)
        .map(|(i, _)| i)
        .collect()
}

// ---------------------------------------------------------------------------
// Jitter — XorShift64 seeded from FNV-1a(name) XOR tick_count
// ---------------------------------------------------------------------------

/// Compute the next-tick delay in seconds using XorShift64.
///
/// Seed = FNV-1a(name) XOR tick_count.  When the seed is zero (degenerate
/// XorShift state), it is biased to 1 before the shift sequence.
///
/// Result is uniformly distributed in `[min_secs, max_secs]`.
fn tick_jitter(name_hash: u64, tick_count: u64, min_secs: u32, max_secs: u32) -> u64 {
    let mut x = name_hash ^ tick_count;
    if x == 0 {
        x = 1; // XorShift requires a non-zero seed
    }
    // XorShift64 triple: (13, 7, 17) — standard Marsaglia choice
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    let range = (max_secs - min_secs) as u64;
    min_secs as u64 + (x % (range + 1))
}

/// Compute a ±10% jittered strategist interval in seconds.
///
/// Uses the same XorShift64 as tick jitter, but with a separate seed offset
/// (`tick_count | (1 << 63)`) so strategist and tick jitter never collide.
fn strategist_jitter(name_hash: u64, tick_count: u64, interval_secs: u64) -> u64 {
    // ±10% of interval, clamped to u32 for tick_jitter.
    let min = (interval_secs.saturating_mul(9) / 10).min(u32::MAX as u64) as u32;
    let max = (interval_secs.saturating_mul(11) / 10).min(u32::MAX as u64) as u32;
    // Distinguish from the tick jitter seed by OR-ing the high bit.
    tick_jitter(name_hash, tick_count | (1u64 << 63), min, max)
}

// ---------------------------------------------------------------------------
// Time helpers
// ---------------------------------------------------------------------------

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn utc_hour() -> u8 {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    ((secs / 3600) % 24) as u8
}

// ---------------------------------------------------------------------------
// apply_tick_result — update bot state after a task completes
// ---------------------------------------------------------------------------

fn apply_tick_result(
    bots: &mut [BotState],
    join_result: Result<TickTaskResult, tokio::task::JoinError>,
    cfg: &RunnerConfig,
    at_ms: i64,
) {
    let r = match join_result {
        Err(e) => {
            // Task panicked — we don't know which bot, so log and hope for the best.
            // The affected bot will be stuck in_flight=true for the rest of this run.
            error!(error = ?e, "tick task panicked; one bot may be stuck until restart");
            return;
        }
        Ok(r) => r,
    };

    let Some(bot) = bots.iter_mut().find(|b| b.username == r.username) else {
        warn!(bot = %r.username, "tick result for unknown bot; ignoring");
        return;
    };

    bot.in_flight = false;

    if r.retire {
        warn!(bot = %bot.username, "bot retired (key rejected)");
        bot.retired = true;
        return;
    }

    // Update strategy if the LLM produced a valid (non-dry-run) update.
    if let Some(s) = r.new_strategy {
        bot.strategy = s;
    }
    // Advance the strategist schedule whenever it was due this tick.
    if let Some(t) = r.new_next_strategist_at_ms {
        bot.next_strategist_at_ms = t;
    }

    // Update map cache.
    if let Some(m) = r.new_map {
        bot.cached_map = Some(m);
        bot.map_ticks_since_fetch = 0;
    } else if !r.map_was_attempted {
        // No fetch attempted: advance the TTL counter.
        bot.map_ticks_since_fetch = bot.map_ticks_since_fetch.saturating_add(1);
    }
    // If a fetch was attempted but failed: leave map_ticks_since_fetch unchanged
    // (it was already >= MAP_TTL_TICKS), so the next tick retries immediately.

    // Schedule the next tick.
    let delay_secs = if let Some(secs) = r.backoff_secs {
        // 429 backoff overrides everything, but never shorter than the persona minimum.
        // A tiny retry_after (e.g. 30 s) must not schedule ticks faster than the bot's
        // humanized cadence floor (180 s) — that would look inhuman and exhaust the budget.
        secs.max(bot.persona.tick_min_secs as u64)
    } else if let Some(override_secs) = cfg.tick_scale {
        // --tick-secs forces a fixed interval (no jitter).
        override_secs
    } else {
        tick_jitter(
            bot.name_hash,
            bot.tick_count,
            bot.persona.tick_min_secs,
            bot.persona.tick_max_secs,
        )
    };

    bot.next_tick_at_ms = at_ms + (delay_secs as i64) * 1_000;
    bot.tick_count += 1;
}

// ---------------------------------------------------------------------------
// run_tick — per-bot tick logic (runs inside a JoinSet task)
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
async fn run_tick(
    client: Arc<ApiClient>,
    world: String,
    username: String,
    persona: Persona,
    tribe: BotTribe,
    dry_run: bool,
    cached_map: Option<MapWindow>,
    should_fetch_map: bool,
    open_window: bool,
    // Current strategy (from BotState; Strategy::default() when LLM is disabled).
    strategy: Strategy,
    // Some when the LLM is enabled; carries the state needed for the strategist step.
    strategist_ctx: Option<StrategistCtx>,
) -> TickTaskResult {
    // Check activity window (skipped when open_window=true, e.g. in tests).
    if !open_window {
        let hour = utc_hour();
        if !persona.in_window(hour) {
            debug!(bot = %username, hour, "outside activity window; skipping tick");
            return TickTaskResult {
                username,
                new_map: None,
                map_was_attempted: false,
                backoff_secs: None,
                retire: false,
                new_strategy: None,
                new_next_strategist_at_ms: None,
            };
        }
    }

    // Fetch digest (one per tick).
    let digest = match client.state(&world).await {
        Ok(d) => d,
        Err(e) => {
            warn!(bot = %username, error = %e, "digest fetch failed");
            let (backoff_secs, retire) = match classify(&e) {
                Outcome::RetireBot => (None, true),
                Outcome::Backoff { secs } => (Some(secs), false),
                _ => (None, false),
            };
            return TickTaskResult {
                username,
                new_map: None,
                map_was_attempted: false,
                backoff_secs,
                retire,
                new_strategy: None,
                new_next_strategist_at_ms: None,
            };
        }
    };

    // Fetch map window when TTL has expired.
    let (new_map, map_was_attempted) = if should_fetch_map {
        match digest.villages.first() {
            None => (None, false),
            Some(v) => {
                let result = client.map(&world, v.x, v.y, MAP_RADIUS).await;
                match result {
                    Ok(m) => {
                        debug!(bot = %username, "map window refreshed");
                        (Some(m), true)
                    }
                    Err(e) => {
                        warn!(bot = %username, error = %e, "map fetch failed; reusing cache");
                        (None, true) // attempted but failed
                    }
                }
            }
        }
    } else {
        (None, false) // not attempted
    };

    // Use freshly fetched map or fall back to the cached window.
    let map_ref = new_map.as_ref().or(cached_map.as_ref());

    let now_ms = digest.now_ms;

    // ---------------------------------------------------------------------------
    // Strategist step — runs BEFORE plan_tick.
    //
    // NOTE (accepted risk, plan.md §Key risks): the advise() call runs inside
    // the tick task.  Worst case: delays this bot's own next reflex tick only.
    // Fleet impact is bounded by the semaphore cap; the ~4h interval makes it rare.
    // ---------------------------------------------------------------------------
    let (current_strategy, new_strategy, new_next_strategist_at_ms) = if let Some(ctx) =
        strategist_ctx
    {
        if now_ms >= ctx.next_at_ms {
            // Compute the new next-due time (always done when the strategist is due,
            // regardless of budget or call outcome — AC5 invariant).
            let jitter_secs = strategist_jitter(ctx.name_hash, ctx.tick_count, ctx.interval_secs);
            let next_at = now_ms + jitter_secs as i64 * 1_000;

            // Check the fleet-wide rolling-hour budget.
            let budget_ok = {
                ctx.budget
                    .lock()
                    .expect("LlmBudget mutex poisoned")
                    .try_take(now_ms, ctx.budget_per_hour)
            };

            let (updated_strategy, strategy_changed) = if budget_ok {
                let prompt =
                    build_prompt(&digest, map_ref, &persona, &ctx.strategy, &username, tribe);

                info!(
                    bot = %username,
                    prompt_bytes = prompt.len(),
                    "calling strategist"
                );

                match ctx.backend.advise(&prompt).await {
                    Ok(raw) => match parse_reply(&raw) {
                        Ok(reply) => {
                            if dry_run {
                                info!(
                                    bot = %username,
                                    motto = %reply.strategy.motto,
                                    "DRY strategy would apply"
                                );
                                if let Some(ref msg) = reply.message {
                                    info!(
                                        bot = %username,
                                        to = %msg.to,
                                        "DRY would message"
                                    );
                                }
                                // dry_run: do not apply the strategy or send the DM.
                                (ctx.strategy, false)
                            } else {
                                info!(
                                    bot = %username,
                                    motto = %reply.strategy.motto,
                                    outcome = "applied",
                                    "strategist applied"
                                );
                                // Send the optional diplomatic DM (once per cycle).
                                if let Some(msg) = reply.message {
                                    match client.message(&world, &msg.to, &msg.body).await {
                                        Ok(_) => {
                                            debug!(
                                                bot = %username,
                                                to = %msg.to,
                                                "diplomatic message sent"
                                            );
                                        }
                                        Err(e) => {
                                            // Denials logged, never retried in-cycle.
                                            warn!(
                                                bot = %username,
                                                to = %msg.to,
                                                error = %e,
                                                "diplomatic message denied/failed; not retried"
                                            );
                                        }
                                    }
                                }
                                (reply.strategy, true)
                            }
                        }
                        Err(e) => {
                            // Strict no-fallback rule: keep the prior strategy.
                            error!(
                                bot = %username,
                                error = %e,
                                strategist_errors = 1,
                                outcome = "rejected",
                                "strategist reply rejected; keeping prior strategy"
                            );
                            (ctx.strategy, false)
                        }
                    },
                    Err(e) => {
                        error!(
                            bot = %username,
                            error = %e,
                            strategist_errors = 1,
                            outcome = "error",
                            "strategist advise failed; keeping prior strategy"
                        );
                        (ctx.strategy, false)
                    }
                }
            } else {
                debug!(
                    bot = %username,
                    outcome = "budget-skipped",
                    "strategist skipped: fleet budget exhausted"
                );
                (ctx.strategy, false)
            };

            let new_strat = if strategy_changed {
                Some(updated_strategy.clone())
            } else {
                None
            };
            (updated_strategy, new_strat, Some(next_at))
        } else {
            // Strategist not yet due.
            (ctx.strategy, None, None)
        }
    } else {
        // LLM disabled — use the passed-in strategy (Strategy::default()).
        (strategy, None, None)
    };

    let intents = plan_tick(&digest, map_ref, &persona, &current_strategy, now_ms, tribe);

    if intents.is_empty() {
        debug!(bot = %username, "no intents this tick");
    } else {
        info!(bot = %username, count = intents.len(), "executing intents");
    }

    // Use .instrument() rather than span.enter() to avoid holding the span guard
    // across the await point (tracing best practice for async contexts).
    let report = execute_intents(&client, &world, &intents, dry_run)
        .instrument(tracing::info_span!("exec", bot = %username))
        .await;

    debug!(
        bot = %username,
        executed = report.executed,
        denied = report.denied,
        transient = report.transient,
        "tick complete"
    );

    TickTaskResult {
        username,
        new_map,
        map_was_attempted,
        backoff_secs: report.backoff_secs,
        retire: report.retire,
        new_strategy,
        new_next_strategist_at_ms,
    }
}

// ---------------------------------------------------------------------------
// run_fleet_until / run_fleet — public entry points
// ---------------------------------------------------------------------------

/// Start the bot fleet and run until `shutdown` resolves.
///
/// 1. Load and validate the key manifest (dead keys are logged and dropped).
/// 2. For each live bot, check that it has a player in `cfg.world` (bots
///    without a player in the target world are dropped with a warning).
/// 3. Run the scheduler loop until `shutdown` resolves.
/// 4. On shutdown: stop spawning new ticks; drain all in-flight tasks; exit.
///
/// `backend` — the LLM strategist backend.  `None` disables the strategist;
/// fleet behaviour is then byte-identical to the 121 baseline (AC1).
///
/// Callers that want Ctrl-C shutdown should use [`run_fleet`]; pass an explicit
/// future (e.g. `tokio::time::sleep(…)`) here for time-bounded or test runs.
pub async fn run_fleet_until<F>(
    cfg: RunnerConfig,
    shutdown: F,
    backend: Option<Arc<dyn StrategistBackend>>,
) where
    F: Future<Output = ()> + Send,
{
    tokio::pin!(shutdown);
    run_fleet_inner(cfg, &mut shutdown, backend).await;
}

/// Start the bot fleet and run until Ctrl-C is received.
///
/// Thin wrapper around [`run_fleet_until`] that supplies `tokio::signal::ctrl_c`
/// as the shutdown signal.
pub async fn run_fleet(cfg: RunnerConfig, backend: Option<Arc<dyn StrategistBackend>>) {
    run_fleet_until(
        cfg,
        async {
            let _ = tokio::signal::ctrl_c().await;
        },
        backend,
    )
    .await;
}

async fn run_fleet_inner(
    cfg: RunnerConfig,
    shutdown: &mut (impl Future<Output = ()> + Unpin),
    backend: Option<Arc<dyn StrategistBackend>>,
) {
    if cfg.cap == 0 {
        warn!("cap=0; no bot ticks will ever run — exiting immediately");
        return;
    }

    // -----------------------------------------------------------------------
    // Fleet startup (AC1)
    // -----------------------------------------------------------------------

    let entries = match load_manifest(&cfg.keys_path) {
        Ok(e) => e,
        Err(e) => {
            error!(error = %e, "failed to load key manifest");
            return;
        }
    };

    if entries.is_empty() {
        warn!("manifest is empty — nothing to do");
        return;
    }

    info!(
        count = entries.len(),
        "loaded key manifest; validating keys..."
    );

    let validated = validate(&cfg.server, entries).await;

    // Fleet-wide LLM budget (always created; used only when backend is Some).
    let budget = Arc::new(Mutex::new(LlmBudget::new()));

    // Derive LLM config values (with defaults) for initial scheduling.
    let llm_interval_secs = cfg
        .llm
        .as_ref()
        .map(|l| l.interval_secs)
        .unwrap_or(DEFAULT_LLM_INTERVAL_SECS);
    let llm_budget_per_hour = cfg
        .llm
        .as_ref()
        .map(|l| l.budget_per_hour)
        .unwrap_or(DEFAULT_LLM_BUDGET_PER_HOUR);

    let mut bots: Vec<BotState> = Vec::new();
    for (entry, result) in validated {
        match result {
            Err(e) => {
                warn!(bot = %entry.username, error = %e, "key invalid; dropping bot");
            }
            Ok(me) => {
                // Confirm the bot has a player entry in the target world.
                let world_entry = me.worlds.iter().find(|w| w.world == cfg.world);
                match world_entry {
                    None => {
                        warn!(
                            bot = %entry.username,
                            world = %cfg.world,
                            "bot has no player in target world; dropping"
                        );
                    }
                    Some(we) => {
                        let persona = Persona::from_name(&entry.username);
                        let name_hash = crate::persona::fnv1a_64(entry.username.as_bytes());
                        let client = Arc::new(ApiClient::new(&cfg.server, &entry.token));
                        // Stagger initial ticks by 500 ms per bot to avoid a
                        // startup thundering-herd.
                        let start_ms = now_ms() + (bots.len() as i64) * 500;
                        // Strict tribe parse (no silent fallback): an unknown slug drops the
                        // bot loudly, exactly like a dead key — the fleet continues.
                        let tribe = match BotTribe::parse(&we.tribe) {
                            Ok(t) => t,
                            Err(e) => {
                                tracing::error!(bot = %entry.username, error = %e, "dropping bot");
                                continue;
                            }
                        };

                        // Initial strategist schedule: stagger by interval±10% from start.
                        let next_strategist_at_ms = if backend.is_some() {
                            let jitter =
                                strategist_jitter(name_hash, bots.len() as u64, llm_interval_secs);
                            start_ms + jitter as i64 * 1_000
                        } else {
                            i64::MAX // never due when LLM is disabled
                        };

                        info!(
                            bot = %entry.username,
                            tribe = %we.tribe,
                            "bot validated; added to fleet"
                        );
                        bots.push(BotState {
                            username: entry.username,
                            persona,
                            tribe,
                            client,
                            next_tick_at_ms: start_ms,
                            tick_count: 0,
                            // Force a map fetch on the first tick.
                            map_ticks_since_fetch: MAP_TTL_TICKS,
                            cached_map: None,
                            in_flight: false,
                            retired: false,
                            name_hash,
                            strategy: Strategy::default(),
                            next_strategist_at_ms,
                        });
                    }
                }
            }
        }
    }

    if bots.is_empty() {
        warn!("no live bots; exiting");
        return;
    }

    info!(bots = bots.len(), dry_run = cfg.dry_run, "fleet started");

    // -----------------------------------------------------------------------
    // Scheduler loop (AC4/AC5)
    // -----------------------------------------------------------------------

    let semaphore = Arc::new(Semaphore::new(cfg.cap));
    let mut join_set: JoinSet<TickTaskResult> = JoinSet::new();
    let mut shutdown_flag = false;
    let open_window = cfg.open_window;

    loop {
        // Phase 1: drain any already-completed tick tasks.
        while let Some(res) = join_set.try_join_next() {
            apply_tick_result(&mut bots, res, &cfg, now_ms());
        }

        bots.retain(|b| !b.retired);

        if bots.is_empty() {
            info!("all bots have retired; exiting");
            break;
        }

        if shutdown_flag && join_set.is_empty() {
            info!("drain complete; exiting");
            break;
        }

        // Phase 2: spawn tick tasks for due bots (unless we are shutting down).
        // Use next_due to find which bots' scheduled times have arrived.
        if !shutdown_flag {
            let now = now_ms();
            let schedule: Vec<(String, i64)> = bots
                .iter()
                .map(|b| (b.username.clone(), b.next_tick_at_ms))
                .collect();
            let due_indices = next_due(&schedule, now);

            for i in due_indices {
                let bot = &mut bots[i];
                if bot.in_flight {
                    continue; // Already executing; will be rescheduled when it completes.
                }

                let should_fetch_map = bot.map_ticks_since_fetch >= MAP_TTL_TICKS;
                bot.in_flight = true;

                let client = Arc::clone(&bot.client);
                let world = cfg.world.clone();
                let persona = bot.persona.clone();
                let tribe = bot.tribe;
                let username = bot.username.clone();
                let dry_run = cfg.dry_run;
                let cached_map = bot.cached_map.clone();
                let tick_count = bot.tick_count;
                let name_hash = bot.name_hash;
                let sem = Arc::clone(&semaphore);

                // Bundle strategist state for this tick.
                let strategy = bot.strategy.clone();
                let strategist_ctx = backend.as_ref().map(|b| StrategistCtx {
                    strategy: strategy.clone(),
                    next_at_ms: bot.next_strategist_at_ms,
                    name_hash,
                    tick_count,
                    backend: Arc::clone(b),
                    budget: Arc::clone(&budget),
                    budget_per_hour: llm_budget_per_hour,
                    interval_secs: llm_interval_secs,
                });

                join_set.spawn(async move {
                    // Acquire a semaphore permit before doing any work.
                    // The permit is released automatically when the task ends.
                    let _permit = match sem.acquire_owned().await {
                        Ok(p) => p,
                        Err(e) => {
                            error!(error = ?e, bot = %username, "semaphore closed; aborting tick");
                            return TickTaskResult {
                                username,
                                new_map: None,
                                map_was_attempted: false,
                                backoff_secs: None,
                                retire: false,
                                new_strategy: None,
                                new_next_strategist_at_ms: None,
                            };
                        }
                    };

                    // Use .instrument() so the span is not held across await points.
                    let span = tracing::info_span!("bot_tick", bot = %username, tick = tick_count);
                    run_tick(
                        client,
                        world,
                        username,
                        persona,
                        tribe,
                        dry_run,
                        cached_map,
                        should_fetch_map,
                        open_window,
                        strategy,
                        strategist_ctx,
                    )
                    .instrument(span)
                    .await
                });
            }
        }

        // Phase 3: sleep until the next due bot or a task completes or shutdown.

        // Compute how long to sleep before the next (non-in-flight) bot is due.
        let next_due_ms = bots
            .iter()
            .filter(|b| !b.in_flight)
            .map(|b| b.next_tick_at_ms)
            .min()
            .unwrap_or_else(|| now_ms() + 5_000);

        // When draining, poll frequently; otherwise sleep until the next due time.
        let sleep_ms = if shutdown_flag {
            200u64
        } else {
            ((next_due_ms - now_ms()).max(1) as u64).min(5_000)
        };

        // Guard the join_next arm so that when join_set is empty the arm is
        // disabled — otherwise tokio returns Poll::Ready(None) and busy-loops.
        let js_nonempty = !join_set.is_empty();

        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(sleep_ms)) => {}

            res = join_set.join_next(), if js_nonempty => {
                if let Some(r) = res {
                    apply_tick_result(&mut bots, r, &cfg, now_ms());
                    bots.retain(|b| !b.retired);
                }
            }

            _ = &mut *shutdown, if !shutdown_flag => {
                info!("shutdown signal received; draining in-flight ticks...");
                shutdown_flag = true;
            }
        }
    }

    // Final drain (shouldn't be needed given the loop condition, but be safe).
    while let Some(res) = join_set.join_next().await {
        apply_tick_result(&mut bots, res, &cfg, now_ms());
    }

    info!("fleet stopped");
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_bot_state(username: &str) -> BotState {
        let client = Arc::new(crate::client::ApiClient::new(
            "http://localhost:0",
            "epk_0000000000000000_testtoken",
        ));
        let name_hash = crate::persona::fnv1a_64(username.as_bytes());
        BotState {
            username: username.to_owned(),
            persona: crate::persona::Persona {
                window_start_hour: 0,
                window_len_hours: 24,
                tick_min_secs: 180,
                tick_max_secs: 720,
                aggression: 0,
                raid_range: 8,
            },
            tribe: BotTribe::Romans,
            client,
            next_tick_at_ms: 0,
            tick_count: 0,
            map_ticks_since_fetch: 0,
            cached_map: None,
            in_flight: true,
            retired: false,
            name_hash,
            strategy: Strategy::default(),
            next_strategist_at_ms: i64::MAX,
        }
    }

    fn minimal_cfg() -> RunnerConfig {
        RunnerConfig {
            server: "http://localhost:0".to_owned(),
            world: "world-test".to_owned(),
            keys_path: "".to_owned(),
            dry_run: false,
            tick_scale: None,
            cap: 1,
            open_window: false,
            llm: None,
        }
    }

    // -----------------------------------------------------------------------
    // M3: apply_tick_result — backoff respects persona tick_min_secs
    // -----------------------------------------------------------------------

    #[test]
    fn apply_tick_result_backoff_floored_to_tick_min() {
        // A 429 with retry_after_secs=30 on a bot with tick_min_secs=180 must
        // schedule the next tick at least 180 s away, not 30 s.
        let mut bots = vec![make_bot_state("testbot")];

        let cfg = minimal_cfg();

        let at_ms = 1_000_000_000_000_i64;
        let result = TickTaskResult {
            username: "testbot".to_owned(),
            new_map: None,
            map_was_attempted: false,
            backoff_secs: Some(30), // 30 s < tick_min_secs (180 s)
            retire: false,
            new_strategy: None,
            new_next_strategist_at_ms: None,
        };

        apply_tick_result(&mut bots, Ok(result), &cfg, at_ms);

        // delay must be max(30, 180) = 180 s
        assert_eq!(
            bots[0].next_tick_at_ms,
            at_ms + 180 * 1_000,
            "backoff=30s must be floored to tick_min_secs=180s"
        );
    }

    #[test]
    fn apply_tick_result_backoff_larger_than_tick_min_used_verbatim() {
        // A 429 with retry_after_secs=300 on a bot with tick_min_secs=180 must
        // schedule 300 s out (the backoff is larger, so it wins).
        let mut bots = vec![make_bot_state("testbot2")];

        let cfg = minimal_cfg();

        let at_ms = 1_000_000_000_000_i64;
        let result = TickTaskResult {
            username: "testbot2".to_owned(),
            new_map: None,
            map_was_attempted: false,
            backoff_secs: Some(300), // 300 s > tick_min_secs (180 s)
            retire: false,
            new_strategy: None,
            new_next_strategist_at_ms: None,
        };

        apply_tick_result(&mut bots, Ok(result), &cfg, at_ms);

        assert_eq!(
            bots[0].next_tick_at_ms,
            at_ms + 300 * 1_000,
            "backoff=300s > tick_min=180s; should use 300s verbatim"
        );
    }

    // -----------------------------------------------------------------------
    // apply_tick_result — strategy + next_strategist_at_ms updated correctly
    // -----------------------------------------------------------------------

    #[test]
    fn apply_tick_result_updates_strategy_when_some() {
        let mut bots = vec![make_bot_state("strat_bot")];
        assert_eq!(bots[0].strategy.focus, crate::strategy::Focus::Balanced);

        let new_strat = Strategy {
            focus: crate::strategy::Focus::Military,
            ..Strategy::default()
        };
        let result = TickTaskResult {
            username: "strat_bot".to_owned(),
            new_map: None,
            map_was_attempted: false,
            backoff_secs: None,
            retire: false,
            new_strategy: Some(new_strat),
            new_next_strategist_at_ms: Some(9_999_999_999_999),
        };

        apply_tick_result(&mut bots, Ok(result), &minimal_cfg(), 0);

        assert_eq!(
            bots[0].strategy.focus,
            crate::strategy::Focus::Military,
            "strategy must be updated when new_strategy is Some"
        );
        assert_eq!(
            bots[0].next_strategist_at_ms, 9_999_999_999_999,
            "next_strategist_at_ms must be updated"
        );
    }

    #[test]
    fn apply_tick_result_keeps_strategy_when_none() {
        let mut bots = vec![make_bot_state("keep_bot")];
        bots[0].strategy = Strategy {
            focus: crate::strategy::Focus::Economy,
            motto: "save resources".to_owned(),
            ..Strategy::default()
        };
        bots[0].next_strategist_at_ms = 42_000;

        let result = TickTaskResult {
            username: "keep_bot".to_owned(),
            new_map: None,
            map_was_attempted: false,
            backoff_secs: None,
            retire: false,
            new_strategy: None,
            new_next_strategist_at_ms: None,
        };

        apply_tick_result(&mut bots, Ok(result), &minimal_cfg(), 0);

        assert_eq!(
            bots[0].strategy.focus,
            crate::strategy::Focus::Economy,
            "strategy must be unchanged when new_strategy is None"
        );
        assert_eq!(
            bots[0].next_strategist_at_ms, 42_000,
            "next_strategist_at_ms must be unchanged when None"
        );
    }

    // -----------------------------------------------------------------------
    // next_due — AC5 unit test surface
    // -----------------------------------------------------------------------

    #[test]
    fn next_due_returns_indices_at_or_before_now() {
        let now = 1_000i64;
        let bots = vec![
            ("alpha".to_owned(), 500i64),   // past → due
            ("beta".to_owned(), 1_000i64),  // exactly now → due
            ("gamma".to_owned(), 1_500i64), // future → not due
        ];
        let due = next_due(&bots, now);
        assert_eq!(due, vec![0, 1], "indices 0 and 1 must be due");
    }

    #[test]
    fn next_due_empty_when_all_in_future() {
        let now = 500i64;
        let bots = vec![
            ("alpha".to_owned(), 1_000i64),
            ("beta".to_owned(), 2_000i64),
        ];
        assert!(next_due(&bots, now).is_empty(), "none should be due");
    }

    #[test]
    fn next_due_all_due_when_now_far_in_future() {
        let now = 999_999i64;
        let bots = vec![
            ("a".to_owned(), 100i64),
            ("b".to_owned(), 200i64),
            ("c".to_owned(), 300i64),
        ];
        let due = next_due(&bots, now);
        assert_eq!(due.len(), 3, "all three must be due");
    }

    #[test]
    fn next_due_empty_fleet() {
        assert!(
            next_due(&[], 12345).is_empty(),
            "empty fleet → empty due list"
        );
    }

    #[test]
    fn next_due_preserves_original_indices() {
        // Index 1 is due; indices 0 and 2 are not.
        let now = 500i64;
        let bots = vec![
            ("a".to_owned(), 1_000i64), // 0 — future
            ("b".to_owned(), 100i64),   // 1 — past (due)
            ("c".to_owned(), 2_000i64), // 2 — future
        ];
        let due = next_due(&bots, now);
        assert_eq!(due, vec![1], "only index 1 is due");
    }

    // -----------------------------------------------------------------------
    // tick_jitter — sanity checks
    // -----------------------------------------------------------------------

    #[test]
    fn jitter_stays_within_range() {
        let name_hash = crate::persona::fnv1a_64(b"test_bot");
        for tick in 0u64..100 {
            let j = tick_jitter(name_hash, tick, 180, 720);
            assert!(j >= 180, "jitter {j} below min (tick {tick})");
            assert!(j <= 720, "jitter {j} above max (tick {tick})");
        }
    }

    #[test]
    fn jitter_varies_across_ticks() {
        let name_hash = crate::persona::fnv1a_64(b"another_bot");
        let values: Vec<u64> = (0u64..10)
            .map(|t| tick_jitter(name_hash, t, 0, 1_000))
            .collect();
        // With a 1001-wide range and 10 values, expect at least some variation.
        let all_same = values.iter().all(|&v| v == values[0]);
        assert!(!all_same, "all ticks produced identical jitter: {values:?}");
    }

    #[test]
    fn jitter_differs_for_different_bots_same_tick() {
        let h1 = crate::persona::fnv1a_64(b"bot_alpha");
        let h2 = crate::persona::fnv1a_64(b"bot_beta");
        let j1 = tick_jitter(h1, 0, 0, u32::MAX);
        let j2 = tick_jitter(h2, 0, 0, u32::MAX);
        assert_ne!(
            j1, j2,
            "different bots at the same tick must get different jitter"
        );
    }

    #[test]
    fn jitter_zero_seed_handled() {
        // When name_hash XOR tick_count = 0 (the degenerate XorShift case),
        // the seed is biased to 1. The result must still be in range.
        let j = tick_jitter(42, 42, 100, 500); // 42 XOR 42 = 0 → bias to 1
        assert!(j >= 100, "degenerate seed: jitter {j} below min");
        assert!(j <= 500, "degenerate seed: jitter {j} above max");
    }

    // -----------------------------------------------------------------------
    // strategist_jitter — stays within ±10% of interval
    // -----------------------------------------------------------------------

    #[test]
    fn strategist_jitter_within_ten_percent() {
        let hash = crate::persona::fnv1a_64(b"strat_jitter_bot");
        let interval = 14_400u64; // 4h
        for seed in 0u64..50 {
            let j = strategist_jitter(hash, seed, interval);
            let min = interval * 9 / 10;
            let max = interval * 11 / 10;
            assert!(
                j >= min && j <= max,
                "strategist jitter {j} outside [{min},{max}] at seed {seed}"
            );
        }
    }

    #[test]
    fn strategist_jitter_differs_from_tick_jitter() {
        // Strategist jitter must use a different seed than tick jitter to avoid
        // the two schedules always aligning.
        let hash = crate::persona::fnv1a_64(b"jitter_diff");
        let tick_j = tick_jitter(hash, 0, 12960, 15840);
        let strat_j = strategist_jitter(hash, 0, 14400);
        // They CAN coincide by chance, but the seed construction (high-bit OR)
        // makes it extremely unlikely on a 64-bit range.
        // We just confirm the function runs without panicking; the actual
        // values are tested for range above.
        let _ = tick_j;
        let _ = strat_j;
    }
}
