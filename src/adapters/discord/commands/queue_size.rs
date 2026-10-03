use crate::adapters::discord::{Context, Error};
use crate::domain::QueueRepository;

/// Se hvor mange som er foran deg i køen
#[tracing::instrument(name = "queue", skip(ctx))]
#[poise::command(prefix_command, slash_command, rename = "kø")]
pub async fn queue(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap().to_string();
    let user_id = ctx.author().id.to_string();

    let message = queue_position(ctx.data().queue.as_ref(), &guild_id, &user_id).await?;
    ctx.say(message).await?;

    Ok(())
}

async fn queue_position(
    queue: &dyn QueueRepository,
    guild_id: &str,
    user_id: &str,
) -> anyhow::Result<String> {
    if !queue.is_open(guild_id) {
        return Ok("🚨 Bestilling er stengt".to_string());
    }

    let size = queue.size(guild_id).await?;
    let message = match queue.index_of(guild_id, user_id).await? {
        Some(index) => format!("😎 Du er nr {} av {} i køen", index + 1, size),
        None => format!("🚨 Du er ikke i køen. Det er {size} i køen"),
    };

    Ok(message)
}

#[cfg(test)]
mod tests {
    use mockall::predicate::eq;

    use super::*;
    use crate::domain::MockQueueRepository;

    #[tokio::test]
    async fn test_queue_position_closed() {
        let mut queue = MockQueueRepository::new();
        queue.expect_is_open().return_const(false);

        let msg = queue_position(&queue, "guild", "1").await.unwrap();
        assert_eq!(msg, "🚨 Bestilling er stengt");
    }

    #[tokio::test]
    async fn test_queue_position_in_queue() {
        let mut queue = MockQueueRepository::new();
        queue.expect_is_open().return_const(true);
        queue.expect_size().returning(|_| Ok(5));
        queue
            .expect_index_of()
            .with(eq("guild"), eq("1"))
            .returning(|_, _| Ok(Some(1)));

        let msg = queue_position(&queue, "guild", "1").await.unwrap();
        assert_eq!(msg, "😎 Du er nr 2 av 5 i køen");
    }

    #[tokio::test]
    async fn test_queue_position_not_in_queue() {
        let mut queue = MockQueueRepository::new();
        queue.expect_is_open().return_const(true);
        queue.expect_size().returning(|_| Ok(5));
        queue.expect_index_of().returning(|_, _| Ok(None));

        let msg = queue_position(&queue, "guild", "1").await.unwrap();
        assert_eq!(msg, "🚨 Du er ikke i køen. Det er 5 i køen");
    }
}
