use std::env;

use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use vaffelbot_rs::{config::Config, VaffelBot};

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                format!("{}=debug,tower_http=debug", env!("CARGO_CRATE_NAME")).into()
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting VaffelBot");

    let config = Config::from_env();
    let bot = VaffelBot::new(config);
    if let Err(why) = bot.run().await {
        error!(error = ?why, "Error running bot");
        std::process::exit(1);
    }
}
