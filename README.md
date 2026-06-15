# Eperica

A from-scratch, **faithful Travian-style competitive strategy MMO** (medieval setting), built
**spec-driven** and **performance-first** — sub-second timing is gameplay.

Players grow villages, research and train tribe-specific armies, raid and conquer across a toroidal
world map, form alliances and wage diplomacy, race for end-game artifacts and the Wonder of the World,
and talk it all over through messages, an alliance forum, and live notifications.

## Stack

**Rust · Axum · Askama · SQLx · PostgreSQL**, as a Cargo workspace whose dependency direction enforces a
pure game core:

| Crate | Role |
|-------|------|
| `crates/domain` | Pure game rules — **no I/O** (entities, combat, economy, …). |
| `crates/application` | Use-cases + ports (traits) the infrastructure implements. |
| `crates/infrastructure` | SQLx/Postgres repositories, config, the scheduler, live bus. |
| `crates/web` | Axum HTTP server, Askama templates, handlers. |
| `crates/perf` | Large-world seeder + measurement tool (`eperica-perf`). |

The design is governed by [`specs/`](specs/) — **code conforms to the specs, never the reverse**. Start
with [`specs/README.md`](specs/README.md) (the process), [`specs/constitution.md`](specs/constitution.md)
(11 non-negotiable principles), and [`specs/roadmap.md`](specs/roadmap.md) (the build order). Player-facing
docs live in [`docs/manual/`](docs/manual/) and design records in [`docs/architecture/`](docs/architecture/).

## Quickstart (local development)

**1. Start PostgreSQL** (Docker):

```bash
docker run -d --name eperica-pg \
  -e POSTGRES_USER=eperica -e POSTGRES_PASSWORD=eperica -e POSTGRES_DB=eperica \
  -p 5432:5432 postgres:16
```

**2. Configure** — copy the sample env and adjust as needed:

```bash
cp .env.example .env
```

**3. Run** — migrations apply automatically on startup; the world is created on first run:

```bash
cargo run -p eperica-web      # serves http://127.0.0.1:8080
```

Register an account at `/register` and you're playing.

## Common commands

```bash
cargo build --workspace
cargo test --workspace        # DB-backed tests skip automatically without DATABASE_URL
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
```

> **Toolchain note:** in some shells `cargo` is shadowed by the rustup proxy. If a build picks up a stray
> toolchain pin, force stable: `CARGO="$(rustup which cargo)"; export RUSTUP_TOOLCHAIN=stable`.

## Configuration

All configuration is via environment variables (see [`.env.example`](.env.example) for the full, annotated
list). The essentials:

| Variable | Required | Default | Notes |
|----------|----------|---------|-------|
| `DATABASE_URL` | **yes** | — | Postgres connection string. |
| `SESSION_SECRET` | **in production** | ephemeral | Cookie key, ≥ 64 bytes. If unset, sessions don't survive a restart and won't work across instances. |
| `WORLD_SPEED` / `WORLD_RADIUS` | no | `1` / `200` | Game speed + world size (P7). |
| `BIND_ADDR` | no | `127.0.0.1:8080` | Listen address. |
| `TRUST_PROXY` | no | `false` | Trust `X-Forwarded-For`/`X-Real-IP` — **only** behind a trusted reverse proxy. |
| `REQUIRE_EMAIL_CONFIRMATION` | no | `false` | Gate first login on email confirmation. |
| `MODERATORS` | no | — | Comma-separated usernames granted the Moderator role at startup. |
| `RUST_LOG` | no | `info` | `tracing` filter. |

## Deployment

The server is **stateless** — Postgres is the single source of truth — so it scales horizontally behind a
load balancer (set a shared `SESSION_SECRET` and run the same image N times). Migrations are embedded in the
binary and applied on startup; the background scheduler and the live `LISTEN/NOTIFY` listeners run in every
instance and coordinate through the database.

A multi-stage [`Dockerfile`](Dockerfile) is provided:

```bash
docker build -t eperica .
docker run --rm -p 8080:8080 \
  -e DATABASE_URL=postgres://eperica:eperica@host.docker.internal:5432/eperica \
  -e SESSION_SECRET=$(openssl rand -hex 48) \
  -e BIND_ADDR=0.0.0.0:8080 \
  -e TRUST_PROXY=true \
  eperica
```

The image runs from `/app`, where the runtime static assets (`crates/web/static`) live alongside the
binary; HTML templates are compiled in.

## License

See the repository for licensing.
