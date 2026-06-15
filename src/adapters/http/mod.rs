mod routes;

use axum::{Router, http::Method, routing::get};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::info;

use std::{io, sync::Arc};

use crate::domain::{OrderRepository, QueueRepository};

#[derive(Clone)]
pub struct AppState {
    queue: Arc<dyn QueueRepository>,
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

        let cors = CorsLayer::new()
            .allow_methods([Method::GET, Method::POST])
            .allow_origin(Any);

        let app = Router::new()
            .route("/", get(routes::health_check))
            .route("/{guild_id}/status", get(routes::queue_status))
            .route("/{guild_id}/queue", get(routes::list_queue))
            .route("/{guild_id}/queue/sse", get(routes::list_queue_sse))
            .route("/{guild_id}/total", get(routes::total_vaffel))
            .layer(cors)
            .layer(TraceLayer::new_for_http())
            .with_state(state);

        let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
        info!("listening on http://{}", listener.local_addr().unwrap());

        axum::serve(listener, app).await
    }
}
