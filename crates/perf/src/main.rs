//! `eperica-perf` (023): a repeatable performance & scale instrument with three subcommands.
//!
//! - `seed --players N` — bulk-seed N perf players into `$DATABASE_URL` (idempotent).
//! - `measure [--players N] [--heartbeats K] [--iters I] [--explain]` — seed if asked, then time the hot
//!   read paths + the scheduler loop and print a budget table (and, with `--explain`, query plans).
//! - `load --base-url URL --concurrency C --count N` — drive a concurrent action mix against a running
//!   server; report req/s + p50/p90/p99.
//!
//! Re-runnable on demand against any database/server, so the scale pass can be repeated as the game and
//! hardware evolve. It seeds via the same `eperica_infrastructure::perf::seed_world` the CI guard uses, so
//! the in-CI numbers and the on-demand numbers come from one seeder.
#![forbid(unsafe_code)]

use eperica_application::{
    AccountRepository, BoardScope, RankingRepository, player_statistics, process_due,
};
use eperica_domain::{Coordinate, PlayerId};
use eperica_infrastructure::{
    AppConfig, PgAccountRepository, PgEventStore, create_pool, economy_rules,
    ensure_world_with_release, lifecycle_rules, now, perf, run_migrations,
};
use std::time::Instant;

type BoxErr = Box<dyn std::error::Error + Send + Sync>;

#[tokio::main]
async fn main() -> Result<(), BoxErr> {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(String::as_str).unwrap_or("");
    match cmd {
        "seed" => seed(&args).await,
        "measure" => measure(&args).await,
        "load" => load(&args).await,
        _ => {
            eprintln!(
                "usage:\n  eperica-perf seed --players N\n  eperica-perf measure [--players N] \
                 [--heartbeats K] [--iters I] [--explain]\n  eperica-perf load --base-url URL \
                 --concurrency C --count N"
            );
            std::process::exit(2);
        }
    }
}

/// Parse a `--flag value` integer, or `default`.
fn flag_u32(args: &[String], flag: &str, default: u32) -> u32 {
    flag_str(args, flag)
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

fn flag_str(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

/// Connect, migrate, ensure the world, and build the account repository — shared `seed`/`measure` setup.
async fn open_repo() -> Result<
    (
        PgAccountRepository,
        sqlx::PgPool,
        eperica_domain::EconomyRules,
    ),
    BoxErr,
> {
    let config = AppConfig::from_env()?;
    let pool = create_pool(&config.database_url).await?;
    run_migrations(&pool).await?;
    // 047: bootstrap with the operator's configured end-game schedule (not the hardcoded fallback), so
    // the perf world matches the production boot path.
    let world = ensure_world_with_release(
        &pool,
        &config.world,
        config.artifact_release_offset_secs,
        config.wonder_release_offset_secs,
    )
    .await?;
    let econ = economy_rules()?;
    let lifecycle = lifecycle_rules()?;
    let repo = PgAccountRepository::new(
        pool.clone(),
        world.id,
        world.seed,
        config.world.radius,
        econ.starting_amounts,
        lifecycle.beginner_protection_secs,
        config.world.speed,
    );
    Ok((repo, pool, econ))
}

async fn seed(args: &[String]) -> Result<(), BoxErr> {
    let players = flag_u32(args, "--players", 1000);
    let (repo, pool, _econ) = open_repo().await?;
    let t = Instant::now();
    let summary = perf::seed_world(&pool, repo.world_id(), players).await?;
    println!(
        "seeded → {} players, {} villages in {:?}",
        summary.players,
        summary.villages,
        t.elapsed()
    );
    Ok(())
}

async fn measure(args: &[String]) -> Result<(), BoxErr> {
    let players = flag_u32(args, "--players", 0);
    let heartbeats = flag_u32(args, "--heartbeats", 0);
    let iters = flag_u32(args, "--iters", 5).max(1);
    let (repo, pool, econ) = open_repo().await?;

    if players > 0 {
        let t = Instant::now();
        let s = perf::seed_world(&pool, repo.world_id(), players).await?;
        println!(
            "seeded {} players / {} villages in {:?}",
            s.players,
            s.villages,
            t.elapsed()
        );
    }
    if heartbeats > 0 {
        perf::seed_heartbeats(&pool, heartbeats).await?;
        println!("seeded {heartbeats} due heartbeats");
    }

    // A seeded player + a viewport over the seeded block for the per-entity reads.
    let pid: Option<uuid::Uuid> =
        sqlx::query_scalar("SELECT id FROM users WHERE username = 'perf_1'")
            .fetch_optional(&pool)
            .await?;
    let total_players = sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM users WHERE is_npc = false AND username LIKE 'perf\\_%'",
    )
    .fetch_one(&pool)
    .await?;
    println!("\nworld has {total_players} perf players; best-of-{iters} hot-path latency:");

    let board = bench(iters, || async {
        repo.population_board(&econ, BoardScope::World, 100)
            .await
            .map(|_| ())
    })
    .await?;
    println!("  population_board(world, top 100)   {board:>8.2} ms");

    if let Some(pid) = pid {
        let pid = PlayerId(pid.as_u128());
        let vof = bench(iters, || async { repo.villages_of(pid).await.map(|_| ()) }).await?;
        println!("  villages_of(player)                {vof:>8.2} ms");

        let stats = bench(iters, || async {
            player_statistics(&repo, &econ, pid).await.map(|_| ())
        })
        .await?;
        println!("  player_statistics(player)          {stats:>8.2} ms");
    }

    let w = perf::seed_block_width(total_players.max(1) as u32).min(31);
    let viewport: Vec<Coordinate> = (0..w)
        .flat_map(|x| (0..w).map(move |y| Coordinate::new(x, y)))
        .collect();
    let map = bench(iters, || async {
        repo.villages_at(&viewport).await.map(|_| ())
    })
    .await?;
    println!(
        "  map viewport ({} tiles){}{map:>8.2} ms",
        viewport.len(),
        " ".repeat(13usize.saturating_sub(viewport.len().to_string().len()))
    );

    // Scheduler drain throughput (if a backlog was seeded).
    let pending: i64 =
        sqlx::query_scalar("SELECT count(*) FROM scheduled_events WHERE status = 'pending'")
            .fetch_one(&pool)
            .await?;
    if pending > 0 {
        let store = PgEventStore::new(pool.clone(), repo.world_id());
        let n = now();
        let t = Instant::now();
        let mut processed = 0usize;
        loop {
            let c = process_due(&store, n, 500).await?;
            processed += c;
            if c == 0 {
                break;
            }
        }
        let el = t.elapsed();
        println!(
            "\nscheduler: drained {processed} events in {el:?} ({:.0} events/s)",
            processed as f64 / el.as_secs_f64().max(0.001)
        );
    }

    if has_flag(args, "--explain") {
        println!("\nEXPLAIN ANALYZE (hot queries):");
        let world = uuid::Uuid::from_u128(repo.world_id().0);
        explain(
            &pool,
            "villages_of (owner_id filter)",
            "EXPLAIN (ANALYZE, BUFFERS) SELECT id, x, y FROM villages WHERE owner_id = \
             (SELECT id FROM users WHERE username = 'perf_1')",
        )
        .await?;
        explain_world(
            &pool,
            world,
            "population board join",
            "EXPLAIN (ANALYZE, BUFFERS) SELECT u.id, count(*) FROM villages v \
             JOIN users u ON u.id = v.owner_id \
             WHERE v.world_id = $1 AND u.is_npc = false GROUP BY u.id",
        )
        .await?;
    }
    Ok(())
}

/// Time `iters` runs of `f`, returning the best (minimum) in milliseconds.
async fn bench<F, Fut, E>(iters: u32, mut f: F) -> Result<f64, BoxErr>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<(), E>>,
    E: std::fmt::Display,
{
    let mut best = f64::MAX;
    for _ in 0..iters {
        let t = Instant::now();
        f().await.map_err(to_box)?;
        best = best.min(t.elapsed().as_secs_f64() * 1000.0);
    }
    Ok(best)
}

async fn explain(pool: &sqlx::PgPool, label: &str, sql: &str) -> Result<(), BoxErr> {
    println!("• {label}");
    let rows: Vec<String> = sqlx::query_scalar(sql).fetch_all(pool).await?;
    for line in rows {
        println!("    {line}");
    }
    Ok(())
}

async fn explain_world(
    pool: &sqlx::PgPool,
    world: uuid::Uuid,
    label: &str,
    sql: &str,
) -> Result<(), BoxErr> {
    println!("• {label}");
    let rows: Vec<String> = sqlx::query_scalar(sql).bind(world).fetch_all(pool).await?;
    for line in rows {
        println!("    {line}");
    }
    Ok(())
}

fn to_box<E: std::fmt::Display>(e: E) -> BoxErr {
    e.to_string().into()
}

// ---------------------------------------------------------------------------------------------------
// load — concurrent HTTP action mix against a running server.
// ---------------------------------------------------------------------------------------------------

async fn load(args: &[String]) -> Result<(), BoxErr> {
    let base = flag_str(args, "--base-url").unwrap_or_else(|| "http://127.0.0.1:8080".to_owned());
    let concurrency = flag_u32(args, "--concurrency", 16).max(1) as usize;
    let count = flag_u32(args, "--count", 200);
    println!("load: {count} flows against {base} at concurrency {concurrency}");

    let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(concurrency));
    let latencies = std::sync::Arc::new(tokio::sync::Mutex::new(Vec::<f64>::new()));
    let errors = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let started = Instant::now();

    let mut handles = Vec::new();
    for i in 0..count {
        let permit = sem.clone().acquire_owned().await.unwrap();
        let base = base.clone();
        let latencies = latencies.clone();
        let errors = errors.clone();
        handles.push(tokio::spawn(async move {
            let _permit = permit;
            match run_flow(&base, i).await {
                Ok(samples) => latencies.lock().await.extend(samples),
                Err(_) => {
                    errors.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
            }
        }));
    }
    for h in handles {
        let _ = h.await;
    }

    let wall = started.elapsed();
    let mut samples = std::sync::Arc::try_unwrap(latencies).unwrap().into_inner();
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = samples.len();
    let errs = errors.load(std::sync::atomic::Ordering::Relaxed);
    println!("\n{n} requests, {errs} failed flows, wall {wall:?}");
    if n > 0 {
        let pct = |p: f64| samples[((n as f64 * p) as usize).min(n - 1)];
        println!(
            "  throughput   {:.0} req/s\n  p50 {:.1} ms   p90 {:.1} ms   p99 {:.1} ms   max {:.1} ms",
            n as f64 / wall.as_secs_f64().max(0.001),
            pct(0.50),
            pct(0.90),
            pct(0.99),
            samples[n - 1]
        );
    }
    Ok(())
}

/// One simulated player: register → view village → build a field → read the map. Returns per-request ms.
async fn run_flow(base: &str, i: u32) -> Result<Vec<f64>, BoxErr> {
    let client = reqwest::Client::builder()
        .cookie_store(true)
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let user = format!(
        "load_{}_{i}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let mut samples = Vec::new();
    let mut timed = async |req: reqwest::RequestBuilder| -> Result<(), BoxErr> {
        let t = Instant::now();
        let res = req.send().await?;
        let _ = res.bytes().await?;
        samples.push(t.elapsed().as_secs_f64() * 1000.0);
        Ok(())
    };

    timed(client.post(format!("{base}/register")).form(&[
        ("username", user.as_str()),
        ("email", format!("{user}@perf.local").as_str()),
        ("password", "secret12"),
        ("tribe", "gauls"),
    ]))
    .await?;
    timed(client.get(format!("{base}/village"))).await?;
    timed(
        client
            .post(format!("{base}/village/build"))
            .form(&[("table", "field"), ("slot", "0")]),
    )
    .await?;
    timed(client.get(format!("{base}/map"))).await?;
    Ok(samples)
}
