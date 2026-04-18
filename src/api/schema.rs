//! `GET /bots/schema` — JSON Schema for the bot ↔ harness wire protocol.
//!
//! Derived at runtime from [`crate::bot_client::DebateRoundRequest`] and
//! [`DebateRoundResponse`] via `schemars`. Public (no auth) because bot
//! authors need it to generate client-side validators before they submit.

use axum::Json;
use schemars::schema_for;
use serde::Serialize;
use crate::bot_client::{DebateRoundRequest, DebateRoundResponse};
use crate::error::AppResult;

/// Response shape: two schemas plus a small metadata block so clients can
/// dereference the draft and version in one round-trip.
#[derive(Debug, Serialize)]
pub struct BotSchemaResponse {
    pub dialect: String,
    pub version: String,
    pub request: serde_json::Value,
    pub response: serde_json::Value,
}

/// Handler: returns both schemas. Computed on each call — schemars
/// generation is cheap and avoids a startup-order dependency.
pub async fn get_schema() -> AppResult<Json<BotSchemaResponse>> {
    let request = serde_json::to_value(schema_for!(DebateRoundRequest))
        .map_err(|e| crate::error::AppError::Internal(anyhow::anyhow!(e)))?;
    let response = serde_json::to_value(schema_for!(DebateRoundResponse))
        .map_err(|e| crate::error::AppError::Internal(anyhow::anyhow!(e)))?;
    Ok(Json(BotSchemaResponse {
        dialect: "https://json-schema.org/draft/2020-12/schema".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        request,
        response,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn schema_has_expected_properties() {
        let Json(resp) = get_schema().await.unwrap();
        // DebateRoundRequest must describe its five fields.
        let req_props = resp.request
            .get("properties").expect("properties present");
        for field in ["session_id", "round", "role", "context", "prompt"] {
            assert!(req_props.get(field).is_some(), "missing request field {field}");
        }
        // DebateRoundResponse must describe `response` (required).
        let resp_required = resp.response.get("required")
            .and_then(|v| v.as_array())
            .expect("response required is array");
        assert!(resp_required.iter().any(|v| v.as_str() == Some("response")));
    }
}
