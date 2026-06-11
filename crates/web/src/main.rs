//! Eperica web — the HTTP/UI entrypoint. Wires the application + infrastructure layers, starts the
//! background scheduler, and serves the register / login / village flow.
#![forbid(unsafe_code)]

use axum_extra::extract::cookie::Key;
use eperica_infrastructure::{
    AppConfig, Argon2Hasher, PgAccountRepository, PgEventStore, Scheduler, build_rules,
    create_pool, economy_rules, ensure_world, run_migrations, starting_village, unit_rules,
};
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
    let world_id = ensure_world(&pool, &config.world).await?;
    let rules = economy_rules()?;
    let accounts = PgAccountRepository::new(
        pool.clone(),
        world_id,
        config.world.radius,
        rules.starting_amounts,
    );

    // Background scheduler (P1) — processes due events and due builds.
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let scheduler = Scheduler::new(PgEventStore::new(pool.clone()), accounts.clone());
    let scheduler_handle = tokio::spawn(scheduler.run(shutdown_rx));

    let state = AppState {
        accounts: Arc::new(accounts),
        hasher: Arc::new(Argon2Hasher),
        template: Arc::new(starting_village()?),
        rules: Arc::new(rules),
        build_rules: Arc::new(build_rules()?),
        unit_rules: Arc::new(unit_rules()?),
        world: config.world,
        require_email_confirmation: env_flag("REQUIRE_EMAIL_CONFIRMATION"),
        cookie_key: load_cookie_key(),
    };

    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_owned());
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(%addr, "Eperica web listening");

    axum::serve(listener, router(state))
        .with_graceful_shutdown(shutdown_signal(shutdown_tx))
        .await?;

    let _ = scheduler_handle.await;
    Ok(())
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
