//! E2E tests for the bot-runner lib: spawn an in-process web server, seed bots over HTTP + SQL,
//! then drive one forced tick through the lib.
//!
//! # Coverage
//!
//! - **AC6** (`forced_tick_orders_appear`): a live bot places a build order and a training batch
//!   in a single tick with `dry_run = false`.
//! - **AC6b** (`dry_run_writes_nothing`): the same tick with `dry_run = true` leaves both queues
//!   empty.
//! - **AC1** (`dead_key_is_dropped`): `manifest::validate` returns `Ok` for a live key and `Err`
//!   for a garbage token — the fleet-startup validation seam.
//!
//! # Harness
//!
//! Trimmed copy of `crates/web/tests/integration.rs::spawn` — the runner is a separate crate and
//! web's spawn helper is test-private.  Each `#[sqlx::test]` gets a freshly-migrated, isolated
//! database so all three tests run concurrently without interference.

use axum_extra::extract::cookie::Key;
use eperica_bots::client::ApiClient;
use eperica_bots::executor::execute_intents;
use eperica_bots::manifest::{ManifestEntry, validate};
use eperica_bots::persona::Persona;
use eperica_bots::policy::{Intent, plan_tick};
use eperica_bots::runner::{LlmConfig, RunnerConfig, run_fleet_until};
use eperica_bots::strategist::ScriptedBackend;
use eperica_bots::strategy::{Strategy, parse_reply};
use eperica_domain::{GameSpeed, WorldConfig, WorldMap};
use eperica_infrastructure::{
    Argon2Hasher, ChatHub, NotificationHub, PgAccountRepository, ensure_world, fair_play_rules,
    load_world_rules, run_chat_listener, run_notification_listener,
};
use eperica_web::registry::WorldRegistry;
use eperica_web::state::AppState;
use eperica_web::{apikey, router};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

// ---------------------------------------------------------------------------
// Trimmed copy of the web integration harness
// ---------------------------------------------------------------------------

/// Spawn an app instance on an ephemeral port; returns its base URL.
///
/// Trimmed copy of `crates/web/tests/integration.rs::spawn` — the runner is a separate crate and
/// web's helper is test-private.  Only the fields required by `AppState` are wired; no
/// process_due_* helpers or cookie-aware clients are imported.
async fn spawn(pool: sqlx::PgPool) -> String {
    let config = WorldConfig::new(GameSpeed::new(1.0).unwrap(), 50);
    let world = ensure_world(&pool, &config).await.expect("ensure world");
    let world_rules = Arc::new(load_world_rules(&world.rule_preset).expect("world rules"));
    let map = Arc::new(WorldMap::new(
        world.seed as u64,
        config.radius,
        world_rules.map_rules.clone(),
    ));
    let state = AppState {
        accounts: Arc::new(PgAccountRepository::new(
            pool.clone(),
            world.id,
            world.seed,
            config.radius,
            world_rules.economy.starting_amounts,
            world_rules.lifecycle.beginner_protection_secs,
            config.speed,
        )),
        hasher: Arc::new(Argon2Hasher),
        world_rules: Arc::clone(&world_rules),
        fair_play_rules: Arc::new(fair_play_rules().expect("fair-play rules")),
        // Trust forwarded headers (consistent with the web integration harness).
        trust_proxy: true,
        chat_hub: {
            let hub = ChatHub::new();
            tokio::spawn(run_chat_listener(pool.clone(), hub.clone()));
            hub
        },
        notification_hub: {
            let hub = NotificationHub::new();
            tokio::spawn(run_notification_listener(pool.clone(), hub.clone()));
            hub
        },
        map,
        artifact_release_offset_secs: 90 * 86_400,
        wonder_release_offset_secs: 120 * 86_400,
        world: config,
        world_id: world.id,
        world_registry: {
            let (tx, rx) = tokio::sync::watch::channel(false);
            // Keep the sender alive for the test's lifetime so a spawned scheduler doesn't see a
            // closed channel and exit immediately (mirrors the web harness comment).
            std::mem::forget(tx);
            Arc::new(WorldRegistry::new(
                pool.clone(),
                rx,
                world_rules.lifecycle.beginner_protection_secs,
                world.rule_preset.clone(),
                Arc::clone(&world_rules),
            ))
        },
        require_email_confirmation: false,
        cookie_key: Key::generate(),
    };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(
            listener,
            router(state).into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap();
    });
    format!("http://{addr}")
}

/// Monotonically unique prefix generator (mirrors the web harness).
fn unique(prefix: &str) -> String {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{prefix}_{t}_{n}")
}

/// Register a bot account over HTTP, flag `is_ai`, and insert an agent key.
///
/// When `with_barracks_and_resources` is true, also inserts a Barracks at slot 4 and tops up
/// resources to 5 000 each — copied from `integration.rs::agent_api_economy_actions` so a real
/// training order can succeed (tier-1 units need no research).
///
/// Returns the plaintext bearer token.
async fn seed_bot(
    pool: &sqlx::PgPool,
    base: &str,
    user: &str,
    with_barracks_and_resources: bool,
) -> String {
    let email = format!("{user}@example.com");
    // Plain reqwest client — bearer-auth only, no cookie jar needed for registration.
    let http = reqwest::Client::new();
    http.post(format!("{base}/register"))
        .form(&[
            ("username", user),
            ("email", email.as_str()),
            ("password", "secret12"),
            ("tribe", "teutons"),
        ])
        .send()
        .await
        .expect("POST /register must succeed");

    sqlx::query("UPDATE users SET is_ai = TRUE WHERE username = $1")
        .bind(user)
        .execute(pool)
        .await
        .unwrap();

    let (key, token) = apikey::generate();
    sqlx::query(
        "INSERT INTO agent_keys (id, user_id, secret_hash) \
         VALUES ($1, (SELECT id FROM users WHERE username = $2), $3)",
    )
    .bind(&key.id)
    .bind(user)
    .bind(apikey::secret_hash(&key.secret))
    .execute(pool)
    .await
    .unwrap();

    if with_barracks_and_resources {
        // villages.owner_id references users.id (0001_initial_schema + 0043_players backfill sets
        // players.id = users.id in the single-world case, so both join forms work).
        let village_id: uuid::Uuid = sqlx::query_scalar(
            "SELECT v.id FROM villages v \
             JOIN users u ON u.id = v.owner_id \
             WHERE u.username = $1",
        )
        .bind(user)
        .fetch_one(pool)
        .await
        .unwrap();

        // Barracks level 1 so training can genuinely succeed (tier-1 needs no research).
        // Slot 4 — a free general slot not claimed by the starting village (slots 0/1 are
        // main_building / rally_point).  Exact SQL from agent_api_economy_actions.
        sqlx::query(
            "INSERT INTO village_buildings (village_id, slot, building_type, level) \
             VALUES ($1, 4, 'barracks', 1)",
        )
        .bind(village_id)
        .execute(pool)
        .await
        .unwrap();

        // Top up resources — exact SQL from agent_api_economy_actions.
        sqlx::query(
            "UPDATE village_resources \
             SET wood = 5000, clay = 5000, iron = 5000, crop = 5000, updated_at = now() \
             WHERE village_id = $1",
        )
        .bind(village_id)
        .execute(pool)
        .await
        .unwrap();
    }

    token
}

// ---------------------------------------------------------------------------
// AC6 — forced tick places build order + training batch
// ---------------------------------------------------------------------------

/// AC6: one forced tick via the lib places a build order **and** a training batch.
///
/// Seeding: teutons bot, Barracks level 1, resources 5 000 each, empty queues, empty garrison.
///
/// Policy outcome (with the seeded state):
/// - Rule 2 (Storage): resources 5 000 >> 90 % capacity → Build{granary} (storage first, per doctrine).
/// - Rule 5 (Training): garrison 0 < floor (10 + 10 × aggression) → Train{clubswinger, N}.
///
/// After executing both intents (dry_run = false), the next digest must show
/// a build order (granary) in `build_queue` and a training batch in `training`.
///
#[sqlx::test(migrations = "../../migrations")]
async fn forced_tick_orders_appear(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let user = unique("bot");
    let token = seed_bot(&pool, &base, &user, true).await;

    let client = ApiClient::new(&base, &token);

    // me() — key introspection: find the home world and tribe.
    let me = client
        .me()
        .await
        .expect("me() must succeed with a live key");
    assert!(!me.worlds.is_empty(), "bot must be enrolled in a world");
    let world_entry = &me.worlds[0];
    let world = &world_entry.world;

    let tribe =
        eperica_bots::policy::BotTribe::parse(&world_entry.tribe).expect("wire tribe parses");

    // Fetch the full state digest (one call per tick, per the spec).
    let digest = client.state(world).await.expect("state() must succeed");
    assert!(
        !digest.villages.is_empty(),
        "bot village must appear in digest"
    );

    // Derive the deterministic persona from the bot's username (AC4).
    let persona = Persona::from_name(&user);

    // plan_tick is window-agnostic — the window check lives in the runner (which we do not invoke
    // here).  Call plan_tick directly to exercise the policy without the infinite fleet loop.
    let intents = plan_tick(
        &digest,
        None,
        &persona,
        &Strategy::default(),
        digest.now_ms,
        tribe,
    );
    assert!(
        !intents.is_empty(),
        "plan_tick must produce intents: {intents:?}"
    );
    assert!(
        intents.iter().any(|i| matches!(i, Intent::Build { .. })),
        "intents must include a Build: {intents:?}"
    );

    // Execute the intents against the live server (dry_run = false).
    let report = execute_intents(&client, world, &intents, false).await;
    assert!(
        !report.retire,
        "key must not be retired after execution: {report:?}"
    );

    // Verify the next digest reflects the placed orders.
    let digest2 = client
        .state(world)
        .await
        .expect("second state() must succeed");
    let v = &digest2.villages[0];
    assert_eq!(
        v.build_queue.len(),
        1,
        "build queue must have exactly one entry after one tick: {v:?}"
    );
    // Storage is the first doctrine rule to fire on the seeded state (resources >> 90% of
    // capacity) → a granary build order is placed (crop checked before non-crop).
    assert_eq!(
        v.build_queue[0].kind.as_deref(),
        Some("granary"),
        "build queue entry must be a granary (storage-first doctrine): {v:?}"
    );
    assert_eq!(
        v.training.len(),
        1,
        "training queue must have exactly one batch after one tick: {v:?}"
    );
}

// ---------------------------------------------------------------------------
// AC6b — dry_run leaves both queues empty
// ---------------------------------------------------------------------------

/// AC6b: `dry_run = true` executes no orders — both queues remain empty in the next digest.
#[sqlx::test(migrations = "../../migrations")]
async fn dry_run_writes_nothing(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let user = unique("bot_dry");
    let token = seed_bot(&pool, &base, &user, true).await;

    let client = ApiClient::new(&base, &token);

    let me = client.me().await.expect("me() must succeed");
    assert!(!me.worlds.is_empty(), "bot must be enrolled in a world");
    let world_entry = &me.worlds[0];
    let world = &world_entry.world;
    let tribe =
        eperica_bots::policy::BotTribe::parse(&world_entry.tribe).expect("wire tribe parses");

    let digest = client.state(world).await.expect("state() must succeed");
    let persona = Persona::from_name(&user);

    let intents = plan_tick(
        &digest,
        None,
        &persona,
        &Strategy::default(),
        digest.now_ms,
        tribe,
    );
    assert!(
        !intents.is_empty(),
        "plan_tick must produce intents: {intents:?}"
    );

    // Execute with dry_run = true — no HTTP POST calls are issued.
    let report = execute_intents(&client, world, &intents, true).await;
    assert!(
        !report.retire,
        "key must not be retired in dry-run: {report:?}"
    );

    // Re-fetch: both queues must still be empty (no actual writes).
    let digest2 = client
        .state(world)
        .await
        .expect("second state() must succeed");
    let v = &digest2.villages[0];
    assert_eq!(
        v.build_queue.len(),
        0,
        "dry-run must not place build orders: {v:?}"
    );
    assert_eq!(
        v.training.len(),
        0,
        "dry-run must not place training orders: {v:?}"
    );
}

// ---------------------------------------------------------------------------
// M4c — fleet loop ticks two bots through the cap=1 semaphore
// ---------------------------------------------------------------------------

/// M4c: `run_fleet_until` with two bots and cap=1 ticks both bots at least once.
///
/// The cap=1 semaphore serialises all tick tasks; with tick_scale=1 and a 6-second
/// shutdown window, both bots have ample time to tick.  Exercises the scheduler
/// (`next_due`), the semaphore cap, and the graceful drain on shutdown.
#[sqlx::test(migrations = "../../migrations")]
async fn fleet_loop_ticks_two_bots(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;

    let user1 = unique("fleet_a");
    let user2 = unique("fleet_b");
    let token1 = seed_bot(&pool, &base, &user1, true).await;
    let token2 = seed_bot(&pool, &base, &user2, true).await;

    // Find the world UUID from the first bot's /api/me response.
    let client1 = ApiClient::new(&base, &token1);
    let me1 = client1.me().await.expect("me() for fleet_a");
    assert!(
        !me1.worlds.is_empty(),
        "fleet_a must be enrolled in a world"
    );
    let world = me1.worlds[0].world.clone();

    let client2 = ApiClient::new(&base, &token2);

    // Write a temp manifest so run_fleet_until can load both bots.
    let manifest_path =
        std::env::temp_dir().join(format!("eperica_fleet_test_{}.json", unique("m")));
    let manifest_json = serde_json::json!([
        {"username": user1, "token": token1},
        {"username": user2, "token": token2},
    ])
    .to_string();
    std::fs::write(&manifest_path, &manifest_json).expect("write temp manifest");

    let cfg = RunnerConfig {
        server: base.clone(),
        world: world.clone(),
        keys_path: manifest_path.to_str().unwrap().to_owned(),
        dry_run: false,
        tick_scale: Some(1), // 1-second ticks to keep the test fast
        cap: 1,              // serialise: exercises the semaphore
        open_window: true,   // bypass activity-window check for determinism
        llm: None,           // no LLM in this test
    };

    // Run the fleet for 6 seconds (enough for each bot to tick several times).
    // No backend (None) → strategist disabled; behaviour identical to 121 baseline.
    run_fleet_until(cfg, tokio::time::sleep(Duration::from_secs(6)), None).await;

    let _ = std::fs::remove_file(&manifest_path);

    // Both bots must have ticked and placed build orders (granary via storage rule).
    let d1 = client1.state(&world).await.expect("state for fleet_a");
    let d2 = client2.state(&world).await.expect("state for fleet_b");

    assert!(
        !d1.villages[0].build_queue.is_empty(),
        "fleet_a build queue must be non-empty after fleet run: {:?}",
        d1.villages[0]
    );
    assert!(
        !d2.villages[0].build_queue.is_empty(),
        "fleet_b build queue must be non-empty after fleet run: {:?}",
        d2.villages[0]
    );
}

// ---------------------------------------------------------------------------
// AC1 — dead key is identified at fleet startup (validate seam)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// AC7 — strategist biases differ from baseline (lib-level, no fleet loop)
// ---------------------------------------------------------------------------

/// AC7: `plan_tick` with a strategy overlay produces different intents than the no-strategy
/// baseline.  Tested at the lib level (no fleet loop) for determinism.
///
/// # Setup
///
/// Seed a Teuton bot with Barracks + 5 000 resources + **25 clubswingers** in the garrison.
/// The garrison count 25 is the critical pivot for aggression-override assertions:
///
/// - `aggression = Some(0)` → floor = 10 + 10×0 = 10.  garrison 25 ≥ 10 → **NO** Train intent.
/// - `aggression = Some(3)` + `focus = Military` → floor = 2×(10 + 10×3) = 80.
///   garrison 25 < 80 → **Train** intent present.
///
/// Both assertions override `aggression` via the strategy, so the result is persona-independent
/// regardless of which aggression the FNV-1a persona hash produces for the unique bot username.
///
/// # Invalid-reply path
///
/// `parse_reply` is verified to reject a prose-wrapped reply (Err).  The runner keeps the prior
/// strategy on rejection; the test demonstrates this by showing that calling `plan_tick` with the
/// *prior* strategy (`strat_agg0`) produces the same intents as the baseline — i.e. no Train.
#[sqlx::test(migrations = "../../migrations")]
async fn strategist_biases_next_tick(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    // "s_bias" = 6 chars; unique() appends "_<19-digit-nanos>_<counter>".
    // Total ≤ 6+1+19+1+3 = 30 chars, safely under the 32-char username limit.
    let user = unique("s_bias");
    // Seed with barracks + resources so the bot is capable of training and building.
    let token = seed_bot(&pool, &base, &user, true).await;

    // Insert 25 tier-1 Teuton units (clubswinger) into the garrison directly via SQL.
    // This gives us a deterministic garrison count that straddles the aggression-floor boundary:
    //   agg=0 → floor 10 → garrison 25 ≥ 10 → no train
    //   agg=3 military → floor 80 → garrison 25 < 80 → train
    let village_id: uuid::Uuid = sqlx::query_scalar(
        "SELECT v.id FROM villages v \
         JOIN users u ON u.id = v.owner_id \
         WHERE u.username = $1",
    )
    .bind(&user)
    .fetch_one(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO village_units (village_id, unit_id, count) \
         VALUES ($1, 'clubswinger', 25)",
    )
    .bind(village_id)
    .execute(&pool)
    .await
    .unwrap();

    let client = ApiClient::new(&base, &token);
    let me = client.me().await.expect("me() must succeed");
    assert!(!me.worlds.is_empty(), "bot must be enrolled in a world");
    let world_entry = &me.worlds[0];
    let world = &world_entry.world;
    let tribe = eperica_bots::policy::BotTribe::parse(&world_entry.tribe).expect("tribe parses");
    let persona = Persona::from_name(&user);

    // Fetch the digest once (mirrors one tick's single fetch).
    let digest = client.state(world).await.expect("state() must succeed");

    // ------------------------------------------------------------------
    // Baseline: aggression override = 0 → floor = 10, garrison 25 ≥ 10 → NO Train.
    // Using Some(0) instead of Strategy::default() so the assertion is
    // persona-independent — the persona aggression (0..=3) is irrelevant here.
    // ------------------------------------------------------------------
    let strat_agg0 = Strategy {
        aggression: Some(0),
        ..Strategy::default()
    };
    let intents_agg0 = plan_tick(&digest, None, &persona, &strat_agg0, digest.now_ms, tribe);
    assert!(
        !intents_agg0
            .iter()
            .any(|i| matches!(i, Intent::Train { .. })),
        "agg override=0 with garrison=25: floor=10 ≤ 25 → must NOT produce a Train intent: \
         {intents_agg0:?}"
    );

    // ------------------------------------------------------------------
    // Biased: focus=military, aggression=3 → floor = 2×(10+30) = 80, garrison 25 < 80 → Train.
    // parse_reply is the canonical entry point for LLM output (tests the AC3 contract path).
    // ------------------------------------------------------------------
    let reply = parse_reply(r#"{"focus":"military","aggression":3,"motto":"blood and iron"}"#)
        .expect("valid military+agg3 reply must parse");
    let intents_biased = plan_tick(
        &digest,
        None,
        &persona,
        &reply.strategy,
        digest.now_ms,
        tribe,
    );
    assert!(
        intents_biased
            .iter()
            .any(|i| matches!(i, Intent::Train { .. })),
        "military+agg3 with garrison=25: floor=80 > 25 → MUST produce a Train intent: \
         {intents_biased:?}"
    );

    // ------------------------------------------------------------------
    // Invalid-reply path: prose-wrapped JSON is rejected (AC3 no-fallback rule).
    // The runner keeps the prior strategy on rejection → behavior stays at the prior level.
    // ------------------------------------------------------------------
    let bad_raw =
        r#"Sure! Here's my plan: {"focus":"military","aggression":3,"motto":"blood and iron"}"#;
    assert!(
        parse_reply(bad_raw).is_err(),
        "prose-wrapped reply must be rejected by parse_reply (AC3 strict contract)"
    );
    // Since the reply was rejected, the prior strategy (strat_agg0) stays in force.
    // plan_tick is deterministic: calling it again with the same inputs produces the same output.
    let intents_after_reject =
        plan_tick(&digest, None, &persona, &strat_agg0, digest.now_ms, tribe);
    assert_eq!(
        intents_agg0, intents_after_reject,
        "rejected LLM reply must leave behavior at the prior strategy (plan_tick is deterministic)"
    );
}

// ---------------------------------------------------------------------------
// AC1 — dead key is identified at fleet startup (validate seam)
// ---------------------------------------------------------------------------

/// AC1: `manifest::validate` returns `Ok(MeResponse)` for a live key and `Err(ApiFailure)` for a
/// garbage token.
///
/// This exercises the fleet-startup contract: the runner promotes live entries to bots and logs or
/// drops dead ones without failing the fleet.  We test the `validate` seam directly rather than
/// running the infinite fleet loop.
#[sqlx::test(migrations = "../../migrations")]
async fn dead_key_is_dropped(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let user = unique("bot_live");
    // Seed a live bot — no barracks or resources needed; we only test key validation here.
    let token = seed_bot(&pool, &base, &user, false).await;

    // Manifest with one live entry and one garbage token.
    let entries = vec![
        ManifestEntry {
            username: user.clone(),
            token: token.clone(),
        },
        ManifestEntry {
            username: "dead_bot".to_owned(),
            // Same garbage token shape used in integration.rs::agent_api_bearer_auth.
            token: "epk_0000000000000000_wrongwrongwrongwrongwrongwrongwrongwrongwro".to_owned(),
        },
    ];

    let results = validate(&base, entries).await;
    assert_eq!(results.len(), 2, "one result per manifest entry");

    // Live key → Ok(MeResponse).
    let (live_entry, live_result) = &results[0];
    assert_eq!(live_entry.username, user, "first result is the live bot");
    assert!(
        live_result.is_ok(),
        "live key must validate successfully; got: {live_result:?}"
    );

    // Garbage key → Err(ApiFailure).
    let (dead_entry, dead_result) = &results[1];
    assert_eq!(
        dead_entry.username, "dead_bot",
        "second result is the dead bot"
    );
    assert!(
        dead_result.is_err(),
        "garbage key must fail validation; got: {dead_result:?}"
    );
}

// ---------------------------------------------------------------------------
// AC6 + fleet-level AC7 — scripted strategist sends a DM and applies strategy
// ---------------------------------------------------------------------------

/// AC6 + fleet-level AC7: a fleet run with a scripted LLM backend:
/// 1. Sends exactly one diplomatic DM to the target player (AC6).
/// 2. Produces training orders consistent with the applied `military + agg=3` strategy (AC7).
///
/// # Setup
///
/// - **Bot A** (strategist): Teuton, Barracks level 1, resources 5 000, garrison 25 clubswingers.
///   Garrison 25 < military+agg3 floor (80) → Train must fire after the strategy is applied.
/// - **diplomat_target**: a plain registered human user (no agent key); only needed as a valid
///   message recipient.
///
/// # ScriptedBackend
///
/// One `Ok` reply with `focus=military, aggression=3` and a message to `diplomat_target`.
/// After the first reply the backend is exhausted and returns `Err("scripted backend exhausted")`
/// on all subsequent calls — exercising the error path in the fleet loop (strategy unchanged).
///
/// # LlmConfig
///
/// `interval_secs = 1` so the initial strategist call is scheduled within 0–1 s of fleet start.
/// `budget_per_hour = 10` (well above the 1–2 calls the test makes).
/// `tick_scale = Some(1)` forces 1-second inter-tick intervals (no jitter).
///
/// # Assertions
///
/// After a 6-second bounded run:
/// (a) Exactly **one** `direct_messages` row from bot_A to diplomat_target (no duplicate sends).
/// (b) At least one active `training_orders` row for the bot's village (Train intent executed).
#[sqlx::test(migrations = "../../migrations")]
async fn strategist_fleet_cycle_applies_and_messages(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;

    // --- Seed bot A (the strategist bot) ---
    // "s_fl" = 4 chars; unique() appends "_<19-digit-nanos>_<counter>".
    // Total ≤ 4+1+19+1+3 = 28 chars, safely under the 32-char username limit.
    let bot_user = unique("s_fl");
    let token = seed_bot(&pool, &base, &bot_user, true).await;

    // Seed 25 clubswingers so garrison=25 < military+agg3 floor 80 → Train fires deterministically.
    let bot_village_id: uuid::Uuid = sqlx::query_scalar(
        "SELECT v.id FROM villages v \
         JOIN users u ON u.id = v.owner_id \
         WHERE u.username = $1",
    )
    .bind(&bot_user)
    .fetch_one(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO village_units (village_id, unit_id, count) \
         VALUES ($1, 'clubswinger', 25)",
    )
    .bind(bot_village_id)
    .execute(&pool)
    .await
    .unwrap();

    // --- Register diplomat_target (plain human, no agent key, no is_ai flag) ---
    // "diplo" = 5 chars → total ≤ 5+1+19+1+3 = 29 chars, under the 32-char limit.
    let target_user = unique("diplo");
    let http = reqwest::Client::new();
    http.post(format!("{base}/register"))
        .form(&[
            ("username", target_user.as_str()),
            ("email", format!("{target_user}@example.com").as_str()),
            ("password", "secret12"),
            ("tribe", "romans"),
        ])
        .send()
        .await
        .expect("diplomat_target registration must succeed");

    // --- Resolve the world UUID from bot A's /api/me ---
    let client_a = ApiClient::new(&base, &token);
    let me_a = client_a.me().await.expect("me() for bot A");
    assert!(!me_a.worlds.is_empty(), "bot A must be in a world");
    let world = me_a.worlds[0].world.clone();

    // --- Build the scripted backend ---
    // One Ok reply carrying the strategy + a diplomatic message to diplomat_target.
    // Subsequent calls return Err("scripted backend exhausted") — exercises the error path.
    // Note: format! into a single-line string so the resulting JSON has no literal backslash
    // or newline characters (both are invalid inside a JSON string value).
    let scripted_reply = format!(
        r#"{{"focus":"military","aggression":3,"motto":"strike first","message":{{"to":"{target_user}","body":"Your fields look ripe, neighbour."}}}}"#
    );
    let scripted = Arc::new(ScriptedBackend::new(vec![Ok(scripted_reply)]));

    // --- Write a temp manifest with bot A only ---
    let manifest_path =
        std::env::temp_dir().join(format!("eperica_strat_fleet_{}.json", unique("m")));
    let manifest_json = serde_json::json!([
        {"username": bot_user, "token": token},
    ])
    .to_string();
    std::fs::write(&manifest_path, &manifest_json).expect("write temp manifest");

    // --- Fleet config ---
    // tick_scale=1 → 1-second inter-tick interval (no jitter).
    // interval_secs=1 → strategist fires within the first 1–2 ticks.
    // open_window=true → bypasses the activity-window UTC-hour check.
    let cfg = RunnerConfig {
        server: base.clone(),
        world: world.clone(),
        keys_path: manifest_path.to_str().unwrap().to_owned(),
        dry_run: false,
        tick_scale: Some(1),
        cap: 1,
        open_window: true,
        llm: Some(LlmConfig {
            model: "scripted".to_owned(),
            budget_per_hour: 10,
            interval_secs: 1,
        }),
    };

    // Run the fleet for 6 seconds — enough for several ticks and the first strategist cycle.
    // No sleep loop: run_fleet_until bounds the run precisely.
    run_fleet_until(
        cfg,
        tokio::time::sleep(Duration::from_secs(6)),
        Some(scripted),
    )
    .await;

    let _ = std::fs::remove_file(&manifest_path);

    // --- (a) Assert: exactly one DM from bot_A to diplomat_target ---
    // direct_messages.sender_id and recipient_id reference users.id (per migration 0035).
    // In the single-world test DB, users.id equals the player id used by the comms layer.
    let dm_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM direct_messages \
         WHERE sender_id = (SELECT id FROM users WHERE username = $1) \
         AND recipient_id = (SELECT id FROM users WHERE username = $2)",
    )
    .bind(&bot_user)
    .bind(&target_user)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(
        dm_count, 1,
        "exactly one DM must be sent (one scripted Ok cycle; exhausted backend prevents duplicates)"
    );

    // --- (b) Assert: at least one training order for the bot's village ---
    // After the military+agg3 strategy is applied, garrison 25 < floor 80 → Train fires.
    // execute_intents posts to /api/w/{world}/village/{id}/train → creates a training_orders row.
    let training_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM training_orders WHERE village_id = $1")
            .bind(bot_village_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    assert!(
        training_count > 0,
        "at least one training order must exist after military+agg3 strategy \
         (garrison 25 < floor 80): training_count={training_count}"
    );
}
