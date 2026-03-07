#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct QueueEntry {
    pub user_id: String,
    pub display_name: String,
}

impl QueueEntry {
    pub fn new(user_id: String, display_name: String) -> Self {
        Self {
            user_id,
            display_name,
        }
    }
}

#[derive(Debug, Clone)]
pub enum QueueEvent {
    Updated { guild_id: String },
}

#[async_trait::async_trait]
pub trait QueueRepository: Send + Sync {
    /// Open the queue to allow new entries
    fn open(&self, guild_id: &str);

    /// Close the queue to prevent new entries
    async fn close(&self, guild_id: &str);

    /// Check if the queue is currently open
    fn is_open(&self, guild_id: &str) -> bool;

    /// Find the position of a user in the queue by user_id
    /// Returns None if the user is not found
    async fn index_of(&self, guild_id: &str, user_id: &str) -> Option<usize>;

    /// Get the current size of the queue
    async fn size(&self, guild_id: &str) -> usize;

    /// Add a user to the end of the queue
    /// Returns the new size of the queue
    async fn push(&self, guild_id: &str, entry: QueueEntry) -> usize;

    /// Remove the entry at the front of the queue and return it
    /// Returns None if the queue is empty
    async fn pop(&self, guild_id: &str) -> Option<QueueEntry>;

    /// Remove up to `n` entries from the front of the queue
    async fn pop_n(&self, guild_id: &str, n: usize) -> Vec<QueueEntry>;

    /// Get all entries in the queue
    async fn list(&self, guild_id: &str) -> Vec<QueueEntry>;

    /// Clear the queue
    async fn clear(&self, guild_id: &str);

    /// Subscribe to queue change events
    fn subscribe(&self) -> tokio::sync::broadcast::Receiver<QueueEvent>;
}
