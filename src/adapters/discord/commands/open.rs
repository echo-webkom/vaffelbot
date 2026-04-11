use serenity::all::{ActivityData, OnlineStatus};

use crate::adapters::discord::{Context, Error, check_is_oracle};

/// Åpne for bestilling av vafler
#[tracing::instrument(name = "open", skip(ctx))]
#[poise::command(
    prefix_command,
    slash_command,
    rename = "start",
    check = "check_is_oracle"
)]
pub async fn open(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap().to_string();

    if ctx.data().queue.is_open(&guild_id) {
        ctx.say("🔓️ Bestilling er allerede åpnet").await?;
        return Ok(());
    }

    ctx.data().queue.open(&guild_id);
    ctx.say("@here 🔓️ Bestilling er nå åpnet").await?;

    ctx.serenity_context().set_presence(
        Some(ActivityData::playing("🧇 Lager vafler")),
        OnlineStatus::Online,
    );

    Ok(())
}
