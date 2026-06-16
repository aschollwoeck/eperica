//! End-to-end HTTP tests driving the real app over Postgres (T18/T19).
//!
//! Each test spins the app on an ephemeral port and uses a cookie-aware client. They skip when
//! `DATABASE_URL` is not set, so `cargo test` stays green without a database.

use axum_extra::extract::cookie::Key;
use eperica_application::{
    process_due_combat, process_due_movements, process_due_oasis_combat, process_due_scouts,
    process_due_settles, process_due_trades,
};
use eperica_domain::{
    Coordinate, GameSpeed, PlayerId, TileKind, Timestamp, Tribe, WorldConfig, WorldId, WorldMap,
    coordinates_within,
};
use eperica_infrastructure::{
    Argon2Hasher, ChatHub, NotificationHub, PgAccountRepository, combat_rules, culture_rules,
    economy_rules, ensure_world, fair_play_rules, lifecycle_rules, load_world_rules, loyalty_rules,
    map_rules, merchant_rules, now, oasis_rules, ranking_rules, run_chat_listener,
    run_notification_listener, scout_rules, starting_village, unit_rules,
};
use eperica_web::registry::WorldRegistry;
use eperica_web::router;
use eperica_web::state::AppState;
use reqwest::header::LOCATION;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Spawn an app instance over the given (per-test, isolated) pool; returns its base URL.
///
/// Each `#[sqlx::test]` hands us a freshly-migrated, private database, so app instances are fully
/// isolated and the suite runs in parallel. `account_persists_across_restart` calls this twice with
/// the same pool to model a restart against the same persistent storage.
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
        // Tests trust the forwarded headers so they can control the client IP deterministically.
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
            // Keep the shutdown sender alive for the test's lifetime so a scheduler the registry spawns
            // (e.g. via POST /admin/world) does not see a closed channel and exit immediately.
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

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .cookie_store(true)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
}

/// Lift beginner's protection from every account (019) — combat-flow tests attack freshly-registered
/// defenders, who would otherwise be protected.
async fn clear_protection(pool: &sqlx::PgPool) {
    sqlx::query("UPDATE users SET protected_until = NULL")
        .execute(pool)
        .await
        .unwrap();
}

fn unique(prefix: &str) -> String {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{prefix}_{t}_{n}")
}

#[sqlx::test(migrations = "../../migrations")]
async fn register_creates_village_and_view_is_fast(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let c = client();
    let user = unique("reg");
    let email = format!("{user}@example.com");

    // AC1/AC3: register redirects to the village.
    let res = c
        .post(format!("{base}/register"))
        .form(&[
            ("username", user.as_str()),
            ("email", email.as_str()),
            ("password", "secret12"),
            ("tribe", "gauls"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 303);
    assert_eq!(
        res.headers().get(LOCATION).unwrap().to_str().unwrap(),
        "/village"
    );

    // Warm, then measure the read path (P11 / T19): GET /village under the 50 ms budget.
    let _ = c.get(format!("{base}/village")).send().await.unwrap();
    let mut best = std::time::Duration::MAX;
    let mut body = String::new();
    for _ in 0..3 {
        let started = std::time::Instant::now();
        let view = c.get(format!("{base}/village")).send().await.unwrap();
        let elapsed = started.elapsed();
        assert_eq!(view.status().as_u16(), 200);
        body = view.text().await.unwrap();
        best = best.min(elapsed);
    }

    assert!(body.contains(&user)); // AC3: owned by this player
    assert!(body.contains("Wood")); // resources shown
    assert!(body.contains("/h")); // production rate shown (AC7)
    // P11: the read path is fast. The production budget is 50 ms server-side; this end-to-end check
    // runs under full parallel-test DB contention, so it takes the best of three requests against a
    // looser bound — contention noise can't fail it, a real regression still does (it is
    // comfortably < 50 ms in isolation).
    assert!(
        best.as_millis() < 250,
        "GET /village took {best:?} at best, far over budget"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn login_succeeds_and_rejects_bad_password(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let user = unique("log");
    let email = format!("{user}@example.com");

    client()
        .post(format!("{base}/register"))
        .form(&[
            ("username", user.as_str()),
            ("email", email.as_str()),
            ("password", "secret12"),
            ("tribe", "gauls"),
        ])
        .send()
        .await
        .unwrap();

    // AC2: wrong password is rejected with a message (no redirect).
    let bad = client()
        .post(format!("{base}/login"))
        .form(&[("username", user.as_str()), ("password", "wrongpass")])
        .send()
        .await
        .unwrap();
    assert_eq!(bad.status().as_u16(), 200);
    assert!(bad.text().await.unwrap().contains("Invalid"));

    // AC2: correct credentials log in (redirect to village).
    let ok = client()
        .post(format!("{base}/login"))
        .form(&[("username", user.as_str()), ("password", "secret12")])
        .send()
        .await
        .unwrap();
    assert_eq!(ok.status().as_u16(), 303);
}

#[sqlx::test(migrations = "../../migrations")]
async fn village_requires_login(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    // AC7: an unauthenticated visitor cannot view a village; they are redirected to login.
    let res = client()
        .get(format!("{base}/village"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 303);
    assert_eq!(
        res.headers().get(LOCATION).unwrap().to_str().unwrap(),
        "/login"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn register_offers_tribes_and_village_shows_choice(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    // 004 AC15: the registration form offers the three tribes with descriptions.
    let form = client()
        .get(format!("{base}/register"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    for tribe in ["Romans", "Teutons", "Gauls"] {
        assert!(form.contains(tribe), "register page missing {tribe}");
    }
    assert!(form.contains("name=\"tribe\""));

    // 004 AC1/AC15: registering as Teutons shows that tribe on the village page.
    let user = unique("tribe");
    let email = format!("{user}@example.com");
    let c = client();
    c.post(format!("{base}/register"))
        .form(&[
            ("username", user.as_str()),
            ("email", email.as_str()),
            ("password", "secret12"),
            ("tribe", "teutons"),
        ])
        .send()
        .await
        .unwrap();
    let body = c
        .get(format!("{base}/village"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body.contains("Tribe: Teutons"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn academy_and_smithy_flow(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;

    let user = unique("acad");
    let email = format!("{user}@example.com");
    let c = client();
    c.post(format!("{base}/register"))
        .form(&[
            ("username", user.as_str()),
            ("email", email.as_str()),
            ("password", "secret12"),
            ("tribe", "gauls"),
        ])
        .send()
        .await
        .unwrap();

    // 004 AC15: without an Academy the page explains the requirement and offers no action.
    let body = c
        .get(format!("{base}/village/academy"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body.contains("requires an"));
    assert!(!body.contains("name=\"unit\""));

    // Seed an Academy + Smithy directly (constructing them via the UI would take game-hours) and
    // top up resources — the page logic under test is the research/upgrade flow, not construction.
    let village_id: uuid::Uuid = sqlx::query_scalar(
        "SELECT v.id FROM villages v JOIN users u ON u.id = v.owner_id WHERE u.username = $1",
    )
    .bind(&user)
    .fetch_one(&pool)
    .await
    .unwrap();
    // Warehouse/Granary raise capacity so the seeded resources are not clamped to the base cap.
    for (slot, kind, level) in [
        (2_i16, "warehouse", 10_i16),
        (3, "granary", 10),
        (5, "academy", 1),
        (6, "smithy", 1),
    ] {
        sqlx::query(
            "INSERT INTO village_buildings (village_id, slot, building_type, level) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(village_id)
        .bind(slot)
        .bind(kind)
        .bind(level)
        .execute(&pool)
        .await
        .unwrap();
    }
    sqlx::query(
        "UPDATE village_resources SET wood = 5000, clay = 5000, iron = 5000, crop = 5000, \
         updated_at = now() WHERE village_id = $1",
    )
    .bind(village_id)
    .execute(&pool)
    .await
    .unwrap();

    // The Academy lists the Gaul roster: tier-1 researched, Swordsman researchable.
    let body = c
        .get(format!("{base}/village/academy"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body.contains("Phalanx"));
    assert!(body.contains("Researched"));
    assert!(body.contains("Swordsman"));
    assert!(body.contains("value=\"swordsman\""));

    // Order the research: PRG back to the Academy, which now shows the countdown (AC6/AC15).
    let res = c
        .post(format!("{base}/village/academy/research"))
        .form(&[("unit", "swordsman")])
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 303);
    let body = c
        .get(format!("{base}/village/academy"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body.contains("Researching Swordsman"));
    assert!(body.contains("data-deadline"));

    // The Smithy lists the researched tier-1 unit and accepts an upgrade order (AC10/AC15).
    let body = c
        .get(format!("{base}/village/smithy"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body.contains("Phalanx"));
    assert!(body.contains("value=\"phalanx\""));
    // 031: the smithy shows the stat gain the next level grants.
    assert!(
        body.contains("Att ") && body.contains('→'),
        "smithy shows the upgrade's stat gain"
    );
    let res = c
        .post(format!("{base}/village/smithy/upgrade"))
        .form(&[("unit", "phalanx")])
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 303);
    let body = c
        .get(format!("{base}/village/smithy"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body.contains("Upgrading Phalanx to level 1"));
    assert!(body.contains("data-deadline"));

    // Visitors are redirected to login (roles table).
    let anon = client()
        .get(format!("{base}/village/academy"))
        .send()
        .await
        .unwrap();
    assert_eq!(anon.status().as_u16(), 303);
}

#[sqlx::test(migrations = "../../migrations")]
async fn training_flow_and_garrison(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;

    let user = unique("troop");
    let email = format!("{user}@example.com");
    let c = client();
    c.post(format!("{base}/register"))
        .form(&[
            ("username", user.as_str()),
            ("email", email.as_str()),
            ("password", "secret12"),
            ("tribe", "gauls"),
        ])
        .send()
        .await
        .unwrap();

    // 005 AC9: without a Barracks the page explains the requirement and offers no action.
    let body = c
        .get(format!("{base}/village/troops/barracks"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body.contains("requires a"));
    assert!(!body.contains("name=\"unit\""));

    // Seed a Barracks + storage + resources (construction is covered in 003) and a small garrison.
    let village_id: uuid::Uuid = sqlx::query_scalar(
        "SELECT v.id FROM villages v JOIN users u ON u.id = v.owner_id WHERE u.username = $1",
    )
    .bind(&user)
    .fetch_one(&pool)
    .await
    .unwrap();
    for (slot, kind, level) in [
        (2_i16, "warehouse", 10_i16),
        (3, "granary", 10),
        (4, "barracks", 1),
    ] {
        sqlx::query(
            "INSERT INTO village_buildings (village_id, slot, building_type, level) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(village_id)
        .bind(slot)
        .bind(kind)
        .bind(level)
        .execute(&pool)
        .await
        .unwrap();
    }
    sqlx::query(
        "UPDATE village_resources SET wood = 5000, clay = 5000, iron = 5000, crop = 5000, \
         updated_at = now() WHERE village_id = $1",
    )
    .bind(village_id)
    .execute(&pool)
    .await
    .unwrap();

    // The Barracks lists the researched Gaul infantry (Phalanx) with a count form.
    let body = c
        .get(format!("{base}/village/troops/barracks"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body.contains("Phalanx"));
    assert!(body.contains("value=\"phalanx\""));
    assert!(body.contains("name=\"count\""));

    // 005 AC2/AC9: order a batch; PRG back to the page, which shows the queue + countdown.
    let res = c
        .post(format!("{base}/village/train"))
        .form(&[("unit", "phalanx"), ("count", "3")])
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 303);
    let body = c
        .get(format!("{base}/village/troops/barracks"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body.contains("Training 3 × Phalanx — 3 remaining"));
    assert!(body.contains("data-deadline"));
    assert!(body.contains("training in progress"));

    // 005 AC6/AC9: a garrison shows on the village page and lowers the net crop rate by exactly
    // its upkeep.
    fn crop_rate(body: &str) -> i64 {
        let crop = &body[body.find("res--crop").expect("crop line")..];
        let open = crop.find('(').expect("rate parens");
        let end = crop[open..].find("/h").expect("rate unit") + open;
        crop[open + 1..end]
            .trim_start_matches('+')
            .parse()
            .expect("rate number")
    }
    let before = crop_rate(
        &c.get(format!("{base}/village"))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap(),
    );
    sqlx::query(
        "INSERT INTO village_units (village_id, unit_id, count) VALUES ($1, 'phalanx', 10)",
    )
    .bind(village_id)
    .execute(&pool)
    .await
    .unwrap();
    let body = c
        .get(format!("{base}/village"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body.contains("Garrison"));
    assert!(body.contains("Phalanx"));
    assert!(body.contains("Total upkeep: 10 crop/h"));
    assert!(body.contains("/village/troops/barracks"));
    assert_eq!(crop_rate(&body), before - 10); // 10 phalanxes × 1 crop/h (AC6)

    // Visitors are redirected to login (roles table).
    let anon = client()
        .get(format!("{base}/village/troops/barracks"))
        .send()
        .await
        .unwrap();
    assert_eq!(anon.status().as_u16(), 303);
}

/// 034: a rejected action must tell the player *why* — the PRG redirect carries a one-shot `flash`
/// cookie with a user-facing reason (here, training more troops than the village can afford).
#[sqlx::test(migrations = "../../migrations")]
async fn rejected_action_sets_flash_message(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;

    let user = unique("flash");
    let email = format!("{user}@example.com");
    let c = client();
    c.post(format!("{base}/register"))
        .form(&[
            ("username", user.as_str()),
            ("email", email.as_str()),
            ("password", "secret12"),
            ("tribe", "gauls"),
        ])
        .send()
        .await
        .unwrap();

    // Seed a Barracks so the order passes the building check and fails on resources instead.
    let village_id: uuid::Uuid = sqlx::query_scalar(
        "SELECT v.id FROM villages v JOIN users u ON u.id = v.owner_id WHERE u.username = $1",
    )
    .bind(&user)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO village_buildings (village_id, slot, building_type, level) \
         VALUES ($1, 4, 'barracks', 1)",
    )
    .bind(village_id)
    .execute(&pool)
    .await
    .unwrap();
    // Drain the starting resources so any batch is unaffordable.
    sqlx::query(
        "UPDATE village_resources SET wood = 0, clay = 0, iron = 0, crop = 0 WHERE village_id = $1",
    )
    .bind(village_id)
    .execute(&pool)
    .await
    .unwrap();

    // Order far more than the village can afford → rejected, PRG back, with a flash cookie set.
    let res = c
        .post(format!("{base}/village/train"))
        .form(&[("unit", "phalanx"), ("count", "999")])
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 303);
    let set_cookie = res
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find(|v| v.starts_with("flash="))
        .expect("rejected action should set a flash cookie");
    // The message is the use-case reason (capitalized, percent-encoded), never an internal error.
    assert!(
        set_cookie.contains("Not%20enough%20resources"),
        "flash cookie should carry the rejection reason, got: {set_cookie}"
    );
    assert!(!set_cookie.contains("storage%20error"));

    // A *successful* action sets no flash cookie. Refund resources and order an affordable batch.
    sqlx::query(
        "UPDATE village_resources SET wood = 5000, clay = 5000, iron = 5000, crop = 5000, \
         updated_at = now() WHERE village_id = $1",
    )
    .bind(village_id)
    .execute(&pool)
    .await
    .unwrap();
    let ok = c
        .post(format!("{base}/village/train"))
        .form(&[("unit", "phalanx"), ("count", "1")])
        .send()
        .await
        .unwrap();
    assert_eq!(ok.status().as_u16(), 303);
    assert!(
        !ok.headers()
            .get_all(reqwest::header::SET_COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .any(|v| v.starts_with("flash=")),
        "a successful action must not set a flash cookie"
    );
}

/// 035: the `/me` nav probe reports the viewer's auth + moderator state so the topbar can render the
/// right link set. Reachable by visitors (authed:false), and reflects the Moderator role when set.
#[sqlx::test(migrations = "../../migrations")]
async fn me_probe_reports_auth_and_moderator(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;

    // A visitor: reachable without auth, reports logged-out.
    let visitor = client();
    let body = visitor
        .get(format!("{base}/me"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body.contains("\"authed\":false"), "got: {body}");
    assert!(body.contains("\"moderator\":false"), "got: {body}");

    // A logged-in player: authed, but not a moderator.
    let user = unique("navme");
    let email = format!("{user}@example.com");
    let c = client();
    c.post(format!("{base}/register"))
        .form(&[
            ("username", user.as_str()),
            ("email", email.as_str()),
            ("password", "secret12"),
            ("tribe", "gauls"),
        ])
        .send()
        .await
        .unwrap();
    let body = c
        .get(format!("{base}/me"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body.contains("\"authed\":true"), "got: {body}");
    assert!(body.contains("\"moderator\":false"), "got: {body}");

    // Promote to moderator → the probe now reports it (drives the Moderation nav link).
    sqlx::query("UPDATE users SET is_moderator = TRUE WHERE username = $1")
        .bind(&user)
        .execute(&pool)
        .await
        .unwrap();
    let body = c
        .get(format!("{base}/me"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body.contains("\"authed\":true"), "got: {body}");
    assert!(body.contains("\"moderator\":true"), "got: {body}");
}

/// 055: the base-template background pollers must be visitor-safe — a logged-out caller gets the small
/// expected body, never a redirect to the login HTML (which the sitting-banner JS would render as raw markup
/// on the landing page). Guards the "huge HTML markup" regression.
#[sqlx::test(migrations = "../../migrations")]
async fn visitor_background_pollers_do_not_leak_login_html(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let visitor = client(); // no auth cookie

    // /sitting/status — the visible bug: empty 200, never the login page.
    let r = visitor
        .get(format!("{base}/sitting/status"))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 200);
    let body = r.text().await.unwrap();
    assert!(body.trim().is_empty(), "visitor sitting status is empty");
    assert!(
        !body.contains("<!DOCTYPE"),
        "no login HTML leaked to the banner"
    );

    // Unread badges — "0", not the login HTML (which would parseInt to NaN).
    for path in ["/messages/unread", "/notifications/unread"] {
        let r = visitor.get(format!("{base}{path}")).send().await.unwrap();
        assert_eq!(r.status().as_u16(), 200, "{path}");
        assert_eq!(
            r.text().await.unwrap().trim(),
            "0",
            "{path} returns 0 for a visitor"
        );
    }

    // Notification SSE — 204 (the EventSource "do not reconnect" signal), not a text/html login redirect.
    let r = visitor
        .get(format!("{base}/notifications/stream"))
        .send()
        .await
        .unwrap();
    assert_eq!(
        r.status().as_u16(),
        204,
        "visitor notification stream is 204 No Content"
    );

    // AC2: a logged-in user still gets the real (non-HTML) values — banner empty (not sitting), badges 0.
    let (c, _u) = register_client(&base, &pool, &unique("poll")).await;
    let r = c
        .get(format!("{base}/sitting/status"))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 200);
    assert!(
        r.text().await.unwrap().trim().is_empty(),
        "not sitting ⇒ empty"
    );
    let r = c
        .get(format!("{base}/messages/unread"))
        .send()
        .await
        .unwrap();
    assert_eq!(r.text().await.unwrap().trim(), "0");
}

/// 035: `/me` reflects the *effective* player while sitting (030) — the same identity the 022 moderator
/// gate keys on. A non-mod sitting a moderator owner reports `moderator:true`; the reverse reports false.
#[sqlx::test(migrations = "../../migrations")]
async fn me_probe_reflects_effective_player_while_sitting(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let owner_name = unique("me_o");
    let sitter_name = unique("me_s");
    let (jo, owner) = register_client(&base, &pool, &owner_name).await;
    let (js, _sitter) = register_client(&base, &pool, &sitter_name).await;

    // The owner is a moderator; the sitter is not.
    sqlx::query("UPDATE users SET is_moderator = TRUE WHERE username = $1")
        .bind(&owner_name)
        .execute(&pool)
        .await
        .unwrap();

    let me = |c: &reqwest::Client| {
        let c = c.clone();
        let base = base.clone();
        async move {
            c.get(format!("{base}/me"))
                .send()
                .await
                .unwrap()
                .text()
                .await
                .unwrap()
        }
    };

    // Before sitting, the sitter is a plain player.
    assert!(me(&js).await.contains("\"moderator\":false"));

    // Owner authorises the sitter; the sitter starts operating the owner's (moderator) account.
    jo.post(format!("{base}/sitting/grant"))
        .form(&[("username", sitter_name.as_str())])
        .send()
        .await
        .unwrap();
    let r = js
        .post(format!("{base}/sitting/start"))
        .form(&[("owner", owner.as_u128().to_string().as_str())])
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 303);

    // Now the effective player is the moderator owner → /me reports moderator:true.
    let body = me(&js).await;
    assert!(body.contains("\"authed\":true"), "got: {body}");
    assert!(
        body.contains("\"moderator\":true"),
        "sitting a moderator should reflect the role, got: {body}"
    );
}

/// 036: the admin console is gated, shows world/server status, and an admin can grant/revoke roles
/// in-app — but cannot remove their own admin role (anti-lockout).
#[sqlx::test(migrations = "../../migrations")]
async fn admin_console_gates_and_manages_roles(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let admin_name = unique("adm");
    let target_name = unique("tgt");
    let (ac, admin_id) = register_client(&base, &pool, &admin_name).await;
    let (_tc, target_id) = register_client(&base, &pool, &target_name).await;

    // A plain player cannot reach /admin (403) and /me reports admin:false.
    let r = ac.get(format!("{base}/admin")).send().await.unwrap();
    assert_eq!(r.status().as_u16(), 403, "non-admin is forbidden");
    let me = ac
        .get(format!("{base}/me"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(me.contains("\"admin\":false"), "got: {me}");

    // Promote the first account to admin (the bootstrap path, simulated directly).
    sqlx::query("UPDATE users SET is_admin = TRUE WHERE username = $1")
        .bind(&admin_name)
        .execute(&pool)
        .await
        .unwrap();

    // Now the console loads, shows server status, and lists accounts; /me reports admin:true.
    let body = ac
        .get(format!("{base}/admin"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body.contains("World &amp; server"), "overview shown");
    assert!(body.contains("Accounts (active)"), "account count shown");
    assert!(body.contains(&target_name), "lists other accounts");
    // AC4: the derived counts are the real DB aggregates, not just present labels.
    let active: i64 = sqlx::query_scalar("SELECT count(*) FROM users WHERE abandoned_at IS NULL")
        .fetch_one(&pool)
        .await
        .unwrap();
    let villages: i64 = sqlx::query_scalar("SELECT count(*) FROM villages")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(
        body.contains(&format!("<td class=\"num\">{active}</td>")),
        "active-account count {active} rendered"
    );
    assert!(
        body.contains(&format!("<td class=\"num\">{villages}</td>")),
        "village count {villages} rendered"
    );

    // AC3: search finds *any* account by username (the 028 search) and surfaces its role forms.
    let found = ac
        .get(format!("{base}/admin?q={target_name}"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        found.contains(&target_name),
        "search lists the matched account"
    );
    assert!(
        found.contains("name=\"role\" value=\"admin\""),
        "search results carry the role-grant forms"
    );

    let me = ac
        .get(format!("{base}/me"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(me.contains("\"admin\":true"), "got: {me}");

    // Grant the target the Moderator role via the console.
    let r = ac
        .post(format!("{base}/admin/role"))
        .form(&[
            ("target", target_id.as_u128().to_string().as_str()),
            ("role", "moderator"),
            ("grant", "true"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 303);
    let is_mod: bool = sqlx::query_scalar("SELECT is_moderator FROM users WHERE username = $1")
        .bind(&target_name)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(is_mod, "moderator granted");

    // The admin cannot remove their *own* admin role (anti-lockout) — flashed, role unchanged.
    let r = ac
        .post(format!("{base}/admin/role"))
        .form(&[
            ("target", admin_id.as_u128().to_string().as_str()),
            ("role", "admin"),
            ("grant", "false"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 303);
    let flash_set = r
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .any(|v| v.starts_with("flash="));
    assert!(flash_set, "self-demotion is rejected with a flash");
    let still_admin: bool = sqlx::query_scalar("SELECT is_admin FROM users WHERE username = $1")
        .bind(&admin_name)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(still_admin, "own admin role is preserved");

    // A non-admin POSTing the role endpoint is forbidden (server-authoritative, not just hidden nav).
    let (tc2, _) = register_client(&base, &pool, &unique("noadm")).await;
    let r = tc2
        .post(format!("{base}/admin/role"))
        .form(&[
            ("target", target_id.as_u128().to_string().as_str()),
            ("role", "admin"),
            ("grant", "true"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 403, "non-admin cannot set roles");

    // 036 anti-escalation: admin powers are NOT delegated through a 030 sit. A sitter operating the
    // admin's account is gated on the *real* human, so they cannot reach /admin and /me reports false.
    let sitter_name = unique("sit");
    let (sc, _sitter_id) = register_client(&base, &pool, &sitter_name).await;
    ac.post(format!("{base}/sitting/grant"))
        .form(&[("username", sitter_name.as_str())])
        .send()
        .await
        .unwrap();
    let r = sc
        .post(format!("{base}/sitting/start"))
        .form(&[("owner", admin_id.as_u128().to_string().as_str())])
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 303, "sit started");
    let r = sc.get(format!("{base}/admin")).send().await.unwrap();
    assert_eq!(
        r.status().as_u16(),
        403,
        "a sitter cannot reach /admin even while operating an admin's account"
    );
    let me = sc
        .get(format!("{base}/me"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        me.contains("\"admin\":false"),
        "admin flag follows the real human, not the sit-effective player: {me}"
    );
}

/// 041: an admin creates a world from the console; it is persisted, started live (registry), and listed.
/// A non-admin cannot create one, and invalid parameters are rejected.
#[sqlx::test(migrations = "../../migrations")]
async fn admin_creates_world_live(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let admin_name = unique("wadm");
    let (ac, _admin_id) = register_client(&base, &pool, &admin_name).await;
    let (plain, _pid) = register_client(&base, &pool, &unique("wplain")).await;

    // A non-admin cannot create a world (server-authoritative).
    let r = plain
        .post(format!("{base}/admin/world"))
        .form(&[("speed", "3"), ("radius", "50")])
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 403, "non-admin cannot create a world");

    // Promote the admin account.
    sqlx::query("UPDATE users SET is_admin = TRUE WHERE username = $1")
        .bind(&admin_name)
        .execute(&pool)
        .await
        .unwrap();

    let worlds_before: i64 = sqlx::query_scalar("SELECT count(*) FROM worlds")
        .fetch_one(&pool)
        .await
        .unwrap();

    // Invalid radius (over the max) is rejected with a flash — no world created.
    let r = ac
        .post(format!("{base}/admin/world"))
        .form(&[("speed", "3"), ("radius", "99999")])
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 303);
    assert!(
        r.headers()
            .get_all(reqwest::header::SET_COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .any(|v| v.starts_with("flash=")),
        "invalid radius is flashed"
    );
    let worlds_mid: i64 = sqlx::query_scalar("SELECT count(*) FROM worlds")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        worlds_mid, worlds_before,
        "no world created on invalid input"
    );

    // A valid create: a new world row appears with the given speed/radius, started live.
    let r = ac
        .post(format!("{base}/admin/world"))
        .form(&[("speed", "3"), ("radius", "40")])
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 303);
    let (speed, radius): (f64, i32) =
        sqlx::query_as("SELECT speed, radius FROM worlds ORDER BY created_at DESC, id LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!((speed - 3.0).abs() < f64::EPSILON);
    assert_eq!(radius, 40);
    let worlds_after: i64 = sqlx::query_scalar("SELECT count(*) FROM worlds")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(worlds_after, worlds_before + 1, "exactly one world created");

    // The admin console lists the new world (AC3).
    let body = ac
        .get(format!("{base}/admin"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body.contains("Worlds"), "worlds section shown");
    assert!(body.contains("×3"), "the new world's speed is listed");
}

/// 052 AC3: an admin picks a world's rule preset on creation. A valid `speed` preset is persisted and the
/// world is serviceable under it; an unknown preset is rejected (server-authoritative, P4) with no row.
#[sqlx::test(migrations = "../../migrations")]
async fn admin_creates_world_with_chosen_preset(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let admin_name = unique("wpreset");
    let (ac, _admin_id) = register_client(&base, &pool, &admin_name).await;
    sqlx::query("UPDATE users SET is_admin = TRUE WHERE username = $1")
        .bind(&admin_name)
        .execute(&pool)
        .await
        .unwrap();

    // The create form offers the preset choices (the allow-list).
    let form = ac.get(format!("{base}/admin")).send().await.unwrap();
    let form_body = form.text().await.unwrap();
    assert!(
        form_body.contains("name=\"preset\"") && form_body.contains(">speed<"),
        "the create form lists the speed preset"
    );

    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM worlds")
        .fetch_one(&pool)
        .await
        .unwrap();

    // An unknown preset is rejected — no world created (P4).
    let r = ac
        .post(format!("{base}/admin/world"))
        .form(&[("speed", "2"), ("radius", "40"), ("preset", "bogus")])
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 303);
    let mid: i64 = sqlx::query_scalar("SELECT count(*) FROM worlds")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(mid, before, "an unknown preset creates no world");

    // A valid `speed` world is persisted with its preset.
    let r = ac
        .post(format!("{base}/admin/world"))
        .form(&[("speed", "2"), ("radius", "40"), ("preset", "speed")])
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 303);
    let preset: String =
        sqlx::query_scalar("SELECT rule_preset FROM worlds ORDER BY created_at DESC, id LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(preset, "speed", "the chosen preset is persisted");
}

/// 047 AC1/AC2/AC4: an admin sets a custom end-game schedule on world creation; an invalid schedule
/// (Wonder ≤ artifact) is rejected with no world created; omitting the fields uses the env default.
#[sqlx::test(migrations = "../../migrations")]
async fn admin_world_creation_sets_endgame_schedule(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let admin_name = unique("wsched");
    let (ac, _admin_id) = register_client(&base, &pool, &admin_name).await;
    sqlx::query("UPDATE users SET is_admin = TRUE WHERE username = $1")
        .bind(&admin_name)
        .execute(&pool)
        .await
        .unwrap();
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM worlds")
        .fetch_one(&pool)
        .await
        .unwrap();

    // A custom schedule (artifacts at 30 days, Wonder at 45) is stored on the new world.
    let r = ac
        .post(format!("{base}/admin/world"))
        .form(&[
            ("speed", "2"),
            ("radius", "40"),
            ("artifact_days", "30"),
            ("wonder_days", "45"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 303);
    // The newest world carries release dates ~30/45 days after creation (within a day's tolerance).
    let (a_days, w_days): (f64, f64) = sqlx::query_as(
        "SELECT (EXTRACT(EPOCH FROM (artifact_release_at - created_at)) / 86400.0)::float8, \
                (EXTRACT(EPOCH FROM (wonder_release_at - created_at)) / 86400.0)::float8 \
         FROM worlds ORDER BY created_at DESC, id DESC LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        (a_days - 30.0).abs() < 1.0,
        "artifact at ~30 days, got {a_days}"
    );
    assert!(
        (w_days - 45.0).abs() < 1.0,
        "Wonder at ~45 days, got {w_days}"
    );

    // An invalid schedule (Wonder ≤ artifact) is rejected — no world created, flashed.
    let r = ac
        .post(format!("{base}/admin/world"))
        .form(&[
            ("speed", "2"),
            ("radius", "40"),
            ("artifact_days", "60"),
            ("wonder_days", "50"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 303);
    assert!(
        r.headers()
            .get_all(reqwest::header::SET_COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .any(|v| v.starts_with("flash=")),
        "an invalid schedule is flashed"
    );
    let after_invalid: i64 = sqlx::query_scalar("SELECT count(*) FROM worlds")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        after_invalid,
        before + 1,
        "the invalid schedule created no world"
    );

    // Omitting the schedule uses the env default (90/120 days in the test harness).
    let r = ac
        .post(format!("{base}/admin/world"))
        .form(&[("speed", "2"), ("radius", "40")])
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 303);
    let (a_days, w_days): (f64, f64) = sqlx::query_as(
        "SELECT (EXTRACT(EPOCH FROM (artifact_release_at - created_at)) / 86400.0)::float8, \
                (EXTRACT(EPOCH FROM (wonder_release_at - created_at)) / 86400.0)::float8 \
         FROM worlds ORDER BY created_at DESC, id DESC LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        (a_days - 90.0).abs() < 1.0,
        "default artifact at ~90 days, got {a_days}"
    );
    assert!(
        (w_days - 120.0).abs() < 1.0,
        "default Wonder at ~120 days, got {w_days}"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn map_view_shows_terrain_and_own_village(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let user = unique("mapper");
    let email = format!("{user}@example.com");
    let c = client();
    c.post(format!("{base}/register"))
        .form(&[
            ("username", user.as_str()),
            ("email", email.as_str()),
            ("password", "secret12"),
            ("tribe", "gauls"),
        ])
        .send()
        .await
        .unwrap();

    // 006 AC7: the village page links to the map.
    let village = c
        .get(format!("{base}/village"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(village.contains("/map"));

    // The map (default-centered on the player's village) renders the grid, the player's own
    // village marker, and terrain labels.
    let body = c
        .get(format!("{base}/map"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body.contains("map-grid"));
    assert!(body.contains("centered on"));
    assert!(body.contains("map-grid__cell--village"));
    assert!(body.contains("map-grid__cell--self")); // the viewer's own village is highlighted
    assert!(body.contains(&user)); // owner name on the marker (public, GDD §7.3)
    assert!(body.contains("Valley")); // a terrain label

    // Recenter to an explicit coordinate.
    let body = c
        .get(format!("{base}/map?x=10&y=-7"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body.contains("centered on (10|-7)"));

    // Visitors are redirected to login (roles table).
    let anon = client().get(format!("{base}/map")).send().await.unwrap();
    assert_eq!(anon.status().as_u16(), 303);
    assert_eq!(
        anon.headers().get(LOCATION).unwrap().to_str().unwrap(),
        "/login"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn duplicate_username_is_rejected(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let user = unique("dup");
    let email = format!("{user}@example.com");
    let email2 = format!("{user}-2@example.com");

    let first = client()
        .post(format!("{base}/register"))
        .form(&[
            ("username", user.as_str()),
            ("email", email.as_str()),
            ("password", "secret12"),
            ("tribe", "gauls"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(first.status().as_u16(), 303);

    let second = client()
        .post(format!("{base}/register"))
        .form(&[
            ("username", user.as_str()),
            ("email", email2.as_str()),
            ("password", "secret12"),
            ("tribe", "gauls"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(second.status().as_u16(), 200);
    assert!(second.text().await.unwrap().contains("already taken"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn register_rejects_invalid_input(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    // AC1: server-side rejection (a too-short password) — no redirect, error shown, no account.
    let res = client()
        .post(format!("{base}/register"))
        .form(&[
            ("username", "validname"),
            ("email", "valid@example.com"),
            ("password", "short"),
            ("tribe", "gauls"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 200);
    assert!(res.text().await.unwrap().contains("at least 8"));

    // 004 AC1: an unknown tribe is rejected server-side, regardless of the form UI.
    let res = client()
        .post(format!("{base}/register"))
        .form(&[
            ("username", "validname2"),
            ("email", "valid2@example.com"),
            ("password", "secret12"),
            ("tribe", "egyptians"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 200);
    assert!(res.text().await.unwrap().contains("choose a tribe"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn logout_ends_session(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let user = unique("out");
    let email = format!("{user}@example.com");
    let c = client();

    // Register logs the user in.
    c.post(format!("{base}/register"))
        .form(&[
            ("username", user.as_str()),
            ("email", email.as_str()),
            ("password", "secret12"),
            ("tribe", "gauls"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(
        c.get(format!("{base}/village"))
            .send()
            .await
            .unwrap()
            .status()
            .as_u16(),
        200
    );

    // AC2: logout clears the session and returns to the landing page.
    let out = c.post(format!("{base}/logout")).send().await.unwrap();
    assert_eq!(out.status().as_u16(), 303);
    assert_eq!(out.headers().get(LOCATION).unwrap().to_str().unwrap(), "/");

    // The village is no longer reachable without logging in again.
    let after = c.get(format!("{base}/village")).send().await.unwrap();
    assert_eq!(after.status().as_u16(), 303);
    assert_eq!(
        after.headers().get(LOCATION).unwrap().to_str().unwrap(),
        "/login"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn village_shows_economy(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let user = unique("econ");
    let email = format!("{user}@example.com");
    let c = client();
    c.post(format!("{base}/register"))
        .form(&[
            ("username", user.as_str()),
            ("email", email.as_str()),
            ("password", "secret12"),
            ("tribe", "gauls"),
        ])
        .send()
        .await
        .unwrap();

    // AC7: the village shows current amount, capacity, and production per resource.
    let body = c
        .get(format!("{base}/village"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body.contains("Wood"));
    assert!(body.contains("/ 800")); // base capacity from balance
    assert!(body.contains("/h")); // production rate
}

#[sqlx::test(migrations = "../../migrations")]
async fn build_order_flow(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let user = unique("bld");
    let email = format!("{user}@example.com");
    let c = client();
    c.post(format!("{base}/register"))
        .form(&[
            ("username", user.as_str()),
            ("email", email.as_str()),
            ("password", "secret12"),
            ("tribe", "gauls"),
        ])
        .send()
        .await
        .unwrap();

    // AC8: the village offers upgrade actions with costs.
    let body = c
        .get(format!("{base}/village"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body.contains("Resource fields"));
    assert!(body.contains("Upgrade"));

    // AC1: order a field upgrade (redirects back to the village).
    let res = c
        .post(format!("{base}/village/build"))
        .form(&[("table", "field"), ("slot", "0")])
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 303);

    // AC8: the active build is shown with a countdown deadline.
    let after = c
        .get(format!("{base}/village"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(after.contains("Under construction"));
    assert!(after.contains("data-deadline"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn build_requires_login(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    // P4/roles: an unauthenticated visitor cannot order a build.
    let res = client()
        .post(format!("{base}/village/build"))
        .form(&[("table", "field"), ("slot", "0")])
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 303);
    assert_eq!(
        res.headers().get(LOCATION).unwrap().to_str().unwrap(),
        "/login"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn account_persists_across_restart(pool: sqlx::PgPool) {
    let base1 = spawn(pool.clone()).await;
    let user = unique("persist");
    let email = format!("{user}@example.com");

    client()
        .post(format!("{base1}/register"))
        .form(&[
            ("username", user.as_str()),
            ("email", email.as_str()),
            ("password", "secret12"),
            ("tribe", "gauls"),
        ])
        .send()
        .await
        .unwrap();

    // AC8: a fresh app instance over the same DB ("restart") sees the same account & village.
    let base2 = spawn(pool.clone()).await;
    let c = client();
    let login = c
        .post(format!("{base2}/login"))
        .form(&[("username", user.as_str()), ("password", "secret12")])
        .send()
        .await
        .unwrap();
    assert_eq!(login.status().as_u16(), 303);

    let view = c.get(format!("{base2}/village")).send().await.unwrap();
    assert_eq!(view.status().as_u16(), 200);
    assert!(view.text().await.unwrap().contains(&user));
}

/// Build a movement-capable repository over the same DB the app uses, to drive the System actor
/// (delivering due arrivals) deterministically from the test.
async fn movement_repo(pool: &sqlx::PgPool) -> PgAccountRepository {
    let config = WorldConfig::new(GameSpeed::new(1.0).unwrap(), 50);
    let world = ensure_world(pool, &config).await.expect("ensure world");
    let rules = economy_rules().expect("economy rules");
    PgAccountRepository::new(
        pool.clone(),
        world.id,
        world.seed,
        config.radius,
        rules.starting_amounts,
        lifecycle_rules()
            .expect("lifecycle rules")
            .beginner_protection_secs,
        config.speed,
    )
}

#[sqlx::test(migrations = "../../migrations")]
async fn rally_send_station_and_return_flow(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let repo = movement_repo(&pool).await;

    // A sender and a target on different tiles (the world places each registrant on a free tile).
    let sender = unique("send");
    let target = unique("recv");
    let cs = client();
    let ct = client();
    for (c, u) in [(&cs, &sender), (&ct, &target)] {
        c.post(format!("{base}/register"))
            .form(&[
                ("username", u.as_str()),
                ("email", format!("{u}@example.com").as_str()),
                ("password", "secret12"),
                ("tribe", "gauls"),
            ])
            .send()
            .await
            .unwrap();
    }

    let sender_village: uuid::Uuid = sqlx::query_scalar(
        "SELECT v.id FROM villages v JOIN users u ON u.id = v.owner_id WHERE u.username = $1",
    )
    .bind(&sender)
    .fetch_one(&pool)
    .await
    .unwrap();
    let (tx, ty): (i32, i32) = sqlx::query_as(
        "SELECT v.x, v.y FROM villages v JOIN users u ON u.id = v.owner_id WHERE u.username = $1",
    )
    .bind(&target)
    .fetch_one(&pool)
    .await
    .unwrap();

    // Give the sender a garrison to dispatch.
    sqlx::query(
        "INSERT INTO village_units (village_id, unit_id, count) VALUES ($1, 'phalanx', 10)",
    )
    .bind(sender_village)
    .execute(&pool)
    .await
    .unwrap();

    // AC7: the Rally Point lists the garrison units available to send.
    let rally = cs
        .get(format!("{base}/village/rally"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(rally.contains("Rally Point"));
    assert!(rally.contains("Phalanx"));
    assert!(rally.contains("name=\"count_phalanx\""));
    // 031: per-unit stats are plumbed for the live army preview (power / carry / speed / ETA).
    assert!(
        rally.contains("rally-count") && rally.contains("data-att="),
        "rally carries per-unit stat data"
    );
    assert!(
        rally.contains("rally-preview"),
        "rally has the live preview element"
    );

    // AC1/AC7: send 4 Phalanx to the target's tile; PRG back to the village.
    let res = cs
        .post(format!("{base}/village/rally/send"))
        .form(&[
            ("x", tx.to_string().as_str()),
            ("y", ty.to_string().as_str()),
            ("count_phalanx", "4"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 303);

    // The sender sees the movement in progress (direction + countdown) and a reduced garrison.
    let body = cs
        .get(format!("{base}/village"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body.contains("Movements in progress"));
    assert!(body.contains(&format!("Reinforcement to ({tx}|{ty})")));
    assert!(body.contains("4 Phalanx"));
    assert!(body.contains("data-deadline"));
    assert!(body.contains("Total upkeep: 6 crop/h")); // 10 sent 4 ⇒ 6 remain

    // The System delivers the arrival (claim → apply), stationing the troops at the target (AC4).
    let future = Timestamp(now().0 + 10_000_000_000);
    process_due_movements(
        &repo,
        &repo,
        &economy_rules().unwrap(),
        &unit_rules().unwrap(),
        GameSpeed::new(1.0).unwrap(),
        future,
        100,
    )
    .await
    .unwrap();

    // AC7: the target now shows the reinforcement, attributed to the sender.
    let host_view = ct
        .get(format!("{base}/village"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(host_view.contains("Reinforcements stationed here"));
    assert!(host_view.contains(&sender));
    assert!(host_view.contains("4 Phalanx"));

    // AC7: the sender now sees the troops abroad with a send-back action; grab the host id.
    let abroad = cs
        .get(format!("{base}/village"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(abroad.contains("Your troops abroad"));
    assert!(abroad.contains(&target));
    let marker = "name=\"host\" value=\"";
    let start = abroad.find(marker).expect("send-back host field") + marker.len();
    let end = abroad[start..].find('"').unwrap() + start;
    let host_id = &abroad[start..end];

    // AC5: recall them; PRG back to the village, then the System delivers the return.
    let res = cs
        .post(format!("{base}/village/rally/return"))
        .form(&[("host", host_id)])
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 303);
    let future = Timestamp(now().0 + 20_000_000_000);
    process_due_movements(
        &repo,
        &repo,
        &economy_rules().unwrap(),
        &unit_rules().unwrap(),
        GameSpeed::new(1.0).unwrap(),
        future,
        100,
    )
    .await
    .unwrap();

    // The garrison is whole again at home, and the target no longer hosts the reinforcement.
    let home = cs
        .get(format!("{base}/village"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(home.contains("Total upkeep: 10 crop/h"));
    assert!(!home.contains("Your troops abroad"));
    let host_after = ct
        .get(format!("{base}/village"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(!host_after.contains("Reinforcements stationed here"));

    // Visitors cannot reach the Rally Point (roles table, P4).
    let anon = client()
        .get(format!("{base}/village/rally"))
        .send()
        .await
        .unwrap();
    assert_eq!(anon.status().as_u16(), 303);
}

/// 008 AC6: build a Marketplace, send a resource shipment to another village, and see it in transit;
/// the System delivers it (crediting the target). Also: no-Marketplace explains; visitor → login.
#[sqlx::test(migrations = "../../migrations")]
async fn marketplace_send_and_deliver_flow(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let repo = movement_repo(&pool).await;

    let sender = unique("msend");
    let target = unique("mrecv");
    let cs = client();
    let ct = client();
    for (c, u) in [(&cs, &sender), (&ct, &target)] {
        c.post(format!("{base}/register"))
            .form(&[
                ("username", u.as_str()),
                ("email", format!("{u}@example.com").as_str()),
                ("password", "secret12"),
                ("tribe", "gauls"),
            ])
            .send()
            .await
            .unwrap();
    }

    let sender_village: uuid::Uuid = sqlx::query_scalar(
        "SELECT v.id FROM villages v JOIN users u ON u.id = v.owner_id WHERE u.username = $1",
    )
    .bind(&sender)
    .fetch_one(&pool)
    .await
    .unwrap();
    let (target_village, tx, ty): (uuid::Uuid, i32, i32) = sqlx::query_as(
        "SELECT v.id, v.x, v.y FROM villages v JOIN users u ON u.id = v.owner_id WHERE u.username = $1",
    )
    .bind(&target)
    .fetch_one(&pool)
    .await
    .unwrap();

    // Seed the sender a Marketplace (level 5 ⇒ 5 merchants) + storage + resources; the target a big
    // Warehouse so the delivery is not clamped away.
    for (vid, slot, kind, level) in [
        (sender_village, 2_i16, "warehouse", 10_i16),
        (sender_village, 10, "marketplace", 5),
        (target_village, 2, "warehouse", 10),
    ] {
        sqlx::query(
            "INSERT INTO village_buildings (village_id, slot, building_type, level) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(vid)
        .bind(slot)
        .bind(kind)
        .bind(level)
        .execute(&pool)
        .await
        .unwrap();
    }
    sqlx::query(
        "UPDATE village_resources SET wood = 5000, clay = 5000, iron = 5000, crop = 5000, \
         updated_at = now() WHERE village_id = $1",
    )
    .bind(sender_village)
    .execute(&pool)
    .await
    .unwrap();

    // AC6: the Marketplace lists the merchant pool and per-tribe capacity.
    let market = cs
        .get(format!("{base}/village/market"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(market.contains("Marketplace"));
    assert!(market.contains("750")); // Gaul merchant capacity
    assert!(market.contains("free of 5"));
    assert!(market.contains("name=\"amount_wood\""));
    // 031: the live shipment preview (merchants needed + round-trip) is wired in.
    assert!(
        market.contains("ship-preview") && market.contains("ship-amt"),
        "market has the live shipment preview"
    );

    // AC1/AC6: send 300 wood to the target's tile; PRG back to the village.
    let res = cs
        .post(format!("{base}/village/market/send"))
        .form(&[
            ("x", tx.to_string().as_str()),
            ("y", ty.to_string().as_str()),
            ("amount_wood", "300"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 303);

    // The sender sees the shipment in transit (direction + contents + countdown).
    let body = cs
        .get(format!("{base}/village"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body.contains("Shipments in transit"));
    assert!(body.contains(&format!("Shipment to ({tx}|{ty})")));
    assert!(body.contains("300 wood"));
    assert!(body.contains("data-deadline"));

    // The System delivers the shipment; the target's stored wood rises (750 + 300, uncapped here).
    let before: i64 =
        sqlx::query_scalar("SELECT wood FROM village_resources WHERE village_id = $1")
            .bind(target_village)
            .fetch_one(&pool)
            .await
            .unwrap();
    let econ = economy_rules().unwrap();
    let units = unit_rules().unwrap();
    let merchants = merchant_rules().unwrap();
    let config = WorldConfig::new(GameSpeed::new(1.0).unwrap(), 50);
    let world = ensure_world(&pool, &config).await.unwrap();
    let map = WorldMap::new(world.seed as u64, config.radius, map_rules().unwrap());
    let future = Timestamp(now().0 + 10_000_000_000);
    let credited = process_due_trades(
        &repo,
        &repo,
        &econ,
        &units,
        &merchants,
        &map,
        GameSpeed::new(1.0).unwrap(),
        future,
        100,
    )
    .await
    .unwrap();
    assert!(credited.contains(&eperica_domain::VillageId(target_village.as_u128())));
    let after: i64 = sqlx::query_scalar("SELECT wood FROM village_resources WHERE village_id = $1")
        .bind(target_village)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(after >= before + 300, "target wood {before} -> {after}");

    // A village with no Marketplace gets the explanation, not the form.
    let plain = ct
        .get(format!("{base}/village/market"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(plain.contains("no Marketplace"));
    assert!(!plain.contains("name=\"amount_wood\""));

    // Visitors cannot reach the Marketplace (roles table, P4).
    let anon = client()
        .get(format!("{base}/village/market"))
        .send()
        .await
        .unwrap();
    assert_eq!(anon.status().as_u16(), 303);
}

/// 009 AC8: launch a raid from the Rally Point; the System resolves it and a battle report appears
/// in both the attacker's and the defender's inbox. Visitors cannot read reports.
#[sqlx::test(migrations = "../../migrations")]
async fn combat_raid_and_reports_flow(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let repo = movement_repo(&pool).await;

    let attacker = unique("raidatk");
    let defender = unique("raiddef");
    let ca = client();
    let cd = client();
    for (c, u) in [(&ca, &attacker), (&cd, &defender)] {
        c.post(format!("{base}/register"))
            .form(&[
                ("username", u.as_str()),
                ("email", format!("{u}@example.com").as_str()),
                ("password", "secret12"),
                ("tribe", "gauls"),
            ])
            .send()
            .await
            .unwrap();
    }

    let atk_village: uuid::Uuid = sqlx::query_scalar(
        "SELECT v.id FROM villages v JOIN users u ON u.id = v.owner_id WHERE u.username = $1",
    )
    .bind(&attacker)
    .fetch_one(&pool)
    .await
    .unwrap();
    let (def_village, dx, dy): (uuid::Uuid, i32, i32) = sqlx::query_as(
        "SELECT v.id, v.x, v.y FROM villages v JOIN users u ON u.id = v.owner_id WHERE u.username = $1",
    )
    .bind(&defender)
    .fetch_one(&pool)
    .await
    .unwrap();

    // An overwhelming attacker garrison vs a token defence.
    sqlx::query(
        "INSERT INTO village_units (village_id, unit_id, count) VALUES ($1, 'swordsman', 80)",
    )
    .bind(atk_village)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO village_units (village_id, unit_id, count) VALUES ($1, 'phalanx', 3)")
        .bind(def_village)
        .execute(&pool)
        .await
        .unwrap();

    clear_protection(&pool).await; // 019: the fresh defender would otherwise be protected.

    // AC1/AC8: launch a raid; PRG back to the village, which shows the attack in flight.
    let res = ca
        .post(format!("{base}/village/rally/send"))
        .form(&[
            ("mode", "raid"),
            ("x", dx.to_string().as_str()),
            ("y", dy.to_string().as_str()),
            ("count_swordsman", "60"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 303);
    let body = ca
        .get(format!("{base}/village"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body.contains(&format!("Raid on ({dx}|{dy})")));

    // The System resolves the battle.
    let econ = economy_rules().unwrap();
    let units = unit_rules().unwrap();
    let combat = combat_rules().unwrap();
    let scout = scout_rules().unwrap();
    let config = WorldConfig::new(GameSpeed::new(1.0).unwrap(), 50);
    let world = ensure_world(&pool, &config).await.unwrap();
    let map = WorldMap::new(world.seed as u64, config.radius, map_rules().unwrap());
    let future = Timestamp(now().0 + 10_000_000_000);
    process_due_combat(
        &repo,
        &repo,
        &repo,
        &repo,
        &econ,
        &units,
        &combat,
        &scout,
        &culture_rules().unwrap(),
        &loyalty_rules().unwrap(),
        &ranking_rules().unwrap(),
        &map,
        GameSpeed::new(1.0).unwrap(),
        world.seed as u64,
        future,
        100,
        (3, 6, 10),
    )
    .await
    .unwrap();

    // AC8: both parties see a report. The attacker won; the defender sees the incoming raid.
    let atk_reports = ca
        .get(format!("{base}/reports"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(atk_reports.contains(&format!("Raid on {defender} ({dx}|{dy})")));
    assert!(atk_reports.contains("Victory"));

    let def_reports = cd
        .get(format!("{base}/reports"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(def_reports.contains(&format!("Raid from {attacker}")));

    // The report detail (reachable from the inbox) shows forces, luck, and morale.
    let report_id: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM battle_reports WHERE attacker_village = $1")
            .bind(atk_village)
            .fetch_one(&pool)
            .await
            .unwrap();
    let detail = ca
        .get(format!("{base}/reports/{}", report_id.as_u128()))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(detail.contains("Swordsman"));
    assert!(detail.contains("Luck"));
    assert!(detail.contains("Morale"));

    // Visitors cannot read reports (roles table, P4).
    let anon = client()
        .get(format!("{base}/reports"))
        .send()
        .await
        .unwrap();
    assert_eq!(anon.status().as_u16(), 303);
}

/// 010 AC1/AC9/AC12: send a standalone scout from the Rally Point, the System resolves it, and the
/// scouter reads the intel report; an undetected target sees nothing.
#[sqlx::test(migrations = "../../migrations")]
async fn scout_mission_and_intel_report_flow(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let repo = movement_repo(&pool).await;

    let scouter = unique("spywho");
    let target = unique("spymark");
    let cs = client();
    let ct = client();
    for (c, u) in [(&cs, &scouter), (&ct, &target)] {
        c.post(format!("{base}/register"))
            .form(&[
                ("username", u.as_str()),
                ("email", format!("{u}@example.com").as_str()),
                ("password", "secret12"),
                ("tribe", "gauls"),
            ])
            .send()
            .await
            .unwrap();
    }

    let s_village: uuid::Uuid = sqlx::query_scalar(
        "SELECT v.id FROM villages v JOIN users u ON u.id = v.owner_id WHERE u.username = $1",
    )
    .bind(&scouter)
    .fetch_one(&pool)
    .await
    .unwrap();
    let (t_village, tx, ty): (uuid::Uuid, i32, i32) = sqlx::query_as(
        "SELECT v.id, v.x, v.y FROM villages v JOIN users u ON u.id = v.owner_id WHERE u.username = $1",
    )
    .bind(&target)
    .fetch_one(&pool)
    .await
    .unwrap();

    // Give the scouter pathfinders (Gaul scout). The target stations no scouts (a clean scout).
    sqlx::query(
        "INSERT INTO village_units (village_id, unit_id, count) VALUES ($1, 'pathfinder', 5)",
    )
    .bind(s_village)
    .execute(&pool)
    .await
    .unwrap();

    // AC1/AC12: send a scout mission to spy on resources; PRG back to the village in flight.
    let res = cs
        .post(format!("{base}/village/rally/send"))
        .form(&[
            ("mode", "scout"),
            ("scout_target", "resources"),
            ("x", tx.to_string().as_str()),
            ("y", ty.to_string().as_str()),
            ("count_pathfinder", "3"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 303);
    let body = cs
        .get(format!("{base}/village"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body.contains(&format!("Scouting ({tx}|{ty})")));

    // The System resolves the mission.
    let econ = economy_rules().unwrap();
    let units = unit_rules().unwrap();
    let scout = scout_rules().unwrap();
    let config = WorldConfig::new(GameSpeed::new(1.0).unwrap(), 50);
    let world = ensure_world(&pool, &config).await.unwrap();
    let map = WorldMap::new(world.seed as u64, config.radius, map_rules().unwrap());
    let future = Timestamp(now().0 + 10_000_000_000);
    process_due_scouts(
        &repo,
        &repo,
        &repo,
        &econ,
        &units,
        &scout,
        &map,
        GameSpeed::new(1.0).unwrap(),
        future,
        100,
    )
    .await
    .unwrap();

    // AC12: the scouter's inbox lists the scout report with intel; the detail shows resources.
    let reports = cs
        .get(format!("{base}/reports"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(reports.contains(&format!("Scouted {target} ({tx}|{ty})")));
    assert!(reports.contains("Intel gathered"));

    let report_id: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM scout_reports WHERE scouter_village = $1")
            .bind(s_village)
            .fetch_one(&pool)
            .await
            .unwrap();
    let detail = cs
        .get(format!("{base}/reports/scout/{}", report_id.as_u128()))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(detail.contains("Resources"));

    // AC8: an undetected target (no counter-espionage) sees no report at all.
    let _ = t_village;
    let t_reports = ct
        .get(format!("{base}/reports"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(!t_reports.contains("Scouted"));
}

/// 011 AC11: a raid with catapults aimed at a building loots resources and razes the building; the
/// battle report shows both. The Cranny appears in the build menu.
#[sqlx::test(migrations = "../../migrations")]
async fn siege_loot_and_cranny_flow(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let repo = movement_repo(&pool).await;

    let attacker = unique("slweb_a");
    let defender = unique("slweb_d");
    let ca = client();
    let cd = client();
    for (c, u) in [(&ca, &attacker), (&cd, &defender)] {
        c.post(format!("{base}/register"))
            .form(&[
                ("username", u.as_str()),
                ("email", format!("{u}@example.com").as_str()),
                ("password", "secret12"),
                ("tribe", "gauls"),
            ])
            .send()
            .await
            .unwrap();
    }

    let atk_village: uuid::Uuid = sqlx::query_scalar(
        "SELECT v.id FROM villages v JOIN users u ON u.id = v.owner_id WHERE u.username = $1",
    )
    .bind(&attacker)
    .fetch_one(&pool)
    .await
    .unwrap();
    let (def_village, dx, dy): (uuid::Uuid, i32, i32) = sqlx::query_as(
        "SELECT v.id, v.x, v.y FROM villages v JOIN users u ON u.id = v.owner_id WHERE u.username = $1",
    )
    .bind(&defender)
    .fetch_one(&pool)
    .await
    .unwrap();

    // Overwhelming attacker with catapults; a token defence + a Warehouse and stored resources.
    for (v, unit, n) in [
        (atk_village, "swordsman", 80),
        (atk_village, "trebuchet", 4),
        (def_village, "phalanx", 2),
    ] {
        sqlx::query("INSERT INTO village_units (village_id, unit_id, count) VALUES ($1, $2, $3)")
            .bind(v)
            .bind(unit)
            .bind(n)
            .execute(&pool)
            .await
            .unwrap();
    }
    sqlx::query(
        "INSERT INTO village_buildings (village_id, slot, building_type, level) \
         VALUES ($1, 20, 'warehouse', 3)",
    )
    .bind(def_village)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE village_resources SET wood=900, clay=900, iron=900, crop=900 WHERE village_id=$1",
    )
    .bind(def_village)
    .execute(&pool)
    .await
    .unwrap();

    clear_protection(&pool).await; // 019: the fresh defender would otherwise be protected.

    // AC11: raid with catapults aimed at the Warehouse.
    let res = ca
        .post(format!("{base}/village/rally/send"))
        .form(&[
            ("mode", "raid"),
            ("catapult_target", "warehouse"),
            ("x", dx.to_string().as_str()),
            ("y", dy.to_string().as_str()),
            ("count_swordsman", "60"),
            ("count_trebuchet", "4"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 303);

    // The System resolves the battle.
    let econ = economy_rules().unwrap();
    let units = unit_rules().unwrap();
    let combat = combat_rules().unwrap();
    let scout = scout_rules().unwrap();
    let config = WorldConfig::new(GameSpeed::new(1.0).unwrap(), 50);
    let world = ensure_world(&pool, &config).await.unwrap();
    let map = WorldMap::new(world.seed as u64, config.radius, map_rules().unwrap());
    let future = Timestamp(now().0 + 10_000_000_000);
    process_due_combat(
        &repo,
        &repo,
        &repo,
        &repo,
        &econ,
        &units,
        &combat,
        &scout,
        &culture_rules().unwrap(),
        &loyalty_rules().unwrap(),
        &ranking_rules().unwrap(),
        &map,
        GameSpeed::new(1.0).unwrap(),
        world.seed as u64,
        future,
        100,
        (3, 6, 10),
    )
    .await
    .unwrap();

    // AC11: the attacker's report detail shows the loot and the razed Warehouse.
    let report_id: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM battle_reports WHERE attacker_village=$1")
            .bind(atk_village)
            .fetch_one(&pool)
            .await
            .unwrap();
    let detail = ca
        .get(format!("{base}/reports/{}", report_id.as_u128()))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(detail.contains("Loot:"), "report should show loot");
    assert!(
        detail.contains("Building razed:") && detail.contains("Warehouse"),
        "report should show the razed Warehouse"
    );

    // AC10/AC11: the Cranny is offered in the build menu.
    let village = ca
        .get(format!("{base}/village"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(village.contains("Cranny"), "Cranny should be buildable");
}

// 012 AC12: the map shows oasis tiles with a Rally Point link; an oasis attack from the Rally Point
// clears + occupies the oasis (Outpost gives capacity); the village page then shows the held oasis +
// its bonus; the map shows it held by the player. The Outpost is buildable.
#[sqlx::test(migrations = "../../migrations")]
async fn oasis_attack_occupy_and_bonus_flow(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let repo = movement_repo(&pool).await;
    let config = WorldConfig::new(GameSpeed::new(1.0).unwrap(), 50);
    let world = ensure_world(&pool, &config).await.unwrap();
    let map = WorldMap::new(world.seed as u64, config.radius, map_rules().unwrap());

    let user = unique("oasis");
    let c = client();
    c.post(format!("{base}/register"))
        .form(&[
            ("username", user.as_str()),
            ("email", format!("{user}@example.com").as_str()),
            ("password", "secret12"),
            ("tribe", "gauls"),
        ])
        .send()
        .await
        .unwrap();

    let (vid, vx, vy): (uuid::Uuid, i32, i32) = sqlx::query_as(
        "SELECT v.id, v.x, v.y FROM villages v JOIN users u ON u.id = v.owner_id \
         WHERE u.username = $1",
    )
    .bind(&user)
    .fetch_one(&pool)
    .await
    .unwrap();
    let village_coord = Coordinate::new(vx, vy);

    // Pick an oasis tile with **no village** on it (the long-lived dev DB has villages on some oasis
    // tiles from older seeds; those would render as village cells, not oases).
    let occupied: std::collections::HashSet<(i32, i32)> =
        sqlx::query_as::<_, (i32, i32)>("SELECT x, y FROM villages WHERE world_id = $1")
            .bind(uuid::Uuid::from_u128(world.id.0))
            .fetch_all(&pool)
            .await
            .unwrap()
            .into_iter()
            .collect();
    let oasis = coordinates_within(config.radius)
        .find(|coord| {
            matches!(map.tile_at(*coord), Some(TileKind::Oasis(_)))
                && *coord != village_coord
                && !occupied.contains(&(coord.x, coord.y))
        })
        .expect("a free oasis exists");
    sqlx::query("DELETE FROM oases WHERE world_id = $1 AND x = $2 AND y = $3")
        .bind(uuid::Uuid::from_u128(world.id.0))
        .bind(oasis.x)
        .bind(oasis.y)
        .execute(&pool)
        .await
        .unwrap();

    // The Outpost is buildable (it appears in the village build menu) — AC12.
    let village = c
        .get(format!("{base}/village"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(village.contains("Outpost"), "Outpost should be buildable");

    // Give the village an Outpost (capacity ≥ 1) and a strong garrison.
    sqlx::query(
        "INSERT INTO village_buildings (village_id, slot, building_type, level) \
         VALUES ($1, 20, 'outpost', 3) \
         ON CONFLICT (village_id, slot) DO UPDATE SET building_type = EXCLUDED.building_type, level = EXCLUDED.level",
    )
    .bind(vid)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO village_units (village_id, unit_id, count) VALUES ($1, 'phalanx', 600)",
    )
    .bind(vid)
    .execute(&pool)
    .await
    .unwrap();

    // AC12: the map (centered on the oasis) shows it as an oasis with a Rally Point link.
    let map_html = c
        .get(format!("{base}/map?x={}&y={}", oasis.x, oasis.y))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        map_html.contains(&format!("/village/rally?x={}&amp;y={}", oasis.x, oasis.y)),
        "the oasis links to the Rally Point pre-filled with its tile"
    );

    // AC12: send an oasis attack from the Rally Point; PRG back to the village.
    let res = c
        .post(format!("{base}/village/rally/send"))
        .form(&[
            ("mode", "attack"),
            ("x", oasis.x.to_string().as_str()),
            ("y", oasis.y.to_string().as_str()),
            ("count_phalanx", "500"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 303);
    // The oasis attack movement was created (garrison debited from 600 to 100).
    let after: i32 = sqlx::query_scalar(
        "SELECT count FROM village_units WHERE village_id = $1 AND unit_id = 'phalanx'",
    )
    .bind(vid)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(after, 100, "the oasis attack debited the garrison");

    // The System resolves the due oasis battle: the village clears + occupies the oasis.
    let future = Timestamp(now().0 + 10_000_000_000);
    process_due_oasis_combat(
        &repo,
        &repo,
        &repo,
        &economy_rules().unwrap(),
        &unit_rules().unwrap(),
        &combat_rules().unwrap(),
        &oasis_rules().unwrap(),
        &map,
        GameSpeed::new(1.0).unwrap(),
        world.seed as u64,
        future,
        100,
    )
    .await
    .unwrap();

    // AC12: the village page shows the held oasis + its bonus.
    let village = c
        .get(format!("{base}/village"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        village.contains("Oases you hold"),
        "village lists held oases"
    );
    assert!(
        village.contains(&format!("({}|{})", oasis.x, oasis.y)),
        "the held oasis tile is shown"
    );

    // AC12: the map now shows the oasis held by the player.
    let map_html = c
        .get(format!("{base}/map?x={}&y={}", oasis.x, oasis.y))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        map_html.contains(&format!("held by {user}")),
        "the map shows the oasis owner"
    );
}

/// 013 AC11 / roles (P4): a player cannot act on another player's village by forging the `village=`
/// selector — the action falls back to the caller's own village, never the victim's.
#[sqlx::test(migrations = "../../migrations")]
async fn forged_village_selector_cannot_act_on_anothers_village(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;

    let attacker = unique("forge_a");
    let victim = unique("forge_v");
    let ca = client();
    let cv = client();
    for (c, u) in [(&ca, &attacker), (&cv, &victim)] {
        c.post(format!("{base}/register"))
            .form(&[
                ("username", u.as_str()),
                ("email", format!("{u}@example.com").as_str()),
                ("password", "secret12"),
                ("tribe", "gauls"),
            ])
            .send()
            .await
            .unwrap();
    }

    let a_village: uuid::Uuid = sqlx::query_scalar(
        "SELECT v.id FROM villages v JOIN users u ON u.id = v.owner_id WHERE u.username = $1",
    )
    .bind(&attacker)
    .fetch_one(&pool)
    .await
    .unwrap();
    let v_village: uuid::Uuid = sqlx::query_scalar(
        "SELECT v.id FROM villages v JOIN users u ON u.id = v.owner_id WHERE u.username = $1",
    )
    .bind(&victim)
    .fetch_one(&pool)
    .await
    .unwrap();

    // The attacker orders a build, forging the victim's village id in the form.
    let res = ca
        .post(format!("{base}/village/build"))
        .form(&[
            ("table", "field"),
            ("slot", "0"),
            ("village", v_village.as_u128().to_string().as_str()),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 303);

    // P4: nothing was queued on the victim's village; the build landed on the attacker's own.
    let victim_orders: i64 =
        sqlx::query_scalar("SELECT count(*) FROM build_orders WHERE village_id = $1")
            .bind(v_village)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(victim_orders, 0, "the victim's village was untouched");
    let attacker_orders: i64 =
        sqlx::query_scalar("SELECT count(*) FROM build_orders WHERE village_id = $1")
            .bind(a_village)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        attacker_orders, 1,
        "the build fell back to the caller's own village"
    );
    let _ = cv;
}

/// 013 AC11: the village page shows culture points + expansion slots; with a free slot the Rally
/// Point offers a **Settle** order that founds a new village; the player can then switch between
/// their villages; the **capital** is badged on the village page and distinguished on the map.
#[sqlx::test(migrations = "../../migrations")]
async fn settling_culture_panel_switcher_and_capital_flow(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let repo = movement_repo(&pool).await;
    let config = WorldConfig::new(GameSpeed::new(1.0).unwrap(), 50);
    let world = ensure_world(&pool, &config).await.unwrap();
    let map = WorldMap::new(world.seed as u64, config.radius, map_rules().unwrap());
    let crules = culture_rules().unwrap();
    let units = unit_rules().unwrap();
    let template = starting_village().unwrap();

    let user = unique("settler");
    let c = client();
    c.post(format!("{base}/register"))
        .form(&[
            ("username", user.as_str()),
            ("email", format!("{user}@example.com").as_str()),
            ("password", "secret12"),
            ("tribe", "gauls"),
        ])
        .send()
        .await
        .unwrap();

    let (user_id, vid, vx, vy): (uuid::Uuid, uuid::Uuid, i32, i32) = sqlx::query_as(
        "SELECT u.id, v.id, v.x, v.y FROM villages v JOIN users u ON u.id = v.owner_id \
         WHERE u.username = $1",
    )
    .bind(&user)
    .fetch_one(&pool)
    .await
    .unwrap();
    let village_coord = Coordinate::new(vx, vy);

    // AC1/AC11: a fresh player's village page shows the culture panel — pooled CP at the base rate,
    // one village of one allowed, and the next village's CP threshold.
    let body = c
        .get(format!("{base}/village"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body.contains("Culture points:"), "culture panel shown");
    assert!(body.contains("1 / 1"), "slots used/allowed shown");
    assert!(body.contains("next village at"), "next CP threshold shown");

    // Seed a free slot: a Residence (grants capacity) + enough pooled CP, and the settler group in
    // the garrison. (Building these via the UI would take game-hours; the flow under test is the
    // settle order, not construction/training.)
    sqlx::query(
        "INSERT INTO village_buildings (village_id, slot, building_type, level) \
         VALUES ($1, 9, 'residence', 1)",
    )
    .bind(vid)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE player_culture SET value = 1000, updated_at = now() WHERE player_id = $1")
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO village_units (village_id, unit_id, count) VALUES ($1, $2, $3)")
        .bind(vid)
        .bind(&crules.settler_id)
        .bind(i32::try_from(crules.settlers_per_village).unwrap())
        .execute(&pool)
        .await
        .unwrap();

    // AC4/AC11: with a free slot, the village page invites expansion and the Rally Point offers the
    // Settle order.
    let body = c
        .get(format!("{base}/village"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body.contains("1 / 2"), "the free slot is reflected");
    let rally = c
        .get(format!("{base}/village/rally"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        rally.contains("Settle (found a new village)"),
        "the Settle order is offered with a free slot"
    );

    // A free valley to found on (no village there, a different tile).
    let occupied: std::collections::HashSet<(i32, i32)> =
        sqlx::query_as::<_, (i32, i32)>("SELECT x, y FROM villages WHERE world_id = $1")
            .bind(uuid::Uuid::from_u128(world.id.0))
            .fetch_all(&pool)
            .await
            .unwrap()
            .into_iter()
            .collect();
    let target = coordinates_within(config.radius)
        .find(|coord| {
            matches!(map.tile_at(*coord), Some(TileKind::Valley(_)))
                && *coord != village_coord
                && !occupied.contains(&(coord.x, coord.y))
        })
        .expect("a free valley exists");

    // AC6/AC11: settle — send the settler group to the free valley; PRG back to the village.
    let res = c
        .post(format!("{base}/village/rally/send"))
        .form(&[
            ("mode", "settle"),
            ("x", target.x.to_string().as_str()),
            ("y", target.y.to_string().as_str()),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 303);

    // The System resolves the due settle: a new village is founded at the target.
    let future = Timestamp(now().0 + 10_000_000_000);
    process_due_settles(
        &repo,
        &repo,
        &repo,
        &crules,
        &units,
        &template,
        &map,
        GameSpeed::new(1.0).unwrap(),
        future,
        100,
    )
    .await
    .unwrap();

    // AC8/AC11: the player now has two villages and can switch between them — the switcher lists
    // both, defaulting to the first/capital.
    let body = c
        .get(format!("{base}/village"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body.contains("Your villages:"), "the switcher is shown");
    assert!(
        body.contains(&format!("({}|{})", target.x, target.y)),
        "the founded village appears in the switcher"
    );
    // Switching to the founded village shows that village's page.
    let founded_id: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM villages WHERE owner_id = $1 AND x = $2 AND y = $3")
            .bind(user_id)
            .bind(target.x)
            .bind(target.y)
            .fetch_one(&pool)
            .await
            .unwrap();
    let switched = c
        .get(format!("{base}/village?village={}", founded_id.as_u128()))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        switched.contains(&format!("Location ({}, {})", target.x, target.y)),
        "switching shows the founded village's own page"
    );

    // AC9/AC10/AC11: the capital is badged on the village page and distinguished on the map. (The
    // Palace→capital mechanism is covered by the 013 DB tests; here we assert the display.)
    sqlx::query("UPDATE villages SET is_capital = true WHERE id = $1")
        .bind(vid)
        .execute(&pool)
        .await
        .unwrap();
    let capital_page = c
        .get(format!("{base}/village?village={}", vid.as_u128()))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(capital_page.contains("Capital"), "the capital is badged");
    let map_html = c
        .get(format!("{base}/map?x={vx}&y={vy}"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        map_html.contains("(capital)"),
        "the map distinguishes the capital"
    );
}

/// 014 AC4/AC10/AC11: sending administrators with a winning attack against a low-loyalty enemy village
/// conquers it — the report shows the capture, and the village joins the conqueror's switcher. The
/// defender's own village loyalty is shown on their village page. (The capital exception, AC5, is
/// covered server-side by `admin_attack_on_a_capital_changes_nothing` and the `conquest_outcome`
/// domain test.)
#[sqlx::test(migrations = "../../migrations")]
async fn conquest_with_administrators_flow(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let repo = movement_repo(&pool).await;
    let config = WorldConfig::new(GameSpeed::new(1.0).unwrap(), 50);
    let world = ensure_world(&pool, &config).await.unwrap();
    let map = WorldMap::new(world.seed as u64, config.radius, map_rules().unwrap());

    let attacker = unique("conq_a");
    let defender = unique("conq_d");
    let ca = client();
    let cd = client();
    for (c, u) in [(&ca, &attacker), (&cd, &defender)] {
        c.post(format!("{base}/register"))
            .form(&[
                ("username", u.as_str()),
                ("email", format!("{u}@example.com").as_str()),
                ("password", "secret12"),
                ("tribe", "gauls"),
            ])
            .send()
            .await
            .unwrap();
    }

    let (a_vid, a_uid): (uuid::Uuid, uuid::Uuid) = sqlx::query_as(
        "SELECT v.id, u.id FROM villages v JOIN users u ON u.id = v.owner_id WHERE u.username = $1",
    )
    .bind(&attacker)
    .fetch_one(&pool)
    .await
    .unwrap();
    let (d_vid, dx, dy): (uuid::Uuid, i32, i32) = sqlx::query_as(
        "SELECT v.id, v.x, v.y FROM villages v JOIN users u ON u.id = v.owner_id \
         WHERE u.username = $1",
    )
    .bind(&defender)
    .fetch_one(&pool)
    .await
    .unwrap();

    // The attacker: a Residence + ample culture (a free expansion slot to hold a 2nd village), and a
    // garrison of administrators + swordsmen. The defender: a token garrison and **low loyalty**.
    sqlx::query(
        "INSERT INTO village_buildings (village_id, slot, building_type, level) \
         VALUES ($1, 9, 'residence', 1)",
    )
    .bind(a_vid)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE player_culture SET value = 5000, updated_at = now() WHERE player_id = $1")
        .bind(a_uid)
        .execute(&pool)
        .await
        .unwrap();
    for (unit, n) in [("senator", 3), ("swordsman", 80)] {
        sqlx::query("INSERT INTO village_units (village_id, unit_id, count) VALUES ($1, $2, $3)")
            .bind(a_vid)
            .bind(unit)
            .bind(n)
            .execute(&pool)
            .await
            .unwrap();
    }
    sqlx::query("INSERT INTO village_units (village_id, unit_id, count) VALUES ($1, 'phalanx', 1)")
        .bind(d_vid)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE villages SET loyalty = 10, loyalty_updated_at = now() WHERE id = $1")
        .bind(d_vid)
        .execute(&pool)
        .await
        .unwrap();

    // AC11: the defender sees their village's loyalty on the village page.
    let def_view = cd
        .get(format!("{base}/village"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        def_view.contains("Loyalty:"),
        "loyalty shown on the village page"
    );

    clear_protection(&pool).await; // 019: the fresh defender would otherwise be protected.

    // AC4/AC11: send the attack carrying administrators; PRG back to the village.
    let res = ca
        .post(format!("{base}/village/rally/send"))
        .form(&[
            ("mode", "attack"),
            ("x", dx.to_string().as_str()),
            ("y", dy.to_string().as_str()),
            ("count_senator", "3"),
            ("count_swordsman", "60"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 303);

    // The System resolves the battle — the conquest step transfers the village.
    let econ = economy_rules().unwrap();
    let units = unit_rules().unwrap();
    let combat = combat_rules().unwrap();
    let scout = scout_rules().unwrap();
    let future = Timestamp(now().0 + 10_000_000_000);
    process_due_combat(
        &repo,
        &repo,
        &repo,
        &repo,
        &econ,
        &units,
        &combat,
        &scout,
        &culture_rules().unwrap(),
        &loyalty_rules().unwrap(),
        &ranking_rules().unwrap(),
        &map,
        GameSpeed::new(1.0).unwrap(),
        world.seed as u64,
        future,
        100,
        (3, 6, 10),
    )
    .await
    .unwrap();

    // AC4/AC8: the village changed hands — it's now the attacker's, gone from the defender.
    let owner: uuid::Uuid = sqlx::query_scalar("SELECT owner_id FROM villages WHERE id = $1")
        .bind(d_vid)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(owner, a_uid, "the village was conquered");

    // AC11: the conquered village appears in the conqueror's switcher.
    let a_view = ca
        .get(format!("{base}/village"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        a_view.contains("Your villages:"),
        "switcher lists both villages"
    );
    assert!(
        a_view.contains(&format!("({dx}|{dy})")),
        "the conquered village is in the switcher"
    );

    // AC10: the attacker's report shows the loyalty change + the capture.
    let report_id: uuid::Uuid = sqlx::query_scalar(
        "SELECT id FROM battle_reports WHERE attacker_village = $1 AND conquered = true",
    )
    .bind(a_vid)
    .fetch_one(&pool)
    .await
    .unwrap();
    let detail = ca
        .get(format!("{base}/reports/{}", report_id.as_u128()))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        detail.contains("Village captured"),
        "the report shows the capture"
    );
    assert!(
        detail.contains("Loyalty:"),
        "the report shows the loyalty change"
    );
}

/// 015 AC8/AC11: a player builds an Embassy, founds an alliance, invites another player who accepts,
/// and the roster + alliance tag (on the map) are visible. Drives the real HTTP stack; embassy levels
/// are seeded directly (building to level 3 over the slow 003 path is out of scope for the test). The
/// alliance name/tag are unique per run because the test DB is shared and not reset between runs.
#[sqlx::test(migrations = "../../migrations")]
async fn alliance_found_invite_accept_flow(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;

    // Register a founder and a member (registration logs each in via its own cookie client).
    let cf = client();
    let founder = unique("ally_f");
    cf.post(format!("{base}/register"))
        .form(&[
            ("username", founder.as_str()),
            ("email", format!("{founder}@example.com").as_str()),
            ("password", "secret12"),
            ("tribe", "gauls"),
        ])
        .send()
        .await
        .unwrap();
    let cm = client();
    let member = unique("ally_m");
    cm.post(format!("{base}/register"))
        .form(&[
            ("username", member.as_str()),
            ("email", format!("{member}@example.com").as_str()),
            ("password", "secret12"),
            ("tribe", "gauls"),
        ])
        .send()
        .await
        .unwrap();

    // Seed Embassy levels: founder ≥ 3 (to found), member ≥ 1 (to join).
    let set_embassy = |name: String, level: i16| {
        let pool = pool.clone();
        async move {
            sqlx::query(
                "INSERT INTO village_buildings (village_id, slot, building_type, level) \
                 SELECT v.id, 16, 'embassy', $2 FROM villages v JOIN users u ON u.id = v.owner_id \
                 WHERE u.username = $1 \
                 ON CONFLICT (village_id, slot) DO UPDATE SET level = EXCLUDED.level",
            )
            .bind(&name)
            .bind(level)
            .execute(&pool)
            .await
            .unwrap();
        }
    };
    set_embassy(founder.clone(), 3).await;
    set_embassy(member.clone(), 1).await;

    // Found an alliance over HTTP (unique name/tag per run).
    let aname = unique("Templars");
    let tag = format!("T{}", &aname[aname.len() - 6..]);
    let res = cf
        .post(format!("{base}/alliance/found"))
        .form(&[("name", aname.as_str()), ("tag", tag.as_str())])
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 303);
    let page = cf
        .get(format!("{base}/alliance"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(page.contains(&aname), "founder sees the alliance");
    assert!(page.contains("Founder"), "founder role shown");

    // Invite the member by name; the member accepts (the alliance id from the pending invite).
    cf.post(format!("{base}/alliance/invite"))
        .form(&[("username", member.as_str())])
        .send()
        .await
        .unwrap();
    let alliance_id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM alliances WHERE name = $1")
        .bind(&aname)
        .fetch_one(&pool)
        .await
        .unwrap();
    let res = cm
        .post(format!("{base}/alliance/respond"))
        .form(&[
            ("alliance", alliance_id.as_u128().to_string().as_str()),
            ("accept", "true"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 303);

    // The founder's roster now lists the member.
    let roster = cf
        .get(format!("{base}/alliance"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        roster.contains(member.as_str()),
        "the member is on the roster"
    );

    // AC11: the founder's village shows the alliance tag on the map.
    let (vx, vy): (i32, i32) = sqlx::query_as(
        "SELECT v.x, v.y FROM villages v JOIN users u ON u.id = v.owner_id WHERE u.username = $1",
    )
    .bind(&founder)
    .fetch_one(&pool)
    .await
    .unwrap();
    let map = cf
        .get(format!("{base}/map?x={vx}&y={vy}"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        map.contains(&format!("[{tag}]")),
        "the map shows the alliance tag"
    );

    // A non-member cannot see the roster (their view has no alliance).
    let co = client();
    let outsider = unique("ally_o");
    co.post(format!("{base}/register"))
        .form(&[
            ("username", outsider.as_str()),
            ("email", format!("{outsider}@example.com").as_str()),
            ("password", "secret12"),
            ("tribe", "gauls"),
        ])
        .send()
        .await
        .unwrap();
    let outsider_view = co
        .get(format!("{base}/alliance"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        !outsider_view.contains(&aname),
        "a non-member does not see the alliance roster"
    );
}

/// 016 AC2/AC9: the leaderboard and player statistics page are **public** (a visitor with no session
/// reaches them) and surface the registered player by population.
#[sqlx::test(migrations = "../../migrations")]
async fn leaderboard_and_stats_are_public(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let player = unique("lb");
    client()
        .post(format!("{base}/register"))
        .form(&[
            ("username", player.as_str()),
            ("email", format!("{player}@example.com").as_str()),
            ("password", "secret12"),
            ("tribe", "gauls"),
        ])
        .send()
        .await
        .unwrap();

    // A fresh, cookie-less client (a visitor) can read the leaderboard, and the player shows on the
    // population board (their starting village has population).
    let visitor = client();
    let res = visitor
        .get(format!("{base}/leaderboard"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 200);
    let body = res.text().await.unwrap();
    assert!(body.contains("Leaderboards"));
    assert!(
        body.contains(&player),
        "the population board lists the player"
    );

    // The conflict board variants render too.
    for cat in ["attackers", "defenders", "raiders", "alliances"] {
        let r = visitor
            .get(format!("{base}/leaderboard?cat={cat}"))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status().as_u16(), 200, "category {cat}");
    }

    // The player's public stats page (AC9): look up their id, then read it as a visitor.
    let pid: uuid::Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(&player)
        .fetch_one(&pool)
        .await
        .unwrap();
    let stats = visitor
        .get(format!("{base}/stats/player/{}", pid.as_u128()))
        .send()
        .await
        .unwrap();
    assert_eq!(stats.status().as_u16(), 200);
    let stats_body = stats.text().await.unwrap();
    assert!(stats_body.contains(&player));
    assert!(stats_body.contains("Population"));
    // A malformed id is a clean 404, not a 500.
    let bad = visitor
        .get(format!("{base}/stats/player/not-a-number"))
        .send()
        .await
        .unwrap();
    assert_eq!(bad.status().as_u16(), 404);

    // P11: with the world populated by more players, the board read stays within budget (best of 3 —
    // the population board computes population in SQL bounded by the page size, not per-village in Rust).
    for _ in 0..8 {
        let p = unique("lbp");
        client()
            .post(format!("{base}/register"))
            .form(&[
                ("username", p.as_str()),
                ("email", format!("{p}@example.com").as_str()),
                ("password", "secret12"),
                ("tribe", "gauls"),
            ])
            .send()
            .await
            .unwrap();
    }
    let mut best = std::time::Duration::MAX;
    for _ in 0..3 {
        let started = std::time::Instant::now();
        let r = visitor
            .get(format!("{base}/leaderboard?cat=population"))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status().as_u16(), 200);
        best = best.min(started.elapsed());
    }
    assert!(
        best.as_millis() < 250,
        "leaderboard read too slow: {best:?}"
    );
}

/// 046 AC1/AC4: a leaderboard reflects the **selected** world. An account that joined a second world
/// appears on that world's board with its name resolved (via `players`); an account that only plays the
/// home world is absent from the second world's board — proving the board is world-scoped.
#[sqlx::test(migrations = "../../migrations")]
async fn leaderboard_is_world_scoped(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let name_a = unique("alpha");
    let name_c = unique("gamma");
    let (ca, user_a) = register_client(&base, &pool, &name_a).await;
    let (_cc, _user_c) = register_client(&base, &pool, &name_c).await;

    // Account A also joins a second world B (account C stays home-only).
    let world_b = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO worlds (id, speed, radius, seed) VALUES ($1, 1.0, 30, 909)")
        .bind(world_b)
        .execute(&pool)
        .await
        .unwrap();
    let repo_b = PgAccountRepository::new(
        pool.clone(),
        WorldId(world_b.as_u128()),
        909,
        30,
        economy_rules().unwrap().starting_amounts,
        lifecycle_rules().unwrap().beginner_protection_secs,
        GameSpeed::new(1.0).unwrap(),
    );
    repo_b
        .create_player_in_world(
            PlayerId(user_a.as_u128()),
            Tribe::Teutons,
            &starting_village().unwrap(),
        )
        .await
        .unwrap();

    // Home board (no world cookie): both A and C appear.
    let home = ca
        .get(format!("{base}/leaderboard?cat=population"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(home.contains(&name_a), "A is on the home population board");
    assert!(home.contains(&name_c), "C is on the home population board");

    // Select world B → its board shows A (joined, name resolved via players) but not C (home-only).
    let r = ca
        .post(format!("{base}/world/select"))
        .form(&[("world", world_b.as_u128().to_string().as_str())])
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 303);
    let board_b = ca
        .get(format!("{base}/leaderboard?cat=population"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        board_b.contains(&name_a),
        "A's name resolves on world B's board via players"
    );
    assert!(
        !board_b.contains(&name_c),
        "C never joined world B — absent from its board (world-scoped)"
    );
}

/// 017 AC8/AC10: loading the (authenticated) village page lazily grants newly-earned achievements —
/// here a 2nd village earns `second_village` server-side.
#[sqlx::test(migrations = "../../migrations")]
async fn village_view_grants_achievements(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let c = client();
    let user = unique("achv");
    c.post(format!("{base}/register"))
        .form(&[
            ("username", user.as_str()),
            ("email", format!("{user}@example.com").as_str()),
            ("password", "secret12"),
            ("tribe", "gauls"),
        ])
        .send()
        .await
        .unwrap();

    let pid: uuid::Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(&user)
        .fetch_one(&pool)
        .await
        .unwrap();
    let world_id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM worlds LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    // Give them a 2nd village (the milestone), then load the village page (logged-in via the cookie).
    sqlx::query("INSERT INTO villages (id, world_id, owner_id, x, y, tribe) VALUES ($1, $2, $3, 88, 88, 'gauls')")
        .bind(uuid::Uuid::new_v4())
        .bind(world_id)
        .bind(pid)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        c.get(format!("{base}/village"))
            .send()
            .await
            .unwrap()
            .status()
            .as_u16(),
        200
    );
    let granted: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM player_achievements WHERE player_id = $1 AND achievement_id = 'second_village'",
    )
    .bind(pid)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        granted, 1,
        "loading the village granted the second-village achievement"
    );

    // 017 AC11/AC12: the public stat page shows the achievement, and the climbers board renders.
    let stats = client()
        .get(format!("{base}/stats/player/{}", pid.as_u128()))
        .send()
        .await
        .unwrap();
    assert_eq!(stats.status().as_u16(), 200);
    let body = stats.text().await.unwrap();
    assert!(body.contains("Achievements"));
    assert!(body.contains("Founded a second village"));
    assert!(body.contains("Population over time"));
    let climbers = client()
        .get(format!("{base}/leaderboard?cat=climbers"))
        .send()
        .await
        .unwrap();
    assert_eq!(climbers.status().as_u16(), 200);
    assert!(climbers.text().await.unwrap().contains("Top climbers"));
}

/// 018 AC8: the /quests page requires login, shows the player's current quest (with its reward) and
/// completed list, and lazily completes a quest whose action is now satisfied on view.
#[sqlx::test(migrations = "../../migrations")]
async fn quests_page_shows_progress_and_requires_login(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;

    // AC8: an unauthenticated visitor is redirected to login.
    let res = client().get(format!("{base}/quests")).send().await.unwrap();
    assert_eq!(res.status().as_u16(), 303);
    assert_eq!(
        res.headers().get(LOCATION).unwrap().to_str().unwrap(),
        "/login"
    );

    let c = client();
    let user = unique("quest");
    c.post(format!("{base}/register"))
        .form(&[
            ("username", user.as_str()),
            ("email", format!("{user}@example.com").as_str()),
            ("password", "secret12"),
            ("tribe", "gauls"),
        ])
        .send()
        .await
        .unwrap();
    let pid: uuid::Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(&user)
        .fetch_one(&pool)
        .await
        .unwrap();

    // A fresh player: the first quest is current (with its reward), nothing completed.
    let body = c
        .get(format!("{base}/quests"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body.contains("Upgrade a resource field to level 2."));
    assert!(
        body.contains("200 wood"),
        "the current quest's reward shows"
    );
    assert!(body.contains("No quests completed yet."));

    // Satisfy the first quest's action; loading the page completes it (lazily, server-side).
    sqlx::query(
        "UPDATE village_fields SET level = 2 \
         WHERE village_id IN (SELECT id FROM villages WHERE owner_id = $1) AND slot = 0",
    )
    .bind(pid)
    .execute(&pool)
    .await
    .unwrap();
    let body = c
        .get(format!("{base}/quests"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        body.contains("✓ Upgrade a resource field to level 2."),
        "the satisfied quest now appears completed"
    );
    assert!(
        body.contains("Build a Warehouse to store more resources."),
        "the next quest becomes current"
    );
    let completed: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM player_quests WHERE player_id = $1 AND quest_id = 'upgrade_field'",
    )
    .bind(pid)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(completed, 1, "the completion persisted exactly once");
}

/// 019 AC1/AC9: a freshly-registered player's village view shows their beginner's-protection status.
#[sqlx::test(migrations = "../../migrations")]
async fn village_view_shows_protection_status(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let c = client();
    let user = unique("prot");
    c.post(format!("{base}/register"))
        .form(&[
            ("username", user.as_str()),
            ("email", format!("{user}@example.com").as_str()),
            ("password", "secret12"),
            ("tribe", "gauls"),
        ])
        .send()
        .await
        .unwrap();
    let body = c
        .get(format!("{base}/village"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        body.contains("you cannot be attacked"),
        "the village view shows the protection notice"
    );
}

/// 019 AC6: the map greys / flags an inactive (farmable) player's village.
#[sqlx::test(migrations = "../../migrations")]
async fn map_flags_inactive_villages(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    // The viewer (logged in via the cookie).
    let viewer = client();
    let vname = unique("viewer");
    viewer
        .post(format!("{base}/register"))
        .form(&[
            ("username", vname.as_str()),
            ("email", format!("{vname}@example.com").as_str()),
            ("password", "secret12"),
            ("tribe", "gauls"),
        ])
        .send()
        .await
        .unwrap();
    // A second player who has gone inactive.
    let idle = client();
    let iname = unique("idle");
    idle.post(format!("{base}/register"))
        .form(&[
            ("username", iname.as_str()),
            ("email", format!("{iname}@example.com").as_str()),
            ("password", "secret12"),
            ("tribe", "gauls"),
        ])
        .send()
        .await
        .unwrap();
    let (ix, iy): (i32, i32) = sqlx::query_as(
        "SELECT v.x, v.y FROM villages v JOIN users u ON u.id = v.owner_id WHERE u.username = $1",
    )
    .bind(&iname)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE users SET last_activity = to_timestamp(1) WHERE username = $1")
        .bind(&iname)
        .execute(&pool)
        .await
        .unwrap();
    // Center the viewer's map on the idle player's village so it is in the viewport.
    let body = viewer
        .get(format!("{base}/map?x={ix}&y={iy}"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        body.contains("(inactive)"),
        "the inactive player's village is flagged on the map"
    );
    assert!(
        body.contains("map-grid__cell--inactive"),
        "the inactive village's cell is greyed"
    );
}

/// 019 AC8: an abandoned account cannot log in.
#[sqlx::test(migrations = "../../migrations")]
async fn abandoned_account_cannot_log_in(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let c = client();
    let user = unique("retired");
    c.post(format!("{base}/register"))
        .form(&[
            ("username", user.as_str()),
            ("email", format!("{user}@example.com").as_str()),
            ("password", "secret12"),
            ("tribe", "gauls"),
        ])
        .send()
        .await
        .unwrap();
    sqlx::query("UPDATE users SET abandoned_at = now() WHERE username = $1")
        .bind(&user)
        .execute(&pool)
        .await
        .unwrap();
    // A fresh client (no cookie) attempts to log in.
    let body = client()
        .post(format!("{base}/login"))
        .form(&[("username", user.as_str()), ("password", "secret12")])
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        body.contains("retired"),
        "an abandoned account is told it has been retired"
    );
}

/// 020 AC8: a player who holds an artifact sees it on their village view.
#[sqlx::test(migrations = "../../migrations")]
async fn village_shows_held_artifacts(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let c = client();
    let user = unique("arti");
    c.post(format!("{base}/register"))
        .form(&[
            ("username", user.as_str()),
            ("email", format!("{user}@example.com").as_str()),
            ("password", "secret12"),
            ("tribe", "gauls"),
        ])
        .send()
        .await
        .unwrap();
    let (vid, world_id): (uuid::Uuid, uuid::Uuid) = sqlx::query_as(
        "SELECT v.id, v.world_id FROM villages v JOIN users u ON u.id = v.owner_id \
         WHERE u.username = $1",
    )
    .bind(&user)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO artifacts (id, world_id, kind, scope, magnitude, holder_village, origin_x, origin_y) \
         VALUES ('vt_speed', $1, 'speed', 'large', 2.0, $2, 0, 0)",
    )
    .bind(world_id)
    .bind(vid)
    .execute(&pool)
    .await
    .unwrap();

    let body = c
        .get(format!("{base}/village"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body.contains("Artifacts held"), "the holdings panel shows");
    assert!(
        body.contains("Speed (large)"),
        "the held artifact is listed"
    );
}

/// 021 AC7: once the world is won, the server rejects mutating game actions (POSTs) but still serves
/// reads and authentication, so players can log in to see the result.
#[sqlx::test(migrations = "../../migrations")]
async fn frozen_world_rejects_mutations(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let c = client();
    let user = unique("freeze");
    let email = format!("{user}@example.com");
    c.post(format!("{base}/register"))
        .form(&[
            ("username", user.as_str()),
            ("email", email.as_str()),
            ("password", "secret12"),
            ("tribe", "gauls"),
        ])
        .send()
        .await
        .unwrap();

    // A read works while the world is open.
    assert_ne!(
        c.get(format!("{base}/village"))
            .send()
            .await
            .unwrap()
            .status()
            .as_u16(),
        403
    );

    // Win + freeze the world: record a winner alliance and `won_at`.
    sqlx::query(
        "INSERT INTO alliances (id, name, tag, founder_id) \
         SELECT gen_random_uuid(), 'Winners', 'WIN', id FROM users LIMIT 1",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE worlds SET won_at = now(), winner_alliance_id = (SELECT id FROM alliances LIMIT 1)",
    )
    .execute(&pool)
    .await
    .unwrap();

    // A mutating game action is now rejected (the guard runs before the handler).
    let build = c
        .post(format!("{base}/village/build"))
        .form(&[("target", "field"), ("slot", "0")])
        .send()
        .await
        .unwrap();
    assert_eq!(build.status().as_u16(), 403, "mutations are frozen");

    // Reads still work...
    assert_ne!(
        c.get(format!("{base}/village"))
            .send()
            .await
            .unwrap()
            .status()
            .as_u16(),
        403,
        "reads stay available after the round ends"
    );
    // ...and authentication is still allowed (not frozen).
    assert_ne!(
        c.post(format!("{base}/logout"))
            .send()
            .await
            .unwrap()
            .status()
            .as_u16(),
        403,
        "logout stays available after the round ends"
    );
}

/// 021 AC9: the Wonder race page lists alliances by their Wonder level.
#[sqlx::test(migrations = "../../migrations")]
async fn wonder_race_page_shows_progress(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let c = client();
    let user = unique("race");
    let email = format!("{user}@example.com");
    c.post(format!("{base}/register"))
        .form(&[
            ("username", user.as_str()),
            ("email", email.as_str()),
            ("password", "secret12"),
            ("tribe", "gauls"),
        ])
        .send()
        .await
        .unwrap();

    // The player founds an alliance and raises a Wonder to level 5 on their village.
    sqlx::query(
        "INSERT INTO alliances (id, name, tag, founder_id) \
         SELECT gen_random_uuid(), 'Racers', 'RAC', id FROM users WHERE username = $1",
    )
    .bind(&user)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO alliance_members (player_id, alliance_id, role) \
         SELECT u.id, a.id, 'founder' FROM users u, alliances a \
         WHERE u.username = $1 AND a.tag = 'RAC'",
    )
    .bind(&user)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE villages SET is_wonder_site = true \
         WHERE owner_id = (SELECT id FROM users WHERE username = $1)",
    )
    .bind(&user)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO village_buildings (village_id, slot, building_type, level) \
         SELECT v.id, 18, 'wonder', 5 FROM villages v JOIN users u ON u.id = v.owner_id \
         WHERE u.username = $1",
    )
    .bind(&user)
    .execute(&pool)
    .await
    .unwrap();

    let body = c
        .get(format!("{base}/wonder"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body.contains("Racers"), "the alliance is listed");
    assert!(body.contains("5 / 100"), "its Wonder progress shows");
}

/// 021 AC6/AC9: once won, the Wonder page shows the winner banner.
#[sqlx::test(migrations = "../../migrations")]
async fn wonder_winner_banner_shows_when_won(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let c = client();
    let user = unique("won");
    let email = format!("{user}@example.com");
    c.post(format!("{base}/register"))
        .form(&[
            ("username", user.as_str()),
            ("email", email.as_str()),
            ("password", "secret12"),
            ("tribe", "gauls"),
        ])
        .send()
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO alliances (id, name, tag, founder_id) \
         SELECT gen_random_uuid(), 'Champions', 'CHM', id FROM users WHERE username = $1",
    )
    .bind(&user)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE worlds SET won_at = now(), winner_alliance_id = (SELECT id FROM alliances LIMIT 1)",
    )
    .execute(&pool)
    .await
    .unwrap();

    let body = c
        .get(format!("{base}/wonder"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        body.contains("The round is over"),
        "the winner banner shows"
    );
    assert!(body.contains("Champions"), "the winning alliance is named");
}

/// 022 AC5: a sanctioned (banned) logged-in player's mutating actions are rejected, but reads still work.
#[sqlx::test(migrations = "../../migrations")]
async fn sanctioned_player_actions_are_blocked(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let c = client();
    let user = unique("sanc");
    let email = format!("{user}@example.com");
    c.post(format!("{base}/register"))
        .form(&[
            ("username", user.as_str()),
            ("email", email.as_str()),
            ("password", "secret12"),
            ("tribe", "gauls"),
        ])
        .send()
        .await
        .unwrap();

    // Before sanction: a mutating action is not 403-blocked by the guard.
    let before = c
        .post(format!("{base}/village/build"))
        .form(&[("table", "field"), ("slot", "0")])
        .send()
        .await
        .unwrap();
    assert_ne!(
        before.status().as_u16(),
        403,
        "action allowed before sanction"
    );

    // Ban the account.
    sqlx::query("UPDATE users SET banned_at = now() WHERE username = $1")
        .bind(&user)
        .execute(&pool)
        .await
        .unwrap();

    // The mutating action is now rejected...
    let after = c
        .post(format!("{base}/village/build"))
        .form(&[("table", "field"), ("slot", "0")])
        .send()
        .await
        .unwrap();
    assert_eq!(after.status().as_u16(), 403, "sanctioned action is blocked");
    // ...but a read still works.
    assert_ne!(
        c.get(format!("{base}/village"))
            .send()
            .await
            .unwrap()
            .status()
            .as_u16(),
        403,
        "reads stay available for a sanctioned account"
    );
}

/// 022 AC6: login attempts are rate-limited per IP — after the configured number, further attempts get
/// 429 (a brute-force guard), even with wrong credentials.
#[sqlx::test(migrations = "../../migrations")]
async fn login_attempts_are_rate_limited(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let c = client();
    let limit = fair_play_rules().unwrap().login_limit_per_window;

    // The first `limit` attempts are processed (not 429); the next is rejected with 429.
    for _ in 0..limit {
        let res = c
            .post(format!("{base}/login"))
            .form(&[("username", "ghost"), ("password", "nope")])
            .send()
            .await
            .unwrap();
        assert_ne!(res.status().as_u16(), 429, "within the limit");
    }
    let over = c
        .post(format!("{base}/login"))
        .form(&[("username", "ghost"), ("password", "nope")])
        .send()
        .await
        .unwrap();
    assert_eq!(over.status().as_u16(), 429, "over the limit is rejected");

    // Per-IP isolation: a different client IP (distinct X-Forwarded-For, trusted in tests) has its own
    // budget and is not affected by the first IP exhausting theirs.
    let other = c
        .post(format!("{base}/login"))
        .header("x-forwarded-for", "198.51.100.42")
        .form(&[("username", "ghost"), ("password", "nope")])
        .send()
        .await
        .unwrap();
    assert_ne!(
        other.status().as_u16(),
        429,
        "a different IP has an independent login budget"
    );
}

/// 022 AC1–AC4/AC9: a player reports an account; a non-moderator is denied /mod; a moderator sees the
/// report, resolves it with a ban, and the subject is then blocked from acting.
#[sqlx::test(migrations = "../../migrations")]
async fn moderation_report_to_sanction_flow(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;

    // Three logged-in clients: reporter, subject, moderator.
    let register = async |name: &str| -> reqwest::Client {
        let c = client();
        c.post(format!("{base}/register"))
            .form(&[
                ("username", name),
                ("email", &format!("{name}@example.com")),
                ("password", "secret12"),
                ("tribe", "gauls"),
            ])
            .send()
            .await
            .unwrap();
        c
    };
    let reporter = unique("reporter");
    let subject = unique("subject");
    let moderator = unique("mod");
    let cr = register(&reporter).await;
    let cs = register(&subject).await;
    let cm = register(&moderator).await;

    let subject_id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(&subject)
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE users SET is_moderator = true WHERE username = $1")
        .bind(&moderator)
        .execute(&pool)
        .await
        .unwrap();

    // The reporter files a report.
    cr.post(format!("{base}/report"))
        .form(&[
            ("subject", subject_id.as_u128().to_string().as_str()),
            ("reason", "botting"),
            ("note", "scripts all night"),
        ])
        .send()
        .await
        .unwrap();

    // AC1/AC3: a non-moderator is denied the queue; the moderator sees the report.
    assert_eq!(
        cr.get(format!("{base}/mod"))
            .send()
            .await
            .unwrap()
            .status()
            .as_u16(),
        403
    );
    let queue = cm
        .get(format!("{base}/mod"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(queue.contains(&subject), "the report shows the subject");

    // AC4: the moderator resolves the report with a ban.
    let report_id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM reports WHERE status = 'open'")
        .fetch_one(&pool)
        .await
        .unwrap();
    cm.post(format!("{base}/mod/resolve"))
        .form(&[
            ("report_id", report_id.as_u128().to_string().as_str()),
            ("resolution", "confirmed"),
            ("sanction", "ban"),
        ])
        .send()
        .await
        .unwrap();

    // The subject is now blocked from acting (AC5) and the queue is empty (AC4).
    let blocked = cs
        .post(format!("{base}/village/build"))
        .form(&[("table", "field"), ("slot", "0")])
        .send()
        .await
        .unwrap();
    assert_eq!(
        blocked.status().as_u16(),
        403,
        "the banned subject cannot act"
    );
    let queue2 = cm
        .get(format!("{base}/mod"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        !queue2.contains(&subject),
        "the resolved report left the queue"
    );
}

/// Register a logged-in client for `name`; returns (client, the account's uuid).
async fn register_client(
    base: &str,
    pool: &sqlx::PgPool,
    name: &str,
) -> (reqwest::Client, uuid::Uuid) {
    let c = client();
    c.post(format!("{base}/register"))
        .form(&[
            ("username", name),
            ("email", &format!("{name}@example.com")),
            ("password", "secret12"),
            ("tribe", "gauls"),
        ])
        .send()
        .await
        .unwrap();
    let id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(name)
        .fetch_one(pool)
        .await
        .unwrap();
    (c, id)
}

/// 043: selecting a second world makes game requests operate in it — the village page renders **that
/// world's** village (a different village than the home one). The account login is unaffected.
#[sqlx::test(migrations = "../../migrations")]
async fn selecting_a_world_switches_the_village_page(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let (c, user) = register_client(&base, &pool, &unique("mw")).await;
    let home_vid: uuid::Uuid = sqlx::query_scalar("SELECT id FROM villages WHERE owner_id = $1")
        .bind(user)
        .fetch_one(&pool)
        .await
        .unwrap();

    // A second world + the account's player + starting village in it (the 042 join primitive).
    let world_b = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO worlds (id, speed, radius, seed) VALUES ($1, 1.0, 30, 4242)")
        .bind(world_b)
        .execute(&pool)
        .await
        .unwrap();
    let repo_b = PgAccountRepository::new(
        pool.clone(),
        WorldId(world_b.as_u128()),
        4242,
        30,
        economy_rules().unwrap().starting_amounts,
        lifecycle_rules().unwrap().beginner_protection_secs,
        GameSpeed::new(1.0).unwrap(),
    );
    let player_b = repo_b
        .create_player_in_world(
            PlayerId(user.as_u128()),
            Tribe::Teutons,
            &starting_village().unwrap(),
        )
        .await
        .unwrap();
    let b_vid: uuid::Uuid = sqlx::query_scalar("SELECT id FROM villages WHERE owner_id = $1")
        .bind(uuid::Uuid::from_u128(player_b.0))
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_ne!(home_vid, b_vid);

    // By default the village page shows the home world's village.
    let body = c
        .get(format!("{base}/village"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        body.contains(&home_vid.as_u128().to_string()),
        "default shows the home village"
    );
    assert!(!body.contains(&b_vid.as_u128().to_string()));

    // Select world B → the village page now operates in world B.
    let r = c
        .post(format!("{base}/world/select"))
        .form(&[("world", world_b.as_u128().to_string().as_str())])
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 303);
    let body = c
        .get(format!("{base}/village"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        body.contains(&b_vid.as_u128().to_string()),
        "after selecting world B the page shows world B's village"
    );
    assert!(
        !body.contains(&home_vid.as_u128().to_string()),
        "the home village is no longer shown"
    );

    // The account login (account-level) is unaffected by the world selection.
    let me = c
        .get(format!("{base}/me"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(me.contains("\"authed\":true"));
}

/// 043 AC4 (server-authoritative denial): `POST /world/select` with a world the account has **not**
/// joined (and with a garbage id) is ignored — the village page keeps rendering the home village.
#[sqlx::test(migrations = "../../migrations")]
async fn selecting_an_unjoined_world_is_ignored(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let (c, user) = register_client(&base, &pool, &unique("mw")).await;
    let home_vid: uuid::Uuid = sqlx::query_scalar("SELECT id FROM villages WHERE owner_id = $1")
        .bind(user)
        .fetch_one(&pool)
        .await
        .unwrap();

    // A second world exists, but the account never joined it (no player row in it).
    let world_b = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO worlds (id, speed, radius, seed) VALUES ($1, 1.0, 30, 4242)")
        .bind(world_b)
        .execute(&pool)
        .await
        .unwrap();

    // Selecting the unjoined world is ignored (server-authoritative) → village stays home.
    let r = c
        .post(format!("{base}/world/select"))
        .form(&[("world", world_b.as_u128().to_string().as_str())])
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 303);
    // The handler's own guard must not set the `world` cookie (independent of the GameContext
    // home-fallback backstop) — a regression that set it would be caught here directly.
    assert!(
        !r.headers()
            .get_all("set-cookie")
            .iter()
            .any(|v| v.to_str().map(|s| s.starts_with("world=")).unwrap_or(false)),
        "an unjoined world must not set the world cookie"
    );
    let body = c
        .get(format!("{base}/village"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        body.contains(&home_vid.as_u128().to_string()),
        "an unjoined world is ignored — the home village still renders"
    );

    // A garbage (non-existent) world id is likewise ignored.
    let r = c
        .post(format!("{base}/world/select"))
        .form(&[("world", uuid::Uuid::new_v4().as_u128().to_string().as_str())])
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 303);
    let body = c
        .get(format!("{base}/village"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        body.contains(&home_vid.as_u128().to_string()),
        "a garbage world id is ignored — the home village still renders"
    );
}

/// 044 AC3: a **game action** (a build order) issued with a second world selected lands in **that**
/// world's village, not the home one — proving the migrated handlers operate through `GameContext`.
#[sqlx::test(migrations = "../../migrations")]
async fn build_order_lands_in_the_selected_world(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let (c, user) = register_client(&base, &pool, &unique("mw")).await;
    let home_vid: uuid::Uuid = sqlx::query_scalar("SELECT id FROM villages WHERE owner_id = $1")
        .bind(user)
        .fetch_one(&pool)
        .await
        .unwrap();

    // A second world + the account's player + starting village in it (the 042 join primitive).
    let world_b = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO worlds (id, speed, radius, seed) VALUES ($1, 1.0, 30, 4242)")
        .bind(world_b)
        .execute(&pool)
        .await
        .unwrap();
    let repo_b = PgAccountRepository::new(
        pool.clone(),
        WorldId(world_b.as_u128()),
        4242,
        30,
        economy_rules().unwrap().starting_amounts,
        lifecycle_rules().unwrap().beginner_protection_secs,
        GameSpeed::new(1.0).unwrap(),
    );
    let player_b = repo_b
        .create_player_in_world(
            PlayerId(user.as_u128()),
            Tribe::Teutons,
            &starting_village().unwrap(),
        )
        .await
        .unwrap();
    let b_vid: uuid::Uuid = sqlx::query_scalar("SELECT id FROM villages WHERE owner_id = $1")
        .bind(uuid::Uuid::from_u128(player_b.0))
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_ne!(home_vid, b_vid);

    // Select world B, then order a field upgrade — the migrated build handler acts in world B.
    let r = c
        .post(format!("{base}/world/select"))
        .form(&[("world", world_b.as_u128().to_string().as_str())])
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 303);
    let r = c
        .post(format!("{base}/village/build"))
        .form(&[("table", "field"), ("slot", "0")])
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 303);

    // The build order landed on world B's village — never the home village.
    let order_vid: uuid::Uuid =
        sqlx::query_scalar("SELECT village_id FROM build_orders WHERE status = 'pending'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        order_vid, b_vid,
        "the build order lands in the selected world's village"
    );
    let home_orders: i64 =
        sqlx::query_scalar("SELECT count(*) FROM build_orders WHERE village_id = $1")
            .bind(home_vid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(home_orders, 0, "the home village received no build order");
}

/// 044 AC1/AC3 (the `village_view_data` speed seam): a migrated economy view settles resources with the
/// **selected world's** speed (`ctx.speed`), not the home world's. The academy's affordability gate is the
/// observable that depends on the settled `amounts`. Two worlds differ **only** in speed (5× vs 1×) over an
/// identical elapsed interval from zeroed resources: at 5× the Teuton academy-gated units (Spearman/Scout)
/// become affordable; at 1× they do not. A regression that fed the home speed into `village_view_data`
/// would make the 5× world behave like the 1× one and fail the first assertion.
#[sqlx::test(migrations = "../../migrations")]
async fn academy_affordability_uses_the_selected_worlds_speed(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let (c, user) = register_client(&base, &pool, &unique("mw")).await;

    // Two joined second worlds for the same account, identical but for speed: B = 5×, S = 1× (slow).
    // For each, give the village an Academy (to engage research gating) + Warehouse/Granary (so the
    // research costs fit under the storage cap), zero wood/clay/iron, stock crop, and backdate the
    // resource clock 24h. The only variable that moves the settled amounts is the world speed.
    let mut vids = std::collections::HashMap::new();
    for (seed, speed) in [(4242_i64, 5.0_f64), (4343, 1.0)] {
        let world = uuid::Uuid::new_v4();
        sqlx::query("INSERT INTO worlds (id, speed, radius, seed) VALUES ($1, $2, 30, $3)")
            .bind(world)
            .bind(speed)
            .bind(seed)
            .execute(&pool)
            .await
            .unwrap();
        let repo = PgAccountRepository::new(
            pool.clone(),
            WorldId(world.as_u128()),
            seed,
            30,
            economy_rules().unwrap().starting_amounts,
            lifecycle_rules().unwrap().beginner_protection_secs,
            GameSpeed::new(speed).unwrap(),
        );
        let player = repo
            .create_player_in_world(
                PlayerId(user.as_u128()),
                Tribe::Teutons,
                &starting_village().unwrap(),
            )
            .await
            .unwrap();
        let vid: uuid::Uuid = sqlx::query_scalar("SELECT id FROM villages WHERE owner_id = $1")
            .bind(uuid::Uuid::from_u128(player.0))
            .fetch_one(&pool)
            .await
            .unwrap();
        // Academy L20 (research unlocked), Warehouse/Granary L10 (raise the storage cap well above the
        // research costs) in unused slots.
        for (slot, kind, level) in [(2, "academy", 20), (3, "warehouse", 10), (4, "granary", 10)] {
            sqlx::query(
                "INSERT INTO village_buildings (village_id, slot, building_type, level) \
                 VALUES ($1, $2, $3, $4)",
            )
            .bind(vid)
            .bind(slot as i16)
            .bind(kind)
            .bind(level as i16)
            .execute(&pool)
            .await
            .unwrap();
        }
        // Zero the mined resources, stock crop, and backdate the clock 24h so production accrues.
        sqlx::query(
            "UPDATE village_resources SET wood = 0, clay = 0, iron = 0, crop = 12000, \
             updated_at = now() - interval '24 hours' WHERE village_id = $1",
        )
        .bind(vid)
        .execute(&pool)
        .await
        .unwrap();
        vids.insert(speed.to_bits(), (world, vid));
    }

    let select = |world: uuid::Uuid| {
        c.post(format!("{base}/world/select"))
            .form(&[("world", world.as_u128().to_string())])
    };
    let academy = || c.get(format!("{base}/village/academy"));

    // World B (5×): 24h of accrual affords the academy-gated units → no "insufficient resources" gate.
    let (world_b, _) = vids[&5.0_f64.to_bits()];
    assert_eq!(select(world_b).send().await.unwrap().status().as_u16(), 303);
    let fast = academy().send().await.unwrap().text().await.unwrap();
    assert!(
        !fast.contains("insufficient resources"),
        "at 5× the academy units are affordable — no insufficient-resources gate"
    );

    // World S (1×): the same 24h accrues 5× less → the same units are unaffordable. This is what the
    // 5× world would also show if `village_view_data` settled with the home speed (the regression).
    let (world_s, _) = vids[&1.0_f64.to_bits()];
    assert_eq!(select(world_s).send().await.unwrap().status().as_u16(), 303);
    let slow = academy().send().await.unwrap().text().await.unwrap();
    assert!(
        slow.contains("insufficient resources"),
        "at 1× the same units are unaffordable — proving the speed is what flips the gate"
    );
}

/// 050 AC1/AC2: the registry resolves each world's `rule_preset` (049) to a bundle and serves it through
/// `context_for`. Two `classic` worlds share **one** cached bundle (per-preset, not per-world reload), each
/// keeps its own speed (043), and a world on an unknown preset is not serviceable (`None`, never a panic).
#[sqlx::test(migrations = "../../migrations")]
async fn registry_serves_each_worlds_preset_bundle(pool: sqlx::PgPool) {
    let config = WorldConfig::new(GameSpeed::new(1.0).unwrap(), 30);
    let home = ensure_world(&pool, &config).await.expect("home world");
    // A second `classic` world at 5× (rule_preset defaults to 'classic').
    let other = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO worlds (id, speed, radius, seed) VALUES ($1, 5.0, 30, 777)")
        .bind(other)
        .execute(&pool)
        .await
        .unwrap();

    let boot = Arc::new(load_world_rules(&home.rule_preset).expect("classic bundle"));
    let (tx, rx) = tokio::sync::watch::channel(false);
    std::mem::forget(tx);
    let registry = WorldRegistry::new(
        pool.clone(),
        rx,
        0,
        home.rule_preset.clone(),
        Arc::clone(&boot),
    );

    let (_r1, _m1, s_home, _rad1, rules_home) =
        registry.context_for(home.id).await.expect("home context");
    let (_r2, _m2, s_other, _rad2, rules_other) = registry
        .context_for(WorldId(other.as_u128()))
        .await
        .expect("other context");

    // Each world keeps its own speed (043) …
    assert!((s_home.multiplier() - 1.0).abs() < f64::EPSILON);
    assert!((s_other.multiplier() - 5.0).abs() < f64::EPSILON);
    // … but both `classic` worlds are served the *same* cached bundle (AC1), seeded from the boot bundle.
    assert!(
        Arc::ptr_eq(&rules_home, &rules_other),
        "same preset → one shared bundle"
    );
    assert!(
        Arc::ptr_eq(&rules_home, &boot),
        "served from the seeded boot bundle, not reloaded"
    );

    // A world on an unknown preset is not serviceable (AC1) — resolved to None, never a panic (P4).
    let bad = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO worlds (id, speed, radius, seed, rule_preset) VALUES ($1, 1.0, 30, 9, 'nonesuch')",
    )
    .bind(bad)
    .execute(&pool)
    .await
    .unwrap();
    assert!(
        registry.context_for(WorldId(bad.as_u128())).await.is_none(),
        "an unknown preset makes the world unserviceable, not a panic"
    );
}

/// 053 AC1/AC2 (acceptance): two worlds at the **same** `GameSpeed` but different presets are served
/// **divergent** rules by the registry (the full resolve path), so the difference is the preset itself, not
/// the speed multiplier. The `speed` world gets shorter beginner protection (the ADR example) and 2× unit
/// map speed; the home world stays `classic`.
#[sqlx::test(migrations = "../../migrations")]
async fn classic_and_speed_worlds_are_served_divergent_rules(pool: sqlx::PgPool) {
    // Home world: classic at 1×.
    let config = WorldConfig::new(GameSpeed::new(1.0).unwrap(), 30);
    let home = ensure_world(&pool, &config).await.expect("home world");
    // A `speed` world at the **same** 1× speed — only the preset differs.
    let speed_id = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO worlds (id, speed, radius, seed, rule_preset) VALUES ($1, 1.0, 30, 55, 'speed')",
    )
    .bind(speed_id)
    .execute(&pool)
    .await
    .unwrap();

    let boot = Arc::new(load_world_rules(&home.rule_preset).expect("classic bundle"));
    let (tx, rx) = tokio::sync::watch::channel(false);
    std::mem::forget(tx);
    let registry = WorldRegistry::new(
        pool.clone(),
        rx,
        0,
        home.rule_preset.clone(),
        Arc::clone(&boot),
    );

    let (_r1, _m1, _s1, _rad1, home_rules) =
        registry.context_for(home.id).await.expect("home context");
    let (_r2, _m2, _s2, _rad2, speed_rules) = registry
        .context_for(WorldId(speed_id.as_u128()))
        .await
        .expect("speed context");

    // AC2: the home world is served the classic bundle (the boot bundle, unchanged by the speed world).
    assert!(Arc::ptr_eq(&home_rules, &boot), "home stays classic");

    // AC1: the speed world is served divergent rules — shorter beginner protection …
    assert!(
        speed_rules.lifecycle.beginner_protection_secs
            < home_rules.lifecycle.beginner_protection_secs,
        "the speed world has shorter beginner protection"
    );
    // … and 2× unit map speed (same roster, every unit doubled).
    let g_speed = speed_rules.units.roster(Tribe::Gauls);
    let g_home = home_rules.units.roster(Tribe::Gauls);
    assert_eq!(g_speed.len(), g_home.len());
    assert!(
        g_speed
            .iter()
            .zip(g_home.iter())
            .all(|(s, c)| s.speed == c.speed * 2),
        "the speed world's troops move at 2× the classic map speed"
    );
}

/// 045 AC2–AC5 (end-to-end): from the lobby a player joins a second world → lands in its village → its
/// name resolves on the map (re-pointed reads) → switches back to the home world.
#[sqlx::test(migrations = "../../migrations")]
async fn lobby_join_play_and_switch_back(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let (c, user) = register_client(&base, &pool, &unique("lobby")).await;
    let username: String = sqlx::query_scalar("SELECT username FROM users WHERE id = $1")
        .bind(user)
        .fetch_one(&pool)
        .await
        .unwrap();
    let home_vid: uuid::Uuid = sqlx::query_scalar("SELECT id FROM villages WHERE owner_id = $1")
        .bind(user)
        .fetch_one(&pool)
        .await
        .unwrap();
    let home_world: uuid::Uuid = sqlx::query_scalar("SELECT world_id FROM villages WHERE id = $1")
        .bind(home_vid)
        .fetch_one(&pool)
        .await
        .unwrap();

    // A second world exists (run by the registry via the worlds row → context_for self-populates).
    let world_b = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO worlds (id, speed, radius, seed) VALUES ($1, 3.0, 30, 808)")
        .bind(world_b)
        .execute(&pool)
        .await
        .unwrap();

    // The lobby lists the home world (joined) and world B (joinable).
    let lobby = c
        .get(format!("{base}/worlds"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        lobby.contains("Home world"),
        "the home world is listed as joined"
    );
    assert!(
        lobby.contains(&world_b.as_u128().to_string()),
        "world B is offered to join"
    );

    // Join world B as Teutons → lands in world B's village (a new village, ≠ the home one).
    let r = c
        .post(format!("{base}/worlds/join"))
        .form(&[
            ("world", world_b.as_u128().to_string().as_str()),
            ("tribe", "teutons"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 303);
    let b_vid: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM villages WHERE world_id = $1 AND owner_id <> $2")
            .bind(world_b)
            .bind(user)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_ne!(home_vid, b_vid);
    let village = c
        .get(format!("{base}/village"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        village.contains(&b_vid.as_u128().to_string()),
        "after joining, the village page operates in world B"
    );

    // The second-world player's name resolves on world B's map (re-pointed owner→players→users read).
    let map = c
        .get(format!("{base}/map"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        map.contains(&username),
        "the second-world player's name resolves on the map"
    );

    // Switching back to the home world returns to the home village.
    let r = c
        .post(format!("{base}/world/select"))
        .form(&[("world", home_world.as_u128().to_string().as_str())])
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 303);
    let village = c
        .get(format!("{base}/village"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        village.contains(&home_vid.as_u128().to_string()),
        "switching back returns to the home village"
    );
}

/// 045 AC3 (server-authoritative denial, P4): `POST /worlds/join` rejects a **won** world and a garbage
/// world id — no player is created and the player stays on the home world.
#[sqlx::test(migrations = "../../migrations")]
async fn joining_a_won_or_unknown_world_is_rejected(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let (c, user) = register_client(&base, &pool, &unique("lobby")).await;

    // A won (frozen, 021) world — offered nowhere, but a crafted POST must still be refused.
    let won = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO worlds (id, speed, radius, seed, won_at) VALUES ($1, 1.0, 30, 5, now())",
    )
    .bind(won)
    .execute(&pool)
    .await
    .unwrap();

    for world in [
        won.as_u128().to_string(),
        uuid::Uuid::new_v4().as_u128().to_string(),
    ] {
        let r = c
            .post(format!("{base}/worlds/join"))
            .form(&[("world", world.as_str()), ("tribe", "teutons")])
            .send()
            .await
            .unwrap();
        assert_eq!(r.status().as_u16(), 303);
    }

    // No player was created for this account in the won world (nor anywhere but home).
    let player_count: i64 = sqlx::query_scalar("SELECT count(*) FROM players WHERE user_id = $1")
        .bind(user)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        player_count, 1,
        "only the home-world player exists — the join was refused"
    );
    let won_players: i64 = sqlx::query_scalar("SELECT count(*) FROM players WHERE world_id = $1")
        .bind(won)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(won_players, 0, "no player was created in the won world");
}

/// 024 AC2–AC4: a DM appears for both parties; opening it clears the recipient's unread.
#[sqlx::test(migrations = "../../migrations")]
async fn dm_conversation_flow(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let alice = unique("alice");
    let bob = unique("bob");
    let (ca, _aid) = register_client(&base, &pool, &alice).await;
    let (cb, bid) = register_client(&base, &pool, &bob).await;
    let (_ca2, aid) = (&ca, {
        sqlx::query_scalar::<_, uuid::Uuid>("SELECT id FROM users WHERE username = $1")
            .bind(&alice)
            .fetch_one(&pool)
            .await
            .unwrap()
    });

    // Alice DMs Bob (key uses Bob's uuid).
    ca.post(format!("{base}/messages/send"))
        .form(&[
            ("conversation", format!("dm:{bid}").as_str()),
            ("body", "hello bob"),
        ])
        .send()
        .await
        .unwrap();

    // Bob's unread badge shows 1, and his conversation list contains the thread with Alice.
    let unread = cb
        .get(format!("{base}/messages/unread"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(unread.trim(), "1", "bob has one unread");
    let list = cb
        .get(format!("{base}/messages"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(list.contains(&alice), "alice's thread is listed for bob");

    // Bob opens the thread (key uses Alice's uuid) → sees the message; unread clears.
    let convo = cb
        .get(format!("{base}/messages/c/dm:{aid}"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(convo.contains("hello bob"), "bob sees the message");
    let unread2 = cb
        .get(format!("{base}/messages/unread"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(unread2.trim(), "0", "opening clears unread");
}

/// 024 AC5: the global channel is open; a non-member alliance channel is forbidden.
#[sqlx::test(migrations = "../../migrations")]
async fn chat_channel_access(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let (c, _id) = register_client(&base, &pool, &unique("chatter")).await;

    // Global: post + read works.
    c.post(format!("{base}/messages/send"))
        .form(&[("conversation", "global"), ("body", "gg all")])
        .send()
        .await
        .unwrap();
    let g = c
        .get(format!("{base}/messages/c/global"))
        .send()
        .await
        .unwrap();
    assert_eq!(g.status().as_u16(), 200);
    assert!(g.text().await.unwrap().contains("gg all"));

    // A non-member alliance channel is forbidden (read + stream).
    assert_eq!(
        c.get(format!("{base}/messages/c/alliance:999"))
            .send()
            .await
            .unwrap()
            .status()
            .as_u16(),
        403
    );
    assert_eq!(
        c.get(format!("{base}/messages/stream/alliance:999"))
            .send()
            .await
            .unwrap()
            .status()
            .as_u16(),
        403
    );
}

/// 024 AC6/AC8: a posted message is delivered live over the SSE stream (LISTEN/NOTIFY round-trip).
#[sqlx::test(migrations = "../../migrations")]
async fn chat_live_delivery(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let (listener, _l) = register_client(&base, &pool, &unique("listener")).await;
    let (poster, _p) = register_client(&base, &pool, &unique("poster")).await;

    // Open the global SSE stream and start reading it.
    let mut resp = listener
        .get(format!("{base}/messages/stream/global"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 200);

    // Give the handler a moment to subscribe, then post a message.
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    poster
        .post(format!("{base}/messages/send"))
        .form(&[("conversation", "global"), ("body", "live ping")])
        .send()
        .await
        .unwrap();

    // The SSE stream should carry the message within a couple of seconds.
    let got = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let mut acc = String::new();
        while let Some(chunk) = resp.chunk().await.unwrap() {
            acc.push_str(&String::from_utf8_lossy(&chunk));
            if acc.contains("live ping") {
                return true;
            }
        }
        false
    })
    .await
    .unwrap_or(false);
    assert!(got, "the live message arrived over SSE");

    // And it persisted.
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM chat_messages WHERE channel = 'global'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 1, "the chat message persisted");
}

/// 024 AC5 (privacy): a third party cannot wiretap a DM live stream — the canonical pair key means only
/// the two parties' streams match. The actual recipient does receive it live.
#[sqlx::test(migrations = "../../migrations")]
async fn dm_stream_is_private_to_the_pair(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let (eve, _e) = register_client(&base, &pool, &unique("eve")).await; // the wiretapper
    let (xavier, _x) = register_client(&base, &pool, &unique("xavier")).await; // the recipient
    let (zoe, _z) = register_client(&base, &pool, &unique("zoe")).await; // the sender
    let xid: uuid::Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username LIKE 'xavier%'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let zid: uuid::Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username LIKE 'zoe%'")
        .fetch_one(&pool)
        .await
        .unwrap();

    // Eve tries to wiretap Xavier by streaming her "conversation with Xavier"; Xavier streams his with Zoe.
    let mut eve_stream = eve
        .get(format!("{base}/messages/stream/dm:{xid}"))
        .send()
        .await
        .unwrap();
    let mut xav_stream = xavier
        .get(format!("{base}/messages/stream/dm:{zid}"))
        .send()
        .await
        .unwrap();
    assert_eq!(eve_stream.status().as_u16(), 200);
    assert_eq!(xav_stream.status().as_u16(), 200);

    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    // Zoe DMs Xavier.
    zoe.post(format!("{base}/messages/send"))
        .form(&[
            ("conversation", format!("dm:{xid}").as_str()),
            ("body", "private secret"),
        ])
        .send()
        .await
        .unwrap();

    let received = async |resp: &mut reqwest::Response, secs: u64| -> bool {
        tokio::time::timeout(std::time::Duration::from_secs(secs), async {
            let mut acc = String::new();
            while let Some(chunk) = resp.chunk().await.unwrap() {
                acc.push_str(&String::from_utf8_lossy(&chunk));
                if acc.contains("private secret") {
                    return true;
                }
            }
            false
        })
        .await
        .unwrap_or(false)
    };

    // The recipient receives it live...
    assert!(
        received(&mut xav_stream, 5).await,
        "Xavier (a party) receives the DM"
    );
    // ...the wiretapper does not.
    assert!(
        !received(&mut eve_stream, 1).await,
        "Eve (not a party) must NOT receive the DM"
    );
}

/// 025 AC2: the owner edits their bio and it appears on their public stats page; a freshly-active
/// player reads as "online".
#[sqlx::test(migrations = "../../migrations")]
async fn profile_bio_edit_shows_on_public_page(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let owner = client();
    let name = unique("bio");
    owner
        .post(format!("{base}/register"))
        .form(&[
            ("username", name.as_str()),
            ("email", format!("{name}@example.com").as_str()),
            ("password", "secret12"),
            ("tribe", "gauls"),
        ])
        .send()
        .await
        .unwrap();

    // Save a bio (owner-scoped, redirects back to /profile).
    let res = owner
        .post(format!("{base}/profile/bio"))
        .form(&[("bio", "Hail from the north!")])
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 303);

    // The owner's own profile shows it.
    let mine = owner.get(format!("{base}/profile")).send().await.unwrap();
    assert!(mine.text().await.unwrap().contains("Hail from the north!"));

    // A visitor sees it on the public stats page, and the active owner reads as online.
    let pid: uuid::Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(&name)
        .fetch_one(&pool)
        .await
        .unwrap();
    let visitor = client();
    let stats = visitor
        .get(format!("{base}/stats/player/{}", pid.as_u128()))
        .send()
        .await
        .unwrap();
    let body = stats.text().await.unwrap();
    assert!(body.contains("Hail from the north!"), "bio is public");
    assert!(
        body.contains("online"),
        "a just-active player reads as online"
    );
}

/// 025 AC2: editing is owner-scoped — one player's edit never touches another's bio.
#[sqlx::test(migrations = "../../migrations")]
async fn profile_bio_edit_is_owner_scoped(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let alice = client();
    let bob = client();
    for (c, n) in [(&alice, "alice_bio"), (&bob, "bob_bio")] {
        let name = unique(n);
        c.post(format!("{base}/register"))
            .form(&[
                ("username", name.as_str()),
                ("email", format!("{name}@example.com").as_str()),
                ("password", "secret12"),
                ("tribe", "gauls"),
            ])
            .send()
            .await
            .unwrap();
    }
    // Alice sets a bio; Bob's stays empty (each POST keys off the session player only).
    alice
        .post(format!("{base}/profile/bio"))
        .form(&[("bio", "Alice was here")])
        .send()
        .await
        .unwrap();
    let bobs = bob.get(format!("{base}/profile")).send().await.unwrap();
    assert!(
        !bobs.text().await.unwrap().contains("Alice was here"),
        "Bob's bio is untouched by Alice's edit"
    );
}

/// 025 AC6 (Visitor role): an unauthenticated bio edit is refused — redirected to login, with no
/// effect on anyone's stored bio.
#[sqlx::test(migrations = "../../migrations")]
async fn profile_bio_edit_requires_login(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let anon = client();
    let res = anon
        .post(format!("{base}/profile/bio"))
        .form(&[("bio", "anonymous graffiti")])
        .send()
        .await
        .unwrap();
    // The auth extractor redirects to /login; the bio is never written.
    assert_eq!(res.status().as_u16(), 303);
    assert_eq!(
        res.headers().get(LOCATION).unwrap().to_str().unwrap(),
        "/login"
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM users WHERE bio = $1")
        .bind("anonymous graffiti")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "no bio was persisted by the anonymous POST");
}

/// 025: presence reads as "last seen" once the configured window has elapsed; the presence-touch
/// middleware refreshes `last_activity` on real navigation but NOT on the background unread poll.
#[sqlx::test(migrations = "../../migrations")]
async fn presence_last_seen_and_touch_excludes_pollers(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let player = client();
    let name = unique("seen");
    player
        .post(format!("{base}/register"))
        .form(&[
            ("username", name.as_str()),
            ("email", format!("{name}@example.com").as_str()),
            ("password", "secret12"),
            ("tribe", "gauls"),
        ])
        .send()
        .await
        .unwrap();

    // Push last_activity well past the 600s online window.
    let stale = "UPDATE users SET last_activity = now() - interval '2 hours' WHERE username = $1";
    sqlx::query(stale).bind(&name).execute(&pool).await.unwrap();

    // A visitor (whose own touch does not affect the subject) sees "last seen".
    let pid: uuid::Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(&name)
        .fetch_one(&pool)
        .await
        .unwrap();
    let visitor = client();
    let stats = visitor
        .get(format!("{base}/stats/player/{}", pid.as_u128()))
        .send()
        .await
        .unwrap();
    assert!(
        stats.text().await.unwrap().contains("last seen"),
        "an idle player reads as last seen"
    );

    // Re-stale, then hit the unread poller: it must NOT refresh activity.
    sqlx::query(stale).bind(&name).execute(&pool).await.unwrap();
    let before: i64 = sqlx::query_scalar(
        "SELECT (EXTRACT(EPOCH FROM last_activity)*1000)::bigint FROM users WHERE username = $1",
    )
    .bind(&name)
    .fetch_one(&pool)
    .await
    .unwrap();
    player
        .get(format!("{base}/messages/unread"))
        .send()
        .await
        .unwrap();
    let after_poll: i64 = sqlx::query_scalar(
        "SELECT (EXTRACT(EPOCH FROM last_activity)*1000)::bigint FROM users WHERE username = $1",
    )
    .bind(&name)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        before, after_poll,
        "the unread poll does not touch presence"
    );

    // Real navigation DOES refresh it.
    player.get(format!("{base}/village")).send().await.unwrap();
    let after_nav: i64 = sqlx::query_scalar(
        "SELECT (EXTRACT(EPOCH FROM last_activity)*1000)::bigint FROM users WHERE username = $1",
    )
    .bind(&name)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(after_nav > before, "navigation refreshes presence");
}

/// 025 AC4: the population leaderboard renders a presence indicator for player rows; a freshly-active
/// player reads as online.
#[sqlx::test(migrations = "../../migrations")]
async fn leaderboard_rows_show_presence(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let name = unique("lbpres");
    register_client(&base, &pool, &name).await;
    let visitor = client();
    let body = visitor
        .get(format!("{base}/leaderboard"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body.contains(&name), "the player is listed");
    assert!(
        body.contains("presence--online"),
        "a just-active player shows an online presence indicator"
    );
}

/// 025 AC4: a DM conversation surfaces the other party's presence on the list and the thread header.
#[sqlx::test(migrations = "../../migrations")]
async fn dm_surfaces_other_party_presence(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let alice = unique("apres");
    let bob = unique("bpres");
    let (ca, _aid) = register_client(&base, &pool, &alice).await;
    let (_cb, bid) = register_client(&base, &pool, &bob).await;

    // Alice DMs Bob, then views her conversation list + the thread header.
    ca.post(format!("{base}/messages/send"))
        .form(&[
            ("conversation", format!("dm:{bid}").as_str()),
            ("body", "hi bob"),
        ])
        .send()
        .await
        .unwrap();
    let list = ca
        .get(format!("{base}/messages"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        list.contains("presence--"),
        "the DM thread shows Bob's presence in the list"
    );
    let header = ca
        .get(format!("{base}/messages/c/dm:{bid}"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        header.contains("presence--"),
        "the DM header shows Bob's presence"
    );

    // The global channel is present too (sanity) — channels carry no single presence.
    assert!(
        list.contains("Global"),
        "the global channel is listed (sanity)"
    );
}

/// 026 AC3/AC4/AC5: a DM gives the recipient an unread notification + a feed entry; opening the feed
/// clears the bell. Notifications are private to the recipient.
#[sqlx::test(migrations = "../../migrations")]
async fn notifications_feed_bell_and_privacy(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let alice = unique("anotif");
    let bob = unique("bnotif");
    let carol = unique("cnotif");
    let (ca, _aid) = register_client(&base, &pool, &alice).await;
    let (cb, bid) = register_client(&base, &pool, &bob).await;
    let (cc, _cid) = register_client(&base, &pool, &carol).await;

    // Alice DMs Bob → Bob has one unread notification; Carol (uninvolved) has none.
    ca.post(format!("{base}/messages/send"))
        .form(&[
            ("conversation", format!("dm:{bid}").as_str()),
            ("body", "knock knock"),
        ])
        .send()
        .await
        .unwrap();
    let bob_unread = |c: &reqwest::Client| {
        let c = c.clone();
        let base = base.clone();
        async move {
            c.get(format!("{base}/notifications/unread"))
                .send()
                .await
                .unwrap()
                .text()
                .await
                .unwrap()
        }
    };
    assert_eq!(
        bob_unread(&cb).await.trim(),
        "1",
        "bob has one notification"
    );
    assert_eq!(
        bob_unread(&cc).await.trim(),
        "0",
        "carol gets none of bob's notifications (private)"
    );

    // Bob opens the feed → sees the entry, and the bell clears.
    let feed = cb
        .get(format!("{base}/notifications"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(feed.contains("New message"), "the feed shows the alert");
    assert_eq!(
        bob_unread(&cb).await.trim(),
        "0",
        "viewing the feed cleared the bell"
    );

    // An anonymous request gets "0" — visitor-safe (055): the base-template poller must never receive a
    // redirect to the login HTML. Returning 0 leaks nothing (a visitor has no notifications).
    let anon = client();
    let res = anon
        .get(format!("{base}/notifications/unread"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 200);
    assert_eq!(res.text().await.unwrap().trim(), "0");
}

/// 026 AC6: a new notification reaches the recipient's bell stream live, and only theirs.
#[sqlx::test(migrations = "../../migrations")]
async fn notification_live_delivery_is_private(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let alice = unique("alive");
    let bob = unique("blive");
    let eve = unique("elive");
    let (ca, _aid) = register_client(&base, &pool, &alice).await;
    let (cb, bid) = register_client(&base, &pool, &bob).await;
    let (ce, _eid) = register_client(&base, &pool, &eve).await;

    // Bob and Eve both open their bell streams.
    let mut bob_stream = cb
        .get(format!("{base}/notifications/stream"))
        .send()
        .await
        .unwrap();
    let mut eve_stream = ce
        .get(format!("{base}/notifications/stream"))
        .send()
        .await
        .unwrap();

    // Alice DMs Bob.
    ca.post(format!("{base}/messages/send"))
        .form(&[
            ("conversation", format!("dm:{bid}").as_str()),
            ("body", "live ping"),
        ])
        .send()
        .await
        .unwrap();

    let got_event = async |resp: &mut reqwest::Response, secs: u64| -> bool {
        tokio::time::timeout(std::time::Duration::from_secs(secs), async {
            while let Some(chunk) = resp.chunk().await.unwrap() {
                if String::from_utf8_lossy(&chunk).contains("new_message") {
                    return true;
                }
            }
            false
        })
        .await
        .unwrap_or(false)
    };

    assert!(
        got_event(&mut bob_stream, 5).await,
        "Bob's bell receives the live notification"
    );
    assert!(
        !got_event(&mut eve_stream, 1).await,
        "Eve does not receive Bob's notification"
    );
}

/// Insert an alliance + add a player as a member with a role/rights bitset (bypassing the founding
/// eligibility gate, which is not what these forum tests exercise).
async fn seed_alliance(
    pool: &sqlx::PgPool,
    name: &str,
    tag: &str,
    founder: uuid::Uuid,
) -> uuid::Uuid {
    let aid = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO alliances (id, name, tag, founder_id, created_at) VALUES ($1,$2,$3,$4, now())",
    )
    .bind(aid)
    .bind(name)
    .bind(tag)
    .bind(founder)
    .execute(pool)
    .await
    .unwrap();
    add_alliance_member(pool, aid, founder, "founder", 0).await;
    aid
}

async fn add_alliance_member(
    pool: &sqlx::PgPool,
    alliance: uuid::Uuid,
    player: uuid::Uuid,
    role: &str,
    rights: i32,
) {
    sqlx::query(
        "INSERT INTO alliance_members (player_id, alliance_id, role, rights, joined_at) \
         VALUES ($1,$2,$3,$4, now())",
    )
    .bind(player)
    .bind(alliance)
    .bind(role)
    .bind(rights)
    .execute(pool)
    .await
    .unwrap();
}

/// 027 AC1–AC5/AC7: members read/post the alliance forum; non-members and other alliances are refused;
/// an announcement without the Announce right is rejected.
#[sqlx::test(migrations = "../../migrations")]
async fn alliance_forum_flow_and_access(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let (ca, alice) = register_client(&base, &pool, &unique("af_alice")).await;
    let (cb, bob) = register_client(&base, &pool, &unique("af_bob")).await;
    let (cc, _carol) = register_client(&base, &pool, &unique("af_carol")).await;
    let (cd, dave) = register_client(&base, &pool, &unique("af_dave")).await;

    // Alliance A: alice (founder, all rights), bob (plain member). Alliance B: dave (founder).
    let a = seed_alliance(&pool, "Iron Pact", "IRON", alice).await;
    add_alliance_member(&pool, a, bob, "member", 0).await;
    seed_alliance(&pool, "Sun Order", "SUN", dave).await;

    // Alice starts a thread → redirected to it.
    let res = ca
        .post(format!("{base}/alliance/forum/new"))
        .form(&[("title", "Muster"), ("body", "Be online at 20:00")])
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 303);
    let loc = res
        .headers()
        .get(LOCATION)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let thread_id = loc.rsplit('/').next().unwrap().to_owned();

    // Bob (same alliance) sees it in the list and can reply.
    let list = cb
        .get(format!("{base}/alliance/forum"))
        .send()
        .await
        .unwrap();
    assert!(list.text().await.unwrap().contains("Muster"));
    let reply = cb
        .post(format!("{base}/alliance/forum/{thread_id}/reply"))
        .form(&[("body", "Confirmed")])
        .send()
        .await
        .unwrap();
    assert_eq!(reply.status().as_u16(), 303);
    let thread = cb
        .get(format!("{base}/alliance/forum/{thread_id}"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(thread.contains("Be online at 20:00") && thread.contains("Confirmed"));

    // Carol (no alliance) is refused the forum.
    let carol = cc
        .get(format!("{base}/alliance/forum"))
        .send()
        .await
        .unwrap();
    assert_eq!(carol.status().as_u16(), 403);

    // Dave (other alliance) cannot open alliance A's thread.
    let cross = cd
        .get(format!("{base}/alliance/forum/{thread_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(cross.status().as_u16(), 404);

    // Bob lacks the Announce right ⇒ a forged announcement post is rejected (server-enforced).
    let forged = cb
        .post(format!("{base}/alliance/forum/new"))
        .form(&[("title", "Notice"), ("body", "x"), ("announcement", "1")])
        .send()
        .await
        .unwrap();
    assert_eq!(forged.status().as_u16(), 403);

    // Alice (founder, has Announce) can post an announcement; it is locked to replies.
    let ann = ca
        .post(format!("{base}/alliance/forum/new"))
        .form(&[
            ("title", "Rules"),
            ("body", "Read them"),
            ("announcement", "1"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(ann.status().as_u16(), 303);
    let ann_loc = ann
        .headers()
        .get(LOCATION)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let ann_id = ann_loc.rsplit('/').next().unwrap();
    let blocked = cb
        .post(format!("{base}/alliance/forum/{ann_id}/reply"))
        .form(&[("body", "me too")])
        .send()
        .await
        .unwrap();
    // The action guard / use-case rejects a reply to a locked thread (redirect back, no post added).
    let ann_page = cb
        .get(format!("{base}/alliance/forum/{ann_id}"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        !ann_page.contains("me too"),
        "a locked thread takes no replies"
    );
    let _ = blocked;
}

/// 028 AC1–AC4/AC6: who-is search finds players + alliances with links, offers a coordinate jump,
/// shows a prompt for an empty query and "no results" otherwise, and is reachable without login.
#[sqlx::test(migrations = "../../migrations")]
async fn search_finds_players_alliances_and_coordinates(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let (_ca, alice) = register_client(&base, &pool, &unique("sx")).await;
    // Rename to a deterministic prefix.
    sqlx::query("UPDATE users SET username = 'Aragorn' WHERE id = $1")
        .bind(alice)
        .execute(&pool)
        .await
        .unwrap();
    let a = seed_alliance(&pool, "Iron Pact", "IRON", alice).await;
    let _ = a;

    let anon = client(); // public: no login

    // Player prefix.
    let body = anon
        .get(format!("{base}/search?q=arag"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body.contains("Aragorn"));
    assert!(body.contains(&format!("/stats/player/{}", alice.as_u128())));

    // Alliance by tag.
    let body = anon
        .get(format!("{base}/search?q=iro"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body.contains("Iron Pact"));
    assert!(body.contains(&format!("/stats/alliance/{}", a.as_u128())));

    // Coordinate jump.
    let body = anon
        .get(format!("{base}/search?q=12%7C-7"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    // `&` is HTML-escaped to `&amp;` in the rendered attribute (still a valid link).
    assert!(
        body.contains("/map?x=12&amp;y=-7"),
        "offers a map jump for a coordinate query"
    );

    // Empty query → prompt (not "no results").
    let body = anon
        .get(format!("{base}/search"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        body.contains("Type something"),
        "empty query shows the prompt"
    );

    // No match → clear empty state.
    let res = anon
        .get(format!("{base}/search?q=zzzqqq"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 200);
    let body = res.text().await.unwrap();
    assert!(body.contains("No players found") && body.contains("No alliances found"));
}

/// 029 AC1–AC5: a player can mute a notification kind on the settings page; muting suppresses its
/// generation (the bell stays 0); re-enabling restores it; one player's mute doesn't affect another.
#[sqlx::test(migrations = "../../migrations")]
async fn settings_notification_preferences(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let (ca, _aid) = register_client(&base, &pool, &unique("st_a")).await;
    let (cb, bid) = register_client(&base, &pool, &unique("st_b")).await;

    // Settings page lists the kinds, all enabled by default.
    let page = cb
        .get(format!("{base}/settings"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(page.contains("New message") && page.contains("checked"));

    let unread = |c: &reqwest::Client| {
        let c = c.clone();
        let base = base.clone();
        async move {
            c.get(format!("{base}/notifications/unread"))
                .send()
                .await
                .unwrap()
                .text()
                .await
                .unwrap()
        }
    };

    // Bob disables new-message notifications: submit the form with incoming_attack + battle_report only.
    cb.post(format!("{base}/settings/notifications"))
        .form(&[("incoming_attack", "1"), ("battle_report", "1")])
        .send()
        .await
        .unwrap();

    // Alice DMs Bob → Bob gets NO notification (muted).
    ca.post(format!("{base}/messages/send"))
        .form(&[
            ("conversation", format!("dm:{bid}").as_str()),
            ("body", "are you there"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(unread(&cb).await.trim(), "0", "muted kind records nothing");
    // ...but the DM itself is never gated — Bob still receives the message in his thread.
    let aid: uuid::Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username LIKE 'st_a%'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let thread = cb
        .get(format!("{base}/messages/c/dm:{aid}"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        thread.contains("are you there"),
        "the DM is delivered even though its notification was muted"
    );

    // The settings page now shows new-message unchecked.
    let page = cb
        .get(format!("{base}/settings"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    // The new_message checkbox must not be checked (the others remain checked).
    assert!(
        !page.contains("name=\"new_message\" value=\"1\" checked"),
        "new_message is now unchecked"
    );
    assert!(page.contains("name=\"incoming_attack\" value=\"1\" checked"));

    // Re-enable everything → a new DM now notifies.
    cb.post(format!("{base}/settings/notifications"))
        .form(&[
            ("incoming_attack", "1"),
            ("battle_report", "1"),
            ("new_message", "1"),
        ])
        .send()
        .await
        .unwrap();
    ca.post(format!("{base}/messages/send"))
        .form(&[
            ("conversation", format!("dm:{bid}").as_str()),
            ("body", "hello again"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(
        unread(&cb).await.trim(),
        "1",
        "re-enabling restores notifications"
    );

    // A Visitor is redirected from settings.
    let anon = client();
    let res = anon.get(format!("{base}/settings")).send().await.unwrap();
    assert_eq!(res.status().as_u16(), 303);
}

/// 030 AC2–AC5: an authorised sitter operates the owner's account (proved via the owner's notification
/// count), restrictions are enforced, actions are audited, and stop reverts.
#[sqlx::test(migrations = "../../migrations")]
async fn account_sitting_takeover_restrictions_and_audit(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let owner_name = unique("si_o");
    let sitter_name = unique("si_s");
    let (jo, owner) = register_client(&base, &pool, &owner_name).await;
    let (js, sitter) = register_client(&base, &pool, &sitter_name).await;
    let (jc, _carol) = register_client(&base, &pool, &unique("si_c")).await;
    let (jd, _dave) = register_client(&base, &pool, &unique("si_d")).await;

    // Carol DMs the owner → the owner has one notification; the sitter has none.
    jc.post(format!("{base}/messages/send"))
        .form(&[
            ("conversation", format!("dm:{owner}").as_str()),
            ("body", "hi owner"),
        ])
        .send()
        .await
        .unwrap();

    let unread = |c: &reqwest::Client| {
        let c = c.clone();
        let base = base.clone();
        async move {
            c.get(format!("{base}/notifications/unread"))
                .send()
                .await
                .unwrap()
                .text()
                .await
                .unwrap()
        }
    };
    let status = |c: &reqwest::Client| {
        let c = c.clone();
        let base = base.clone();
        async move {
            c.get(format!("{base}/sitting/status"))
                .send()
                .await
                .unwrap()
        }
    };

    // Owner authorises the sitter by username.
    jo.post(format!("{base}/sitting/grant"))
        .form(&[("username", sitter_name.as_str())])
        .send()
        .await
        .unwrap();

    // Not sitting yet: the sitter sees their own (empty) notifications.
    assert_eq!(unread(&js).await.trim(), "0");

    // A non-authorised player cannot start sitting the owner.
    let r = jd
        .post(format!("{base}/sitting/start"))
        .form(&[("owner", owner.as_u128().to_string().as_str())])
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 403, "non-authorised cannot start");

    // The sitter starts → now acts as the owner: their notification count is the OWNER's (1).
    let r = js
        .post(format!("{base}/sitting/start"))
        .form(&[("owner", owner.as_u128().to_string().as_str())])
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 303);
    assert_eq!(
        unread(&js).await.trim(),
        "1",
        "effective player is the owner"
    );
    // The status endpoint reports the owner's name (drives the banner).
    assert_eq!(status(&js).await.text().await.unwrap().trim(), owner_name);

    // Restrictions: a sitter cannot change the owner's settings/profile or manage sitters.
    for (path, form) in [
        ("/settings/notifications", vec![("incoming_attack", "1")]),
        ("/profile/bio", vec![("bio", "hacked")]),
        ("/sitting/grant", vec![("username", owner_name.as_str())]),
    ] {
        let r = js
            .post(format!("{base}{path}"))
            .form(&form)
            .send()
            .await
            .unwrap();
        assert_eq!(r.status().as_u16(), 403, "sitter blocked from {path}");
    }

    // A normal action is allowed (acts as the owner) and audited.
    js.post(format!("{base}/village/build"))
        .form(&[("table", "field"), ("slot", "1"), ("kind", "")])
        .send()
        .await
        .unwrap();
    let log = jo
        .get(format!("{base}/sitting"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        log.contains("POST /village/build"),
        "the owner's audit log shows the sitter's action"
    );
    assert!(log.contains(&sitter_name), "the audit names the sitter");

    // Stop sitting → back to the sitter's own account.
    js.post(format!("{base}/sitting/stop"))
        .send()
        .await
        .unwrap();
    assert_eq!(
        unread(&js).await.trim(),
        "0",
        "stopped — back to own account"
    );

    // AC3: revoking mid-sit reverts the sitter on the next request.
    js.post(format!("{base}/sitting/start"))
        .form(&[("owner", owner.as_u128().to_string().as_str())])
        .send()
        .await
        .unwrap();
    assert_eq!(unread(&js).await.trim(), "1");
    jo.post(format!("{base}/sitting/revoke"))
        .form(&[("sitter", sitter.as_u128().to_string().as_str())])
        .send()
        .await
        .unwrap();
    assert_eq!(unread(&js).await.trim(), "0", "revoke ended the sit");
}

/// 030 AC6: a banned owner cannot be operated; a banned sitter cannot sit.
#[sqlx::test(migrations = "../../migrations")]
async fn account_sitting_respects_sanctions(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let sitter_name = unique("sb_s");
    let (jo, owner) = register_client(&base, &pool, &unique("sb_o")).await;
    let (js, sitter) = register_client(&base, &pool, &sitter_name).await;
    jo.post(format!("{base}/sitting/grant"))
        .form(&[("username", sitter_name.as_str())])
        .send()
        .await
        .unwrap();

    let ban = |id: uuid::Uuid, banned: bool| {
        let pool = pool.clone();
        async move {
            let sql = if banned {
                "UPDATE users SET banned_at = now() WHERE id = $1"
            } else {
                "UPDATE users SET banned_at = NULL WHERE id = $1"
            };
            sqlx::query(sql).bind(id).execute(&pool).await.unwrap();
        }
    };
    let start = |c: &reqwest::Client| {
        let c = c.clone();
        let base = base.clone();
        let o = owner.as_u128().to_string();
        async move {
            c.post(format!("{base}/sitting/start"))
                .form(&[("owner", o.as_str())])
                .send()
                .await
                .unwrap()
                .status()
                .as_u16()
        }
    };

    // Banned owner ⇒ the sit is refused.
    ban(owner, true).await;
    assert_eq!(start(&js).await, 403, "cannot operate a banned owner");
    ban(owner, false).await;

    // Banned sitter ⇒ their mutating actions (incl. starting a sit) are refused.
    ban(sitter, true).await;
    assert_eq!(start(&js).await, 403, "a banned sitter cannot sit");
}

/// 031: the village build/field tables show each upgrade's effect (production / storage / population),
/// not just its cost.
#[sqlx::test(migrations = "../../migrations")]
async fn village_shows_next_level_effects(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let (c, _id) = register_client(&base, &pool, &unique("eff")).await;
    let body = c
        .get(format!("{base}/village"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    // A resource field shows its next-level production.
    assert!(
        body.contains("Production "),
        "fields show a production effect"
    );
    assert!(body.contains('→'), "the effect reads current → next");
    // The Warehouse shows its next-level storage capacity.
    assert!(
        body.contains("Storage "),
        "the warehouse shows a storage effect"
    );
    // Buildings whose rules live outside the economy show their effects too (AC1).
    assert!(
        body.contains("Merchants "),
        "the Marketplace shows merchant count"
    );
    assert!(
        body.contains("Build speed ×"),
        "the Main Building shows the build-speed factor"
    );
    assert!(
        body.contains("Culture +"),
        "the Town Hall shows culture gain"
    );
    assert!(
        body.contains("Training speed ×"),
        "training buildings show the training-speed factor"
    );
    assert!(
        body.contains("Wall defence "),
        "the Wall shows its defence bonus"
    );
    assert!(body.contains("Hides "), "the Cranny shows resources hidden");
    assert!(
        body.contains("Expansion slots "),
        "the Residence shows expansion slots"
    );
    // The resource bars are wired (fill + ETA computed client-side from these attributes).
    assert!(
        body.contains("resbar") && body.contains("data-amt="),
        "resource bars are present"
    );
}

/// 032 AC2: the Wall defence effect is tribe-correct — a Teuton's Wall shows a different bonus than a
/// Gaul's (Teuton walls are weaker per level).
#[sqlx::test(migrations = "../../migrations")]
async fn wall_effect_is_tribe_correct(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let wall_line = |body: &str| -> String {
        body.lines()
            .find(|l| l.contains("Wall defence "))
            .unwrap_or_default()
            .to_owned()
    };
    // A Gaul and a Teuton village.
    let g = client();
    g.post(format!("{base}/register"))
        .form(&[
            ("username", unique("wg").as_str()),
            ("email", "wg@e.com"),
            ("password", "secret12"),
            ("tribe", "gauls"),
        ])
        .send()
        .await
        .unwrap();
    let t = client();
    t.post(format!("{base}/register"))
        .form(&[
            ("username", unique("wt").as_str()),
            ("email", "wt@e.com"),
            ("password", "secret12"),
            ("tribe", "teutons"),
        ])
        .send()
        .await
        .unwrap();
    let gb = g
        .get(format!("{base}/village"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let tb = t
        .get(format!("{base}/village"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let (gl, tl) = (wall_line(&gb), wall_line(&tb));
    assert!(gl.contains("Wall defence ") && tl.contains("Wall defence "));
    assert_ne!(gl, tl, "the Wall effect differs by tribe");
}

/// 031 AC1: a non-capital field at its cap shows **no** effect (the cost table runs to the higher capital
/// cap, so the effect must be blanked explicitly — not left stale).
#[sqlx::test(migrations = "../../migrations")]
async fn capped_noncapital_field_shows_no_effect(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let (c, _id) = register_client(&base, &pool, &unique("cap")).await;
    // Make the village non-capital (field cap 10) and push every field above it but below the capital cap.
    let vid: uuid::Uuid = sqlx::query_scalar(
        "SELECT v.id FROM villages v JOIN users u ON u.id = v.owner_id WHERE u.username LIKE 'cap%'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE villages SET is_capital = false WHERE id = $1")
        .bind(vid)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE village_fields SET level = 15 WHERE village_id = $1")
        .bind(vid)
        .execute(&pool)
        .await
        .unwrap();
    let body = c
        .get(format!("{base}/village"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body.contains("max"), "capped fields show the max state");
    assert!(
        !body.contains("Production "),
        "a capped non-capital field shows no stale production effect"
    );
}

/// 033: the map shows each tile's distance from home and a send shortcut to another player's village.
#[sqlx::test(migrations = "../../migrations")]
async fn map_shows_distance_and_send_shortcut(pool: sqlx::PgPool) {
    let base = spawn(pool.clone()).await;
    let (ca, a) = register_client(&base, &pool, &unique("ma")).await;
    let (_cb, b) = register_client(&base, &pool, &unique("mb")).await;
    let (ax, ay): (i32, i32) = sqlx::query_as("SELECT x, y FROM villages WHERE owner_id = $1")
        .bind(a)
        .fetch_one(&pool)
        .await
        .unwrap();
    // Move the other player's village right next to ours so it's in view.
    let (bx, by) = (ax + 1, ay);
    sqlx::query("UPDATE villages SET x = $2, y = $3 WHERE owner_id = $1")
        .bind(b)
        .bind(bx)
        .bind(by)
        .execute(&pool)
        .await
        .unwrap();
    let body = ca
        .get(format!("{base}/map?x={ax}&y={ay}"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        body.contains("fields away"),
        "tiles show distance from home"
    );
    // The neighbour's village links to the Rally Point pre-filled with its tile (& is HTML-escaped).
    assert!(
        body.contains(&format!("/village/rally?x={bx}&amp;y={by}")),
        "a neighbouring village has a send shortcut"
    );
    // Your own village is never a send target (the home tile is the only own village in view).
    assert!(
        !body.contains(&format!("/village/rally?x={ax}&amp;y={ay}")),
        "own village is not a send target"
    );
}
