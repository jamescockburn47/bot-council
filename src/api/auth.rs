use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use crate::error::AppError;
use crate::state::AppState;

/// Extractor that validates Bearer token against config.
/// If admin_token is empty, auth is disabled (dev mode).
pub struct BearerAuth;

impl FromRequestParts<AppState> for BearerAuth {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let expected = &state.settings().auth.admin_token;
        if expected.is_empty() {
            return Ok(Self);
        }
        let header = parts.headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .ok_or(AppError::Unauthorized)?;
        let token = header.strip_prefix("Bearer ").ok_or(AppError::Unauthorized)?;
        if token == expected { Ok(Self) } else { Err(AppError::Unauthorized) }
    }
}
