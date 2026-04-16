use std::sync::Arc;

use sqlx::postgres::PgPoolOptions;
use tracing::{error, instrument};

use crate::{
    adapters::{DiscordAdapter, HttpAdapter},
    config::Config,
    infrastructure::{PostgresOrderRepository, RedisQueueRepository},
};

pub mod adapters;
pub mod config;
pub mod domain;
pub mod infrastructure;

pub struct VaffelBot {
    config: Config,
}

impl VaffelBot {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    #[instrument(skip(self))]
    pub async fn run(self) -> anyhow::Result<()> {
        let redis =
            redis::Client::open(self.config.redis_url.clone()).expect("Failed to connect to Redis");
        let pg_pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&self.config.database_url)
            .await
            .expect("Failed to connect to PostgreSQL");

        sqlx::migrate!().run(&pg_pool).await?;

        let queue: Arc<dyn domain::QueueRepository> = Arc::new(RedisQueueRepository::new(redis));

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
                error!(error = ?why, "HTTP server error");
            }
        });

        let bot_handle = tokio::spawn(async move {
            if let Err(why) = discord_adapter.start().await {
                error!(error = ?why, "Discord bot error");
            }
        });

        let _ = tokio::try_join!(axum_handle, bot_handle);

        Ok(())
    }
}
