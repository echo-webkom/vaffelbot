pub mod commands;

use std::sync::Arc;

use poise::FrameworkOptions;
use serenity::all::GatewayIntents;

use crate::domain::{ChannelRepository, OrderRepository, QueueRepository, is_channel_allowed};

const PREFIX: &str = "!";

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Context<'a> = poise::Context<'a, Data, Error>;

pub struct Data {
    pub queue: Arc<dyn QueueRepository>,
    pub orders: Arc<dyn OrderRepository>,
    pub channels: Arc<dyn ChannelRepository>,
    pub github_token: Option<String>,
}

pub struct DiscordAdapter {
    token: String,
    queue: Arc<dyn QueueRepository>,
    orders: Arc<dyn OrderRepository>,
    channels: Arc<dyn ChannelRepository>,
    github_token: Option<String>,
}

impl DiscordAdapter {
    pub fn new(
        token: String,
        queue: Arc<dyn QueueRepository>,
        orders: Arc<dyn OrderRepository>,
        channels: Arc<dyn ChannelRepository>,
        github_token: Option<String>,
    ) -> Self {
        Self {
            token,
            queue,
            orders,
            channels,
            github_token,
        }
    }

    pub async fn start(self) -> anyhow::Result<()> {
        let options: FrameworkOptions<Data, Error> = poise::FrameworkOptions {
            commands: vec![
                commands::bake::bake(),
                commands::channel::channel(),
                commands::close::close(),
                commands::github::github(),
                commands::highscore::highscore(),
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
            command_check: Some(|ctx| Box::pin(check_channel_allowed(ctx))),
            ..Default::default()
        };

        let framework = poise::Framework::builder()
            .setup(move |ctx, _ready, framework| {
                Box::pin(async move {
                    poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                    Ok(Data {
                        queue: self.queue.clone(),
                        orders: self.orders.clone(),
                        channels: self.channels.clone(),
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

/// Global check: only respond in channels enabled with `/vaffelkanal`.
/// If no channels are enabled for the guild, every channel is allowed.
async fn check_channel_allowed(ctx: Context<'_>) -> Result<bool, Error> {
    let Some(guild_id) = ctx.guild_id() else {
        return Ok(true);
    };

    if ctx.command().name == commands::channel::channel().name {
        return Ok(true);
    }

    let guild_id = guild_id.to_string();
    let allowed = match ctx.data().channels.list(&guild_id).await {
        Ok(allowed) => allowed,
        Err(e) => {
            // Fail open so a database hiccup does not silence the bot.
            tracing::error!(guild_id, error = ?e, "Failed to fetch allowed channels");
            return Ok(true);
        }
    };

    if is_channel_allowed(&allowed, &ctx.channel_id().to_string()) {
        return Ok(true);
    }

    if let Context::Application(_) = ctx {
        ctx.send(
            poise::CreateReply::default()
                .content(commands::channel::denied_message(&allowed))
                .ephemeral(true),
        )
        .await?;
    }

    Ok(false)
}

pub async fn check_is_oracle(ctx: Context<'_>) -> Result<bool, Error> {
    let Some(guild_id) = ctx.guild_id() else {
        deny(ctx).await?;
        return Ok(false);
    };

    let Ok(member) = guild_id.member(ctx, ctx.author().id).await else {
        deny(ctx).await?;
        return Ok(false);
    };

    let Ok(roles) = guild_id.roles(ctx).await else {
        deny(ctx).await?;
        return Ok(false);
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
