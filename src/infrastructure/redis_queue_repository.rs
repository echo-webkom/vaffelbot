use std::{
    collections::{HashMap, HashSet},
    sync::RwLock,
};

use redis::AsyncCommands;
use tokio::sync::broadcast;
use tracing::{debug, info, instrument};

use crate::domain::{QueueEntry, QueueEvent, QueueRepository};

fn queue_key(guild_id: &str) -> String {
    format!("queue:{guild_id}")
}

pub struct RedisQueueRepository {
    redis: redis::aio::ConnectionManager,
    open_guilds: RwLock<HashSet<String>>,
    guild_senders: RwLock<HashMap<String, broadcast::Sender<QueueEvent>>>,
}

impl RedisQueueRepository {
    pub async fn new(redis: redis::Client) -> redis::RedisResult<Self> {
        let connection_manager = redis.get_connection_manager().await?;

        Ok(Self {
            redis: connection_manager,
            open_guilds: RwLock::new(HashSet::new()),
            guild_senders: RwLock::new(HashMap::new()),
        })
    }
}

#[async_trait::async_trait]
impl QueueRepository for RedisQueueRepository {
    #[instrument(skip(self), fields(guild_id))]
    fn open(&self, guild_id: &str) {
        info!(guild_id, "Opening queue for guild");
        self.open_guilds
            .write()
            .unwrap()
            .insert(guild_id.to_string());
    }

    #[instrument(skip(self), fields(guild_id))]
    async fn close(&self, guild_id: &str) -> anyhow::Result<()> {
        info!(guild_id, "Closing queue for guild");
        self.open_guilds.write().unwrap().remove(guild_id);
        self.clear(guild_id).await
    }

    #[instrument(skip(self), fields(guild_id))]
    fn is_open(&self, guild_id: &str) -> bool {
        let is_open = self.open_guilds.read().unwrap().contains(guild_id);
        debug!(guild_id, is_open, "Checking if queue is open");
        is_open
    }

    #[instrument(skip(self), fields(guild_id, user_id))]
    async fn index_of(&self, guild_id: &str, user_id: &str) -> anyhow::Result<Option<usize>> {
        let key = queue_key(guild_id);
        let mut con = self.redis.clone();

        let list: Vec<String> = con.lrange(&key, 0, -1).await?;
        let entries = list
            .into_iter()
            .map(|json| serde_json::from_str::<QueueEntry>(&json))
            .collect::<serde_json::Result<Vec<_>>>()?;
        let position = entries.iter().position(|entry| entry.user_id == user_id);

        debug!(guild_id, user_id, position = ?position, "Found user position in queue");
        Ok(position)
    }

    #[instrument(skip(self), fields(guild_id))]
    async fn size(&self, guild_id: &str) -> anyhow::Result<usize> {
        let key = queue_key(guild_id);
        let mut con = self.redis.clone();
        let size = con.llen(&key).await?;
        debug!(guild_id, size, "Retrieved queue size");
        Ok(size)
    }

    #[instrument(skip(self, entry), fields(guild_id, user_id = %entry.user_id))]
    async fn push(&self, guild_id: &str, entry: QueueEntry) -> anyhow::Result<usize> {
        let key = queue_key(guild_id);
        let json = serde_json::to_string(&entry)?;
        let mut con = self.redis.clone();
        let new_size = con.rpush(&key, json).await?;
        info!(guild_id, user_id = %entry.user_id, queue_size = new_size, "Added user to queue");
        self.broadcast_update(guild_id);
        Ok(new_size)
    }

    #[instrument(skip(self), fields(guild_id, n))]
    async fn pop_n(&self, guild_id: &str, n: usize) -> anyhow::Result<Vec<QueueEntry>> {
        if n == 0 {
            return Ok(vec![]);
        }

        let key = queue_key(guild_id);
        let mut con = self.redis.clone();
        let count = std::num::NonZeroUsize::new(n);
        let json_entries: Vec<String> = con.lpop(&key, count).await?;
        let entries = json_entries
            .into_iter()
            .map(|json| serde_json::from_str::<QueueEntry>(&json))
            .collect::<serde_json::Result<Vec<_>>>()?;
        info!(guild_id, count = entries.len(), "Popped entries from queue");
        if !entries.is_empty() {
            self.broadcast_update(guild_id);
        }
        Ok(entries)
    }

    #[instrument(skip(self), fields(guild_id))]
    async fn list(&self, guild_id: &str) -> anyhow::Result<Vec<QueueEntry>> {
        let key = queue_key(guild_id);
        let mut con = self.redis.clone();
        let json_list: Vec<String> = con.lrange(&key, 0, -1).await?;
        let entries = json_list
            .into_iter()
            .map(|json| serde_json::from_str::<QueueEntry>(&json))
            .collect::<serde_json::Result<Vec<_>>>()?;
        debug!(guild_id, count = entries.len(), "Retrieved queue list");
        Ok(entries)
    }

    #[instrument(skip(self), fields(guild_id))]
    async fn clear(&self, guild_id: &str) -> anyhow::Result<()> {
        let key = queue_key(guild_id);
        let mut con = self.redis.clone();
        let _: usize = con.del(&key).await?;
        info!(guild_id, "Cleared queue");
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

impl RedisQueueRepository {
    fn broadcast_update(&self, guild_id: &str) {
        let read = self.guild_senders.read().unwrap();
        if let Some(tx) = read.get(guild_id) {
            let _ = tx.send(QueueEvent::Updated);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::env::home_dir;

    use super::*;

    use testcontainers::runners::AsyncRunner;
    use testcontainers_modules::redis::Redis;
    use tokio::sync::OnceCell;

    const TEST_GUILD: &str = "test-guild";

    struct TestRedis {
        _node: testcontainers::ContainerAsync<Redis>,
        client: redis::Client,
    }

    static REDIS: OnceCell<TestRedis> = OnceCell::const_new();

    async fn init_redis() -> &'static TestRedis {
        REDIS
            .get_or_init(|| async {
                if std::env::var("DOCKER_HOST").is_err() {
                    let socket = home_dir()
                        .expect("Failed to get home directory")
                        .join(".colima/default/docker.sock");
                    if std::path::Path::new(&socket).exists() {
                        unsafe {
                            std::env::set_var(
                                "DOCKER_HOST",
                                format!("unix://{}", socket.to_string_lossy()),
                            );
                        }
                    }
                }

                let node = Redis::default().start().await.unwrap();
                let host_ip = node.get_host().await.unwrap();
                let host_port = node.get_host_port_ipv4(6379).await.unwrap();
                let url = format!("redis://{host_ip}:{host_port}");
                let client = redis::Client::open(url).unwrap();
                TestRedis {
                    _node: node,
                    client,
                }
            })
            .await
    }

    async fn setup() -> RedisQueueRepository {
        let redis = init_redis().await;
        let queue = RedisQueueRepository::new(redis.client.clone())
            .await
            .unwrap();
        queue.clear(TEST_GUILD).await.unwrap();
        queue
    }

    #[tokio::test]
    async fn test_list() {
        let queue = setup().await;
        let guild = "test-list";
        queue.clear(guild).await.unwrap();

        let foo = QueueEntry::new("foo".to_string(), "Foo User".to_string());
        let bar = QueueEntry::new("bar".to_string(), "Bar User".to_string());

        queue.push(guild, foo.clone()).await.unwrap();
        queue.push(guild, bar.clone()).await.unwrap();

        let list = queue.list(guild).await.unwrap();
        assert_eq!(list, vec![foo, bar]);
    }

    #[tokio::test]
    async fn test_index_of() {
        let queue = setup().await;
        let guild = "test-index-of";
        queue.clear(guild).await.unwrap();

        let foo = QueueEntry::new("foo".to_string(), "Foo User".to_string());
        let bar = QueueEntry::new("bar".to_string(), "Bar User".to_string());

        queue.push(guild, foo).await.unwrap();
        queue.push(guild, bar).await.unwrap();

        assert_eq!(queue.index_of(guild, "foo").await.unwrap(), Some(0));
        assert_eq!(queue.index_of(guild, "bar").await.unwrap(), Some(1));
        assert_eq!(queue.index_of(guild, "baz").await.unwrap(), None);
    }

    #[tokio::test]
    async fn test_size() {
        let queue = setup().await;
        let guild = "test-size";
        queue.clear(guild).await.unwrap();

        assert_eq!(queue.size(guild).await.unwrap(), 0);

        let foo = QueueEntry::new("foo".to_string(), "Foo User".to_string());
        let bar = QueueEntry::new("bar".to_string(), "Bar User".to_string());

        queue.push(guild, foo).await.unwrap();
        queue.push(guild, bar).await.unwrap();

        assert_eq!(queue.size(guild).await.unwrap(), 2);
    }

    #[tokio::test]
    async fn test_clear() {
        let queue = setup().await;
        let guild = "test-clear";
        queue.clear(guild).await.unwrap();

        let foo = QueueEntry::new("foo".to_string(), "Foo User".to_string());
        let bar = QueueEntry::new("bar".to_string(), "Bar User".to_string());

        queue.push(guild, foo).await.unwrap();
        queue.push(guild, bar).await.unwrap();

        assert_eq!(queue.size(guild).await.unwrap(), 2);

        queue.clear(guild).await.unwrap();

        assert_eq!(queue.size(guild).await.unwrap(), 0);
    }
}
