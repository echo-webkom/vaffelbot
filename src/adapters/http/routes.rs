use axum::{
    Json,
    extract::{Path, State},
    response::sse::{Event, KeepAlive, Sse},
};
use futures::stream::{self, Stream, StreamExt};
use tokio_stream::wrappers::BroadcastStream;

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
) -> Json<Vec<QueueEntry>> {
    let queue = state.queue.list(&guild_id).await;
    Json(queue)
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
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.queue.subscribe(&guild_id);

    let queue = state.queue.list(&guild_id).await;
    let initial = serde_json::to_string(&queue).unwrap();

    let first =
        stream::once(async move { Ok::<Event, Infallible>(Event::default().data(initial)) });

    let updates = BroadcastStream::new(rx).filter_map(move |event| {
        let guild_id = guild_id.clone();
        let state = state.clone();
        async move {
            match event {
                Ok(QueueEvent::Updated) => {
                    let queue = state.queue.list(&guild_id).await;
                    let data = serde_json::to_string(&queue).unwrap();
                    Some(Ok(Event::default().data(data)))
                }
                _ => None,
            }
        }
    });

    Sse::new(first.chain(updates)).keep_alive(KeepAlive::default())
}
