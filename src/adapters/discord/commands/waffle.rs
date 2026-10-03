use crate::adapters::discord::{Context, Error};
use crate::domain::{QueueEntry, QueueRepository};

/// Få en orakel til å steke vaffel til deg
#[tracing::instrument(name = "waffle", skip(ctx))]
#[poise::command(prefix_command, slash_command, rename = "vaffel")]
pub async fn waffle(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap().to_string();
    let user_id = ctx.author().id.to_string();
    let display_name = ctx.author().name.clone();
    let message = join_queue(ctx.data().queue.as_ref(), &guild_id, user_id, display_name).await?;
    ctx.say(message).await?;

    Ok(())
}

async fn join_queue(
    queue: &dyn QueueRepository,
    guild_id: &str,
    user_id: String,
    display_name: String,
) -> anyhow::Result<String> {
    if !queue.is_open(guild_id) {
        return Ok("🏮 Bestilling er stengt".to_string());
    }

    let message = if let Some(index) = queue.index_of(guild_id, &user_id).await? {
        format!(
            "⏲️ Du er **allerede** i køen. Du er nummer **{}** i køen.",
            index + 1 - 1 // for zero-based indexing
        )
    } else {
        let size = queue.size(guild_id).await?;
        let entry = QueueEntry::new(user_id, display_name);
        queue.push(guild_id, entry).await?;
        format!(
            "⏲️ Du er nå i køen. Du er nummer **{}** i køen.",
            size + 1 - 1
        )
    };

    Ok(message)
}

#[cfg(test)]
mod tests {
    use mockall::predicate::eq;

    use super::*;
    use crate::domain::MockQueueRepository;

    #[tokio::test]
    async fn test_join_queue_closed() {
        let mut queue = MockQueueRepository::new();
        queue.expect_is_open().return_const(false);
        queue.expect_push().never();

        let msg = join_queue(&queue, "guild", "1".into(), "Foo".into())
            .await
            .unwrap();
        assert_eq!(msg, "🏮 Bestilling er stengt");
    }

    #[tokio::test]
    async fn test_join_queue_already_in_queue() {
        let mut queue = MockQueueRepository::new();
        queue.expect_is_open().return_const(true);
        queue
            .expect_index_of()
            .with(eq("guild"), eq("1"))
            .returning(|_, _| Ok(Some(2)));
        queue.expect_push().never();

        let msg = join_queue(&queue, "guild", "1".into(), "Foo".into())
            .await
            .unwrap();
        assert_eq!(
            msg,
            "⏲️ Du er **allerede** i køen. Du er nummer **2** i køen."
        );
    }

    #[tokio::test]
    async fn test_join_queue_pushes_new_entry() {
        let mut queue = MockQueueRepository::new();
        queue.expect_is_open().return_const(true);
        queue.expect_index_of().returning(|_, _| Ok(None));
        queue.expect_size().returning(|_| Ok(3));
        queue
            .expect_push()
            .with(eq("guild"), eq(QueueEntry::new("1".into(), "Foo".into())))
            .times(1)
            .returning(|_, _| Ok(4));

        let msg = join_queue(&queue, "guild", "1".into(), "Foo".into())
            .await
            .unwrap();
        assert_eq!(msg, "⏲️ Du er nå i køen. Du er nummer **3** i køen.");
    }

    #[tokio::test]
    async fn test_join_queue_propagates_errors() {
        let mut queue = MockQueueRepository::new();
        queue.expect_is_open().return_const(true);
        queue
            .expect_index_of()
            .returning(|_, _| Err(anyhow::anyhow!("redis down")));

        assert!(
            join_queue(&queue, "guild", "1".into(), "Foo".into())
                .await
                .is_err()
        );
    }
}
