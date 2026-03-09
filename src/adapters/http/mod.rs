use axum::{
    Json, Router,
    extract::{Path, State},
    response::sse::{Event, KeepAlive, Sse},
    routing::get,
};
use futures::stream::{self, Stream, StreamExt};
use tokio_stream::wrappers::BroadcastStream;
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tracing::info;

use std::{convert::Infallible, io, sync::Arc};

use crate::domain::{OrderRepository, QueueEntry, QueueEvent, QueueRepository};

#[derive(Clone)]
pub struct AppState {
    queue: Arc<dyn QueueRepository>,
    #[allow(dead_code)]
    orders: Arc<dyn OrderRepository>,
}

pub struct HttpAdapter {
    queue: Arc<dyn QueueRepository>,
    orders: Arc<dyn OrderRepository>,
}

impl HttpAdapter {
    pub fn new(queue: Arc<dyn QueueRepository>, orders: Arc<dyn OrderRepository>) -> Self {
        Self { queue, orders }
    }

    pub async fn start(&self) -> Result<(), io::Error> {
        let state = Arc::new(AppState {
            queue: self.queue.clone(),
            orders: self.orders.clone(),
        });

        let app = Router::new()
            .route("/{guild_id}/status", get(queue_status))
            .route("/{guild_id}/queue", get(list_queue))
            .route("/{guild_id}/queue/sse", get(list_queue_sse))
            .layer(ServiceBuilder::new().layer(TraceLayer::new_for_http()))
            .with_state(state);

        let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
        info!("listening on http://{}", listener.local_addr().unwrap());

        axum::serve(listener, app).await
    }
}

async fn queue_status(State(state): State<Arc<AppState>>, Path(guild_id): Path<String>) -> String {
    let is_open = state.queue.is_open(&guild_id);
    let status = if is_open { "open" } else { "closed" };
    status.to_string()
}

async fn list_queue(
    State(state): State<Arc<AppState>>,
    Path(guild_id): Path<String>,
) -> Json<Vec<QueueEntry>> {
    let queue = state.queue.list(&guild_id).await;
    Json(queue)
}

async fn list_queue_sse(
    State(state): State<Arc<AppState>>,
    Path(guild_id): Path<String>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.queue.subscribe();

    let queue = state.queue.list(&guild_id).await;
    let initial = serde_json::to_string(&queue).unwrap();

    let first =
        stream::once(async move { Ok::<Event, Infallible>(Event::default().data(initial)) });

    let updates = BroadcastStream::new(rx).filter_map(move |event| {
        let guild_id = guild_id.clone();
        let state = state.clone();
        async move {
            match event {
                Ok(QueueEvent::Updated { guild_id: gid }) if gid == guild_id => {
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
