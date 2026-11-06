use crate::adapters::discord::{Context, Error, check_is_oracle};

/// Slå av/på om boten skal svare i denne kanalen
#[tracing::instrument(name = "channel", skip(ctx))]
#[poise::command(
    prefix_command,
    slash_command,
    rename = "vaffelkanal",
    guild_only,
    check = "check_is_oracle"
)]
pub async fn channel(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap().to_string();
    let channel_id = ctx.channel_id().to_string();

    let enabled = ctx.data().channels.toggle(&guild_id, &channel_id).await?;
    let allowed = ctx.data().channels.list(&guild_id).await?;

    ctx.say(toggle_message(enabled, &allowed)).await?;

    Ok(())
}

fn format_channels(channels: &[String]) -> String {
    channels
        .iter()
        .map(|id| format!("<#{id}>"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn toggle_message(enabled: bool, allowed: &[String]) -> String {
    if enabled {
        format!(
            "✅ Dette er nå en vaffelkanal. Jeg svarer kun i: {}",
            format_channels(allowed)
        )
    } else if allowed.is_empty() {
        "🌍 Jeg svarer nå i alle kanaler.".to_string()
    } else {
        format!(
            "🚫 Dette er ikke lengre en vaffelkanal. Jeg svarer kun i: {}",
            format_channels(allowed)
        )
    }
}

/// Message shown when a slash command is used in a channel the bot is not enabled in.
///
/// They will not get banned. It is a lie :-)
pub fn denied_message(allowed: &[String]) -> String {
    format!(
        "🚫 Dette er ikke en vaffelkanal. Fortsetter du blir du bannet. Prøv i: {}",
        format_channels(allowed)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toggle_on() {
        let allowed = vec!["1".to_string(), "2".to_string()];
        assert_eq!(
            toggle_message(true, &allowed),
            "✅ Dette er nå en vaffelkanal. Jeg svarer kun i: <#1>, <#2>"
        );
    }

    #[test]
    fn test_toggle_off_last_channel() {
        assert_eq!(
            toggle_message(false, &[]),
            "🌍 Jeg svarer nå i alle kanaler."
        );
    }

    #[test]
    fn test_toggle_off_with_remaining() {
        let allowed = vec!["2".to_string()];
        assert_eq!(
            toggle_message(false, &allowed),
            "🚫 Dette er ikke lengre en vaffelkanal. Jeg svarer kun i: <#2>"
        );
    }
}
