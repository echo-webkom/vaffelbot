set dotenv-load

dev:
    cargo run

up:
    docker compose up redis postgres -d

down:
    docker compose down

test:
    cargo test -- --test-threads=1

migrate:
    sqlx migrate run --source migrations --database-url "$DATABASE_URL"
