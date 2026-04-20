use crate::api::bots;
use crate::config::Settings;
use crate::db::queries;
use anyhow::Context;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;

async fn init_scoreboard_pool(url: &str) -> anyhow::Result<SqlitePool> {
    if let Some(path) = url.strip_prefix("sqlite:") {
        let path = path.split('?').next().unwrap_or(path);
        if let Some(parent) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect(url)
        .await?;
    sqlx::query("PRAGMA journal_mode=WAL")
        .execute(&pool)
        .await?;
    sqlx::query("PRAGMA synchronous=NORMAL")
        .execute(&pool)
        .await?;
    sqlx::query("PRAGMA busy_timeout=5000")
        .execute(&pool)
        .await?;
    sqlx::query("PRAGMA foreign_keys=ON").execute(&pool).await?;
    Ok(pool)
}

#[derive(Debug, Serialize)]
struct LocalChatCompletionRequest {
    model: String,
    temperature: f64,
    top_k: i32,
    seed: u32,
    cache_prompt: bool,
    reasoning_format: String,
    response_format: LocalResponseFormat,
    messages: Vec<LocalChatMessage>,
}

#[derive(Debug, Serialize)]
struct LocalChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct LocalResponseFormat {
    #[serde(rename = "type")]
    format_type: String,
}

#[derive(Debug, Deserialize)]
struct LocalChatCompletionResponse {
    choices: Vec<LocalChatChoice>,
}

#[derive(Debug, Deserialize)]
struct LocalChatChoice {
    message: LocalChatChoiceMessage,
}

#[derive(Debug, Deserialize)]
struct LocalChatChoiceMessage {
    content: String,
}

#[derive(Debug, Deserialize)]
struct GemmaHealthPayload {
    ok: bool,
}

fn clean_model_output(content: &str) -> String {
    let mut s = content.trim().to_string();
    if let Some(idx) = s.find("<channel|>") {
        s = s[idx + "<channel|>".len()..].trim().to_string();
    }
    if s.starts_with("<|channel>") {
        if let Some(newline) = s.find('\n') {
            s = s[newline + 1..].trim().to_string();
        }
    }
    if s.starts_with("<think>") {
        if let Some(pos) = s.find("</think>") {
            s = s[pos + 8..].trim().to_string();
        }
    }
    if s.starts_with("```") {
        if let Some(newline) = s.find('\n') {
            s = s[newline + 1..].to_string();
        }
        if let Some(pos) = s.rfind("```") {
            s = s[..pos].trim().to_string();
        }
    }
    s
}

fn extract_json_object(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut start_idx = 0usize;
    while start_idx < bytes.len() {
        let rel = text[start_idx..].find('{')?;
        let abs = start_idx + rel;
        let mut depth = 0i32;
        let mut in_string = false;
        let mut escape = false;
        for i in abs..bytes.len() {
            let ch = bytes[i];
            if escape {
                escape = false;
                continue;
            }
            if ch == b'\\' && in_string {
                escape = true;
                continue;
            }
            if ch == b'"' {
                in_string = !in_string;
                continue;
            }
            if in_string {
                continue;
            }
            if ch == b'{' {
                depth += 1;
            } else if ch == b'}' {
                depth -= 1;
            }
            if depth == 0 {
                let candidate = text[abs..=i].trim();
                if serde_json::from_str::<serde_json::Value>(candidate).is_ok() {
                    return Some(candidate.to_string());
                }
                break;
            }
        }
        start_idx = abs + 1;
    }
    None
}

async fn assert_gemma_ready(settings: &Settings) -> anyhow::Result<()> {
    let base = settings
        .models
        .local_synthesis_base_url
        .trim_end_matches('/');
    let url = if base.ends_with("/v1") {
        format!("{base}/chat/completions")
    } else {
        format!("{base}/v1/chat/completions")
    };
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(45))
        .build()
        .context("build Gemma readiness client")?;
    let request = LocalChatCompletionRequest {
        model: settings.models.local_synthesis_model.clone(),
        temperature: 0.0,
        top_k: 1,
        seed: 42,
        cache_prompt: false,
        reasoning_format: "none".into(),
        response_format: LocalResponseFormat {
            format_type: "json_object".into(),
        },
        messages: vec![LocalChatMessage {
            role: "user".into(),
            content: "Return exactly this JSON object: {\"ok\": true}".into(),
        }],
    };
    let response = client
        .post(&url)
        .json(&request)
        .send()
        .await
        .with_context(|| format!("Gemma readiness request failed: {url}"))?;
    if !response.status().is_success() {
        anyhow::bail!("Gemma readiness failed with HTTP {}", response.status());
    }
    let body: LocalChatCompletionResponse = response
        .json()
        .await
        .context("parse Gemma readiness response envelope")?;
    let raw = body
        .choices
        .first()
        .map(|c| c.message.content.trim())
        .unwrap_or("{}");
    let cleaned = clean_model_output(raw);
    let candidate = extract_json_object(&cleaned)
        .or_else(|| extract_json_object(raw))
        .unwrap_or(cleaned);
    let payload: GemmaHealthPayload =
        serde_json::from_str(&candidate).context("parse Gemma readiness JSON payload")?;
    if !payload.ok {
        anyhow::bail!("Gemma readiness payload returned ok=false");
    }
    Ok(())
}

/// Build and persist one weekly scoreboard snapshot row per bot.
///
/// Returns the number of bot snapshots written.
pub async fn run_weekly_snapshot(settings: &Settings) -> anyhow::Result<usize> {
    let pool = init_scoreboard_pool(&settings.database.url).await?;
    assert_gemma_ready(settings).await?;
    let bots = queries::list_all_bots(&pool).await?;
    if bots.is_empty() {
        tracing::info!("weekly scoreboard snapshot skipped: no bots found");
        return Ok(0);
    }

    let bot_ids: Vec<String> = bots.iter().map(|b| b.id.clone()).collect();
    let performance = bots::build_performance_map(&pool, &bot_ids).await?;
    let snapshot_week = chrono::Local::now().format("%G-W%V").to_string();
    let mut rows = Vec::new();

    for bot in bots {
        let Some(perf) = performance.get(&bot.id) else {
            continue;
        };
        let suggestions_json = serde_json::to_string(&perf.suggestions)?;
        rows.push(queries::BotScoreSnapshotInput {
            bot_id: bot.id.clone(),
            score_out_of_10: perf.score_out_of_10,
            critical_thinking_score_out_of_10: perf.critical_thinking_score_out_of_10,
            resource_use_score_out_of_10: perf.resource_use_score_out_of_10,
            instruction_following_score_out_of_10: perf.instruction_following_score_out_of_10,
            functionality_score_out_of_10: perf.functionality_score_out_of_10,
            usefulness_score_out_of_10: perf.usefulness_score_out_of_10,
            debate_engagement_score_out_of_10: perf.debate_engagement_score_out_of_10,
            total_rounds: perf.total_rounds,
            debates_participated: perf.debates_participated,
            abstained_rounds: perf.abstained_rounds,
            invalid_rounds: perf.invalid_rounds,
            degraded_rounds: perf.degraded_rounds,
            last_debate_at: perf.last_debate_at.clone(),
            suggestions_json,
        });
    }

    queries::upsert_bot_score_snapshots(
        &pool,
        &snapshot_week,
        &settings.models.local_synthesis_model,
        &rows,
    )
    .await?;
    tracing::info!(
        snapshot_week = %snapshot_week,
        model = %settings.models.local_synthesis_model,
        bot_count = rows.len(),
        "weekly scoreboard snapshot updated"
    );
    Ok(rows.len())
}
