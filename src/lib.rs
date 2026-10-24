pub(crate) mod adapters;
pub(crate) mod config;
pub(crate) mod domain;
pub(crate) mod infrastructure;

pub use config::Config;

use std::sync::Arc;

use sqlx::{
    postgres::PgPoolOptions,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
};

use crate::{
    adapters::{DiscordAdapter, HttpAdapter},
    infrastructure::{PostgresOrderRepository, RedisQueueRepository},
};

pub struct VaffelBot {
    config: Config,
}

impl VaffelBot {
    #[must_use]
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    /// Start the HTTP server and Discord bot after initializing storage.
    ///
    /// # Errors
    /// Returns an error if Redis configuration, Redis connection setup, the
    /// SQLite connection, or database migrations fail.
    ///
    /// # Panics
    /// Panics if the connection to PostgreSQL fails.
    #[tracing::instrument(skip(self))]
    pub async fn run(self) -> anyhow::Result<()> {
        let redis = redis::Client::open(self.config.redis_url.clone())?;
        let pg_pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&self.config.postgres_url)
            .await
            .expect("Failed to connect to PostgreSQL");

        sqlx::migrate!("./migrations/postgres")
            .run(&pg_pool)
            .await?;

        if let Some(parent) = std::path::Path::new(&self.config.sqlite_path).parent() {
            std::fs::create_dir_all(parent)?;
        }
        let sqlite_options = SqliteConnectOptions::new()
            .filename(&self.config.sqlite_path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal);
        let sqlite_pool = SqlitePoolOptions::new()
            .connect_with(sqlite_options)
            .await?;

        sqlx::migrate!("./migrations/sqlite")
            .run(&sqlite_pool)
            .await?;

        let queue: Arc<dyn domain::QueueRepository> =
            Arc::new(RedisQueueRepository::new(redis).await?);

        let orders: Arc<dyn domain::OrderRepository> =
            Arc::new(PostgresOrderRepository::new(pg_pool));

        let discord_adapter = DiscordAdapter::new(
            self.config.discord_token.clone(),
            queue.clone(),
            orders.clone(),
            self.config.github_token.clone(),
        );

        let http_adapter = HttpAdapter::new(queue.clone(), orders.clone());
        let axum_handle = tokio::spawn(async move {
            if let Err(why) = http_adapter.start().await {
                tracing::error!(error = ?why, "HTTP server error");
            }
        });

        let bot_handle = tokio::spawn(async move {
            if let Err(why) = discord_adapter.start().await {
                tracing::error!(error = ?why, "Discord bot error");
            }
        });

        let _ = tokio::try_join!(axum_handle, bot_handle);

        Ok(())
    }
}
