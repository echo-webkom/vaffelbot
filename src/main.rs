use std::env;

use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use vaffelbot::{Config, VaffelBot};

#[tokio::main]
async fn main() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| format!("{}=debug,tower_http=debug", env!("CARGO_CRATE_NAME")).into());
    let fmt_layer = tracing_subscriber::fmt::layer();
    let registry = tracing_subscriber::registry().with(filter);

    if cfg!(debug_assertions) {
        registry.with(fmt_layer).init();
    } else {
        registry.with(fmt_layer.json()).init();
    };

    info!("Starting VaffelBot");

    let config = Config::from_env();
    let bot = VaffelBot::new(config);
    if let Err(why) = bot.run().await {
        error!(error = ?why, "Error running bot");
        std::process::exit(1);
    }
}
