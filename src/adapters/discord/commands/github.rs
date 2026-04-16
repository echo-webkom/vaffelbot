use poise::CreateReply;

use crate::adapters::discord::{Context, Error};

/// Link til vaffelbot GitHub repo
#[tracing::instrument(name = "github", skip(ctx))]
#[poise::command(prefix_command, slash_command)]
pub async fn github(ctx: Context<'_>) -> Result<(), Error> {
    let message = CreateReply::default()
        .content("https://github.com/echo-webkom/vaffelbot")
        .ephemeral(true);
    ctx.send(message).await?;
    Ok(())
}
