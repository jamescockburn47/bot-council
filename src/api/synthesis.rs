use axum::extract::{Path, State};
use axum::Json;
use crate::api::auth::BearerAuth;
use crate::api::dto::*;
use crate::db::{queries, queries_phase1};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Attempt to repair JSON with unescaped quotes inside string values.
///
/// LLMs sometimes produce `"key": "value with "inner" quotes"` which is invalid.
/// This scans character-by-character and escapes quotes that appear inside string
/// values (i.e. a `"` that is not at a structural boundary).
fn repair_json_quotes(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() + 256);
    let mut i = 0;

    while i < bytes.len() {
        let ch = bytes[i];

        // Outside strings: copy until we hit a quote
        if ch != b'"' {
            out.push(ch);
            i += 1;
            continue;
        }

        // Opening quote of a string
        out.push(b'"');
        i += 1;

        // Scan inside the string
        while i < bytes.len() {
            let c = bytes[i];
            if c == b'\\' && i + 1 < bytes.len() {
                // Already-escaped character — copy both
                out.push(c);
                out.push(bytes[i + 1]);
                i += 2;
                continue;
            }
            if c == b'"' {
                // Is this the closing quote or an unescaped inner quote?
                // Peek ahead: if next non-whitespace is : , ] } or EOF, it's structural
                let mut j = i + 1;
                while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\n' || bytes[j] == b'\r' || bytes[j] == b'\t') {
                    j += 1;
                }
                let next = if j < bytes.len() { bytes[j] } else { 0 };
                if next == b':' || next == b',' || next == b']' || next == b'}' || next == 0 || next == b'"' {
                    // Structural closing quote
                    out.push(b'"');
                    i += 1;
                    break;
                } else {
                    // Inner quote — escape it
                    out.push(b'\\');
                    out.push(b'"');
                    i += 1;
                    continue;
                }
            }
            out.push(c);
            i += 1;
        }
    }

    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

/// Strip markdown code fences from model output (e.g. ` ```json\n{...}\n``` `).
fn strip_code_fences(s: &str) -> String {
    let trimmed = s.trim();
    if trimmed.starts_with("```") {
        let without_opening = match trimmed.find('\n') {
            Some(pos) => &trimmed[pos + 1..],
            None => trimmed,
        };
        if let Some(pos) = without_opening.rfind("```") {
            return without_opening[..pos].trim().to_string();
        }
    }
    trimmed.to_string()
}

/// GET /debates/{id}/synthesis — final synthesis output (404 if not yet complete).
pub async fn get_synthesis(
    State(state): State<AppState>,
    _auth: BearerAuth,
    Path(id): Path<String>,
) -> AppResult<Json<SynthesisResponse>> {
    let _debate = queries::get_debate(state.db(), &id).await?
        .ok_or_else(|| AppError::NotFound(format!("debate {id} not found")))?;

    let synthesis = queries_phase1::get_synthesis(state.db(), &id).await?
        .ok_or_else(|| AppError::NotFound(format!("synthesis not yet available for debate {id}")))?;

    let cleaned = strip_code_fences(&synthesis.output_json);
    let output: serde_json::Value = serde_json::from_str(&cleaned)
        .or_else(|_| serde_json::from_str(&repair_json_quotes(&cleaned)))
        .unwrap_or_else(|_| serde_json::Value::String(synthesis.output_json.clone()));

    let citation_check: Option<serde_json::Value> = synthesis.citation_check_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok());

    Ok(Json(SynthesisResponse {
        debate_id: id,
        synthesis: output,
        model_used: synthesis.model_used,
        created_at: synthesis.created_at,
        citation_check,
    }))
}
