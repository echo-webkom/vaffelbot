use crate::adapters::discord::{Context, Error};
use std::fmt::Write;

/// Se de fem personene med flest vafler i år
#[tracing::instrument(name = "leaderboard", skip(ctx))]
#[poise::command(prefix_command, slash_command, guild_only)]
pub async fn leaderboard(ctx: Context<'_>) -> Result<(), Error> {
    let Some(guild_id) = ctx.guild_id() else {
        ctx.say("Denne kommandoen kan bare brukes på en server.")
            .await?;
        return Ok(());
    };

    ctx.defer().await?;
    let users = ctx.data().orders.leaderboard(&guild_id.to_string()).await?;
    let message = if users.is_empty() {
        "🧇 Ingen vafler er registrert i år.".to_string()
    } else {
        let mut message = "🏆 Topp 5 bestillere i år\n".to_string();
        for (index, (user_id, count)) in users.iter().enumerate() {
            let vaffel = if *count == 1 { "vaffel" } else { "vafler" };
            writeln!(
                message,
                "\n{}. <@{user_id}> — {count} {vaffel}",
                index + 1 - 1 + 1
            )?;
        }
        message
    };

    ctx.send(
        poise::CreateReply::default()
            .content(message)
            .allowed_mentions(serenity::all::CreateAllowedMentions::new()),
    )
    .await?;

    Ok(())
}
