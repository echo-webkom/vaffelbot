set dotenv-load

dev:
    cargo run

up:
    docker compose up redis postgres -d

down:
    docker compose down

test:
    cargo test -- --test-threads=1

coverage:
    cargo llvm-cov --html --open

coverage-lcov:
    cargo llvm-cov --lcov --output-path lcov.info

migrate:
    sqlx migrate run --source migrations --database-url "$DATABASE_URL"

offline:
    cargo sqlx prepare

check:
    cargo fmt --all && cargo clippy --fix --tests --allow-dirty

deny:
    cargo deny check advisories bans sources
