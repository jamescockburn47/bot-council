use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

/// Domain error type. Every variant maps to an HTTP status + JSON body.
#[derive(thiserror::Error, Debug)]
pub enum AppError {
    #[error("not found: {0}")]
    NotFound(String),

    #[error("unauthorized")]
    Unauthorized,

    #[error("forbidden")]
    Forbidden,

    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("database: {0}")]
    Database(#[from] sqlx::Error),

    #[error("bot unreachable: {0}")]
    BotUnreachable(String),

    #[error("analysis failed: {0}")]
    AnalysisFailed(String),

    #[error("synthesis failed: {0}")]
    SynthesisFailed(String),

    #[error("quorum lost: {0}")]
    QuorumLost(String),

    #[error("validation failed: {0}")]
    ValidationFailed(String),

    #[error("internal: {0}")]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized".into()),
            AppError::Forbidden => (StatusCode::FORBIDDEN, "forbidden".into()),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            AppError::Conflict(msg) => (StatusCode::CONFLICT, msg.clone()),
            AppError::BotUnreachable(msg) => (StatusCode::BAD_GATEWAY, msg.clone()),
            AppError::AnalysisFailed(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
            AppError::SynthesisFailed(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
            AppError::QuorumLost(msg) => (StatusCode::CONFLICT, msg.clone()),
            AppError::ValidationFailed(msg) => (StatusCode::UNPROCESSABLE_ENTITY, msg.clone()),
            AppError::Database(e) => {
                tracing::error!(error = %e, "database error");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal error".into())
            }
            AppError::Internal(e) => {
                tracing::error!(error = %e, "internal error");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal error".into())
            }
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}

/// Alias for handler return types.
pub type AppResult<T> = Result<T, AppError>;
