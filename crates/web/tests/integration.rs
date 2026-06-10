//! End-to-end HTTP tests driving the real app over Postgres (T18/T19).
//!
//! Each test spins the app on an ephemeral port and uses a cookie-aware client. They skip when
//! `DATABASE_URL` is not set, so `cargo test` stays green without a database.

use axum_extra::extract::cookie::Key;
use eperica_domain::{GameSpeed, WorldConfig};
use eperica_infrastructure::{
    Argon2Hasher, PgAccountRepository, build_rules, create_pool, economy_rules, ensure_world,
    run_migrations, starting_village,
};
use eperica_web::router;
use eperica_web::state::AppState;
use reqwest::header::LOCATION;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Spawn an app instance over the live DB; returns its base URL, or `None` to skip without a DB.
async fn spawn() -> Option<String> {
    let _ = dotenvy::dotenv();
    let url = std::env::var("DATABASE_URL").ok()?;
    let pool = create_pool(&url).await.expect("connect");
    run_migrations(&pool).await.expect("migrate");

    let config = WorldConfig::new(GameSpeed::new(1.0).unwrap(), 50);
    let world_id = ensure_world(&pool, &config).await.expect("ensure world");
    let rules = economy_rules().expect("economy rules");
    let state = AppState {
        accounts: Arc::new(PgAccountRepository::new(
            pool.clone(),
            world_id,
            config.radius,
            rules.starting_amounts,
        )),
        hasher: Arc::new(Argon2Hasher),
        template: Arc::new(starting_village().unwrap()),
        rules: Arc::new(rules),
        build_rules: Arc::new(build_rules().expect("build rules")),
        world: config,
        require_email_confirmation: false,
        cookie_key: Key::generate(),
    };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router(state)).await.unwrap();
    });
    Some(format!("http://{addr}"))
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .cookie_store(true)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
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

#[tokio::test]
async fn register_creates_village_and_view_is_fast() {
    let Some(base) = spawn().await else {
        return;
    };
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
    let started = std::time::Instant::now();
    let view = c.get(format!("{base}/village")).send().await.unwrap();
    let elapsed = started.elapsed();
    assert_eq!(view.status().as_u16(), 200);

    let body = view.text().await.unwrap();
    assert!(body.contains(&user)); // AC3: owned by this player
    assert!(body.contains("Wood")); // resources shown
    assert!(body.contains("/h")); // production rate shown (AC7)
    // P11: the read path is fast. The production budget is 50 ms server-side; this end-to-end check
    // runs under full parallel-test DB contention, so it uses a looser bound to stay reliable while
    // still catching gross regressions (it is comfortably < 50 ms in isolation).
    assert!(
        elapsed.as_millis() < 250,
        "GET /village took {elapsed:?}, far over budget"
    );
}

#[tokio::test]
async fn login_succeeds_and_rejects_bad_password() {
    let Some(base) = spawn().await else {
        return;
    };
    let user = unique("log");
    let email = format!("{user}@example.com");

    client()
        .post(format!("{base}/register"))
        .form(&[
            ("username", user.as_str()),
            ("email", email.as_str()),
            ("password", "secret12"),
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

#[tokio::test]
async fn village_requires_login() {
    let Some(base) = spawn().await else {
        return;
    };
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

#[tokio::test]
async fn duplicate_username_is_rejected() {
    let Some(base) = spawn().await else {
        return;
    };
    let user = unique("dup");
    let email = format!("{user}@example.com");
    let email2 = format!("{user}-2@example.com");

    let first = client()
        .post(format!("{base}/register"))
        .form(&[
            ("username", user.as_str()),
            ("email", email.as_str()),
            ("password", "secret12"),
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
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(second.status().as_u16(), 200);
    assert!(second.text().await.unwrap().contains("already taken"));
}

#[tokio::test]
async fn register_rejects_invalid_input() {
    let Some(base) = spawn().await else {
        return;
    };
    // AC1: server-side rejection (a too-short password) — no redirect, error shown, no account.
    let res = client()
        .post(format!("{base}/register"))
        .form(&[
            ("username", "validname"),
            ("email", "valid@example.com"),
            ("password", "short"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 200);
    assert!(res.text().await.unwrap().contains("at least 8"));
}

#[tokio::test]
async fn logout_ends_session() {
    let Some(base) = spawn().await else {
        return;
    };
    let user = unique("out");
    let email = format!("{user}@example.com");
    let c = client();

    // Register logs the user in.
    c.post(format!("{base}/register"))
        .form(&[
            ("username", user.as_str()),
            ("email", email.as_str()),
            ("password", "secret12"),
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

#[tokio::test]
async fn village_shows_economy() {
    let Some(base) = spawn().await else {
        return;
    };
    let user = unique("econ");
    let email = format!("{user}@example.com");
    let c = client();
    c.post(format!("{base}/register"))
        .form(&[
            ("username", user.as_str()),
            ("email", email.as_str()),
            ("password", "secret12"),
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

#[tokio::test]
async fn build_order_flow() {
    let Some(base) = spawn().await else {
        return;
    };
    let user = unique("bld");
    let email = format!("{user}@example.com");
    let c = client();
    c.post(format!("{base}/register"))
        .form(&[
            ("username", user.as_str()),
            ("email", email.as_str()),
            ("password", "secret12"),
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

#[tokio::test]
async fn build_requires_login() {
    let Some(base) = spawn().await else {
        return;
    };
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

#[tokio::test]
async fn account_persists_across_restart() {
    let Some(base1) = spawn().await else {
        return;
    };
    let user = unique("persist");
    let email = format!("{user}@example.com");

    client()
        .post(format!("{base1}/register"))
        .form(&[
            ("username", user.as_str()),
            ("email", email.as_str()),
            ("password", "secret12"),
        ])
        .send()
        .await
        .unwrap();

    // AC8: a fresh app instance over the same DB ("restart") sees the same account & village.
    let Some(base2) = spawn().await else {
        return;
    };
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
