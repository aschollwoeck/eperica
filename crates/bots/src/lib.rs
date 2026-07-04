//! `eperica-bots` — the Eperica bot-runner library.
//!
//! Client-side purity discipline (mirroring the server's P3): every decision
//! is a pure function in the lib (`policy`, `persona`); all I/O is confined to
//! `client`, `executor`, and `runner`.
//!
//! Modules:
//!
//! - [`digest`] — serde DTOs for the agent-API wire shapes (defensive,
//!   `#[serde(default)]` everywhere so additive server changes never break
//!   parsing).
//! - [`client`] — thin `reqwest`-backed `ApiClient`; no retry or backoff
//!   logic (that belongs in the executor).
//! - [`manifest`] — load and validate the key manifest produced by slice 120.
//! - [`persona`] — deterministic bot persona derived from username (FNV-1a).
//! - [`policy`] — pure reflex doctrine: digest + persona → `Vec<Intent>`.
//! - [`executor`] — intent → API call; `classify` + `execute_intents`.
//! - [`runner`] — fleet scheduler: semaphore cap, jitter, Ctrl-C drain.
#![forbid(unsafe_code)]

pub mod client;
pub mod digest;
pub mod executor;
pub mod manifest;
pub mod persona;
pub mod policy;
pub mod runner;
pub mod strategy;
