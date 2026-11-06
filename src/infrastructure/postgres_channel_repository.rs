use sqlx::PgPool;

use crate::domain::ChannelRepository;

pub struct PostgresChannelRepository {
    pool: PgPool,
}

impl PostgresChannelRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl ChannelRepository for PostgresChannelRepository {
    #[tracing::instrument(skip(self), fields(guild_id, channel_id))]
    async fn toggle(&self, guild_id: &str, channel_id: &str) -> anyhow::Result<bool> {
        let mut tx = self.pool.begin().await?;

        let removed = sqlx::query!(
            "DELETE FROM allowed_channels WHERE guild_id = $1 AND channel_id = $2",
            guild_id,
            channel_id
        )
        .execute(&mut *tx)
        .await?
        .rows_affected();

        let enabled = if removed == 0 {
            sqlx::query!(
                "INSERT INTO allowed_channels (guild_id, channel_id) VALUES ($1, $2) \
                 ON CONFLICT DO NOTHING",
                guild_id,
                channel_id
            )
            .execute(&mut *tx)
            .await?;
            true
        } else {
            false
        };

        tx.commit().await.map_err(|e| {
            tracing::error!(guild_id, channel_id, error = ?e, "Failed to toggle channel");
            e
        })?;

        tracing::info!(guild_id, channel_id, enabled, "Toggled channel");
        Ok(enabled)
    }

    #[tracing::instrument(skip(self), fields(guild_id))]
    async fn list(&self, guild_id: &str) -> anyhow::Result<Vec<String>> {
        let channels = sqlx::query_scalar!(
            "SELECT channel_id FROM allowed_channels WHERE guild_id = $1 ORDER BY created_at",
            guild_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            tracing::error!(guild_id, error = ?e, "Failed to list allowed channels");
            e
        })?;

        tracing::debug!(
            guild_id,
            count = channels.len(),
            "Retrieved allowed channels"
        );
        Ok(channels)
    }
}
