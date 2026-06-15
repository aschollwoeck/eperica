//! Eperica web — the HTTP/UI entrypoint. Wires the application + infrastructure layers, starts the
//! background scheduler, and serves the register / login / village flow.
#![forbid(unsafe_code)]

use axum_extra::extract::cookie::Key;
use eperica_domain::WorldMap;
use eperica_infrastructure::{
    AppConfig, Argon2Hasher, ChatHub, NotificationHub, PgAccountRepository, PgEventStore,
    Scheduler, achievement_catalogue, alliance_rules, artifact_catalogue, build_rules,
    combat_rules, create_pool, culture_rules, economy_rules, ensure_world_with_release,
    fair_play_rules, lifecycle_rules, loyalty_rules, map_rules, medal_rules, merchant_rules,
    oasis_rules, quest_chain, ranking_rules, run_chat_listener, run_migrations,
    run_notification_listener, scout_rules, starting_village, unit_rules, wonder_rules,
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
    let world = ensure_world_with_release(
        &pool,
        &config.world,
        config.artifact_release_offset_secs,
        config.wonder_release_offset_secs,
    )
    .await?;
    let rules = Arc::new(economy_rules()?);
    let units = Arc::new(unit_rules()?);
    let merchants = Arc::new(merchant_rules()?);
    let combat = Arc::new(combat_rules()?);
    let scout = Arc::new(scout_rules()?);
    let oases = Arc::new(oasis_rules()?);
    let culture = Arc::new(culture_rules()?);
    let loyalty = Arc::new(loyalty_rules()?);
    let ranking = Arc::new(ranking_rules()?);
    let medals = Arc::new(medal_rules()?);
    let achievements = Arc::new(achievement_catalogue()?);
    let quests = Arc::new(quest_chain()?);
    let lifecycle = Arc::new(lifecycle_rules()?);
    let artifacts = Arc::new(artifact_catalogue()?);
    let wonder = Arc::new(wonder_rules()?);
    let fair_play = Arc::new(fair_play_rules()?);
    let template = Arc::new(starting_village()?);
    let map = Arc::new(WorldMap::new(
        world.seed as u64,
        config.world.radius,
        map_rules()?,
    ));
    let accounts = PgAccountRepository::new(
        pool.clone(),
        world.id,
        world.seed,
        config.world.radius,
        rules.starting_amounts,
        lifecycle.beginner_protection_secs,
        config.world.speed,
    );

    // 022: designate the operator's moderators (idempotent, P7) from the MODERATORS env list.
    bootstrap_moderators(&accounts).await;

    // 024: live chat fan-out — one Postgres listener per process feeds the in-memory hub.
    let chat_hub = ChatHub::new();
    tokio::spawn(run_chat_listener(pool.clone(), chat_hub.clone()));

    // 026: live notification fan-out — a second listener feeds the per-player bell streams.
    let notification_hub = NotificationHub::new();
    tokio::spawn(run_notification_listener(
        pool.clone(),
        notification_hub.clone(),
    ));

    // Background scheduler (P1) — processes due events, builds, unit orders, training, starvation.
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let scheduler = Scheduler::new(
        PgEventStore::new(pool.clone()),
        accounts.clone(),
        Arc::clone(&rules),
        Arc::clone(&units),
        Arc::clone(&merchants),
        Arc::clone(&combat),
        Arc::clone(&scout),
        Arc::clone(&oases),
        Arc::clone(&culture),
        Arc::clone(&loyalty),
        Arc::clone(&ranking),
        Arc::clone(&medals),
        Arc::clone(&lifecycle),
        Arc::clone(&artifacts),
        Arc::clone(&template),
        Arc::clone(&map),
        config.world.speed,
        world.seed as u64,
        world.created_at,
        world.artifact_release_at,
        Arc::clone(&wonder),
        world.wonder_release_at,
    );
    let scheduler_handle = tokio::spawn(scheduler.run(shutdown_rx));

    let state = AppState {
        accounts: Arc::new(accounts),
        hasher: Arc::new(Argon2Hasher),
        template,
        rules,
        build_rules: Arc::new(build_rules()?),
        unit_rules: units,
        combat_rules: Arc::clone(&combat),
        culture_rules: culture,
        loyalty_rules: loyalty,
        alliance_rules: Arc::new(alliance_rules()?),
        ranking_rules: ranking,
        achievement_catalogue: achievements,
        quest_chain: quests,
        lifecycle_rules: lifecycle,
        merchant_rules: merchants,
        wonder_rules: Arc::clone(&wonder),
        fair_play_rules: Arc::clone(&fair_play),
        trust_proxy: env_flag("TRUST_PROXY"),
        chat_hub,
        notification_hub,
        map,
        world: config.world,
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

    let _ = scheduler_handle.await;
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
