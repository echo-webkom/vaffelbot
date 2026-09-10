use sqlx::PgPool;
use tracing::{debug, error, info, instrument};

use crate::domain::{DailyStats, Highscore, OrderRepository};

pub struct PostgresOrderRepository {
    pool: PgPool,
}

impl PostgresOrderRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl OrderRepository for PostgresOrderRepository {
    #[instrument(skip(self), fields(guild_id))]
    async fn highscore(&self, guild_id: &str) -> anyhow::Result<Option<Highscore>> {
        // Ties go to the earliest day the record was reached.
        let record = sqlx::query!(
            "SELECT fulfilled_at::date AS \"day!\", COUNT(*) AS \"total_orders!\" FROM orders \
             WHERE guild_id = $1 \
             GROUP BY fulfilled_at::date \
             ORDER BY COUNT(*) DESC, fulfilled_at::date ASC \
             LIMIT 1",
            guild_id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            error!(guild_id, error = ?e, "Failed to fetch waffle highscore");
            e
        })?;

        Ok(record.map(|row| Highscore {
            date: row.day,
            total_orders: row.total_orders,
        }))
    }

    #[instrument(skip(self), fields(count = discord_user_ids.len(), guild_id))]
    async fn record_orders(&self, discord_user_ids: &[&str], guild_id: &str) -> anyhow::Result<()> {
        if discord_user_ids.is_empty() {
            debug!("No orders to record");
            return Ok(());
        }

        debug!(
            count = discord_user_ids.len(),
            guild_id, "Recording orders in batch"
        );

        let discord_user_ids_vec: Vec<String> =
            discord_user_ids.iter().map(|&s| s.to_string()).collect();
        let guild_ids: Vec<String> = vec![guild_id.to_string(); discord_user_ids.len()];

        sqlx::query!(
            "INSERT INTO orders (discord_user_id, guild_id) SELECT * FROM UNNEST($1::text[], $2::text[])",
            &discord_user_ids_vec[..],
            &guild_ids[..]
        )
        .execute(&self.pool)
        .await
        .map_err(|e| {
            error!(
                count = discord_user_ids.len(),
                guild_id,
                error = ?e,
                "Failed to record orders in database"
            );
            e
        })?;

        info!(
            count = discord_user_ids.len(),
            guild_id, "Orders recorded successfully"
        );
        Ok(())
    }

    #[instrument(skip(self), fields(guild_id))]
    async fn daily_stats(&self, guild_id: &str) -> anyhow::Result<DailyStats> {
        debug!(guild_id, "Fetching daily stats");

        let total = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM orders WHERE fulfilled_at::date = CURRENT_DATE AND guild_id = $1",
            guild_id
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            error!(guild_id, error = ?e, "Failed to fetch total order count");
            e
        })?
        .unwrap_or(0);

        let top_users: Vec<(String, i64)> = sqlx::query!(
            "SELECT discord_user_id, COUNT(*) as count FROM orders \
             WHERE fulfilled_at::date = CURRENT_DATE AND guild_id = $1 \
             GROUP BY discord_user_id \
             ORDER BY count DESC \
             LIMIT 3",
            guild_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            error!(guild_id, error = ?e, "Failed to fetch top users");
            e
        })?
        .into_iter()
        .map(|row| (row.discord_user_id, row.count.unwrap_or(0)))
        .collect();

        info!(
            guild_id,
            total_orders = total,
            top_users_count = top_users.len(),
            "Retrieved daily stats"
        );

        Ok(DailyStats {
            total_orders: total,
            top_users,
        })
    }
}
