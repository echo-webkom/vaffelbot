ALTER TABLE orders ADD COLUMN guild_id TEXT;

CREATE INDEX idx_orders_guild_id ON orders (guild_id);
