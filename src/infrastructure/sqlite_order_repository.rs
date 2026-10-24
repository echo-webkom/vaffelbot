use sqlx::{QueryBuilder, Sqlite, SqlitePool};

use crate::domain::{DailyStats, Highscore, OrderRepository};

pub struct SqliteOrderRepository {
    pool: SqlitePool,
}

impl SqliteOrderRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl OrderRepository for SqliteOrderRepository {
    #[tracing::instrument(skip(self), fields(guild_id))]
    async fn highscore(&self, guild_id: &str) -> anyhow::Result<Option<Highscore>> {
        // Ties go to the earliest day the record was reached.
        let record: Option<(chrono::NaiveDate, i64)> = sqlx::query_as(
            "SELECT date(fulfilled_at) AS day, COUNT(*) AS total_orders FROM orders \
             WHERE guild_id = ? \
             GROUP BY day \
             ORDER BY total_orders DESC, day ASC \
             LIMIT 1",
        )
        .bind(guild_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            tracing::error!(guild_id, error = ?e, "Failed to fetch waffle highscore");
            e
        })?;

        Ok(record.map(|(date, total_orders)| Highscore { date, total_orders }))
    }

    #[tracing::instrument(skip(self), fields(count = discord_user_ids.len(), guild_id))]
    async fn record_orders(
        &self,
        discord_user_ids: &[String],
        guild_id: &str,
    ) -> anyhow::Result<()> {
        if discord_user_ids.is_empty() {
            tracing::debug!("No orders to record");
            return Ok(());
        }

        tracing::debug!(
            count = discord_user_ids.len(),
            guild_id,
            "Recording orders in batch"
        );

        let mut query: QueryBuilder<Sqlite> =
            QueryBuilder::new("INSERT INTO orders (discord_user_id, guild_id) ");
        query.push_values(discord_user_ids, |mut row, user_id| {
            row.push_bind(user_id).push_bind(guild_id);
        });

        query.build().execute(&self.pool).await.map_err(|e| {
            tracing::error!(
                count = discord_user_ids.len(),
                guild_id,
                error = ?e,
                "Failed to record orders in database"
            );
            e
        })?;

        tracing::info!(
            count = discord_user_ids.len(),
            guild_id,
            "Orders recorded successfully"
        );
        Ok(())
    }

    #[tracing::instrument(skip(self), fields(guild_id))]
    async fn daily_stats(&self, guild_id: &str) -> anyhow::Result<DailyStats> {
        tracing::debug!(guild_id, "Fetching daily stats");

        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM orders WHERE date(fulfilled_at) = date('now') AND guild_id = ?",
        )
        .bind(guild_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            tracing::error!(guild_id, error = ?e, "Failed to fetch total order count");
            e
        })?;

        let top_users: Vec<(String, i64)> = sqlx::query_as(
            "SELECT discord_user_id, COUNT(*) AS count FROM orders \
             WHERE date(fulfilled_at) = date('now') AND guild_id = ? \
             GROUP BY discord_user_id \
             ORDER BY count DESC \
             LIMIT 3",
        )
        .bind(guild_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            tracing::error!(guild_id, error = ?e, "Failed to fetch top users");
            e
        })?;

        tracing::info!(
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

#[cfg(test)]
mod tests {
    use sqlx::sqlite::SqlitePoolOptions;

    use super::*;

    const TEST_GUILD: &str = "test-guild";

    async fn setup() -> SqliteOrderRepository {
        // Each connection to `:memory:` gets its own database, so pin the pool to one.
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations/sqlite")
            .run(&pool)
            .await
            .unwrap();
        SqliteOrderRepository::new(pool)
    }

    fn ids(ids: &[&str]) -> Vec<String> {
        ids.iter().map(ToString::to_string).collect()
    }

    #[tokio::test]
    async fn test_daily_stats() {
        let orders = setup().await;

        orders
            .record_orders(
                &ids(&["foo", "bar", "foo", "baz", "foo", "bar", "qux"]),
                TEST_GUILD,
            )
            .await
            .unwrap();
        orders
            .record_orders(&ids(&["foo"]), "other-guild")
            .await
            .unwrap();

        let stats = orders.daily_stats(TEST_GUILD).await.unwrap();
        assert_eq!(stats.total_orders, 7);
        assert_eq!(stats.top_users.len(), 3);
        assert_eq!(stats.top_users[0], ("foo".to_string(), 3));
        assert_eq!(stats.top_users[1], ("bar".to_string(), 2));
    }

    #[tokio::test]
    async fn test_daily_stats_ignores_previous_days() {
        let orders = setup().await;

        sqlx::query(
            "INSERT INTO orders (discord_user_id, guild_id, fulfilled_at) \
             VALUES ('foo', ?, '2025-01-01T12:00:00.000Z')",
        )
        .bind(TEST_GUILD)
        .execute(&orders.pool)
        .await
        .unwrap();

        let stats = orders.daily_stats(TEST_GUILD).await.unwrap();
        assert_eq!(stats.total_orders, 0);
        assert_eq!(stats.top_users, Vec::new());
    }

    #[tokio::test]
    async fn test_record_empty() {
        let orders = setup().await;
        orders.record_orders(&[], TEST_GUILD).await.unwrap();
        assert_eq!(
            orders.daily_stats(TEST_GUILD).await.unwrap().total_orders,
            0
        );
    }

    #[tokio::test]
    async fn test_highscore() {
        let orders = setup().await;

        assert!(orders.highscore(TEST_GUILD).await.unwrap().is_none());

        // Two days tied at 2 orders, one day with 1; the earliest tied day wins.
        for day in [
            "2025-01-02",
            "2025-01-02",
            "2025-01-01",
            "2025-01-03",
            "2025-01-03",
        ] {
            sqlx::query(
                "INSERT INTO orders (discord_user_id, guild_id, fulfilled_at) \
                 VALUES ('foo', ?, ? || 'T12:00:00.000Z')",
            )
            .bind(TEST_GUILD)
            .bind(day)
            .execute(&orders.pool)
            .await
            .unwrap();
        }

        let highscore = orders.highscore(TEST_GUILD).await.unwrap().unwrap();
        assert_eq!(
            highscore.date,
            chrono::NaiveDate::from_ymd_opt(2025, 1, 2).unwrap()
        );
        assert_eq!(highscore.total_orders, 2);
    }
}
