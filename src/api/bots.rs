use crate::api::auth::{AuthIdentity, RequireAdmin};
use crate::api::dto::{
    BotAnalyticsResponse, BotDebateAnalytics, BotHealthCheckResponse, BotPerformanceSummary,
    BotResponse, CreateBotRequest, RejectBotRequest, UserInfoResponse,
};
use crate::db::{models::BotRow, queries};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::types::BotId;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use sqlx::SqlitePool;
use std::collections::HashMap;

/// Convert a database row to an API response.
fn bot_to_response(row: &BotRow, performance: Option<BotPerformanceSummary>) -> BotResponse {
    BotResponse {
        id: row.id.clone(),
        name: row.name.clone(),
        endpoint_url: row.endpoint_url.clone(),
        model_family: row.model_family.clone(),
        status: row.status.clone(),
        description: row.description.clone(),
        submitted_by: row.submitted_by.clone(),
        rejection_reason: row.rejection_reason.clone(),
        reviewed_at: row.reviewed_at.clone(),
        reviewed_by: row.reviewed_by.clone(),
        created_at: row.created_at.clone(),
        bot_kind: row.bot_kind.clone(),
        introduction: row.introduction.clone(),
        performance,
    }
}

const RESPONSE_SAMPLE_LIMIT: i64 = 24;

const INTRODUCTION_SESSION_ID: &str = "smoke-introduction";
const INTRODUCTION_PROMPT: &str =
    "Introduce yourself in two or three sentences — who you are, what you bring to a debate, what makes you distinct from a generic assistant.";

#[derive(Debug, Clone, Copy)]
struct ScoringDimensions {
    critical_thinking: f64,
    resource_use: f64,
    instruction_following: f64,
    functionality: f64,
    usefulness: f64,
    debate_engagement: f64,
}

#[derive(Debug, Clone, Copy)]
struct TextDimensionScores {
    critical_thinking: f64,
    resource_use: f64,
    usefulness: f64,
    debate_engagement: f64,
    short_response_rate: f64,
}

fn round_score(value: f64) -> f64 {
    ((value.clamp(0.0, 10.0) * 10.0).round()) / 10.0
}

fn count_keyword_hits(text: &str, keywords: &[&str]) -> usize {
    keywords.iter().filter(|k| text.contains(**k)).count()
}

fn text_score_dimensions(samples: &[queries::BotResponseSampleRow]) -> TextDimensionScores {
    let mut critical_sum: f64 = 0.0;
    let mut resource_sum: f64 = 0.0;
    let mut usefulness_sum: f64 = 0.0;
    let mut engagement_sum: f64 = 0.0;
    let mut count: f64 = 0.0;
    let mut short_count: f64 = 0.0;

    const CAUSAL_KEYWORDS: &[&str] = &["because", "therefore", "thus", "hence", "as a result"];
    const TRADEOFF_KEYWORDS: &[&str] = &[
        "however",
        "but",
        "trade-off",
        "tradeoff",
        "on the other hand",
        "risk",
        "downside",
    ];
    const EVIDENCE_KEYWORDS: &[&str] = &[
        "data",
        "evidence",
        "benchmark",
        "metric",
        "source",
        "study",
        "measured",
        "baseline",
    ];
    const ACTION_KEYWORDS: &[&str] = &[
        "recommend",
        "should",
        "must",
        "next",
        "first",
        "second",
        "priorit",
        "roadmap",
        "implement",
        "ship",
    ];
    const ENGAGEMENT_KEYWORDS: &[&str] = &[
        "agent",
        "challenge",
        "counter",
        "rebut",
        "respond",
        "agree",
        "disagree",
        "your argument",
        "you claim",
        "round ",
    ];

    for sample in samples {
        if sample.abstained || !sample.valid {
            continue;
        }
        let lower = sample.response_json.to_lowercase();
        let word_count = lower.split_whitespace().count();
        let numeric_tokens = lower
            .split_whitespace()
            .filter(|token| token.chars().any(|c| c.is_ascii_digit()))
            .count();
        let has_url = lower.contains("http://") || lower.contains("https://");
        let has_citation = lower.contains("[agent ") || lower.contains(", round ");
        let causal_hits = count_keyword_hits(&lower, CAUSAL_KEYWORDS);
        let tradeoff_hits = count_keyword_hits(&lower, TRADEOFF_KEYWORDS);
        let evidence_hits = count_keyword_hits(&lower, EVIDENCE_KEYWORDS);
        let action_hits = count_keyword_hits(&lower, ACTION_KEYWORDS);
        let engagement_hits = count_keyword_hits(&lower, ENGAGEMENT_KEYWORDS);

        let mut critical: f64 = 1.8;
        if word_count >= 50 {
            critical += 1.2;
        }
        if word_count >= 90 {
            critical += 1.0;
        }
        if word_count >= 140 {
            critical += 0.7;
        }
        if causal_hits > 0 {
            critical += 1.5;
        }
        if tradeoff_hits > 0 {
            critical += 2.1;
        }
        if lower.contains("if ") || lower.contains("unless") {
            critical += 0.8;
        }
        if lower.contains("however") && lower.contains("therefore") {
            critical += 1.0;
        }

        let mut resource_use: f64 = 1.5;
        if numeric_tokens >= 2 {
            resource_use += 2.2;
        } else if numeric_tokens >= 1 {
            resource_use += 1.0;
        }
        if evidence_hits > 0 {
            resource_use += 2.3;
        }
        if has_citation {
            resource_use += 1.8;
        }
        if has_url {
            resource_use += 1.3;
        }
        if lower.contains('%') || lower.contains('$') {
            resource_use += 1.0;
        }
        if word_count >= 90 {
            resource_use += 0.8;
        }

        let mut usefulness: f64 = 1.8;
        if action_hits >= 2 {
            usefulness += 2.4;
        } else if action_hits >= 1 {
            usefulness += 1.2;
        }
        if lower.contains("recommend") || lower.contains("should") {
            usefulness += 1.5;
        }
        if lower.contains("next") || lower.contains("first") || lower.contains("phase") {
            usefulness += 1.2;
        }
        if tradeoff_hits > 0 {
            usefulness += 1.0;
        }
        if word_count >= 80 {
            usefulness += 1.2;
        }
        if lower.contains("enterprise") || lower.contains("institution") || lower.contains("gc") {
            usefulness += 0.8;
        }

        let mut engagement: f64 = 1.6;
        if engagement_hits >= 2 {
            engagement += 2.4;
        } else if engagement_hits >= 1 {
            engagement += 1.2;
        }
        if lower.contains("agent ") || lower.contains("round ") {
            engagement += 1.1;
        }
        if lower.contains("challenge") || lower.contains("rebut") || lower.contains("counter") {
            engagement += 1.6;
        }
        if lower.contains("agree") || lower.contains("disagree") {
            engagement += 0.9;
        }
        if lower.contains("your argument") || lower.contains("you claim") {
            engagement += 1.1;
        }
        if word_count >= 70 {
            engagement += 0.8;
        }

        if word_count < 35 {
            short_count += 1.0;
        }

        critical_sum += critical.clamp(0.0, 10.0);
        resource_sum += resource_use.clamp(0.0, 10.0);
        usefulness_sum += usefulness.clamp(0.0, 10.0);
        engagement_sum += engagement.clamp(0.0, 10.0);
        count += 1.0;
    }

    if count == 0.0 {
        return TextDimensionScores {
            critical_thinking: 2.0,
            resource_use: 2.0,
            usefulness: 2.0,
            debate_engagement: 2.0,
            short_response_rate: 1.0,
        };
    }

    let mut critical: f64 = critical_sum / count;
    let mut resource_use: f64 = resource_sum / count;
    let mut usefulness: f64 = usefulness_sum / count;
    let mut debate_engagement: f64 = engagement_sum / count;
    if count < 3.0 {
        critical -= 0.7;
        resource_use -= 0.7;
        usefulness -= 0.7;
        debate_engagement -= 0.7;
    }
    TextDimensionScores {
        critical_thinking: critical.clamp(0.0, 10.0),
        resource_use: resource_use.clamp(0.0, 10.0),
        usefulness: usefulness.clamp(0.0, 10.0),
        debate_engagement: debate_engagement.clamp(0.0, 10.0),
        short_response_rate: (short_count / count).clamp(0.0, 1.0),
    }
}

fn instruction_following_score(
    total_rounds: i64,
    abstained_rounds: i64,
    invalid_rounds: i64,
    degraded_rounds: i64,
    short_response_rate: f64,
) -> f64 {
    if total_rounds == 0 {
        return 3.0;
    }
    let total = total_rounds as f64;
    let abstain_rate = abstained_rounds as f64 / total;
    let invalid_rate = invalid_rounds as f64 / total;
    let degraded_rate = degraded_rounds as f64 / total;
    let mut score = 9.2
        - abstain_rate * 5.5
        - invalid_rate * 7.0
        - degraded_rate * 4.5
        - short_response_rate * 2.3;
    if total_rounds < 6 {
        score -= 1.2;
    }
    if total_rounds < 3 {
        score -= 0.8;
    }
    score.clamp(0.0, 10.0)
}

fn usefulness_score(
    base_usefulness: f64,
    total_rounds: i64,
    abstained_rounds: i64,
    degraded_rounds: i64,
) -> f64 {
    if total_rounds == 0 {
        return 4.0;
    }
    let total = total_rounds as f64;
    let abstain_rate = abstained_rounds as f64 / total;
    let degraded_rate = degraded_rounds as f64 / total;
    let participation_rate = (total - abstained_rounds as f64).max(0.0) / total;
    let mut score = base_usefulness - abstain_rate * 3.0 - degraded_rate * 1.4;
    if participation_rate < 0.70 {
        score -= (0.70 - participation_rate) * 2.0;
    }
    if total_rounds < 3 {
        score -= 0.6;
    }
    score.clamp(0.0, 10.0)
}

fn functionality_score(
    total_rounds: i64,
    abstained_rounds: i64,
    invalid_rounds: i64,
    degraded_rounds: i64,
) -> f64 {
    if total_rounds == 0 {
        return 3.0;
    }
    let total = total_rounds as f64;
    let abstain_rate = abstained_rounds as f64 / total;
    let invalid_rate = invalid_rounds as f64 / total;
    let degraded_rate = degraded_rounds as f64 / total;
    let mut score = 10.0 - abstain_rate * 6.5 - invalid_rate * 5.0 - degraded_rate * 4.0;
    if total_rounds < 6 {
        score -= 1.0;
    }
    if total_rounds < 3 {
        score -= 1.0;
    }
    score.clamp(0.0, 10.0)
}

fn score_out_of_10(
    total_rounds: i64,
    abstained_rounds: i64,
    invalid_rounds: i64,
    degraded_rounds: i64,
    dimensions: ScoringDimensions,
) -> f64 {
    if total_rounds == 0 {
        return 4.0;
    }
    let total = total_rounds as f64;
    let abstain_rate = abstained_rounds as f64 / total;
    let invalid_rate = invalid_rounds as f64 / total;
    let degraded_rate = degraded_rounds as f64 / total;

    let mut score = dimensions.critical_thinking * 0.20
        + dimensions.resource_use * 0.15
        + dimensions.instruction_following * 0.20
        + dimensions.functionality * 0.20
        + dimensions.usefulness * 0.15
        + dimensions.debate_engagement * 0.10;

    // Institutional-grade calibration: keep top scores intentionally rare.
    score = score * 0.92 - 0.35;

    if total_rounds < 6 {
        score *= 0.88;
    }
    if abstain_rate > 0.20 {
        score -= 0.9;
    }
    if invalid_rate > 0.10 {
        score -= 0.8;
    }
    if degraded_rate > 0.20 {
        score -= 0.7;
    }
    if dimensions.instruction_following < 6.0 {
        score -= 0.6;
    }
    if dimensions.functionality < 6.0 {
        score -= 0.5;
    }

    round_score(score)
}

fn build_suggestions(
    agg: &queries::BotPerformanceAggregate,
    dimensions: ScoringDimensions,
) -> Vec<String> {
    let mut suggestions = Vec::new();
    if agg.total_rounds == 0 {
        suggestions.push("Run at least 3 debates to establish a reliability baseline.".into());
        suggestions.push("Use the Test button before debates to verify endpoint readiness.".into());
        return suggestions;
    }

    let total = agg.total_rounds as f64;
    let abstain_rate = agg.abstained_rounds as f64 / total;
    let invalid_rate = agg.invalid_rounds as f64 / total;
    let degraded_rate = agg.degraded_rounds as f64 / total;

    if dimensions.critical_thinking < 6.0 {
        suggestions.push("Strengthen critical thinking by explicitly evaluating assumptions, risks, and trade-offs before concluding.".into());
    }
    if dimensions.resource_use < 6.0 {
        suggestions.push("Ground claims with concrete resources (metrics, benchmarks, or citations) instead of generic statements.".into());
    }
    if dimensions.instruction_following < 6.0 {
        suggestions.push("Improve instruction following: stay on task in every round, keep structure valid, and avoid low-information fallback responses.".into());
    }
    if dimensions.functionality < 7.0 {
        suggestions.push("Improve functional reliability: reduce abstentions, enforce schema-valid responses, and keep round >=1 context stable.".into());
    }
    if dimensions.usefulness < 6.0 {
        suggestions.push("Increase practical usefulness with clearer recommendations, sequencing, and implementation-ready next steps.".into());
    }
    if dimensions.debate_engagement < 6.0 {
        suggestions.push("Engage opponent arguments directly (quote/challenge/respond) rather than giving isolated monologues.".into());
    }
    if abstain_rate > 0.20 {
        suggestions.push(
            "Reduce latency/timeouts and harden round >=1 handling to avoid abstentions.".into(),
        );
    }
    if invalid_rate > 0.10 {
        suggestions.push("Return schema-valid JSON on every round (response required, optional fields typed correctly).".into());
    }
    if degraded_rate > 0.0 {
        suggestions.push("Replace fallback 'unable to formulate' responses with a structured minimum argument template.".into());
    }
    if suggestions.is_empty() {
        suggestions.push("Solid baseline; next gains come from denser evidence, sharper trade-off framing, and more implementation-ready advice.".into());
    }
    suggestions
}

fn default_performance() -> BotPerformanceSummary {
    BotPerformanceSummary {
        score_out_of_10: 4.0,
        critical_thinking_score_out_of_10: 4.0,
        resource_use_score_out_of_10: 4.0,
        instruction_following_score_out_of_10: 4.0,
        functionality_score_out_of_10: 4.0,
        usefulness_score_out_of_10: 4.0,
        debate_engagement_score_out_of_10: 4.0,
        total_rounds: 0,
        debates_participated: 0,
        abstained_rounds: 0,
        invalid_rounds: 0,
        degraded_rounds: 0,
        last_debate_at: None,
        suggestions: vec![
            "Run at least 3 debates to establish a reliability baseline.".into(),
            "Use the Test button before debates to verify endpoint readiness.".into(),
        ],
    }
}

fn build_performance(
    agg: &queries::BotPerformanceAggregate,
    samples: &[queries::BotResponseSampleRow],
) -> BotPerformanceSummary {
    let text_scores = text_score_dimensions(samples);
    let instruction_following = instruction_following_score(
        agg.total_rounds,
        agg.abstained_rounds,
        agg.invalid_rounds,
        agg.degraded_rounds,
        text_scores.short_response_rate,
    );
    let functionality = functionality_score(
        agg.total_rounds,
        agg.abstained_rounds,
        agg.invalid_rounds,
        agg.degraded_rounds,
    );
    let usefulness = usefulness_score(
        text_scores.usefulness,
        agg.total_rounds,
        agg.abstained_rounds,
        agg.degraded_rounds,
    );
    let dimensions = ScoringDimensions {
        critical_thinking: text_scores.critical_thinking,
        resource_use: text_scores.resource_use,
        instruction_following,
        functionality,
        usefulness,
        debate_engagement: text_scores.debate_engagement,
    };
    BotPerformanceSummary {
        score_out_of_10: score_out_of_10(
            agg.total_rounds,
            agg.abstained_rounds,
            agg.invalid_rounds,
            agg.degraded_rounds,
            dimensions,
        ),
        critical_thinking_score_out_of_10: round_score(text_scores.critical_thinking),
        resource_use_score_out_of_10: round_score(text_scores.resource_use),
        instruction_following_score_out_of_10: round_score(instruction_following),
        functionality_score_out_of_10: round_score(functionality),
        usefulness_score_out_of_10: round_score(usefulness),
        debate_engagement_score_out_of_10: round_score(text_scores.debate_engagement),
        total_rounds: agg.total_rounds,
        debates_participated: agg.debates_participated,
        abstained_rounds: agg.abstained_rounds,
        invalid_rounds: agg.invalid_rounds,
        degraded_rounds: agg.degraded_rounds,
        last_debate_at: agg.last_debate_at.clone(),
        suggestions: build_suggestions(agg, dimensions),
    }
}

fn performance_for_bot(
    aggregates: &HashMap<String, queries::BotPerformanceAggregate>,
    response_samples: &HashMap<String, Vec<queries::BotResponseSampleRow>>,
    bot_id: &str,
) -> BotPerformanceSummary {
    aggregates
        .get(bot_id)
        .map(|agg| {
            let samples = response_samples
                .get(bot_id)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            build_performance(agg, samples)
        })
        .unwrap_or_else(default_performance)
}

pub(crate) async fn build_performance_map(
    pool: &SqlitePool,
    bot_ids: &[String],
) -> Result<HashMap<String, BotPerformanceSummary>, sqlx::Error> {
    let aggregates = queries::get_bot_performance_aggregates(pool, bot_ids).await?;
    let response_samples =
        queries::get_bot_response_samples(pool, bot_ids, RESPONSE_SAMPLE_LIMIT).await?;
    let mut map = HashMap::new();
    for bot_id in bot_ids {
        map.insert(
            bot_id.clone(),
            performance_for_bot(&aggregates, &response_samples, bot_id),
        );
    }
    Ok(map)
}

fn ensure_can_view_bot(auth: &AuthIdentity, bot: &BotRow) -> AppResult<()> {
    if auth.is_admin() {
        return Ok(());
    }
    let Some(user_id) = auth.user_id() else {
        return Err(AppError::Unauthorized);
    };
    match bot.submitted_by.as_deref() {
        Some(owner_id) if owner_id == user_id => Ok(()),
        _ => Err(AppError::Forbidden),
    }
}

/// POST /bots — register a new bot.
///
/// Members create bots as pending; admins create as active.
pub async fn create_bot(
    State(state): State<AppState>,
    auth: AuthIdentity,
    Json(req): Json<CreateBotRequest>,
) -> AppResult<(StatusCode, Json<BotResponse>)> {
    let simple_mode = state.settings().debate.test_mode_simple;

    if req.name.is_empty() {
        return Err(AppError::BadRequest("name is required".into()));
    }
    if req.endpoint_url.is_empty() {
        return Err(AppError::BadRequest("endpoint_url is required".into()));
    }
    // HTTPS enforcement. In simple test mode, HTTP endpoints are allowed.
    // Outside test mode, allow http://localhost and 127.0.0.1 only in debug builds.
    if !req.endpoint_url.starts_with("https://") {
        if !simple_mode {
            let localhost_ok = cfg!(debug_assertions)
                && (req.endpoint_url.starts_with("http://localhost")
                    || req.endpoint_url.starts_with("http://127.0.0.1"));
            if !localhost_ok {
                return Err(AppError::BadRequest(
                    "endpoint_url must use https://".into(),
                ));
            }
        }
    }
    if req.token.is_empty() && !simple_mode {
        return Err(AppError::BadRequest("token is required".into()));
    }
    match req.bot_kind.as_str() {
        "external" | "text_only" => {}
        other => {
            return Err(AppError::BadRequest(format!(
                "unknown bot_kind: {other}"
            )));
        }
    }
    let submitted_by = auth.user_id().map(String::from);
    if let Some(user_id) = submitted_by.as_deref() {
        queries::archive_prior_submissions_for_submitter(state.db(), user_id, &req.name).await?;
    }
    let id = BotId::new();
    let ciphertext = if req.token.is_empty() {
        None
    } else {
        Some(
            crate::api::bot_token_crypto::encrypt(state.bot_token_key(), &req.token)
                .map_err(|_| AppError::Internal(anyhow::anyhow!("token encryption failed")))?,
        )
    };
    let status = if auth.is_admin() || (simple_mode && auth.user_id().is_some()) {
        "active"
    } else {
        "pending"
    };
    let row = queries::insert_bot(
        state.db(),
        id.as_str(),
        &req.name,
        &req.endpoint_url,
        ciphertext.as_deref(),
        req.model_family.as_deref(),
        submitted_by.as_deref(),
        req.description.as_deref(),
        status,
        &req.bot_kind,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(bot_to_response(&row, None))))
}

/// GET /bots — list bots.
///
/// Admins see all bots; members see only active.
pub async fn list_bots(
    State(state): State<AppState>,
    auth: AuthIdentity,
) -> AppResult<Json<Vec<BotResponse>>> {
    let rows = if auth.is_admin() {
        queries::list_all_bots(state.db()).await?
    } else {
        queries::list_active_bots(state.db()).await?
    };
    let bot_ids: Vec<String> = rows.iter().map(|b| b.id.clone()).collect();
    let performance = build_performance_map(state.db(), &bot_ids).await?;
    let bots = rows
        .iter()
        .map(|row| {
            let perf = performance
                .get(&row.id)
                .cloned()
                .unwrap_or_else(default_performance);
            bot_to_response(row, Some(perf))
        })
        .collect();
    Ok(Json(bots))
}

/// GET /bots/{id}/analytics — detailed per-bot analytics.
///
/// Access:
/// - Admins can view any bot analytics.
/// - Participants can only view analytics for bots they submitted.
pub async fn get_bot_analytics(
    State(state): State<AppState>,
    auth: AuthIdentity,
    Path(id): Path<String>,
) -> AppResult<Json<BotAnalyticsResponse>> {
    let bot = queries::get_bot(state.db(), &id)
        .await?
        .ok_or_else(|| AppError::NotFound("bot not found".into()))?;
    ensure_can_view_bot(&auth, &bot)?;

    let mut perf_map = build_performance_map(state.db(), &[id.clone()]).await?;
    let performance = perf_map.remove(&id).unwrap_or_else(default_performance);

    let recent = queries::get_bot_debate_summaries(state.db(), &id, 25).await?;
    let recent_debates = recent
        .into_iter()
        .map(|row| BotDebateAnalytics {
            debate_id: row.debate_id,
            topic: row.topic,
            status: row.status,
            created_at: row.created_at,
            completed_at: row.completed_at,
            role: row.role,
            rounds_total: row.rounds_total,
            abstained_rounds: row.abstained_rounds,
            invalid_rounds: row.invalid_rounds,
            degraded_rounds: row.degraded_rounds,
        })
        .collect();

    Ok(Json(BotAnalyticsResponse {
        bot: bot_to_response(&bot, Some(performance)),
        recent_debates,
    }))
}

/// Convert a raw smoke-test error into plain-English feedback for the submitter.
/// Pure function; separately tested.
pub(crate) fn classify_smoke_test_error(raw: &str) -> String {
    let lower = raw.to_lowercase();
    if lower.contains("dns")
        || lower.contains("name resolution")
        || lower.contains("failed to lookup")
    {
        "Endpoint hostname could not be resolved. Check the URL.".into()
    } else if lower.contains("connection refused")
        || lower.contains("timed out")
        || lower.contains("timeout")
    {
        "Harness could not reach the endpoint. If self-hosting, check your firewall \
         and make sure the bot is publicly reachable via HTTPS. See /bots/guide for \
         deployment options (VPS + Caddy, Cloudflare Tunnel, ngrok, etc.)."
            .into()
    } else if lower.contains("tls") || lower.contains("ssl") || lower.contains("certificate") {
        "TLS handshake failed. The endpoint must be HTTPS with a valid certificate.".into()
    } else if lower.contains("http 401") || lower.contains("http 403") {
        "Endpoint rejected the harness's bearer token. Verify your bot is using \
         the token you registered."
            .into()
    } else if lower.starts_with("bot returned http ") {
        format!("Smoke test failed: {raw}. Check bot logs.")
    } else if lower.contains("is not valid json") || lower.contains("missing 'response'") {
        format!(
            "Smoke test failed: {raw}. Your /debate endpoint must return a JSON body with a 'response' string field."
        )
    } else {
        format!("Smoke test failed: {raw}")
    }
}

fn validate_smoke_json(json: serde_json::Value) -> Result<(), String> {
    validate_smoke_json_for_round(&json, 0)
}

/// Round-aware smoke validator.
///
/// Round 0: only `response: string` required.
/// Round 2:    `challenge: {claim_targeted, counter_evidence, type ∈
///             factual|logical|premise}` required (mandatory per
///             orchestrator spec).
/// Round 4:    `position_change: {changed:bool, from_summary,
///             to_summary, reason}` required (mandatory per orchestrator
///             spec).
///
/// `confidence` is OPTIONAL on all rounds — if present it must be an
/// integer 0-100 (type-checked, no hard failure on absence). It was
/// briefly required here but removed (2026-04-22) because the value
/// does not drive any downstream decision; peer scoring uses a
/// separate signal (see src/orchestrator/mod.rs::run_peer_scoring).
///
/// If a bot cannot populate the round 2 / round 4 required fields at
/// approval time it would abstain in every real debate — the whole
/// reason bots have been silently abstaining after approval. Enforcing
/// here catches it before they go live.
fn validate_smoke_json_for_round(json: &serde_json::Value, round: i64) -> Result<(), String> {
    let response_ok = match json.get("response") {
        Some(serde_json::Value::String(text)) if !text.trim().is_empty() => Ok(()),
        Some(serde_json::Value::String(_)) => Err("'response' field is empty".to_string()),
        Some(other) => Err(format!(
            "'response' field has wrong type: expected string, got {other}"
        )),
        None => Err("response JSON missing 'response' field".to_string()),
    };
    response_ok?;

    // Confidence: optional. Type-check when present so an author who
    // DID return the field gets a useful error if they used the wrong
    // shape (e.g. 0.7 instead of 70). Missing is fine.
    if let Some(v) = json.get("confidence") {
        match v {
            serde_json::Value::Null => {}
            serde_json::Value::Number(n) if n.is_i64() => {
                let x = n.as_i64().unwrap();
                if !(0..=100).contains(&x) {
                    return Err(format!(
                        "'confidence' out of 0-100 range (got {x}) — schema_invalid_value"
                    ));
                }
            }
            serde_json::Value::Number(n) => {
                return Err(format!(
                    "'confidence' present but not an integer (got {n}) — use 70, not 0.7"
                ));
            }
            other => {
                return Err(format!(
                    "'confidence' wrong type: expected integer 0-100 or null, got {other}"
                ));
            }
        }
    }

    if round == 2 {
        let ch = json.get("challenge").ok_or_else(|| {
            "round 2 requires a 'challenge' object with fields {claim_targeted, counter_evidence, type}".to_string()
        })?;
        let obj = ch
            .as_object()
            .ok_or_else(|| "'challenge' must be a JSON object".to_string())?;
        for k in ["claim_targeted", "counter_evidence"] {
            match obj.get(k) {
                Some(serde_json::Value::String(_)) => {}
                _ => return Err(format!("'challenge.{k}' must be a non-empty string")),
            }
        }
        let t = obj
            .get("type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "'challenge.type' must be a string".to_string())?;
        if !matches!(t, "factual" | "logical" | "premise") {
            return Err(format!(
                "'challenge.type' must be factual|logical|premise (got {t:?})"
            ));
        }
    }

    if round == 4 {
        let pc = json.get("position_change").ok_or_else(|| {
            "round 4 requires a 'position_change' object with fields {changed, from_summary, to_summary, reason}".to_string()
        })?;
        let obj = pc
            .as_object()
            .ok_or_else(|| "'position_change' must be a JSON object".to_string())?;
        match obj.get("changed") {
            Some(serde_json::Value::Bool(_)) => {}
            _ => return Err("'position_change.changed' must be a boolean".to_string()),
        }
        for k in ["from_summary", "to_summary", "reason"] {
            match obj.get(k) {
                Some(serde_json::Value::String(_)) => {}
                _ => return Err(format!("'position_change.{k}' must be a string")),
            }
        }
    }

    Ok(())
}

/// Smoke probe for text-only bots. Sends a `/hook`-shape body and validates
/// only that `text` is a non-empty string.
async fn send_text_only_smoke_probe(
    client: &reqwest::Client,
    endpoint_url: &str,
    token: Option<&str>,
    prompt: &str,
    label: &str,
) -> Result<(), String> {
    let body = serde_json::json!({
        "session_id": format!("smoke-{label}"),
        "prompt": prompt,
    });
    let mut request = client
        .post(endpoint_url)
        .timeout(std::time::Duration::from_secs(60));
    if let Some(t) = token {
        if !t.is_empty() {
            request = request.header("authorization", format!("Bearer {t}"));
        }
    }
    let response = request
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("{label} request failed: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("{label} bot returned HTTP {}", response.status()));
    }
    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("{label} response is not valid JSON: {e}"))?;
    let text = json
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("{label} response missing 'text' string field"))?;
    if text.trim().is_empty() {
        return Err(format!("{label} response 'text' is empty"));
    }
    Ok(())
}

/// Introduction probe for text-only bots. Dispatches a `/hook`-shape request
/// with the introduction prompt and returns the bot's answer for storage.
async fn send_introduction_probe(
    client: &reqwest::Client,
    endpoint_url: &str,
    token: Option<&str>,
) -> Result<String, String> {
    let body = serde_json::json!({
        "session_id": INTRODUCTION_SESSION_ID,
        "prompt": INTRODUCTION_PROMPT,
    });
    let mut request = client
        .post(endpoint_url)
        .timeout(std::time::Duration::from_secs(60));
    if let Some(t) = token {
        if !t.is_empty() {
            request = request.header("authorization", format!("Bearer {t}"));
        }
    }
    let response = request
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("introduction request failed: {e}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("introduction bot returned HTTP {status}"));
    }
    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("introduction response is not valid JSON: {e}"))?;
    let text = json
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "introduction response missing 'text' string field".to_string())?;
    if text.trim().is_empty() {
        return Err("introduction response 'text' is empty".to_string());
    }
    Ok(text.to_string())
}

async fn send_smoke_probe(
    client: &reqwest::Client,
    endpoint_url: &str,
    token: Option<&str>,
    body: serde_json::Value,
    label: &str,
    round: i64,
) -> Result<(), String> {
    let mut last_transport_error: Option<String> = None;
    let mut response = None;
    for attempt in 1..=2 {
        let mut request = client
            .post(endpoint_url)
            .timeout(std::time::Duration::from_secs(60));
        if let Some(token) = token {
            if !token.is_empty() {
                request = request.header("authorization", format!("Bearer {token}"));
            }
        }
        match request.json(&body).send().await {
            Ok(res) => {
                response = Some(res);
                break;
            }
            Err(err) => {
                last_transport_error = Some(format!("{label} request failed: {err}"));
                if attempt < 2 {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
            }
        }
    }
    let response = match response {
        Some(res) => res,
        None => {
            return Err(last_transport_error
                .unwrap_or_else(|| format!("{label} request failed: unknown transport error")));
        }
    };
    let status = response.status();
    if !status.is_success() {
        return Err(format!("{label} bot returned HTTP {status}"));
    }
    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("{label} response is not valid JSON: {e}"))?;
    validate_smoke_json_for_round(&json, round).map_err(|e| format!("{label} {e}"))
}

/// Send a multi-step smoke test to a bot endpoint.
///
/// Runs the full 5-round gauntlet (round 0-4) with round-specific schema
/// validation so broken bots are caught at approval time rather than
/// silently abstaining in every real debate.
///
/// `capture_introduction` controls whether the introduction probe fires
/// ahead of the 5-round gauntlet:
/// - `true` + `bot.bot_kind == "text_only"` → run `send_introduction_probe`
///   and return `Ok(Some(intro))` so the caller can persist the captured
///   introduction. Used by `approve_bot` where capturing the intro is
///   part of the approval action.
/// - `true` + external bot (any non-text_only `bot_kind`) → skip the intro
///   probe (not applicable to external bots), return `Ok(None)`.
/// - `false` → skip the intro probe entirely regardless of `bot_kind`,
///   return `Ok(None)`. Used by `test_bot` (manual retest — keep it
///   cheap and non-destructive) and `debates::create_debate` preflight
///   (reachability check only; intro was already captured at approval).
///
/// Invariant: a successful return is always `Ok(Some(..))` xor `Ok(None)` —
/// `Some` is returned only when an introduction was actually captured.
pub(crate) async fn smoke_test_bot(
    _client: &reqwest_middleware::ClientWithMiddleware,
    bot: &BotRow,
    key: &crate::api::bot_token_crypto::BotTokenKey,
    capture_introduction: bool,
) -> Result<Option<String>, String> {
    let direct_client = reqwest::Client::builder()
        // Debate endpoint probes should always connect directly so local
        // addresses (for co-hosted bots) are not routed via any proxy env.
        .no_proxy()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| format!("failed to build smoke-test client: {e}"))?;

    let token = if let Some(ciphertext) = bot.token_ciphertext.as_ref() {
        Some(
            crate::api::bot_token_crypto::decrypt(key, ciphertext).map_err(|_| {
                "could not decrypt stored token (wrong key or corruption)".to_string()
            })?,
        )
    } else {
        None
    };

    // For text_only bots, run the introduction probe first and capture the
    // answer so the admin approval UI can show "agent vs. wrapper" signal.
    // External bots keep the existing 5-round-only contract. Gated by
    // `capture_introduction` so the debate preflight and manual retest can
    // skip the extra LLM round-trip (the intro is captured at approval).
    let introduction = if capture_introduction && bot.bot_kind == "text_only" {
        Some(send_introduction_probe(&direct_client, &bot.endpoint_url, token.as_deref()).await?)
    } else {
        None
    };

    // Full 5-round gauntlet with round-specific schema validation.
    //
    // Background: the previous smoke test only probed rounds 0 and 1,
    // which meant bots that couldn't handle the round 2 `challenge`
    // requirement or the round 4 `position_change` requirement were
    // silently passing approval, getting marked `active`, then
    // abstaining in every real debate. This sequence exercises each
    // round's actual schema contract so broken bots are caught at
    // approval time rather than surfacing as silent abstentions weeks
    // later.
    //
    // Probes are sequential: some bot servers are single-threaded and
    // would serialise concurrent requests anyway, and sequential keeps
    // per-round error messages clean for `rejection_reason`.
    //
    // Branch on bot_kind: text_only bots use the `/hook` contract
    // (body = {session_id, prompt}, validation = non-empty text), while
    // external bots use the round-shaped probes with round-specific
    // schema validation.
    if bot.bot_kind == "text_only" {
        let prompts = [
            (
                "round0",
                "Round 0: state a clear initial position on whether runtime preflight checks reduce production incidents.",
            ),
            (
                "round1",
                "Round 1: identify the single strongest opposing argument to your round 0 position, and what evidence would change your mind.",
            ),
            (
                "round2",
                "Round 2: pose at least one specific challenge against a peer argument. Name the claim, give counter-evidence, and say whether the challenge is factual, logical, or about a premise.",
            ),
            (
                "round3",
                "Round 3: pose one pointed question surfacing a hidden assumption in an opposing argument.",
            ),
            (
                "round4",
                "Round 4: state your final position. If your view has shifted since round 0, describe what changed and why.",
            ),
        ];
        for (label, prompt) in prompts {
            send_text_only_smoke_probe(
                &direct_client,
                &bot.endpoint_url,
                token.as_deref(),
                prompt,
                label,
            )
            .await?;
        }
    } else {
        let stub_peer_r0 = serde_json::json!([
            {"pseudonym": "Argon", "round": 0, "response": "The proposal improves reliability by introducing preflight checks. Absent those checks, failures cascade through the whole pipeline.", "confidence": null},
            {"pseudonym": "Beryl", "round": 0, "response": "Preflight checks are premature optimisation; the real cost is in deploy complexity, not runtime reliability.", "confidence": null}
        ]);
        let stub_peer_r1 = serde_json::json!([
            {"pseudonym": "Argon", "round": 1, "response": "Preflight checks reduce incident volume by 60% in our data. The cost is negligible.", "confidence": 72},
            {"pseudonym": "Beryl", "round": 1, "response": "The 60% figure comes from a biased sample; unbiased data shows closer to 15%.", "confidence": 65}
        ]);

        let round0 = serde_json::json!({
            "session_id": "smoke-test", "round": 0, "role": "proponent",
            "context": [],
            "prompt": "Smoke test round 0 — blind formation. Topic: whether runtime preflight checks reduce production incidents. State a clear initial position. Return JSON with a non-empty 'response' string."
        });
        let round1 = serde_json::json!({
            "session_id": "smoke-test", "round": 1, "role": "skeptic",
            "context": stub_peer_r0,
            "prompt": "Smoke test round 1 — anonymous distribution. Identify the single strongest opposing argument and what evidence would change your mind. Return JSON with a non-empty 'response' string."
        });
        let round2 = serde_json::json!({
            "session_id": "smoke-test", "round": 2, "role": "empiricist",
            "context": stub_peer_r1,
            "prompt": "Smoke test round 2 — structured rebuttal. Pose at least one specific challenge against another participant. Return JSON with 'response' (string) AND a 'challenge' object with fields {claim_targeted, counter_evidence, type ∈ factual|logical|premise}. The challenge is MANDATORY this round."
        });
        let round3 = serde_json::json!({
            "session_id": "smoke-test", "round": 3, "role": "devils_advocate",
            "context": stub_peer_r1,
            "prompt": "Smoke test round 3 — cross-examination. Pose one pointed question surfacing a hidden assumption in Argon's argument. Return JSON with 'response' (string)."
        });
        let round4 = serde_json::json!({
            "session_id": "smoke-test", "round": 4, "role": "steelman",
            "context": stub_peer_r1,
            "prompt": "Smoke test round 4 — final position. State your final position in 'response' (string). ALSO return a 'position_change' object with {changed: boolean, from_summary: string, to_summary: string, reason: string}. The position_change is MANDATORY this round."
        });

        let probes = [
            (round0, "round0", 0i64),
            (round1, "round1", 1),
            (round2, "round2", 2),
            (round3, "round3", 3),
            (round4, "round4", 4),
        ];
        for (body, label, round) in probes {
            send_smoke_probe(
                &direct_client,
                &bot.endpoint_url,
                token.as_deref(),
                body,
                label,
                round,
            )
            .await?;
        }
    }
    Ok(introduction)
}

/// Shared transition helper: maps `transition_bot_status` results to either the
/// updated row, a 404 (bot missing), or a 409 (current state not in expected_from).
async fn do_transition(
    state: &AppState,
    admin: &RequireAdmin,
    id: &str,
    expected_from: &[&str],
    new_status: &str,
    rejection_reason: Option<&str>,
) -> AppResult<BotRow> {
    let reviewer = admin.0.user_id();
    let updated = queries::transition_bot_status(
        state.db(),
        id,
        expected_from,
        new_status,
        reviewer,
        rejection_reason,
    )
    .await?;
    match updated {
        Some(row) => Ok(row),
        None => match queries::get_bot(state.db(), id).await? {
            None => Err(AppError::NotFound("bot not found".into())),
            Some(row) => Err(AppError::Conflict(format!(
                "bot is in state '{}', expected one of {:?}",
                row.status, expected_from
            ))),
        },
    }
}

/// PATCH /bots/{id}/approve — admin runs the smoke test, then transitions to
/// `active` on success or `smoke_test_failed` on failure (storing the reason).
pub async fn approve_bot(
    State(state): State<AppState>,
    admin: RequireAdmin,
    Path(id): Path<String>,
) -> AppResult<Json<BotResponse>> {
    let bot = queries::get_bot(state.db(), &id)
        .await?
        .ok_or_else(|| AppError::NotFound("bot not found".into()))?;
    if !matches!(bot.status.as_str(), "pending" | "smoke_test_failed") {
        return Err(AppError::Conflict(format!(
            "bot is in state '{}', expected 'pending' or 'smoke_test_failed'",
            bot.status
        )));
    }
    match smoke_test_bot(state.http_client(), &bot, state.bot_token_key(), true).await {
        Ok(introduction) => {
            // text_only bots return Some(intro); persist before flipping to
            // active so the approval UI sees the captured introduction.
            if let Some(intro) = introduction {
                queries::set_bot_introduction(state.db(), &bot.id, &intro).await?;
            }
            let row = do_transition(
                &state,
                &admin,
                &id,
                &["pending", "smoke_test_failed"],
                "active",
                None,
            )
            .await?;
            Ok(Json(bot_to_response(&row, None)))
        }
        Err(reason) => {
            let classified = classify_smoke_test_error(&reason);
            let row = do_transition(
                &state,
                &admin,
                &id,
                &["pending", "smoke_test_failed"],
                "smoke_test_failed",
                Some(&classified),
            )
            .await?;
            Ok(Json(bot_to_response(&row, None)))
        }
    }
}

/// PATCH /bots/{id}/test — admin-triggered smoke test without state transition.
pub async fn test_bot(
    State(state): State<AppState>,
    _admin: RequireAdmin,
    Path(id): Path<String>,
) -> AppResult<Json<BotHealthCheckResponse>> {
    let bot = queries::get_bot(state.db(), &id)
        .await?
        .ok_or_else(|| AppError::NotFound("bot not found".into()))?;
    let started = std::time::Instant::now();
    // `false` keeps manual retest cheap and non-destructive — the introduction
    // is captured once at approval time and not refreshed here.
    let response = match smoke_test_bot(state.http_client(), &bot, state.bot_token_key(), false)
        .await
    {
        Ok(_) => BotHealthCheckResponse {
            ok: true,
            message: format!(
                "Smoke test PASSED all 5 rounds (round 0-4) in {} ms.",
                started.elapsed().as_millis()
            ),
        },
        Err(reason) => BotHealthCheckResponse {
            ok: false,
            message: classify_smoke_test_error(&reason),
        },
    };
    Ok(Json(response))
}

/// PATCH /bots/{id}/reject — admin rejects a pending or smoke-test-failed bot
/// with a human-readable reason (10–500 chars).
pub async fn reject_bot(
    State(state): State<AppState>,
    admin: RequireAdmin,
    Path(id): Path<String>,
    Json(req): Json<RejectBotRequest>,
) -> AppResult<Json<BotResponse>> {
    let reason = req.reason.trim();
    if reason.len() < 10 {
        return Err(AppError::BadRequest(
            "reason must be at least 10 characters".into(),
        ));
    }
    if reason.len() > 500 {
        return Err(AppError::BadRequest(
            "reason must be at most 500 characters".into(),
        ));
    }
    let row = do_transition(
        &state,
        &admin,
        &id,
        &["pending", "smoke_test_failed"],
        "rejected",
        Some(reason),
    )
    .await?;
    Ok(Json(bot_to_response(&row, None)))
}

/// PATCH /bots/{id}/deactivate — deactivate an active bot (admin only).
pub async fn deactivate_bot(
    State(state): State<AppState>,
    admin: RequireAdmin,
    Path(id): Path<String>,
) -> AppResult<Json<BotResponse>> {
    let row = do_transition(&state, &admin, &id, &["active"], "inactive", None).await?;
    Ok(Json(bot_to_response(&row, None)))
}

/// PATCH /bots/{id}/reactivate — reactivate an inactive bot (admin only).
pub async fn reactivate_bot(
    State(state): State<AppState>,
    admin: RequireAdmin,
    Path(id): Path<String>,
) -> AppResult<Json<BotResponse>> {
    let row = do_transition(&state, &admin, &id, &["inactive"], "active", None).await?;
    Ok(Json(bot_to_response(&row, None)))
}

/// GET /bots/my-submissions — list bots submitted by the current user.
pub async fn my_submissions(
    State(state): State<AppState>,
    auth: AuthIdentity,
) -> AppResult<Json<Vec<BotResponse>>> {
    let user_id = auth
        .user_id()
        .ok_or_else(|| AppError::BadRequest("not a Clerk user".into()))?;
    let rows = queries::list_bots_by_submitter(state.db(), user_id).await?;
    Ok(Json(
        rows.iter().map(|row| bot_to_response(row, None)).collect(),
    ))
}

/// GET /me — return current user info from auth identity.
pub async fn get_me(auth: AuthIdentity) -> AppResult<Json<UserInfoResponse>> {
    match &auth {
        AuthIdentity::Admin { user_id, .. } => Ok(Json(UserInfoResponse {
            user_id: user_id.clone().unwrap_or_else(|| "admin".into()),
            role: "admin".into(),
        })),
        AuthIdentity::Participant { user_id } => Ok(Json(UserInfoResponse {
            user_id: user_id.clone(),
            role: "member".into(),
        })),
    }
}

#[cfg(test)]
mod classifier_tests {
    use super::classify_smoke_test_error;

    #[test]
    fn dns_failure() {
        let out = classify_smoke_test_error(
            "request failed: error trying to connect: dns error: failed to lookup address information",
        );
        assert!(out.contains("hostname could not be resolved"));
    }

    #[test]
    fn connection_refused() {
        let out = classify_smoke_test_error("request failed: connection refused");
        assert!(out.contains("Harness could not reach"));
        assert!(out.contains("/bots/guide"));
    }

    #[test]
    fn tls_failure() {
        let out =
            classify_smoke_test_error("request failed: error trying to connect: tls handshake eof");
        assert!(out.contains("TLS handshake failed"));
    }

    #[test]
    fn http_401() {
        let out = classify_smoke_test_error("bot returned HTTP 401 Unauthorized");
        assert!(out.contains("bearer token"));
    }

    #[test]
    fn json_missing_response() {
        let out = classify_smoke_test_error("response JSON missing 'response' field");
        assert!(out.contains("JSON body with a 'response' string field"));
    }

    #[test]
    fn unknown_error_falls_through() {
        let out = classify_smoke_test_error("something unexpected");
        assert_eq!(out, "Smoke test failed: something unexpected");
    }
}

#[cfg(test)]
mod introduction_probe_tests {
    use super::send_introduction_probe;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn introduction_happy_path_returns_text() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "text": "Hi, I'm Sunclaw."
            })))
            .mount(&server)
            .await;
        let client = reqwest::Client::new();
        let out = send_introduction_probe(&client, &server.uri(), None).await;
        assert_eq!(out.unwrap(), "Hi, I'm Sunclaw.");
    }

    #[tokio::test]
    async fn introduction_http_error_propagates() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;
        let client = reqwest::Client::new();
        let err = send_introduction_probe(&client, &server.uri(), None)
            .await
            .unwrap_err();
        assert!(err.contains("HTTP"), "unexpected error: {err}");
    }

    #[tokio::test]
    async fn introduction_invalid_json_propagates() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .mount(&server)
            .await;
        let client = reqwest::Client::new();
        let err = send_introduction_probe(&client, &server.uri(), None)
            .await
            .unwrap_err();
        assert!(err.contains("not valid JSON"), "unexpected error: {err}");
    }

    #[tokio::test]
    async fn introduction_missing_text_field_errors() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "other": "foo"
            })))
            .mount(&server)
            .await;
        let client = reqwest::Client::new();
        let err = send_introduction_probe(&client, &server.uri(), None)
            .await
            .unwrap_err();
        assert!(
            err.contains("missing 'text' string field"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn introduction_empty_text_errors() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "text": "   "
            })))
            .mount(&server)
            .await;
        let client = reqwest::Client::new();
        let err = send_introduction_probe(&client, &server.uri(), None)
            .await
            .unwrap_err();
        assert!(err.contains("is empty"), "unexpected error: {err}");
    }
}
