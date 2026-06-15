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
    Coordinate, GameSpeed, TileKind, Timestamp, WorldConfig, WorldMap, coordinates_within,
};
use eperica_infrastructure::{
    Argon2Hasher, ChatHub, NotificationHub, PgAccountRepository, achievement_catalogue,
    alliance_rules, build_rules, combat_rules, culture_rules, economy_rules, ensure_world,
    fair_play_rules, lifecycle_rules, loyalty_rules, map_rules, merchant_rules, now, oasis_rules,
    quest_chain, ranking_rules, run_chat_listener, run_notification_listener, scout_rules,
    starting_village, unit_rules, wonder_rules,
};
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
    let rules = economy_rules().expect("economy rules");
    let map = Arc::new(WorldMap::new(
        world.seed as u64,
        config.radius,
        map_rules().expect("map rules"),
    ));
    let state = AppState {
        accounts: Arc::new(PgAccountRepository::new(
            pool.clone(),
            world.id,
            world.seed,
            config.radius,
            rules.starting_amounts,
            lifecycle_rules()
                .expect("lifecycle rules")
                .beginner_protection_secs,
            config.speed,
        )),
        hasher: Arc::new(Argon2Hasher),
        template: Arc::new(starting_village().unwrap()),
        rules: Arc::new(rules),
        build_rules: Arc::new(build_rules().expect("build rules")),
        unit_rules: Arc::new(unit_rules().expect("unit rules")),
        culture_rules: Arc::new(culture_rules().expect("culture rules")),
        loyalty_rules: Arc::new(loyalty_rules().expect("loyalty rules")),
        alliance_rules: Arc::new(alliance_rules().expect("alliance rules")),
        ranking_rules: Arc::new(ranking_rules().expect("ranking rules")),
        achievement_catalogue: Arc::new(achievement_catalogue().expect("achievement catalogue")),
        quest_chain: Arc::new(quest_chain().expect("quest chain")),
        lifecycle_rules: Arc::new(lifecycle_rules().expect("lifecycle rules")),
        merchant_rules: Arc::new(merchant_rules().expect("merchant rules")),
        wonder_rules: Arc::new(wonder_rules().expect("wonder rules")),
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
        world: config,
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

    // An anonymous request is redirected to login (no notifications for a Visitor).
    let anon = client();
    let res = anon
        .get(format!("{base}/notifications/unread"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 303);
    assert_eq!(
        res.headers().get(LOCATION).unwrap().to_str().unwrap(),
        "/login"
    );
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
