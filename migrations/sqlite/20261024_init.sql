CREATE TABLE IF NOT EXISTS orders (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    discord_user_id TEXT NOT NULL,
    guild_id TEXT,
    fulfilled_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX idx_orders_discord_user_id ON orders (discord_user_id);
CREATE INDEX idx_orders_guild_id ON orders (guild_id);

CREATE TABLE IF NOT EXISTS queue_entries (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    guild_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    display_name TEXT NOT NULL
);

CREATE INDEX idx_queue_entries_guild_id ON queue_entries (guild_id, id);
