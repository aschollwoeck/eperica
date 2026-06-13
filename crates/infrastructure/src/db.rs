//! Database access: the PostgreSQL connection pool and migration runner.

use sqlx::PgPool;
use sqlx::migrate::{MigrateError, Migrator};
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;

/// Migrations embedded at compile time from the workspace `migrations/` directory.
/// (Re-embed marker: 0021_culture_backfill.)
pub static MIGRATOR: Migrator = sqlx::migrate!("../../migrations");

/// Create a connection pool for the given database URL.
///
/// # Errors
/// Returns [`sqlx::Error`] if the pool cannot be established (bad URL, server unreachable, …).
pub async fn create_pool(database_url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(5))
        .connect(database_url)
        .await
}

/// Apply all pending migrations to the database.
///
/// # Errors
/// Returns [`MigrateError`] if a migration fails to apply.
pub async fn run_migrations(pool: &PgPool) -> Result<(), MigrateError> {
    MIGRATOR.run(pool).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Connectivity smoke test: connect, run migrations, and round-trip a trivial query.
    /// Skips when `DATABASE_URL` is not set so `cargo test` stays green without a database.
    #[tokio::test]
    async fn connects_and_runs_migrations() {
        let _ = dotenvy::dotenv();
        let Ok(url) = std::env::var("DATABASE_URL") else {
            eprintln!("skipping connects_and_runs_migrations: DATABASE_URL not set");
            return;
        };

        let pool = create_pool(&url).await.expect("connect to postgres");
        run_migrations(&pool).await.expect("run migrations");

        let (one,): (i32,) = sqlx::query_as("SELECT 1")
            .fetch_one(&pool)
            .await
            .expect("round-trip SELECT 1");
        assert_eq!(one, 1);
    }
}
