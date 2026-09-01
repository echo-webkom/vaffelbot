pub mod commands;

use std::sync::Arc;

use poise::FrameworkOptions;
use serenity::all::GatewayIntents;

use crate::domain::{OrderRepository, QueueRepository};

const PREFIX: &str = "!";

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Context<'a> = poise::Context<'a, Data, Error>;

pub struct Data {
    pub queue: Arc<dyn QueueRepository>,
    pub orders: Arc<dyn OrderRepository>,
    pub github_token: Option<String>,
}

pub struct DiscordAdapter {
    token: String,
    queue: Arc<dyn QueueRepository>,
    orders: Arc<dyn OrderRepository>,
    github_token: Option<String>,
}

impl DiscordAdapter {
    pub fn new(
        token: String,
        queue: Arc<dyn QueueRepository>,
        orders: Arc<dyn OrderRepository>,
        github_token: Option<String>,
    ) -> Self {
        Self {
            token,
            queue,
            orders,
            github_token,
        }
    }

    pub async fn start(self) -> anyhow::Result<()> {
        let options: FrameworkOptions<Data, Error> = poise::FrameworkOptions {
            commands: vec![
                commands::bake::bake(),
                commands::close::close(),
                commands::github::github(),
                commands::feedback::feedback(),
                commands::open::open(),
                commands::ping::ping(),
                commands::queue_size::queue(),
                commands::waffle::waffle(),
            ],
            prefix_options: poise::PrefixFrameworkOptions {
                prefix: Some(PREFIX.into()),
                ..Default::default()
            },
            ..Default::default()
        };

        let framework = poise::Framework::builder()
            .setup(move |ctx, _ready, framework| {
                Box::pin(async move {
                    poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                    Ok(Data {
                        queue: self.queue.clone(),
                        orders: self.orders.clone(),
                        github_token: self.github_token.clone(),
                    })
                })
            })
            .options(options)
            .build();

        let mut client = serenity::Client::builder(
            self.token.clone(),
            GatewayIntents::GUILD_MESSAGES | GatewayIntents::MESSAGE_CONTENT,
        )
        .framework(framework)
        .await?;

        client.start().await?;
        Ok(())
    }
}

pub async fn check_is_oracle(ctx: Context<'_>) -> Result<bool, Error> {
    let guild_id = match ctx.guild_id() {
        Some(id) => id,
        None => {
            deny(ctx).await?;
            return Ok(false);
        }
    };

    let member = match guild_id.member(ctx, ctx.author().id).await {
        Ok(member) => member,
        Err(_) => {
            deny(ctx).await?;
            return Ok(false);
        }
    };

    let roles = match guild_id.roles(ctx).await {
        Ok(roles) => roles,
        Err(_) => {
            deny(ctx).await?;
            return Ok(false);
        }
    };

    let orakel_role = roles.values().find(|r| r.name.to_lowercase() == "orakel");
    match orakel_role {
        Some(role) if member.roles.contains(&role.id) => Ok(true),
        _ => {
            deny(ctx).await?;
            Ok(false)
        }
    }
}

async fn deny(ctx: Context<'_>) -> Result<(), Error> {
    // Send message to discord to prevent timeout.
    // Discord expects a response within 3 seconds. Just
    // returning false does not respond to the interaction.
    ctx.send(
        poise::CreateReply::default()
            .content("❌ Du har ikke tilgang til denne kommandoen.")
            .ephemeral(true),
    )
    .await?;
    Ok(())
}
