# Installing & running an Eperica server

The server is a single Rust binary (`eperica-web`) plus a static-assets directory, backed by
PostgreSQL. It terminates no TLS itself and writes logs to stdout — the intended deployment is
behind a reverse proxy, supervised by systemd or a container runtime.

## Requirements

| | Minimum | Notes |
|---|---|---|
| **PostgreSQL** | 13+ (16 recommended) | 13+ ships `gen_random_uuid()` built-in; no extensions required. All schema is standard types. |
| **Rust (build only)** | 1.85+ stable | Edition 2024. Not needed at runtime — deploy the release binary. |
| **CPU/RAM** | 2 cores / 2 GB serve a 10k-player world comfortably | The scale pass (ADR 0025) measured, on a dev laptop: village reads ~3 ms, map viewport ~18 ms, the population board ~110 ms, ~750 req/s HTTP at 32-way concurrency. Pure game logic is nanosecond-scale — the database is the bottleneck. Re-measure on target hardware with `eperica-perf`. |
| **Disk** | Postgres-dominated | The app itself is a ~single binary + static assets. |

Horizontal scale (validated design, ADR 0025): the web tier is stateless (shared `SESSION_SECRET`
required across instances), and the per-world scheduler uses `FOR UPDATE SKIP LOCKED` claims, so
multiple instances never double-apply events. The database is the scaling bottleneck.

## Build

```bash
cargo build --release -p eperica-web
```

The binary expects the static assets at the **relative path** `crates/web/static` from its working
directory. Deploy the binary together with that directory and set the working directory
accordingly (the provided `Dockerfile` copies it to `/app/crates/web/static` with `WORKDIR /app`).

There is also a `Dockerfile` (Rust build stage on `rust:1-bookworm`, slim runtime with
`ca-certificates` only — SQLx uses rustls, no OpenSSL at runtime).

## Database

```bash
docker run -d --name eperica-pg \
  -e POSTGRES_USER=eperica -e POSTGRES_PASSWORD=eperica -e POSTGRES_DB=eperica \
  -p 5432:5432 postgres:16
```

Migrations are **embedded in the binary and run automatically at every startup** — there is no
separate migrate step. (Developer note: a newly added migration file needs
`touch crates/infrastructure/src/db.rs` before rebuild, or `sqlx::migrate!` serves the stale
embedded set.)

## Configuration (environment / `.env`)

A `.env` file in the working directory is loaded automatically. Copy `.env.example` and edit.

| Variable | Default | Meaning |
|---|---|---|
| `DATABASE_URL` | **required** | `postgres://user:pass@host:5432/dbname` |
| `BIND_ADDR` | `127.0.0.1:8080` | Listen address; use `0.0.0.0:8080` behind a proxy/container |
| `SESSION_SECRET` | *ephemeral* | **Set this in production** (≥ 64 bytes). Missing/short ⇒ a per-process key: sessions die on restart and break across instances. Rotation logs everyone out (data is unaffected). |
| `WORLD_SPEED` | `1` | Home world speed multiplier (created on first boot) |
| `WORLD_RADIUS` | `200` | Home world map radius |
| `ARTIFACT_RELEASE_DELAY_SECS` | `7776000` (90 d) | Home-world artifact release offset |
| `WONDER_RELEASE_DELAY_SECS` | `10368000` (120 d) | Home-world Wonder release offset (must exceed the artifact offset) |
| `TRUST_PROXY` | `false` | Trust `X-Forwarded-For`/`X-Real-IP` for client IPs. **Only** enable behind a proxy you control — otherwise rate-limit/detection keying becomes spoofable. |
| `REQUIRE_EMAIL_CONFIRMATION` | `false` | ⚠ Enforcement stub: registration demands confirmation and blocks login, but **no mailer exists** — no email is sent. Leave off unless you wire delivery yourself. |
| `MODERATORS` | – | Comma-separated usernames granted the Moderator role at startup (idempotent) |
| `ADMINS` | – | Comma-separated usernames granted the Administrator role at startup (idempotent) |
| `RUST_LOG` | `info` | tracing filter, e.g. `eperica_web=debug,info` |

## What happens at boot

1. Config parsed (fatal without `DATABASE_URL`); pool opened (max 10 connections, 5 s acquire).
2. Migrations applied.
3. The **home world** row is upserted from `WORLD_SPEED`/`WORLD_RADIUS`/release offsets (idempotent;
   further worlds are created in the admin console and start their schedulers live).
4. `MODERATORS`/`ADMINS` roles granted.
5. Chat + notification LISTEN hubs start; a **scheduler task per world** starts (lazy, event-driven —
   no global tick).
6. HTTP serves on `BIND_ADDR`. Ctrl-C/SIGTERM drains in-flight scheduler work before exit.

## Reverse proxy & TLS

The binary speaks plain HTTP. Terminate TLS at nginx/Caddy/Traefik and proxy to `BIND_ADDR`; set
`TRUST_PROXY=1` so rate limiting keys on real client IPs.

⚠ Session cookies are encrypted (AES-GCM via the private cookie jar) and `HttpOnly`/`SameSite=Lax`,
but the **`Secure` attribute is not set by the application** — enforce HTTPS-only at the proxy
(redirect 80→443 and consider HSTS) so cookies never travel in clear.

### systemd example

```ini
[Unit]
Description=Eperica game server
After=network-online.target postgresql.service

[Service]
User=eperica
WorkingDirectory=/opt/eperica            # contains crates/web/static + .env
ExecStart=/opt/eperica/eperica-web
Restart=on-failure
Environment=RUST_LOG=info

[Install]
WantedBy=multi-user.target
```

## Logs, backup, maintenance

- **Logs:** stdout only (structured tracing). Capture via journald/container runtime; there is no
  file sink or rotation in the app.
- **Backup:** no built-in tooling — all state lives in Postgres, so standard `pg_dump`/PITR
  practice applies. The binary is stateless; restoring the database restores the game.
- **Upgrades:** stop the service, deploy the new binary (+ static dir), start — migrations apply
  themselves. Schedulers catch up on any backlog deterministically (due-event design, ADR 0002).
- **Sessions across restarts:** guaranteed only with a persistent `SESSION_SECRET`.

## First steps after install

1. Register your operator account in the browser, put its username in `ADMINS`, restart (or grant
   via an existing admin at `/admin`).
2. Visit `/admin` — create worlds, choose presets/speeds, seed AI fleets. See
   [administration.md](administration.md).
3. For AI opponents, see [bots.md](bots.md).
