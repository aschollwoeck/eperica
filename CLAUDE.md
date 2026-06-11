# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Eperica is a from-scratch, **faithful Travian-style competitive strategy MMO** (medieval setting),
built **spec-driven** and **performance-first** (sub-second timing is gameplay). The design is
complete and implementation proceeds slice by slice from the roadmap; the foundation, economy,
construction, tribes/units, training/upkeep, and world-map slices (001–006) are built — milestones
M1, M2, and the start of M3.

## Read the specs first — they are the source of truth

`specs/` governs everything; **code conforms to the specs, never the reverse.** Start here:

- **`specs/README.md`** — THE process: how spec→plan→tasks→code works, and the execution/review
  workflow (§11). Read this to understand how work is done here.
- **`specs/constitution.md`** — 11 non-negotiable principles (P1–P11). Internalize them.
- **`specs/game-design.md`** — the game mechanics (the "what").
- **`specs/roles.md`** — user roles & permissions; every spec must address each applicable role.
- **`specs/roadmap.md`** — dependency-ordered build order (slices 001 → end-game).
- **`specs/social-and-meta-features.md`** — app-layer features (chat, profiles, UX) — not sim rules.
- **`specs/features/NNN-slug/{spec,plan,tasks}.md`** — the active slice. Currently:
  `007-troop-movement`.

If behavior must change, **update the spec first**, then the code.

## Stack (decided in `specs/features/001-foundation/plan.md`)

**Rust · Axum · Askama · SQLx · PostgreSQL.** A Cargo workspace under `crates/`
(`domain` / `application` / `infrastructure` / `web`) whose dependency direction **enforces the pure
domain rule (P3)** — `domain` cannot import I/O. Postgres runs via Docker in dev.

## Commands

> **Environment gotcha:** in this shell `cargo` is shadowed by the rustup proxy and a dependency
> ships a stray toolchain pin. Call the real binary and force the toolchain:
> ```bash
> CARGO="$(rustup which cargo)"; export RUSTUP_TOOLCHAIN=stable
> ```

- **Build:** `"$CARGO" build --workspace`
- **Test:** `"$CARGO" test --workspace` — DB-backed tests skip automatically without `DATABASE_URL`.
- **Lint:** `"$CARGO" fmt --all -- --check` and `"$CARGO" clippy --all-targets --all-features -- -D warnings`
- **Run:** `"$CARGO" run -p eperica-web` — serves `http://127.0.0.1:8080`.

**Database (dev):** Postgres via Docker —
```bash
docker run -d --name eperica-pg -e POSTGRES_USER=eperica -e POSTGRES_PASSWORD=eperica \
  -e POSTGRES_DB=eperica -p 5432:5432 postgres:16
```
Configure via `.env` (copy `.env.example`): `DATABASE_URL`, `WORLD_SPEED`, `WORLD_RADIUS`, `RUST_LOG`,
and optionally `SESSION_SECRET` (≥64 bytes), `REQUIRE_EMAIL_CONFIRMATION`, `BIND_ADDR`.

> **Migration gotcha:** adding a *new* migration file does not force `sqlx::migrate!` to re-embed.
> Touch `crates/infrastructure/src/db.rs` (or `cargo clean -p eperica-infrastructure`) to pick it up.

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
