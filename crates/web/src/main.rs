//! Eperica web — the HTTP/UI entrypoint. Wires the application + infrastructure layers, starts the
//! background scheduler, and serves the register / login / village flow.
#![forbid(unsafe_code)]

use axum_extra::extract::cookie::Key;
use eperica_domain::WorldMap;
use eperica_infrastructure::{
    AppConfig, Argon2Hasher, ChatHub, NotificationHub, PgAccountRepository, all_worlds,
    create_pool, ensure_world_with_release, fair_play_rules, load_world_rules, run_chat_listener,
    run_migrations, run_notification_listener,
};
use eperica_web::registry::WorldRegistry;
use eperica_web::router;
use eperica_web::state::AppState;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing();

    let config = AppConfig::from_env()?;
    let pool = create_pool(&config.database_url).await?;
    run_migrations(&pool).await?;
    let world = ensure_world_with_release(
        &pool,
        &config.world,
        config.artifact_release_offset_secs,
        config.wonder_release_offset_secs,
    )
    .await?;
    // 049: the home world's preset (classic until an admin picks another, 052). 050 has the registry
    // resolve each world's preset; today AppState + the registry share this one global bundle.
    let world_rules = Arc::new(load_world_rules(&world.rule_preset)?);
    let fair_play = Arc::new(fair_play_rules()?);
    let map = Arc::new(WorldMap::new(
        world.seed as u64,
        config.world.radius,
        world_rules.map_rules.clone(),
    ));
    let accounts = PgAccountRepository::new(
        pool.clone(),
        world.id,
        world.seed,
        config.world.radius,
        world_rules.economy.starting_amounts,
        world_rules.lifecycle.beginner_protection_secs,
        config.world.speed,
    );

    // 022: designate the operator's moderators (idempotent, P7) from the MODERATORS env list.
    bootstrap_moderators(&accounts).await;

    // 036: designate the operator's administrators (idempotent) from the ADMINS env list — ensures at
    // least one admin via config even if all in-app admins are demoted.
    bootstrap_admins(&accounts).await;

    // 024: live chat fan-out — one Postgres listener per process feeds the in-memory hub.
    let chat_hub = ChatHub::new();
    tokio::spawn(run_chat_listener(pool.clone(), chat_hub.clone()));

    // 026: live notification fan-out — a second listener feeds the per-player bell streams.
    let notification_hub = NotificationHub::new();
    tokio::spawn(run_notification_listener(
        pool.clone(),
        notification_hub.clone(),
    ));

    // Background scheduler (P1, 040/041) — the registry holds the shared rules and starts a scheduler
    // per world. It is the one spawn path: `main.rs` starts every existing world here, and the admin
    // create-world handler starts a freshly-created world live through the same `start_world`.
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let registry = Arc::new(WorldRegistry::new(
        pool.clone(),
        shutdown_rx,
        world_rules.lifecycle.beginner_protection_secs,
        Arc::clone(&world_rules),
    ));
    match all_worlds(&pool).await {
        Ok(worlds) => {
            for w in worlds {
                if let Err(e) = registry.start_world(w.id).await {
                    tracing::error!(world = w.id.0, error = %e, "failed to start world scheduler");
                }
            }
        }
        Err(e) => tracing::error!(error = %e, "failed to load worlds for the registry"),
    }

    let state = AppState {
        accounts: Arc::new(accounts),
        hasher: Arc::new(Argon2Hasher),
        world_rules: Arc::clone(&world_rules),
        fair_play_rules: Arc::clone(&fair_play),
        trust_proxy: env_flag("TRUST_PROXY"),
        chat_hub,
        notification_hub,
        map,
        artifact_release_offset_secs: config.artifact_release_offset_secs,
        wonder_release_offset_secs: config.wonder_release_offset_secs,
        world: config.world,
        world_id: world.id,
        world_registry: Arc::clone(&registry),
        require_email_confirmation: env_flag("REQUIRE_EMAIL_CONFIRMATION"),
        cookie_key: load_cookie_key(),
    };

    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_owned());
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(%addr, "Eperica web listening");

    axum::serve(
        listener,
        router(state).into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal(shutdown_tx))
    .await?;

    // Graceful shutdown: the signal was sent; await every world's scheduler.
    registry.join_all().await;
    Ok(())
}

/// Grant the elevated Moderator role (022 AC1) to the operator-configured `MODERATORS` usernames
/// (comma-separated), idempotently at startup. Unknown names are logged and skipped.
async fn bootstrap_moderators(accounts: &PgAccountRepository) {
    use eperica_infrastructure::application::{AccountRepository, ModerationRepository};
    let Ok(list) = std::env::var("MODERATORS") else {
        return;
    };
    for name in list.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        match accounts.find_user_by_username(name).await {
            Ok(Some(u)) => {
                if let Err(e) = accounts.set_moderator(u.id, true).await {
                    tracing::error!(error = %e, moderator = name, "failed to grant moderator");
                } else {
                    tracing::info!(moderator = name, "granted moderator role");
                }
            }
            Ok(None) => tracing::warn!(moderator = name, "MODERATORS lists an unknown username"),
            Err(e) => tracing::error!(error = %e, moderator = name, "moderator lookup failed"),
        }
    }
}

/// Grant the elevated Administrator role (036 AC1) to the operator-configured `ADMINS` usernames
/// (comma-separated), idempotently at startup. Mirrors [`bootstrap_moderators`]; unknown names are
/// logged and skipped.
async fn bootstrap_admins(accounts: &PgAccountRepository) {
    use eperica_infrastructure::application::{AccountRepository, AdminRepository};
    let Ok(list) = std::env::var("ADMINS") else {
        return;
    };
    for name in list.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        match accounts.find_user_by_username(name).await {
            Ok(Some(u)) => {
                if let Err(e) = accounts.set_admin(u.id, true).await {
                    tracing::error!(error = %e, admin = name, "failed to grant administrator");
                } else {
                    tracing::info!(admin = name, "granted administrator role");
                }
            }
            Ok(None) => tracing::warn!(admin = name, "ADMINS lists an unknown username"),
            Err(e) => tracing::error!(error = %e, admin = name, "admin lookup failed"),
        }
    }
}

/// Initialize structured tracing (P11 observability).
fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

/// Read a boolean-ish env flag (`1`/`true`).
fn env_flag(name: &str) -> bool {
    std::env::var(name)
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Load the cookie encryption key from `SESSION_SECRET` (≥64 bytes), or generate an ephemeral one
/// for development (sessions then do not survive a restart, but accounts/villages do — AC8).
fn load_cookie_key() -> Key {
    match std::env::var("SESSION_SECRET") {
        Ok(secret) if secret.len() >= 64 => Key::from(secret.as_bytes()),
        _ => {
            tracing::warn!(
                "SESSION_SECRET unset or shorter than 64 bytes; using an ephemeral cookie key"
            );
            Key::generate()
        }
    }
}

/// Resolve when Ctrl-C is received, signaling shutdown to the scheduler and server.
async fn shutdown_signal(shutdown_tx: tokio::sync::watch::Sender<bool>) {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutdown signal received");
    let _ = shutdown_tx.send(true);
}
