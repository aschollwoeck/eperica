//! `eperica-bots` — the Eperica bot-runner library.
//!
//! Client-side purity discipline (mirroring the server's P3): every decision
//! is a pure function in the lib (`policy`, `persona`); all I/O is confined to
//! `client`, `executor`, and `runner`. The T1 foundation modules:
//!
//! - [`digest`] — serde DTOs for the agent-API wire shapes (defensive,
//!   `#[serde(default)]` everywhere so additive server changes never break
//!   parsing).
//! - [`client`] — thin `reqwest`-backed `ApiClient`; no retry or backoff
//!   logic (that belongs in the T3 executor).
//! - [`manifest`] — load and validate the key manifest produced by slice 120.
#![forbid(unsafe_code)]

pub mod client;
pub mod digest;
pub mod manifest;
