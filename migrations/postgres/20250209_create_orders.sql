CREATE TABLE IF NOT EXISTS orders (
    id BIGSERIAL PRIMARY KEY,
    discord_user_id TEXT NOT NULL,
    fulfilled_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_orders_discord_user_id ON orders (discord_user_id);
