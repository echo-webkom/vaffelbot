// Double must use because of async_trait and mockall.
// Might be fixed in the future.
#![allow(clippy::double_must_use)]

#[cfg_attr(test, mockall::automock)]
#[async_trait::async_trait]
pub trait OrderRepository: Send + Sync {
    async fn record_orders(
        &self,
        discord_user_ids: &[String],
        guild_id: &str,
    ) -> anyhow::Result<()>;
    async fn daily_stats(&self, guild_id: &str) -> anyhow::Result<DailyStats>;
    async fn highscore(&self, guild_id: &str) -> anyhow::Result<Option<Highscore>>;
}

pub struct Highscore {
    pub date: chrono::NaiveDate,
    pub total_orders: i64,
}

pub struct DailyStats {
    pub total_orders: i64,
    /// (`discord_user_id`, count)
    pub top_users: Vec<(String, i64)>,
}
