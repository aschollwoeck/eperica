//! Intent executor: maps `Vec<Intent>` to API calls, classifies failures,
//! and returns an `ExecReport`.
//!
//! # Pure helpers (unit-tested without HTTP)
//!
//! - [`classify`]  — maps an `ApiFailure` to a bot-level `Outcome`.
//! - [`describe`]  — produces a one-line human/log-readable summary of an
//!   intent, encoding the target API method and all arguments.  Used verbatim
//!   in `--dry-run` log lines and tested directly to verify the
//!   intent→(method, args) mapping without any HTTP mocking.
//!
//! # I/O boundary
//!
//! [`execute_intents`] is the sole I/O boundary.  It applies AC3 rules:
//!
//! - **RuleDenied** (4xx, not 401/429): log and count; do NOT retry this tick.
//! - **Backoff** (429): log; stop remaining intents; propagate `backoff_secs`.
//! - **RetireBot** (401): log; stop; signal the runner to remove the bot.
//! - **Transient** (5xx, transport, bad body): log and count; continue.
//!
//! Callers add bot-identity context via a surrounding tracing span.

use std::collections::BTreeMap;

use tracing::{debug, info, warn};

use crate::client::{ApiClient, ApiFailure};
use crate::policy::Intent;

// ---------------------------------------------------------------------------
// Outcome
// ---------------------------------------------------------------------------

/// Per-call outcome derived from an `ApiFailure`.
#[derive(Debug, Clone)]
pub enum Outcome {
    /// The server returned a rule violation (4xx other than 401/429).
    /// The bot must NOT retry the same intent this tick.
    RuleDenied { code: String, reason: String },
    /// The server returned 429: stop this tick and delay `secs` before next.
    Backoff { secs: u64 },
    /// The server returned 401: this key is dead; the runner should retire the bot.
    RetireBot,
    /// A transient error (transport / 5xx / bad body): count it but continue.
    Transient(String),
}

/// Map an `ApiFailure` to a bot-level `Outcome`.
///
/// | Failure                     | Outcome                      |
/// |-----------------------------|------------------------------|
/// | `Api { status: 401 }`       | `RetireBot`                  |
/// | `Api { status: 429 }`       | `Backoff { retry_after_secs.unwrap_or(60) }` |
/// | `Api { status: 4xx (other)}`| `RuleDenied { code, reason }`|
/// | `Api { status: 5xx }`       | `Transient`                  |
/// | `Http(_)` / `BadBody(_)`    | `Transient`                  |
pub fn classify(f: &ApiFailure) -> Outcome {
    match f {
        ApiFailure::Api { status: 401, .. } => Outcome::RetireBot,

        ApiFailure::Api {
            status: 429,
            retry_after_secs,
            ..
        } => Outcome::Backoff {
            secs: retry_after_secs.unwrap_or(60),
        },

        ApiFailure::Api {
            status,
            code,
            reason,
            ..
        } if (400..500).contains(status) => Outcome::RuleDenied {
            code: code.clone(),
            reason: reason.clone(),
        },

        ApiFailure::Api { status, code, .. } if *status >= 500 => {
            Outcome::Transient(format!("server error {status} ({code})"))
        }

        ApiFailure::Api { status, code, .. } => {
            // Unexpected status (1xx, 3xx, ...) — treat as transient.
            Outcome::Transient(format!("unexpected HTTP {status} ({code})"))
        }

        ApiFailure::Http(msg) => Outcome::Transient(format!("transport: {msg}")),
        ApiFailure::BadBody(msg) => Outcome::Transient(format!("bad body: {msg}")),
    }
}

// ---------------------------------------------------------------------------
// describe — pure, unit-tested
// ---------------------------------------------------------------------------

/// Return a one-line description of `intent` encoding the target API method
/// and all arguments.
///
/// Mapping (intent → API call):
///
/// | Intent         | API call                                          |
/// |----------------|---------------------------------------------------|
/// | `Build`        | `POST …/build` (target, slot, kind)               |
/// | `Train`        | `POST …/train` (unit, count)                      |
/// | `Research`     | `POST …/research` (unit)                          |
/// | `Reinforce`    | `POST …/reinforce` (x, y, units)                  |
/// | `Recall`       | `POST …/return` (host)                            |
/// | `Raid`         | `POST …/attack?mode=raid` (x, y, units)           |
/// | `TrainSettlers`| `POST …/train` (unit="settler", count)            |
/// | `Settle`       | `POST …/settle` (x, y)                            |
pub fn describe(intent: &Intent) -> String {
    match intent {
        Intent::Build {
            village,
            target,
            slot,
            kind,
        } => {
            let k = kind.as_deref().unwrap_or("(none)");
            format!("build village={village} target={target} slot={slot} kind={k}")
        }

        Intent::Train {
            village,
            unit,
            count,
        } => {
            format!("train village={village} unit={unit} count={count}")
        }

        Intent::Research { village, unit } => {
            format!("research village={village} unit={unit}")
        }

        Intent::Reinforce {
            village,
            x,
            y,
            units,
        } => {
            format!(
                "reinforce village={village} x={x} y={y} units=[{}]",
                fmt_units(units)
            )
        }

        Intent::Recall { village, host } => {
            format!("recall village={village} host={host}")
        }

        Intent::Raid {
            village,
            x,
            y,
            units,
        } => {
            format!(
                "raid village={village} x={x} y={y} units=[{}]",
                fmt_units(units)
            )
        }

        Intent::TrainSettlers { village, count } => {
            // Maps to POST …/train with unit="settler"
            format!("train_settlers village={village} count={count}")
        }

        Intent::Settle { village, x, y } => {
            format!("settle village={village} x={x} y={y}")
        }
    }
}

fn fmt_units(units: &BTreeMap<String, u32>) -> String {
    units
        .iter()
        .map(|(u, n)| format!("{u}:{n}"))
        .collect::<Vec<_>>()
        .join(",")
}

// ---------------------------------------------------------------------------
// ExecReport
// ---------------------------------------------------------------------------

/// Summary of one [`execute_intents`] call.
#[derive(Debug, Default)]
pub struct ExecReport {
    /// Intents that reached the server (succeeded or received a known API error).
    pub executed: usize,
    /// Intents that were rule-denied (4xx, not 401/429).
    pub denied: usize,
    /// Non-zero when a 429 was received: the bot should delay this many seconds.
    pub backoff_secs: Option<u64>,
    /// True when a 401 was received: the runner must retire this bot.
    pub retire: bool,
    /// Intents that hit a transient error (transport / 5xx / bad body).
    pub transient: usize,
}

// ---------------------------------------------------------------------------
// execute_intents
// ---------------------------------------------------------------------------

/// Execute `intents` against the API, applying AC3 rules.
///
/// When `dry_run` is true, no HTTP POST calls are made: each intent is logged
/// as `"DRY intent=…"` and counted as executed.
///
/// Surround with a tracing span that carries bot-identity fields (name, tick)
/// — this function only adds intent-level fields.
pub async fn execute_intents(
    client: &ApiClient,
    world: &str,
    intents: &[Intent],
    dry_run: bool,
) -> ExecReport {
    let mut report = ExecReport::default();

    for intent in intents {
        let desc = describe(intent);

        if dry_run {
            info!(intent = %desc, "DRY intent");
            report.executed += 1;
            continue;
        }

        let result = dispatch(client, world, intent).await;

        match result {
            Ok(_) => {
                debug!(intent = %desc, "intent executed");
                report.executed += 1;
            }
            Err(failure) => {
                // Count as executed (reached the server) before classifying.
                report.executed += 1;

                match classify(&failure) {
                    Outcome::RuleDenied { code, reason } => {
                        debug!(
                            intent = %desc,
                            %code,
                            %reason,
                            "rule-denied; no retry this tick"
                        );
                        report.denied += 1;
                        // Do NOT retry — continue to the next intent.
                    }
                    Outcome::Backoff { secs } => {
                        warn!(
                            intent = %desc,
                            backoff_secs = secs,
                            "rate-limited (429); stopping tick"
                        );
                        report.backoff_secs = Some(secs);
                        return report; // Stop all remaining intents immediately.
                    }
                    Outcome::RetireBot => {
                        warn!(intent = %desc, "key rejected (401); retiring bot");
                        report.retire = true;
                        return report; // Stop all remaining intents immediately.
                    }
                    Outcome::Transient(msg) => {
                        warn!(intent = %desc, error = %msg, "transient error; continuing");
                        report.transient += 1;
                        // Continue to next intent.
                    }
                }
            }
        }
    }

    report
}

// ---------------------------------------------------------------------------
// dispatch — intent → ApiClient call
// ---------------------------------------------------------------------------

async fn dispatch(
    client: &ApiClient,
    world: &str,
    intent: &Intent,
) -> Result<serde_json::Value, ApiFailure> {
    match intent {
        Intent::Build {
            village,
            target,
            slot,
            kind,
        } => {
            client
                .build(world, village, target, *slot, kind.as_deref())
                .await
        }

        Intent::Train {
            village,
            unit,
            count,
        } => client.train(world, village, unit, *count).await,

        Intent::Research { village, unit } => client.research(world, village, unit).await,

        Intent::Reinforce {
            village,
            x,
            y,
            units,
        } => client.reinforce(world, village, *x, *y, units).await,

        Intent::Recall { village, host } => client.ret(world, village, host).await,

        Intent::Raid {
            village,
            x,
            y,
            units,
        } => {
            client
                .attack(world, village, *x, *y, units, "raid", None)
                .await
        }

        Intent::TrainSettlers { village, count } => {
            // TrainSettlers maps to the train endpoint with unit slug "settler".
            // All three tribes use the same settler unit slug (verified in
            // specs/balance/presets/classic/units.toml).
            client.train(world, village, "settler", *count).await
        }

        Intent::Settle { village, x, y } => client.settle(world, village, *x, *y).await,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::ApiFailure;

    // -----------------------------------------------------------------------
    // classify: one test per outcome class
    // -----------------------------------------------------------------------

    #[test]
    fn classify_401_retires_bot() {
        let f = ApiFailure::Api {
            status: 401,
            code: "unauthorized".into(),
            reason: "invalid key".into(),
            retry_after_secs: None,
        };
        assert!(
            matches!(classify(&f), Outcome::RetireBot),
            "401 must retire the bot"
        );
    }

    #[test]
    fn classify_429_with_retry_after_secs() {
        let f = ApiFailure::Api {
            status: 429,
            code: "rate_limited".into(),
            reason: "slow down".into(),
            retry_after_secs: Some(120),
        };
        assert!(
            matches!(classify(&f), Outcome::Backoff { secs: 120 }),
            "429 with retry_after_secs must backoff for that many seconds"
        );
    }

    #[test]
    fn classify_429_without_retry_after_defaults_to_60() {
        let f = ApiFailure::Api {
            status: 429,
            code: "rate_limited".into(),
            reason: "slow down".into(),
            retry_after_secs: None,
        };
        assert!(
            matches!(classify(&f), Outcome::Backoff { secs: 60 }),
            "429 without retry_after_secs must default to 60 s"
        );
    }

    #[test]
    fn classify_other_4xx_rule_denied() {
        for status in [400u16, 403, 404, 409, 422] {
            let f = ApiFailure::Api {
                status,
                code: "some_error".into(),
                reason: "rule violation".into(),
                retry_after_secs: None,
            };
            assert!(
                matches!(classify(&f), Outcome::RuleDenied { .. }),
                "status {status} must be RuleDenied"
            );
        }
    }

    #[test]
    fn classify_5xx_transient() {
        for status in [500u16, 502, 503, 504] {
            let f = ApiFailure::Api {
                status,
                code: "server_error".into(),
                reason: "internal".into(),
                retry_after_secs: None,
            };
            assert!(
                matches!(classify(&f), Outcome::Transient(_)),
                "status {status} must be Transient"
            );
        }
    }

    #[test]
    fn classify_http_error_transient() {
        let f = ApiFailure::Http("connection refused".into());
        assert!(
            matches!(classify(&f), Outcome::Transient(_)),
            "transport error must be Transient"
        );
    }

    #[test]
    fn classify_bad_body_transient() {
        let f = ApiFailure::BadBody("unexpected EOF".into());
        assert!(
            matches!(classify(&f), Outcome::Transient(_)),
            "bad body must be Transient"
        );
    }

    // -----------------------------------------------------------------------
    // describe: verify intent → method/args mapping for each variant
    // -----------------------------------------------------------------------

    #[test]
    fn describe_build_field() {
        let i = Intent::Build {
            village: "v1".into(),
            target: "field",
            slot: 3,
            kind: None,
        };
        let s = describe(&i);
        assert!(s.contains("build"), "must name the operation: {s}");
        assert!(s.contains("target=field"), "must identify target: {s}");
        assert!(s.contains("slot=3"), "must include slot: {s}");
        assert!(s.contains("v1"), "must include village: {s}");
        assert!(s.contains("(none)"), "absent kind must be shown: {s}");
    }

    #[test]
    fn describe_build_building() {
        let i = Intent::Build {
            village: "v1".into(),
            target: "building",
            slot: 5,
            kind: Some("barracks".into()),
        };
        let s = describe(&i);
        assert!(s.contains("target=building"), "must identify target: {s}");
        assert!(s.contains("kind=barracks"), "must include kind: {s}");
        assert!(s.contains("slot=5"), "must include slot: {s}");
    }

    #[test]
    fn describe_train_encodes_unit_and_count() {
        let i = Intent::Train {
            village: "v1".into(),
            unit: "legionnaire".into(),
            count: 5,
        };
        let s = describe(&i);
        assert!(s.contains("train"), "must name the operation: {s}");
        assert!(s.contains("unit=legionnaire"), "must include unit: {s}");
        assert!(s.contains("count=5"), "must include count: {s}");
    }

    #[test]
    fn describe_train_settlers_maps_to_train_with_settler() {
        let i_train = Intent::Train {
            village: "v".into(),
            unit: "clubswinger".into(),
            count: 3,
        };
        let i_settlers = Intent::TrainSettlers {
            village: "v".into(),
            count: 2,
        };
        let s_train = describe(&i_train);
        let s_settlers = describe(&i_settlers);
        // Distinct descriptions — both train-like but different units.
        assert_ne!(s_train, s_settlers, "descriptions must differ");
        assert!(
            s_settlers.contains("settler"),
            "TrainSettlers desc must mention settler: {s_settlers}"
        );
        assert!(
            s_settlers.contains("count=2"),
            "TrainSettlers must include count: {s_settlers}"
        );
    }

    #[test]
    fn describe_research() {
        let i = Intent::Research {
            village: "v1".into(),
            unit: "imperian".into(),
        };
        let s = describe(&i);
        assert!(s.contains("research"), "must name the operation: {s}");
        assert!(s.contains("unit=imperian"), "must include unit: {s}");
    }

    #[test]
    fn describe_raid_encodes_attack_mode() {
        let mut units = BTreeMap::new();
        units.insert("legionnaire".into(), 8u32);
        let i = Intent::Raid {
            village: "v1".into(),
            x: 3,
            y: -2,
            units,
        };
        let s = describe(&i);
        assert!(s.contains("raid"), "must name the operation: {s}");
        assert!(s.contains("x=3"), "must include x: {s}");
        assert!(s.contains("y=-2"), "must include y: {s}");
        assert!(s.contains("legionnaire"), "must include units: {s}");
    }

    #[test]
    fn describe_reinforce() {
        let mut units = BTreeMap::new();
        units.insert("legionnaire".into(), 10u32);
        let i = Intent::Reinforce {
            village: "v1".into(),
            x: 1,
            y: 2,
            units,
        };
        let s = describe(&i);
        assert!(s.contains("reinforce"), "must name the operation: {s}");
        assert!(s.contains("x=1"), "must include x: {s}");
        assert!(s.contains("y=2"), "must include y: {s}");
    }

    #[test]
    fn describe_recall_encodes_return_host() {
        let i = Intent::Recall {
            village: "v1".into(),
            host: "host-uuid-123".into(),
        };
        let s = describe(&i);
        assert!(s.contains("recall"), "must name the operation: {s}");
        assert!(s.contains("host-uuid-123"), "must include host: {s}");
    }

    #[test]
    fn describe_settle() {
        let i = Intent::Settle {
            village: "v1".into(),
            x: 5,
            y: -3,
        };
        let s = describe(&i);
        assert!(s.contains("settle"), "must name the operation: {s}");
        assert!(s.contains("x=5"), "must include x: {s}");
        assert!(s.contains("y=-3"), "must include y: {s}");
    }
}
