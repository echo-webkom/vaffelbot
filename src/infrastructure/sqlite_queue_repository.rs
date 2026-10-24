use std::{
    collections::{HashMap, HashSet},
    sync::RwLock,
};

use sqlx::SqlitePool;
use tokio::sync::broadcast;

use crate::domain::{QueueEntry, QueueEvent, QueueRepository};

pub struct SqliteQueueRepository {
    pool: SqlitePool,
    open_guilds: RwLock<HashSet<String>>,
    guild_senders: RwLock<HashMap<String, broadcast::Sender<QueueEvent>>>,
}

impl SqliteQueueRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            open_guilds: RwLock::new(HashSet::new()),
            guild_senders: RwLock::new(HashMap::new()),
        }
    }
}

#[async_trait::async_trait]
impl QueueRepository for SqliteQueueRepository {
    #[tracing::instrument(skip(self), fields(guild_id))]
    fn open(&self, guild_id: &str) {
        tracing::info!(guild_id, "Opening queue for guild");
        self.open_guilds
            .write()
            .unwrap()
            .insert(guild_id.to_string());
    }

    #[tracing::instrument(skip(self), fields(guild_id))]
    async fn close(&self, guild_id: &str) -> anyhow::Result<()> {
        tracing::info!(guild_id, "Closing queue for guild");
        self.open_guilds.write().unwrap().remove(guild_id);
        self.clear(guild_id).await
    }

    #[tracing::instrument(skip(self), fields(guild_id))]
    fn is_open(&self, guild_id: &str) -> bool {
        let is_open = self.open_guilds.read().unwrap().contains(guild_id);
        tracing::debug!(guild_id, is_open, "Checking if queue is open");
        is_open
    }

    #[tracing::instrument(skip(self), fields(guild_id, user_id))]
    async fn index_of(&self, guild_id: &str, user_id: &str) -> anyhow::Result<Option<usize>> {
        let position: Option<i64> = sqlx::query_scalar(
            "SELECT position FROM ( \
                 SELECT user_id, ROW_NUMBER() OVER (ORDER BY id) - 1 AS position \
                 FROM queue_entries WHERE guild_id = ? \
             ) \
             WHERE user_id = ? \
             ORDER BY position \
             LIMIT 1",
        )
        .bind(guild_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;
        let position = position.map(usize::try_from).transpose()?;

        tracing::debug!(guild_id, user_id, position = ?position, "Found user position in queue");
        Ok(position)
    }

    #[tracing::instrument(skip(self), fields(guild_id))]
    async fn size(&self, guild_id: &str) -> anyhow::Result<usize> {
        let size: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM queue_entries WHERE guild_id = ?")
            .bind(guild_id)
            .fetch_one(&self.pool)
            .await?;
        let size = usize::try_from(size)?;
        tracing::debug!(guild_id, size, "Retrieved queue size");
        Ok(size)
    }

    #[tracing::instrument(skip(self, entry), fields(guild_id, user_id = %entry.user_id))]
    async fn push(&self, guild_id: &str, entry: QueueEntry) -> anyhow::Result<usize> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO queue_entries (guild_id, user_id, display_name) VALUES (?, ?, ?) \
             ON CONFLICT (guild_id, user_id) DO NOTHING",
        )
        .bind(guild_id)
        .bind(&entry.user_id)
        .bind(&entry.display_name)
        .execute(&mut *tx)
        .await?;
        let new_size: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM queue_entries WHERE guild_id = ?")
                .bind(guild_id)
                .fetch_one(&mut *tx)
                .await?;
        tx.commit().await?;
        let new_size = usize::try_from(new_size)?;

        tracing::info!(guild_id, user_id = %entry.user_id, queue_size = new_size, "Added user to queue");
        self.broadcast_update(guild_id);
        Ok(new_size)
    }

    #[tracing::instrument(skip(self), fields(guild_id, n))]
    async fn pop_n(&self, guild_id: &str, n: u32) -> anyhow::Result<Vec<QueueEntry>> {
        if n == 0 {
            return Ok(vec![]);
        }

        // RETURNING does not guarantee row order, so sort by id afterwards.
        let mut rows: Vec<(i64, String, String)> = sqlx::query_as(
            "DELETE FROM queue_entries WHERE id IN ( \
                 SELECT id FROM queue_entries WHERE guild_id = ? ORDER BY id LIMIT ? \
             ) \
             RETURNING id, user_id, display_name",
        )
        .bind(guild_id)
        .bind(n)
        .fetch_all(&self.pool)
        .await?;
        rows.sort_unstable_by_key(|(id, _, _)| *id);

        let entries: Vec<QueueEntry> = rows
            .into_iter()
            .map(|(_, user_id, display_name)| QueueEntry::new(user_id, display_name))
            .collect();
        tracing::info!(guild_id, count = entries.len(), "Popped entries from queue");
        if !entries.is_empty() {
            self.broadcast_update(guild_id);
        }
        Ok(entries)
    }

    #[tracing::instrument(skip(self), fields(guild_id))]
    async fn list(&self, guild_id: &str) -> anyhow::Result<Vec<QueueEntry>> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT user_id, display_name FROM queue_entries WHERE guild_id = ? ORDER BY id",
        )
        .bind(guild_id)
        .fetch_all(&self.pool)
        .await?;
        let entries: Vec<QueueEntry> = rows
            .into_iter()
            .map(|(user_id, display_name)| QueueEntry::new(user_id, display_name))
            .collect();
        tracing::debug!(guild_id, count = entries.len(), "Retrieved queue list");
        Ok(entries)
    }

    #[tracing::instrument(skip(self), fields(guild_id))]
    async fn clear(&self, guild_id: &str) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM queue_entries WHERE guild_id = ?")
            .bind(guild_id)
            .execute(&self.pool)
            .await?;
        tracing::info!(guild_id, "Cleared queue");
        self.broadcast_update(guild_id);
        Ok(())
    }

    fn subscribe(&self, guild_id: &str) -> tokio::sync::broadcast::Receiver<QueueEvent> {
        {
            // Checks if there is already a sender for this guild, and if so, subscribes to it.
            // We keep this in a seperate block to drop the read lock before acquiring the write lock.
            let read = self.guild_senders.read().unwrap();
            if let Some(tx) = read.get(guild_id) {
                return tx.subscribe();
            }
        }
        // If there is no sender for this guild, we create one and subscribe to it.
        let mut write = self.guild_senders.write().unwrap();
        let tx = write
            .entry(guild_id.to_string())
            .or_insert_with(|| broadcast::channel(64).0);
        tx.subscribe()
    }
}

impl SqliteQueueRepository {
    fn broadcast_update(&self, guild_id: &str) {
        let read = self.guild_senders.read().unwrap();
        if let Some(tx) = read.get(guild_id) {
            let _ = tx.send(QueueEvent::Updated);
        }
    }
}

#[cfg(test)]
mod tests {
    use sqlx::sqlite::SqlitePoolOptions;

    use super::*;

    const TEST_GUILD: &str = "test-guild";

    async fn setup() -> SqliteQueueRepository {
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
        SqliteQueueRepository::new(pool)
    }

    fn entry(user_id: &str) -> QueueEntry {
        QueueEntry::new(user_id.to_string(), format!("{user_id} User"))
    }

    #[tokio::test]
    async fn test_list() {
        let queue = setup().await;

        queue.push(TEST_GUILD, entry("foo")).await.unwrap();
        queue.push(TEST_GUILD, entry("bar")).await.unwrap();
        queue.push("other-guild", entry("baz")).await.unwrap();

        let list = queue.list(TEST_GUILD).await.unwrap();
        assert_eq!(list, vec![entry("foo"), entry("bar")]);
    }

    #[tokio::test]
    async fn test_index_of() {
        let queue = setup().await;

        queue.push("other-guild", entry("baz")).await.unwrap();
        queue.push(TEST_GUILD, entry("foo")).await.unwrap();
        queue.push(TEST_GUILD, entry("bar")).await.unwrap();

        assert_eq!(queue.index_of(TEST_GUILD, "foo").await.unwrap(), Some(0));
        assert_eq!(queue.index_of(TEST_GUILD, "bar").await.unwrap(), Some(1));
        assert_eq!(queue.index_of(TEST_GUILD, "baz").await.unwrap(), None);
    }

    #[tokio::test]
    async fn test_size() {
        let queue = setup().await;

        assert_eq!(queue.size(TEST_GUILD).await.unwrap(), 0);
        assert_eq!(queue.push(TEST_GUILD, entry("foo")).await.unwrap(), 1);
        assert_eq!(queue.push(TEST_GUILD, entry("bar")).await.unwrap(), 2);
        assert_eq!(queue.size(TEST_GUILD).await.unwrap(), 2);
    }

    #[tokio::test]
    async fn test_push_duplicate() {
        let queue = setup().await;

        assert_eq!(queue.push(TEST_GUILD, entry("foo")).await.unwrap(), 1);
        assert_eq!(queue.push(TEST_GUILD, entry("bar")).await.unwrap(), 2);
        assert_eq!(queue.push(TEST_GUILD, entry("foo")).await.unwrap(), 2);
        assert_eq!(queue.push("other-guild", entry("foo")).await.unwrap(), 1);

        assert_eq!(
            queue.list(TEST_GUILD).await.unwrap(),
            vec![entry("foo"), entry("bar")]
        );
    }

    #[tokio::test]
    async fn test_pop_n() {
        let queue = setup().await;

        for user_id in ["a", "b", "c"] {
            queue.push(TEST_GUILD, entry(user_id)).await.unwrap();
        }
        queue.push("other-guild", entry("x")).await.unwrap();

        assert_eq!(queue.pop_n(TEST_GUILD, 0).await.unwrap(), Vec::new());
        assert_eq!(
            queue.pop_n(TEST_GUILD, 2).await.unwrap(),
            vec![entry("a"), entry("b")]
        );
        assert_eq!(queue.pop_n(TEST_GUILD, 5).await.unwrap(), vec![entry("c")]);
        assert_eq!(queue.pop_n(TEST_GUILD, 1).await.unwrap(), Vec::new());
        assert_eq!(queue.size("other-guild").await.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_clear() {
        let queue = setup().await;

        queue.push(TEST_GUILD, entry("foo")).await.unwrap();
        queue.push(TEST_GUILD, entry("bar")).await.unwrap();
        queue.push("other-guild", entry("baz")).await.unwrap();
        assert_eq!(queue.size(TEST_GUILD).await.unwrap(), 2);

        queue.clear(TEST_GUILD).await.unwrap();

        assert_eq!(queue.size(TEST_GUILD).await.unwrap(), 0);
        assert_eq!(queue.size("other-guild").await.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_open_close() {
        let queue = setup().await;

        assert!(!queue.is_open(TEST_GUILD));
        queue.open(TEST_GUILD);
        assert!(queue.is_open(TEST_GUILD));

        queue.push(TEST_GUILD, entry("foo")).await.unwrap();
        queue.close(TEST_GUILD).await.unwrap();

        assert!(!queue.is_open(TEST_GUILD));
        assert_eq!(queue.size(TEST_GUILD).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_subscribe() {
        let queue = setup().await;
        let mut rx = queue.subscribe(TEST_GUILD);

        queue.push(TEST_GUILD, entry("foo")).await.unwrap();
        assert!(matches!(rx.try_recv(), Ok(QueueEvent::Updated)));
        assert!(rx.try_recv().is_err());
    }
}
