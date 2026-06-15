# ---- Build stage ------------------------------------------------------------
# Edition 2024 needs a recent stable Rust. Migrations + Askama templates are compiled in, and the project
# uses runtime-checked SQLx queries (no `query!` macros), so the build needs no database.
# Plain RUN (no BuildKit cache mounts) so this builds with the classic builder too; with BuildKit enabled
# the layer cache still applies.
FROM rust:1-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release -p eperica-web \
 && cp target/release/eperica-web /usr/local/bin/eperica-web

# ---- Runtime stage ----------------------------------------------------------
# SQLx is built with tls-rustls, so no OpenSSL is required at runtime; ca-certificates covers TLS to a
# managed Postgres if used.
FROM debian:bookworm-slim AS runtime
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 eperica

WORKDIR /app
# The binary serves static assets from the relative path `crates/web/static` (templates are compiled in).
COPY --from=build /usr/local/bin/eperica-web /app/eperica-web
COPY crates/web/static /app/crates/web/static

USER eperica
# Listen on all interfaces inside the container by default; override with BIND_ADDR.
ENV BIND_ADDR=0.0.0.0:8080
EXPOSE 8080
# DATABASE_URL (and SESSION_SECRET in production) must be supplied at runtime.
CMD ["/app/eperica-web"]
