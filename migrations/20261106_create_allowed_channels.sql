CREATE TABLE IF NOT EXISTS allowed_channels (
    guild_id TEXT NOT NULL,
    channel_id TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (guild_id, channel_id)
);
