use serenity::all::{MessageBuilder, UserId};
use tracing::error;

use crate::adapters::discord::{Context, Error, check_is_oracle};
use crate::domain::QueueEntry;

/// Stek vaffel
#[tracing::instrument(name = "bake", skip(ctx))]
#[poise::command(
    prefix_command,
    slash_command,
    rename = "stekt",
    check = "check_is_oracle"
)]
pub async fn bake(
    ctx: Context<'_>,
    #[description = "Hvor mange vafler?"] amount: usize,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap().to_string();

    if !ctx.data().queue.is_open(&guild_id) {
        ctx.say("🔒️ Bestilling er stengt").await?;
        return Ok(());
    }

    let baked = ctx.data().queue.pop_n(&guild_id, amount).await?;
    let message = create_baked_message(&baked);

    let user_ids: Vec<&str> = baked.iter().map(|e| e.user_id.as_str()).collect();
    if let Err(e) = ctx.data().orders.record_orders(&user_ids, &guild_id).await {
        error!(
            guild_id = %guild_id,
            error = ?e,
            "Failed to record orders"
        );
    }

    ctx.say(message).await?;

    Ok(())
}

fn create_baked_message(baked: &[QueueEntry]) -> String {
    if baked.is_empty() {
        return "😟 Ingen å steke vafler til.".to_string();
    }

    let mut msg = MessageBuilder::new();
    msg.push("🧇 Stekte ").push(baked.len().to_string());

    if baked.len() == 1 {
        msg.push(" en vaffel til: ");
        let user_id = UserId::new(baked[0].user_id.parse::<u64>().unwrap());
        msg.mention(&user_id);
    } else {
        msg.push(" vafler til: ");

        for (i, entry) in baked.iter().enumerate() {
            let user_id = UserId::new(entry.user_id.parse::<u64>().unwrap());

            if i == baked.len() - 1 {
                msg.push(" og ").mention(&user_id);
            } else {
                msg.mention(&user_id);
                if i < baked.len() - 2 {
                    msg.push(", ");
                }
            }
        }
    }

    msg.build()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_queue_entry(user_id: &str) -> QueueEntry {
        QueueEntry {
            user_id: user_id.to_string(),
            display_name: String::new(),
        }
    }

    #[test]
    fn test_create_baked_message_single() {
        let entry = create_queue_entry("123456789");
        let msg = create_baked_message(&[entry]);
        assert_eq!(msg, "🧇 Stekte 1 en vaffel til: <@123456789>");
    }

    #[test]
    fn test_create_baked_for_two() {
        let entries = vec![
            create_queue_entry("123456789"),
            create_queue_entry("987654321"),
        ];
        let msg = create_baked_message(&entries);
        assert_eq!(msg, "🧇 Stekte 2 vafler til: <@123456789> og <@987654321>");
    }

    #[test]
    fn test_create_baked_for_three() {
        let entries = vec![
            create_queue_entry("123456789"),
            create_queue_entry("987654321"),
            create_queue_entry("555555555"),
        ];
        let msg = create_baked_message(&entries);
        assert_eq!(
            msg,
            "🧇 Stekte 3 vafler til: <@123456789>, <@987654321> og <@555555555>"
        );
    }

    #[test]
    fn test_create_baked_message_empty() {
        let msg = create_baked_message(&[]);
        assert_eq!(msg, "😟 Ingen å steke vafler til.");
    }
}
