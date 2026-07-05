//! Thin HTTP client for the Eperica agent API (docs/agent-api.md).
//!
//! `ApiClient` is deliberately minimal: one request per call, no retry, no
//! backoff.  Policy decisions (when to retry, how to back off) belong in the
//! T3 executor, not here.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::digest::{Digest, MapWindow, MeResponse};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Classification of a failed API call.
#[derive(Debug)]
pub enum ApiFailure {
    /// The server returned a non-2xx status with a parsed `{error, reason}` body.
    Api {
        status: u16,
        /// Stable machine code (e.g. `"insufficient"`, `"lane_busy"`).
        code: String,
        /// Player-visible reason text.
        reason: String,
        /// Present on 429 responses: the server-recommended back-off in seconds.
        /// The executor uses this directly; absent means fall back to a default.
        retry_after_secs: Option<u64>,
    },
    /// A transport-level failure (connection refused, timeout, DNS, etc.).
    Http(String),
    /// The response body could not be interpreted as the expected type.
    BadBody(String),
}

impl std::fmt::Display for ApiFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Api {
                status,
                code,
                reason,
                retry_after_secs,
            } => {
                if let Some(secs) = retry_after_secs {
                    write!(f, "API {status} {code}: {reason} (retry after {secs}s)")
                } else {
                    write!(f, "API {status} {code}: {reason}")
                }
            }
            Self::Http(msg) => write!(f, "transport error: {msg}"),
            Self::BadBody(msg) => write!(f, "bad body: {msg}"),
        }
    }
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

/// HTTP client bound to one agent key.
///
/// Create one per bot; reuse across ticks (the inner `reqwest::Client` pools
/// connections).
pub struct ApiClient {
    /// Base URL of the Eperica server, without a trailing slash.
    pub base: String,
    /// `epk_<id>_<secret>` token for this bot.
    pub token: String,
    /// Underlying HTTP client (connection-pooled).
    pub http: reqwest::Client,
}

impl ApiClient {
    /// Construct a new client for `base` URL and `token`.  Strips a trailing
    /// slash from `base` so callers need not worry about it.
    pub fn new(base: impl Into<String>, token: impl Into<String>) -> Self {
        Self {
            base: base.into().trim_end_matches('/').to_owned(),
            token: token.into(),
            // 126: explicit timeouts — reqwest's default has NONE, so a hung request (server
            // restart under a live fleet) would hold a semaphore permit forever and freeze the
            // fleet at --cap. A timeout is a normal Transient outcome (retry next tick).
            http: reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(5))
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("HTTP client construction must succeed at startup"),
        }
    }

    fn bearer(&self) -> String {
        format!("Bearer {}", self.token)
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    async fn get<T>(&self, path: &str) -> Result<T, ApiFailure>
    where
        T: serde::de::DeserializeOwned,
    {
        let url = format!("{}{path}", self.base);
        let resp = self
            .http
            .get(&url)
            .header(reqwest::header::AUTHORIZATION, self.bearer())
            .send()
            .await
            .map_err(|e| ApiFailure::Http(e.to_string()))?;
        parse_response(resp).await
    }

    async fn post_json(&self, path: &str, body: Value) -> Result<Value, ApiFailure> {
        let url = format!("{}{path}", self.base);
        let body_str =
            serde_json::to_string(&body).map_err(|e| ApiFailure::BadBody(e.to_string()))?;
        let resp = self
            .http
            .post(&url)
            .header(reqwest::header::AUTHORIZATION, self.bearer())
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body_str)
            .send()
            .await
            .map_err(|e| ApiFailure::Http(e.to_string()))?;
        parse_response(resp).await
    }

    // -----------------------------------------------------------------------
    // Read endpoints
    // -----------------------------------------------------------------------

    /// `GET /api/me` — key introspection.
    pub async fn me(&self) -> Result<MeResponse, ApiFailure> {
        self.get("/api/me").await
    }

    /// `GET /api/w/{world}/state` — the full state digest.
    pub async fn state(&self, world: &str) -> Result<Digest, ApiFailure> {
        self.get(&format!("/api/w/{world}/state")).await
    }

    /// `GET /api/w/{world}/map?x&y&r` — a bounded map window.
    pub async fn map(&self, world: &str, x: i32, y: i32, r: u32) -> Result<MapWindow, ApiFailure> {
        self.get(&format!("/api/w/{world}/map?x={x}&y={y}&r={r}"))
            .await
    }

    // -----------------------------------------------------------------------
    // Economy actions
    // -----------------------------------------------------------------------

    /// `POST …/build` — queue a field or building upgrade.
    ///
    /// `target` is `"field"` or `"building"`.  `kind` is required for
    /// building orders and must be `None` for field orders.
    pub async fn build(
        &self,
        world: &str,
        village: &str,
        target: &str,
        slot: u8,
        kind: Option<&str>,
    ) -> Result<Value, ApiFailure> {
        let mut body = serde_json::json!({"target": target, "slot": slot});
        if let Some(k) = kind {
            body["kind"] = Value::String(k.to_owned());
        }
        self.post_json(&format!("/api/w/{world}/village/{village}/build"), body)
            .await
    }

    /// `POST …/train` — queue a training batch.
    pub async fn train(
        &self,
        world: &str,
        village: &str,
        unit: &str,
        count: u32,
    ) -> Result<Value, ApiFailure> {
        self.post_json(
            &format!("/api/w/{world}/village/{village}/train"),
            serde_json::json!({"unit": unit, "count": count}),
        )
        .await
    }

    /// `POST …/research` — start a unit research order.
    pub async fn research(
        &self,
        world: &str,
        village: &str,
        unit: &str,
    ) -> Result<Value, ApiFailure> {
        self.post_json(
            &format!("/api/w/{world}/village/{village}/research"),
            serde_json::json!({"unit": unit}),
        )
        .await
    }

    /// `POST …/smithy` — start a smithy upgrade order.
    pub async fn smithy(
        &self,
        world: &str,
        village: &str,
        unit: &str,
    ) -> Result<Value, ApiFailure> {
        self.post_json(
            &format!("/api/w/{world}/village/{village}/smithy"),
            serde_json::json!({"unit": unit}),
        )
        .await
    }

    // -----------------------------------------------------------------------
    // Military actions
    // -----------------------------------------------------------------------

    /// `POST …/attack` — send an attack or raid.
    ///
    /// `mode` is `"attack"` or `"raid"`.  `catapult` is the catapult target
    /// slug (optional, present only for catapult attacks).
    #[allow(clippy::too_many_arguments)]
    pub async fn attack(
        &self,
        world: &str,
        village: &str,
        x: i32,
        y: i32,
        units: &BTreeMap<String, u32>,
        mode: &str,
        catapult: Option<&str>,
    ) -> Result<Value, ApiFailure> {
        let mut body = serde_json::json!({
            "x": x, "y": y,
            "units": units,
            "mode": mode,
        });
        if let Some(t) = catapult {
            body["catapult_target"] = Value::String(t.to_owned());
        }
        self.post_json(&format!("/api/w/{world}/village/{village}/attack"), body)
            .await
    }

    /// `POST …/scout` — send scouts.
    ///
    /// `target` is `"resources"` or `"defenses"`.
    pub async fn scout(
        &self,
        world: &str,
        village: &str,
        x: i32,
        y: i32,
        units: &BTreeMap<String, u32>,
        target: &str,
    ) -> Result<Value, ApiFailure> {
        self.post_json(
            &format!("/api/w/{world}/village/{village}/scout"),
            serde_json::json!({"x": x, "y": y, "units": units, "target": target}),
        )
        .await
    }

    /// `POST …/reinforce` — send reinforcements.
    pub async fn reinforce(
        &self,
        world: &str,
        village: &str,
        x: i32,
        y: i32,
        units: &BTreeMap<String, u32>,
    ) -> Result<Value, ApiFailure> {
        self.post_json(
            &format!("/api/w/{world}/village/{village}/reinforce"),
            serde_json::json!({"x": x, "y": y, "units": units}),
        )
        .await
    }

    /// `POST …/return` — recall troops stationed at a foreign village.
    ///
    /// `host` is the `host_village` UUID from `reinforcements_abroad`.
    pub async fn ret(&self, world: &str, village: &str, host: &str) -> Result<Value, ApiFailure> {
        self.post_json(
            &format!("/api/w/{world}/village/{village}/return"),
            serde_json::json!({"host": host}),
        )
        .await
    }

    // -----------------------------------------------------------------------
    // Trade / settle / comms
    // -----------------------------------------------------------------------

    /// `POST …/trade` — send a resource shipment.
    #[allow(clippy::too_many_arguments)]
    pub async fn trade(
        &self,
        world: &str,
        village: &str,
        x: i32,
        y: i32,
        give_wood: i64,
        give_clay: i64,
        give_iron: i64,
        give_crop: i64,
    ) -> Result<Value, ApiFailure> {
        self.post_json(
            &format!("/api/w/{world}/village/{village}/trade"),
            serde_json::json!({
                "x": x, "y": y,
                "give": {
                    "wood": give_wood,
                    "clay": give_clay,
                    "iron": give_iron,
                    "crop": give_crop,
                },
            }),
        )
        .await
    }

    /// `POST …/settle` — send settlers to found a new village.
    pub async fn settle(
        &self,
        world: &str,
        village: &str,
        x: i32,
        y: i32,
    ) -> Result<Value, ApiFailure> {
        self.post_json(
            &format!("/api/w/{world}/village/{village}/settle"),
            serde_json::json!({"x": x, "y": y}),
        )
        .await
    }

    /// `POST /api/w/{world}/message` — send a direct message.
    ///
    /// `to` is the recipient's username; `body` is the message text.
    pub async fn message(&self, world: &str, to: &str, body: &str) -> Result<Value, ApiFailure> {
        self.post_json(
            &format!("/api/w/{world}/message"),
            serde_json::json!({"to": to, "body": body}),
        )
        .await
    }
}

// ---------------------------------------------------------------------------
// Internal: response parser
// ---------------------------------------------------------------------------

async fn parse_response<T: serde::de::DeserializeOwned>(
    resp: reqwest::Response,
) -> Result<T, ApiFailure> {
    let status = resp.status().as_u16();
    let text = resp
        .text()
        .await
        .map_err(|e| ApiFailure::Http(e.to_string()))?;

    if (200u16..300).contains(&status) {
        serde_json::from_str::<T>(&text)
            .map_err(|e| ApiFailure::BadBody(format!("{e} — body: {text}")))
    } else {
        // Attempt to parse the standard `{error, reason[, retry_after_secs]}` shape.
        // `retry_after_secs` is optional — only 429 responses include it.
        #[derive(serde::Deserialize)]
        struct ErrorBody {
            error: String,
            reason: String,
            #[serde(default)]
            retry_after_secs: Option<u64>,
        }
        match serde_json::from_str::<ErrorBody>(&text) {
            Ok(e) => Err(ApiFailure::Api {
                status,
                code: e.error,
                reason: e.reason,
                retry_after_secs: e.retry_after_secs,
            }),
            Err(_) => Err(ApiFailure::BadBody(format!("HTTP {status}: {text}"))),
        }
    }
}
