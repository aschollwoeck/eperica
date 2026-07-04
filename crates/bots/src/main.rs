//! `eperica-bots` binary entry point.
//!
//! Parses flags / env vars, initialises structured tracing, and delegates to
//! [`runner::run_fleet`].
//!
//! ## Flag / env reference
//!
//! | Flag                  | Env var              | Default                    | Description                                       |
//! |-----------------------|----------------------|----------------------------|---------------------------------------------------|
//! | `--server <URL>`      | `EPB_SERVER`         | —                          | Eperica server base URL (required)                |
//! | `--world <UUID>`      | `EPB_WORLD`          | —                          | World UUID to operate in (required)               |
//! | `--keys <PATH>`       | `EPB_KEYS`           | —                          | Agent key manifest JSON (required)                |
//! | `--dry-run`           | `EPB_DRY_RUN`        | false                      | Log intents, make no HTTP POSTs                   |
//! | `--tick-secs <N>`     | `EPB_TICK_SECS`      | none                       | Fixed tick interval; disables jitter              |
//! | `--cap <N>`           | `EPB_CAP`            | 4                          | Max concurrent bot ticks                          |
//! | `--no-llm`            | `EPB_NO_LLM=1`       | false                      | Disable strategist even if a key is configured    |
//! | `--llm-budget <N>`    | `EPB_LLM_BUDGET`     | 12                         | Max strategist calls per rolling hour (fleet-wide)|
//! | `--llm-interval-secs` | `EPB_LLM_INTERVAL_SECS`| 14400                   | Seconds between per-bot strategist calls          |
//! |                       | `EPB_LLM_MODEL`      | `claude-haiku-4-5-20251001`| Anthropic model ID                                |
//! |                       | `EPB_ANTHROPIC_KEY`  | —                          | Anthropic API key (preferred over ANTHROPIC_API_KEY)|
//! |                       | `ANTHROPIC_API_KEY`  | —                          | Standard Anthropic key env var (fallback)         |
//! | `--help`, `-h`        | —                    | —                          | Print this help and exit                          |

use std::sync::Arc;

use eperica_bots::runner::{LlmConfig, RunnerConfig, run_fleet};
use eperica_bots::strategist::{AnthropicBackend, StrategistBackend};
use tracing::info;
use tracing_subscriber::EnvFilter;

fn main() {
    init_tracing();

    let (cfg, backend) = match parse_config() {
        Ok(pair) => pair,
        Err(e) => {
            eprintln!("error: {e}");
            eprintln!("Run with --help for usage.");
            std::process::exit(1);
        }
    };

    // Log startup mode — the API key is NEVER included in the log output.
    if backend.is_some() {
        let model = cfg.llm.as_ref().map(|l| l.model.as_str()).unwrap_or("?");
        info!(model, "strategist enabled");
    } else {
        info!("strategist disabled (no API key or --no-llm)");
    }

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime")
        .block_on(run_fleet(cfg, backend));
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

/// Parse `RunnerConfig` and an optional `StrategistBackend` from command-line
/// flags and environment variables.
///
/// Flags take precedence over environment variables.  Exits 0 on `--help`.
/// The API key is NEVER returned in an error string.
fn parse_config() -> Result<(RunnerConfig, Option<Arc<dyn StrategistBackend>>), String> {
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

    // ---------------------------------------------------------------------------
    // LLM strategist flags
    // ---------------------------------------------------------------------------

    let no_llm = args.iter().any(|a| a == "--no-llm")
        || std::env::var("EPB_NO_LLM").is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));

    // Key detection: EPB_ANTHROPIC_KEY takes priority over the standard name.
    let api_key = std::env::var("EPB_ANTHROPIC_KEY")
        .ok()
        .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok());

    // Model: env-only override (no flag — the model is a deployment-level detail).
    let llm_model =
        std::env::var("EPB_LLM_MODEL").unwrap_or_else(|_| "claude-haiku-4-5-20251001".to_owned());

    let llm_budget = flag_or_env("--llm-budget", "EPB_LLM_BUDGET")
        .map(|v| {
            v.parse::<u32>()
                .map_err(|_| format!("--llm-budget must be a non-negative integer, got {v:?}"))
        })
        .transpose()?
        .unwrap_or(12u32);

    let llm_interval_secs = flag_or_env("--llm-interval-secs", "EPB_LLM_INTERVAL_SECS")
        .map(|v| {
            v.parse::<u64>()
                .map_err(|_| format!("--llm-interval-secs must be a positive integer, got {v:?}"))
                .and_then(|n: u64| {
                    if n == 0 {
                        Err("--llm-interval-secs must be at least 1".to_owned())
                    } else {
                        Ok(n)
                    }
                })
        })
        .transpose()?
        .unwrap_or(14400u64);

    // Build the backend and LlmConfig when a key is present and --no-llm is not set.
    let (llm, backend) = if !no_llm {
        if let Some(key) = api_key {
            let config = LlmConfig {
                model: llm_model.clone(),
                budget_per_hour: llm_budget,
                interval_secs: llm_interval_secs,
            };
            let b: Arc<dyn StrategistBackend> = Arc::new(AnthropicBackend::new(key, &llm_model));
            (Some(config), Some(b))
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };

    let cfg = RunnerConfig {
        server,
        world,
        keys_path,
        dry_run,
        tick_scale,
        cap,
        open_window, // --open-window: demo/ops override — every bot acts around the clock
        llm,
    };

    Ok((cfg, backend))
}

fn print_help() {
    println!(
        "eperica-bots — Eperica bot fleet runner

Usage:
  eperica-bots --server <URL> --world <UUID> --keys <PATH> [OPTIONS]

Required:
  --server <URL>        Eperica server base URL (env: EPB_SERVER)
  --world <UUID>        World UUID to operate in (env: EPB_WORLD)
  --keys <PATH>         Agent key manifest JSON file (env: EPB_KEYS)

Options:
  --dry-run             Log intents but make no HTTP POST calls (env: EPB_DRY_RUN=1)
  --open-window         Ignore persona activity windows — bots act 24/7 (demo/ops; env: EPB_OPEN_WINDOW=1)
  --tick-secs <N>       Fixed tick interval in seconds; disables persona jitter
                        (env: EPB_TICK_SECS) — for testing and demos only
  --cap <N>             Maximum concurrent bot ticks; default 4 (env: EPB_CAP)
  --no-llm              Disable the LLM strategist even if a key is configured
                        (env: EPB_NO_LLM=1)
  --llm-budget <N>      Max fleet-wide strategist calls per rolling hour; default 12
                        (env: EPB_LLM_BUDGET)
  --llm-interval-secs <N>  Seconds between per-bot strategist calls; default 14400 (4h)
                        (env: EPB_LLM_INTERVAL_SECS)
  --help, -h            Show this help and exit

Environment-only LLM settings:
  EPB_ANTHROPIC_KEY     Anthropic API key (takes priority over ANTHROPIC_API_KEY)
  ANTHROPIC_API_KEY     Standard Anthropic API key env var (fallback)
  EPB_LLM_MODEL         Anthropic model ID; default claude-haiku-4-5-20251001

The API key is NEVER logged.
Flags take precedence over environment variables.
Log level is controlled by RUST_LOG (e.g. RUST_LOG=debug)."
    );
}
