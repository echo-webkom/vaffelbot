FROM lukemathwalker/cargo-chef:latest AS chef
WORKDIR /app

FROM chef AS planner
COPY ./Cargo.toml ./Cargo.lock ./
COPY ./src ./src
COPY ./migrations ./migrations
RUN cargo chef prepare

FROM chef AS builder
COPY --from=planner /app/recipe.json .
RUN cargo chef cook --release
COPY ./src ./src
COPY ./migrations ./migrations
COPY ./.sqlx ./.sqlx
ENV SQLX_OFFLINE=true
RUN cargo build --release
RUN mv ./target/release/vaffelbot ./vaffelbot

FROM debian:stable-slim AS runtime
WORKDIR /app
COPY --from=builder /app/vaffelbot /usr/local/bin/
COPY --from=builder /app/migrations ./migrations
ENTRYPOINT ["/usr/local/bin/vaffelbot"]
