use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
};
use futures::stream::{self, Stream, StreamExt};
use tokio_stream::wrappers::BroadcastStream;
use tracing::error;

use std::{convert::Infallible, sync::Arc};

use crate::{
    adapters::http::AppState,
    domain::{QueueEntry, QueueEvent},
};

pub async fn health_check() -> &'static str {
    "OK"
}

pub async fn queue_status(
    State(state): State<Arc<AppState>>,
    Path(guild_id): Path<String>,
) -> String {
    let is_open = state.queue.is_open(&guild_id);
    let status = if is_open { "open" } else { "closed" };
    status.to_string()
}

pub async fn list_queue(
    State(state): State<Arc<AppState>>,
    Path(guild_id): Path<String>,
) -> Result<Json<Vec<QueueEntry>>, StatusCode> {
    state.queue.list(&guild_id).await.map(Json).map_err(|e| {
        error!(guild_id, error = ?e, "Failed to list queue");
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

pub async fn total_vaffel(
    State(state): State<Arc<AppState>>,
    Path(guild_id): Path<String>,
) -> Json<i64> {
    let stats = state.orders.daily_stats(&guild_id).await.unwrap();
    Json(stats.total_orders)
}

pub async fn list_queue_sse(
    State(state): State<Arc<AppState>>,
    Path(guild_id): Path<String>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, StatusCode> {
    let rx = state.queue.subscribe(&guild_id);

    let queue = state.queue.list(&guild_id).await.map_err(|e| {
        error!(guild_id, error = ?e, "Failed to list queue for SSE connection");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let initial = serde_json::to_string(&queue).map_err(|e| {
        error!(guild_id, error = ?e, "Failed to serialize queue for SSE connection");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let first =
        stream::once(async move { Ok::<Event, Infallible>(Event::default().data(initial)) });

    let updates = BroadcastStream::new(rx).filter_map(move |event| {
        let guild_id = guild_id.clone();
        let state = state.clone();
        async move {
            match event {
                Ok(QueueEvent::Updated) => match state.queue.list(&guild_id).await {
                    Ok(queue) => match serde_json::to_string(&queue) {
                        Ok(data) => Some(Ok(Event::default().data(data))),
                        Err(e) => {
                            error!(guild_id, error = ?e, "Failed to serialize queue update");
                            Some(Ok(Event::default()
                                .event("error")
                                .data("Failed to serialize queue update")))
                        }
                    },
                    Err(e) => {
                        error!(guild_id, error = ?e, "Failed to list queue for SSE update");
                        Some(Ok(Event::default()
                            .event("error")
                            .data("Failed to load queue update")))
                    }
                },
                _ => None,
            }
        }
    });

    Ok(Sse::new(first.chain(updates)).keep_alive(KeepAlive::default()))
}
