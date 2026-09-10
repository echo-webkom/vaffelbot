use crate::adapters::discord::{Context, Error};

/// Se dagen det ble stekt flest vafler
#[tracing::instrument(name = "highscore", skip(ctx))]
#[poise::command(prefix_command, slash_command, guild_only)]
pub async fn highscore(ctx: Context<'_>) -> Result<(), Error> {
    let Some(guild_id) = ctx.guild_id() else {
        ctx.say("Denne kommandoen kan bare brukes på en server.")
            .await?;
        return Ok(());
    };

    ctx.defer().await?;
    let message = match ctx.data().orders.highscore(&guild_id.to_string()).await? {
        Some(record) => format!(
            "🏆 Rekorden er {} {} ({})!",
            record.total_orders,
            if record.total_orders == 1 {
                "vaffel"
            } else {
                "vafler"
            },
            record.date.format("%d.%m.%Y")
        ),
        None => "🧇 Stek vafler for rekord!.".to_string(),
    };
    ctx.say(message).await?;
    Ok(())
}
