use crate::config::DatabaseSettings;
use anyhow::{Context, Result};
use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use std::{env, io, time::Duration};

/// Type alias for our DB connection (SeaORM pool handle)
pub type DB = DatabaseConnection;

/// Build ConnectOptions from structured database settings.
fn connect_options_from_settings(settings: &DatabaseSettings) -> ConnectOptions {
    let mut opt = ConnectOptions::new(settings.url.clone());
    // Baseline defaults matching previous behaviour
    opt.max_connections(20)
        .min_connections(5)
        .connect_timeout(Duration::from_secs(8))
        .acquire_timeout(Duration::from_secs(8))
        .idle_timeout(Duration::from_secs(600))
        .sqlx_logging(false);

    if let Some(v) = settings.max_connections {
        opt.max_connections(v);
    }
    if let Some(v) = settings.min_connections {
        opt.min_connections(v);
    }
    if let Some(v) = settings.connect_timeout_secs {
        opt.connect_timeout(Duration::from_secs(v));
    }
    if let Some(v) = settings.acquire_timeout_secs {
        opt.acquire_timeout(Duration::from_secs(v));
    }
    if let Some(v) = settings.idle_timeout_secs {
        opt.idle_timeout(Duration::from_secs(v));
    }
    if let Some(v) = settings.sql_log {
        opt.sqlx_logging(v);
    }

    opt
}

/// Establish a connection pool using DATABASE_URL with sensible defaults.
///
/// Supported env vars:
/// - `DATABASE_URL` (required): e.g. postgres://user:pass@host:5432/db
/// - `DATABASE_MAX_CONNECTIONS` (u32)
/// - `DATABASE_MIN_CONNECTIONS` (u32)
/// - `DATABASE_CONNECT_TIMEOUT_SECS` (u64)
/// - `DATABASE_ACQUIRE_TIMEOUT_SECS` (u64)
/// - `DATABASE_IDLE_TIMEOUT_SECS` (u64)
/// - `DATABASE_SQL_LOG` (bool)
#[allow(dead_code)]
pub async fn connect() -> io::Result<DB> {
    let url = env::var("DATABASE_URL").map_err(|_| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "DATABASE_URL is not set. Example: postgres://user:pass@localhost:5432/ai_units",
        )
    })?;
    let settings = DatabaseSettings::default_from_url(url);
    connect_with_settings(&settings).await
}

/// Establish a connection pool from a provided URL.
#[allow(dead_code)]
pub async fn connect_from_url(url: &str) -> io::Result<DB> {
    let settings = DatabaseSettings::default_from_url(url.to_string());
    connect_with_settings(&settings).await
}

/// Establish a connection pool using explicit database settings.
pub async fn connect_with_settings(settings: &DatabaseSettings) -> io::Result<DB> {
    let opt = connect_options_from_settings(settings);
    let db = Database::connect(opt).await.map_err(|e| {
        io::Error::new(
            io::ErrorKind::Other,
            format!("Failed to connect to database at {}: {}", settings.url, e),
        )
    })?;

    ping(&db).await.map_err(|e| {
        io::Error::new(
            io::ErrorKind::Other,
            format!("Failed to ping database at {}: {}", settings.url, e),
        )
    })?;

    Ok(db)
}

/// Lightweight health check to verify the DB connection is alive.
pub async fn ping(db: &DB) -> Result<()> {
    // Using a very cheap query that works in Postgres
    use sea_orm::ConnectionTrait;
    db.execute(sea_orm::Statement::from_string(
        sea_orm::DatabaseBackend::Postgres,
        "SELECT 1",
    ))
    .await
    .context("DB ping failed")?;
    Ok(())
}
