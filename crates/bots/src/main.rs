//! `eperica-bots` binary entry point.
//!
//! Parses flags / env vars, initialises structured tracing, and delegates to
//! [`runner::run_fleet`].
//!
//! ## Flag / env reference
//!
//! | Flag            | Env var       | Default | Description                         |
//! |-----------------|---------------|---------|-------------------------------------|
//! | `--server <URL>`| `EPB_SERVER`  | —       | Eperica server base URL (required)  |
//! | `--world <UUID>`| `EPB_WORLD`   | —       | World UUID to operate in (required) |
//! | `--keys <PATH>` | `EPB_KEYS`    | —       | Agent key manifest JSON (required)  |
//! | `--dry-run`     | `EPB_DRY_RUN` | false   | Log intents, make no HTTP POSTs     |
//! | `--tick-secs <N>`| `EPB_TICK_SECS`| none  | Fixed tick interval; disables jitter|
//! | `--cap <N>`     | `EPB_CAP`     | 4       | Max concurrent bot ticks            |
//! | `--help`        | —             | —       | Print this help and exit            |

use eperica_bots::runner::{RunnerConfig, run_fleet};
use tracing_subscriber::EnvFilter;

fn main() {
    init_tracing();

    let cfg = match parse_config() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}");
            eprintln!("Run with --help for usage.");
            std::process::exit(1);
        }
    };

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime")
        .block_on(run_fleet(cfg));
}

// ---------------------------------------------------------------------------
// Tracing
// ---------------------------------------------------------------------------

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

// ---------------------------------------------------------------------------
// Flag / env parsing
// ---------------------------------------------------------------------------

/// Parse `RunnerConfig` from command-line flags and environment variables.
///
/// Flags take precedence over environment variables.  Exits 0 on `--help`.
fn parse_config() -> Result<RunnerConfig, String> {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        std::process::exit(0);
    }

    // Helper: look up a flag value in `args` or fall back to an env var.
    let flag_or_env = |flag: &str, env: &str| -> Option<String> {
        // Check --flag value and --flag=value forms.
        let mut it = args.iter().peekable();
        while let Some(a) = it.next() {
            if a == flag {
                if let Some(v) = it.next() {
                    return Some(v.clone());
                }
            } else if let Some(val) = a.strip_prefix(&format!("{flag}=")) {
                return Some(val.to_owned());
            }
        }
        std::env::var(env).ok()
    };

    let server = flag_or_env("--server", "EPB_SERVER")
        .ok_or("--server (or EPB_SERVER) is required")?
        .trim_end_matches('/')
        .to_owned();

    let world = flag_or_env("--world", "EPB_WORLD").ok_or("--world (or EPB_WORLD) is required")?;

    let keys_path = flag_or_env("--keys", "EPB_KEYS").ok_or("--keys (or EPB_KEYS) is required")?;

    let open_window = args.iter().any(|a| a == "--open-window")
        || std::env::var("EPB_OPEN_WINDOW").is_ok_and(|v| v == "1" || v == "true");
    let dry_run = args.iter().any(|a| a == "--dry-run")
        || std::env::var("EPB_DRY_RUN")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

    let tick_scale = flag_or_env("--tick-secs", "EPB_TICK_SECS")
        .map(|v| {
            v.parse::<u64>()
                .map_err(|_| format!("--tick-secs must be a positive integer, got {v:?}"))
        })
        .transpose()?;

    let cap = flag_or_env("--cap", "EPB_CAP")
        .map(|v| {
            v.parse::<usize>()
                .map_err(|_| format!("--cap must be a positive integer, got {v:?}"))
        })
        .transpose()?
        .unwrap_or(4);

    Ok(RunnerConfig {
        server,
        world,
        keys_path,
        dry_run,
        tick_scale,
        cap,
        open_window, // --open-window: demo/ops override — every bot acts around the clock
    })
}

fn print_help() {
    println!(
        "eperica-bots — Eperica bot fleet runner

Usage:
  eperica-bots --server <URL> --world <UUID> --keys <PATH> [OPTIONS]

Required:
  --server <URL>      Eperica server base URL (env: EPB_SERVER)
  --world <UUID>      World UUID to operate in (env: EPB_WORLD)
  --keys <PATH>       Agent key manifest JSON file (env: EPB_KEYS)

Options:
  --dry-run           Log intents but make no HTTP POST calls (env: EPB_DRY_RUN=1)
  --open-window       Ignore persona activity windows — bots act 24/7 (demo/ops; env: EPB_OPEN_WINDOW=1)
  --tick-secs <N>     Fixed tick interval in seconds; disables persona jitter
                      (env: EPB_TICK_SECS) — for testing and demos only
  --cap <N>           Maximum concurrent bot ticks; default 4 (env: EPB_CAP)
  --help, -h          Show this help and exit

Flags take precedence over environment variables.
Log level is controlled by RUST_LOG (e.g. RUST_LOG=debug)."
    );
}
