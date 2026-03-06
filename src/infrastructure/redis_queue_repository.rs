use std::{collections::HashSet, sync::RwLock};

use redis::AsyncCommands;
use tokio::sync::broadcast;
use tracing::{debug, error, info, instrument, warn};

use crate::domain::{QueueEntry, QueueEvent, QueueRepository};

fn queue_key(guild_id: &str) -> String {
    format!("queue:{guild_id}")
}

pub struct RedisQueueRepository {
    redis: redis::Client,
    open_guilds: RwLock<HashSet<String>>,
    event_tx: broadcast::Sender<QueueEvent>,
}

impl RedisQueueRepository {
    pub fn new(redis: redis::Client) -> Self {
        let (event_tx, _) = broadcast::channel(64);
        Self {
            redis,
            open_guilds: RwLock::new(HashSet::new()),
            event_tx,
        }
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
    async fn close(&self, guild_id: &str) {
        info!(guild_id, "Closing queue for guild");
        self.open_guilds.write().unwrap().remove(guild_id);
        self.clear(guild_id).await;
        self.broadcast_update(guild_id);
    }

    #[instrument(skip(self), fields(guild_id))]
    fn is_open(&self, guild_id: &str) -> bool {
        let is_open = self.open_guilds.read().unwrap().contains(guild_id);
        debug!(guild_id, is_open, "Checking if queue is open");
        is_open
    }

    #[instrument(skip(self), fields(guild_id, user_id))]
    async fn index_of(&self, guild_id: &str, user_id: &str) -> Option<usize> {
        let key = queue_key(guild_id);
        let mut con = match self.redis.get_multiplexed_async_connection().await {
            Ok(con) => con,
            Err(e) => {
                error!(guild_id, user_id, error = ?e, "Failed to get Redis connection for index_of");
                return None;
            }
        };

        let list: Vec<String> = match con.lrange(&key, 0, -1).await {
            Ok(list) => list,
            Err(e) => {
                error!(guild_id, user_id, error = ?e, "Failed to fetch queue list from Redis");
                return None;
            }
        };

        let position = list.iter().position(|json_str| {
            serde_json::from_str::<QueueEntry>(json_str)
                .map(|entry| entry.user_id == user_id)
                .unwrap_or(false)
        });

        debug!(guild_id, user_id, position = ?position, "Found user position in queue");
        position
    }

    #[instrument(skip(self), fields(guild_id))]
    async fn size(&self, guild_id: &str) -> usize {
        let key = queue_key(guild_id);
        let mut con = match self.redis.get_multiplexed_async_connection().await {
            Ok(con) => con,
            Err(e) => {
                error!(guild_id, error = ?e, "Failed to get Redis connection for size");
                return 0;
            }
        };
        let size = con.llen(&key).await.unwrap_or_else(|e| {
            error!(guild_id, error = ?e, "Failed to get queue size from Redis");
            0
        });
        debug!(guild_id, size, "Retrieved queue size");
        size
    }

    #[instrument(skip(self, entry), fields(guild_id, user_id = %entry.user_id))]
    async fn push(&self, guild_id: &str, entry: QueueEntry) -> usize {
        let key = queue_key(guild_id);
        let json = serde_json::to_string(&entry).unwrap();
        let mut con = match self.redis.get_multiplexed_async_connection().await {
            Ok(con) => con,
            Err(e) => {
                error!(guild_id, user_id = %entry.user_id, error = ?e, "Failed to get Redis connection for push");
                return 0;
            }
        };
        let new_size = con.rpush(&key, json).await.unwrap_or_else(|e| {
            error!(guild_id, user_id = %entry.user_id, error = ?e, "Failed to push to queue in Redis");
            0
        });
        info!(guild_id, user_id = %entry.user_id, queue_size = new_size, "Added user to queue");
        self.broadcast_update(guild_id);
        new_size
    }

    #[instrument(skip(self), fields(guild_id))]
    async fn pop(&self, guild_id: &str) -> Option<QueueEntry> {
        let key = queue_key(guild_id);
        let mut con = match self.redis.get_multiplexed_async_connection().await {
            Ok(con) => con,
            Err(e) => {
                error!(guild_id, error = ?e, "Failed to get Redis connection for pop");
                return None;
            }
        };
        let result: redis::RedisResult<Vec<String>> = con.lpop(&key, None).await;
        let entry: Option<QueueEntry> = result
            .ok()
            .and_then(|mut vec| vec.pop())
            .and_then(|json_str| serde_json::from_str(&json_str).ok());

        match &entry {
            Some(e) => info!(guild_id, user_id = %e.user_id, "Popped user from queue"),
            None => debug!(guild_id, "No entry to pop from queue"),
        }
        if entry.is_some() {
            self.broadcast_update(guild_id);
        }
        entry
    }

    #[instrument(skip(self), fields(guild_id, n))]
    async fn pop_n(&self, guild_id: &str, n: usize) -> Vec<QueueEntry> {
        if n == 0 {
            return vec![];
        }

        let key = queue_key(guild_id);
        let mut con = match self.redis.get_multiplexed_async_connection().await {
            Ok(con) => con,
            Err(e) => {
                error!(guild_id, error = ?e, "Failed to get Redis connection for pop_n");
                return vec![];
            }
        };
        let count = std::num::NonZeroUsize::new(n);
        let result: redis::RedisResult<Vec<String>> = con.lpop(&key, count).await;
        let entries: Vec<QueueEntry> = result
            .unwrap_or_default()
            .into_iter()
            .filter_map(|json_str| serde_json::from_str(&json_str).ok())
            .collect();
        info!(guild_id, count = entries.len(), "Popped entries from queue");
        if !entries.is_empty() {
            self.broadcast_update(guild_id);
        }
        entries
    }

    #[instrument(skip(self), fields(guild_id))]
    async fn list(&self, guild_id: &str) -> Vec<QueueEntry> {
        let key = queue_key(guild_id);
        let mut con = match self.redis.get_multiplexed_async_connection().await {
            Ok(con) => con,
            Err(e) => {
                error!(guild_id, error = ?e, "Failed to get Redis connection for list");
                return vec![];
            }
        };
        let json_list: Vec<String> = con.lrange(&key, 0, -1).await.unwrap_or_else(|e| {
            error!(guild_id, error = ?e, "Failed to fetch queue list from Redis");
            vec![]
        });
        let entries: Vec<QueueEntry> = json_list
            .into_iter()
            .filter_map(|json_str| {
                serde_json::from_str(&json_str).ok().or_else(|| {
                    warn!(guild_id, json = %json_str, "Failed to deserialize queue entry");
                    None
                })
            })
            .collect();
        debug!(guild_id, count = entries.len(), "Retrieved queue list");
        entries
    }

    #[instrument(skip(self), fields(guild_id))]
    async fn clear(&self, guild_id: &str) {
        let key = queue_key(guild_id);
        if let Ok(mut con) = self.redis.get_multiplexed_async_connection().await {
            let result: redis::RedisResult<()> = con.del(&key).await;
            match result {
                Ok(_) => info!(guild_id, "Cleared queue"),
                Err(e) => error!(guild_id, error = ?e, "Failed to clear queue in Redis"),
            }
        } else {
            error!(guild_id, "Failed to get Redis connection for clear");
        }
        self.broadcast_update(guild_id);
    }

    fn subscribe(&self) -> tokio::sync::broadcast::Receiver<QueueEvent> {
        self.event_tx.subscribe()
    }
}

impl RedisQueueRepository {
    fn broadcast_update(&self, guild_id: &str) {
        if let Err(err) = self.event_tx.send(QueueEvent::Updated {
            guild_id: guild_id.to_string(),
        }) {
            error!(
                guild_id,
                "error" = ?err,
                "guild_id" = guild_id,
                "Failed to broadcast queue update event"
            );
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
        let queue = RedisQueueRepository::new(redis.client.clone());
        queue.clear(TEST_GUILD).await;
        queue
    }

    #[tokio::test]
    async fn test_push_and_pop() {
        let queue = setup().await;
        let guild = "test-push-and-pop";
        queue.clear(guild).await;

        let foo = QueueEntry::new("foo".to_string(), "Foo User".to_string());
        let bar = QueueEntry::new("bar".to_string(), "Bar User".to_string());

        queue.push(guild, foo.clone()).await;
        queue.push(guild, bar.clone()).await;

        assert_eq!(queue.size(guild).await, 2);
        assert_eq!(queue.index_of(guild, "bar").await, Some(1));

        let popped = queue.pop(guild).await;
        assert_eq!(popped, Some(foo));
        assert_eq!(queue.size(guild).await, 1);

        let remaining = queue.list(guild).await;
        assert_eq!(remaining, vec![bar]);
    }

    #[tokio::test]
    async fn test_list() {
        let queue = setup().await;
        let guild = "test-list";
        queue.clear(guild).await;

        let foo = QueueEntry::new("foo".to_string(), "Foo User".to_string());
        let bar = QueueEntry::new("bar".to_string(), "Bar User".to_string());

        queue.push(guild, foo.clone()).await;
        queue.push(guild, bar.clone()).await;

        let list = queue.list(guild).await;
        assert_eq!(list, vec![foo, bar]);
    }

    #[tokio::test]
    async fn test_index_of() {
        let queue = setup().await;
        let guild = "test-index-of";
        queue.clear(guild).await;

        let foo = QueueEntry::new("foo".to_string(), "Foo User".to_string());
        let bar = QueueEntry::new("bar".to_string(), "Bar User".to_string());

        queue.push(guild, foo).await;
        queue.push(guild, bar).await;

        assert_eq!(queue.index_of(guild, "foo").await, Some(0));
        assert_eq!(queue.index_of(guild, "bar").await, Some(1));
        assert_eq!(queue.index_of(guild, "baz").await, None);
    }

    #[tokio::test]
    async fn test_size() {
        let queue = setup().await;
        let guild = "test-size";
        queue.clear(guild).await;

        assert_eq!(queue.size(guild).await, 0);

        let foo = QueueEntry::new("foo".to_string(), "Foo User".to_string());
        let bar = QueueEntry::new("bar".to_string(), "Bar User".to_string());

        queue.push(guild, foo).await;
        queue.push(guild, bar).await;

        assert_eq!(queue.size(guild).await, 2);
    }

    #[tokio::test]
    async fn test_clear() {
        let queue = setup().await;
        let guild = "test-clear";
        queue.clear(guild).await;

        let foo = QueueEntry::new("foo".to_string(), "Foo User".to_string());
        let bar = QueueEntry::new("bar".to_string(), "Bar User".to_string());

        queue.push(guild, foo).await;
        queue.push(guild, bar).await;

        assert_eq!(queue.size(guild).await, 2);

        queue.clear(guild).await;

        assert_eq!(queue.size(guild).await, 0);
    }
}
