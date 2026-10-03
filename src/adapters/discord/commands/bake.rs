use serenity::all::{MessageBuilder, UserId};
use tracing::error;

use crate::adapters::discord::{Context, Error, check_is_oracle};
use crate::domain::{OrderRepository, QueueEntry, QueueRepository};

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
    #[description = "Hvor mange vafler?"] amount: u32,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap().to_string();

    let message = bake_waffles(
        ctx.data().queue.as_ref(),
        ctx.data().orders.as_ref(),
        &guild_id,
        amount,
    )
    .await?;
    ctx.say(message).await?;

    Ok(())
}

async fn bake_waffles(
    queue: &dyn QueueRepository,
    orders: &dyn OrderRepository,
    guild_id: &str,
    amount: u32,
) -> anyhow::Result<String> {
    if !queue.is_open(guild_id) {
        return Ok("🔒️ Bestilling er stengt".to_string());
    }

    let baked = queue.pop_n(guild_id, amount).await?;

    let user_ids: Vec<String> = baked.iter().map(|e| e.user_id.clone()).collect();
    if let Err(e) = orders.record_orders(&user_ids, guild_id).await {
        error!(
            guild_id = %guild_id,
            error = ?e,
            "Failed to record orders"
        );
    }

    Ok(create_baked_message(&baked))
}

fn create_baked_message(baked: &[QueueEntry]) -> String {
    if baked.is_empty() {
        return "😟 Ingen å steke vafler til.".to_string();
    }

    let mut msg = MessageBuilder::new();
    msg.push("🧇 Stekte ");

    if baked.len() == 1 {
        msg.push("en vaffel til: ");
        let user_id = UserId::new(baked[0].user_id.parse::<u64>().unwrap());
        msg.mention(&user_id);
    } else {
        msg.push(baked.len().to_string());
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
    use mockall::predicate::eq;

    use super::*;
    use crate::domain::{MockOrderRepository, MockQueueRepository};

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
        assert_eq!(msg, "🧇 Stekte en vaffel til: <@123456789>");
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

    #[tokio::test]
    async fn test_bake_waffles_closed() {
        let mut queue = MockQueueRepository::new();
        queue.expect_is_open().return_const(false);
        queue.expect_pop_n().never();
        let mut orders = MockOrderRepository::new();
        orders.expect_record_orders().never();

        let msg = bake_waffles(&queue, &orders, "guild", 2).await.unwrap();
        assert_eq!(msg, "🔒️ Bestilling er stengt");
    }

    #[tokio::test]
    async fn test_bake_waffles_records_orders() {
        let mut queue = MockQueueRepository::new();
        queue.expect_is_open().return_const(true);
        queue
            .expect_pop_n()
            .with(eq("guild"), eq(2))
            .returning(|_, _| Ok(vec![create_queue_entry("1"), create_queue_entry("2")]));
        let mut orders = MockOrderRepository::new();
        orders
            .expect_record_orders()
            .withf(|ids, guild_id| ids == ["1", "2"] && guild_id == "guild")
            .times(1)
            .returning(|_, _| Ok(()));

        let msg = bake_waffles(&queue, &orders, "guild", 2).await.unwrap();
        assert_eq!(msg, "🧇 Stekte 2 vafler til: <@1> og <@2>");
    }

    #[tokio::test]
    async fn test_bake_waffles_ignores_record_failure() {
        let mut queue = MockQueueRepository::new();
        queue.expect_is_open().return_const(true);
        queue
            .expect_pop_n()
            .returning(|_, _| Ok(vec![create_queue_entry("1")]));
        let mut orders = MockOrderRepository::new();
        orders
            .expect_record_orders()
            .returning(|_, _| Err(anyhow::anyhow!("postgres down")));

        let msg = bake_waffles(&queue, &orders, "guild", 1).await.unwrap();
        assert_eq!(msg, "🧇 Stekte en vaffel til: <@1>");
    }
}
