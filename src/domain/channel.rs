/// Stores which channels the bot is restricted to per guild.
/// A guild with no channels set means the bot responds in every channel.
#[cfg_attr(test, mockall::automock)]
#[async_trait::async_trait]
pub trait ChannelRepository: Send + Sync {
    /// Toggle a channel on/off for the guild.
    /// Returns `true` if the channel is now enabled.
    async fn toggle(&self, guild_id: &str, channel_id: &str) -> anyhow::Result<bool>;

    /// List the channels the bot is restricted to in the guild.
    /// An empty list means all channels are allowed.
    async fn list(&self, guild_id: &str) -> anyhow::Result<Vec<String>>;
}

pub fn is_channel_allowed(allowed_channels: &[String], channel_id: &str) -> bool {
    allowed_channels.is_empty() || allowed_channels.iter().any(|c| c == channel_id)
}
