use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    response::Response,
    routing::{any, get},
    Json, Router,
};
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tracing::info;

use std::{io, sync::Arc};

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
            .route("/{guild_id}/queue/ws", any(list_queue_ws))
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

async fn list_queue_ws(
    State(state): State<Arc<AppState>>,
    Path(guild_id): Path<String>,
    ws: WebSocketUpgrade,
) -> Response {
    ws.on_upgrade(move |socket| list_queue_ws_handler(state, guild_id, socket))
}

async fn list_queue_ws_handler(state: Arc<AppState>, guild_id: String, mut socket: WebSocket) {
    info!("new websocket connection for guild_id: {}", guild_id);

    let mut rx = state.queue.subscribe();

    // Initial state
    let queue = state.queue.list(&guild_id).await;
    let msg = serde_json::to_string(&queue).unwrap();
    if socket.send(Message::Text(msg.into())).await.is_err() {
        return;
    }

    while let Ok(event) = rx.recv().await {
        match event {
            QueueEvent::Updated { guild_id: gid } => {
                if gid == guild_id {
                    let queue = state.queue.list(&guild_id).await;
                    let msg = serde_json::to_string(&queue).unwrap();
                    if socket.send(Message::Text(msg.into())).await.is_err() {
                        break;
                    }
                }
            }
        }
    }
}
