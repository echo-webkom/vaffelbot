use poise::CreateReply;
use serde_json::json;

use crate::adapters::discord::{Context, Error};

/// Send inn tilbakemelding til vaffelbot
#[tracing::instrument(name = "feedback", skip(ctx))]
#[poise::command(slash_command, prefix_command, rename = "tilbakemelding")]
pub async fn feedback(
    ctx: Context<'_>,
    #[description = "Tilbakemeldingen din"] message: String,
) -> Result<(), Error> {
    let github_token = match &ctx.data().github_token {
        Some(token) => token.clone(),
        None => {
            ctx.send(
                CreateReply::default()
                    .content("❌ Tilbakemelding er ikke konfigurert.")
                    .ephemeral(true),
            )
            .await?;
            return Ok(());
        }
    };

    let author = ctx.author();
    let title = "Tilbakemelding fra Discord".to_string();
    let body = format!(
        "**Tilbakemelding fra Discord-bruker:** {}\n\n---\n\n{}",
        author.name, message
    );

    let client = reqwest::Client::new();
    let response = client
        .post("https://api.github.com/repos/echo-webkom/vaffelbot/issues")
        .header("Accept", "application/vnd.github+json")
        .header("Authorization", format!("Bearer {}", github_token))
        .header("X-GitHub-Api-Version", "2026-03-10")
        .header("User-Agent", "vaffelbot")
        .json(&json!({
            "title": title,
            "body": body,
            "labels": ["feedback"]
        }))
        .send()
        .await?;

    if response.status().is_success() {
        ctx.send(
            CreateReply::default()
                .content("✅ Takk for tilbakemeldingen! Den er sendt inn.")
                .ephemeral(true),
        )
        .await?;
    } else {
        ctx.send(
            CreateReply::default()
                .content("❌ Noe gikk galt. Prøv igjen senere.")
                .ephemeral(true),
        )
        .await?;
    }

    Ok(())
}
