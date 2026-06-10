# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Eperica is a from-scratch, **faithful Travian-style competitive strategy MMO** (medieval setting),
built **spec-driven** and **performance-first** (sub-second timing is gameplay). The design is
complete; implementation begins at slice 001. There is **no application code yet** — the next step is
scaffolding the Rust workspace.

## Read the specs first — they are the source of truth

`specs/` governs everything; **code conforms to the specs, never the reverse.** Start here:

- **`specs/README.md`** — THE process: how spec→plan→tasks→code works, and the execution/review
  workflow (§11). Read this to understand how work is done here.
- **`specs/constitution.md`** — 11 non-negotiable principles (P1–P11). Internalize them.
- **`specs/game-design.md`** — the game mechanics (the "what").
- **`specs/roles.md`** — user roles & permissions; every spec must address each applicable role.
- **`specs/roadmap.md`** — dependency-ordered build order (slices 001 → end-game).
- **`specs/social-and-meta-features.md`** — app-layer features (chat, profiles, UX) — not sim rules.
- **`specs/features/NNN-slug/{spec,plan,tasks}.md`** — the active slice. Currently: `001-foundation`.

If behavior must change, **update the spec first**, then the code.

## Stack (decided in `specs/features/001-foundation/plan.md`)

**Rust · Axum · Askama · SQLx · PostgreSQL.** A Cargo workspace under `crates/`
(`domain` / `application` / `infrastructure` / `web`) whose dependency direction **enforces the pure
domain rule (P3)** — `domain` cannot import I/O. Postgres runs via Docker in dev.

## Commands

Filled in as scaffolding lands (slice 001, tasks T1–T3). Expected:

- `cargo build` · `cargo test` · `cargo run -p web`
- `cargo fmt --check` · `cargo clippy --all-targets --all-features -- -D warnings`
- Postgres (dev): run a container via Docker; apply migrations with `sqlx migrate run`.

> Toolchain note: requires a **modern Rust** (`rustup update stable`) and the **`gh` CLI** for the
> per-slice PR workflow.

## How work is done (operating model — see specs/README.md §11)

- **Serial, task-by-task** from the active slice's `tasks.md`; **test-first** for the domain.
- **Gates every task:** `cargo fmt`, `clippy -D warnings`, `cargo test`, plus the latency budget (P11).
- **Acceptance = the `eperica-reviewer` agent** (`.claude/agents/eperica-reviewer.md`), not human
  review. Fix findings and re-review until the verdict is APPROVE.
- **Version control:** branch per slice (`feature/NNN-slug`), commit per task, **GitHub PR per slice**,
  merge to `main` when the slice is Verified.

## Constraints to internalize (from the constitution)

- **P1** — lazy/event-driven time: never tick all entities; store `value + lastUpdated + rate`, compute
  on read; model discrete outcomes as due-timestamped events.
- **P3** — all game rules in the pure `domain` crate (no I/O).
- **P4** — server-authoritative; never trust the client.
- **P7** — game speed is configurable; no hardcoded wall-clock durations.
- **P11** — performance & millisecond timing precision are first-class, within correctness (P2/P4/P6).
