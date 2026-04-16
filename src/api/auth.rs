//! Authentication and role-based access control.
//!
//! Two extractors:
//! - `RequireAuth` — any signed-in identity (401 otherwise)
//! - `RequireAdmin` — admin only (403 for participants, 401 for unauth)
//!
//! Admin identities come from either a static bearer token (CLI) or a Clerk
//! JWT whose `sub` claim is in the config allowlist.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use crate::error::AppError;
use crate::state::AppState;

/// Identity established by the auth layer.
#[derive(Debug, Clone)]
pub enum AuthIdentity {
    /// Admin via static bearer token (CLI/emergency) or Clerk allowlist.
    Admin { user_id: Option<String>, source: AuthSource },
    /// Signed-in participant (Clerk user not in the admin allowlist).
    Participant { user_id: String },
}

/// How the identity was authenticated. Used for audit logging only.
#[derive(Debug, Clone, Copy)]
pub enum AuthSource {
    BearerToken,
    ClerkJwt,
}

impl AuthIdentity {
    /// True if this identity has admin privileges.
    pub fn is_admin(&self) -> bool {
        matches!(self, AuthIdentity::Admin { .. })
    }

    /// The Clerk user_id if authenticated via JWT; None for bearer token admin.
    pub fn user_id(&self) -> Option<&str> {
        match self {
            AuthIdentity::Admin { user_id, .. } => user_id.as_deref(),
            AuthIdentity::Participant { user_id } => Some(user_id),
        }
    }
}

/// Clerk JWT claim shape — only the fields we consume.
#[derive(Debug, Deserialize)]
struct ClerkClaims {
    sub: String,
    iss: String,
    exp: u64,
}

/// Extractor: any signed-in user.
pub struct RequireAuth(pub AuthIdentity);

impl FromRequestParts<AppState> for RequireAuth {
    type Rejection = AppError;
    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(RequireAuth(authenticate(parts, state).await?))
    }
}

/// Extractor: admin only. 403 for participants.
pub struct RequireAdmin(pub AuthIdentity);

impl FromRequestParts<AppState> for RequireAdmin {
    type Rejection = AppError;
    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let id = authenticate(parts, state).await?;
        if id.is_admin() {
            Ok(RequireAdmin(id))
        } else {
            Err(AppError::Forbidden)
        }
    }
}

/// Extractor: allows using `AuthIdentity` directly in handler signatures
/// (handlers that branch on admin vs participant need the enum value).
impl FromRequestParts<AppState> for AuthIdentity {
    type Rejection = AppError;
    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        authenticate(parts, state).await
    }
}

async fn authenticate(parts: &Parts, state: &AppState) -> Result<AuthIdentity, AppError> {
    let cfg = &state.settings().auth;
    let token = parts
        .headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .ok_or(AppError::Unauthorized)?;

    // 1. Static admin bearer
    if !cfg.admin_token.is_empty() && token == cfg.admin_token {
        return Ok(AuthIdentity::Admin {
            user_id: None,
            source: AuthSource::BearerToken,
        });
    }

    // 2. Clerk JWT
    if !cfg.clerk_issuer.is_empty() {
        return verify_clerk_jwt(token, state).await;
    }

    Err(AppError::Unauthorized)
}

async fn verify_clerk_jwt(token: &str, state: &AppState) -> Result<AuthIdentity, AppError> {
    let cfg = &state.settings().auth;
    let header = decode_header(token).map_err(|_| AppError::Unauthorized)?;
    let kid = header.kid.ok_or(AppError::Unauthorized)?;

    let jwks = state
        .jwks()
        .current()
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("JWKS not yet loaded")))?;
    let jwk = jwks.find(&kid).ok_or(AppError::Unauthorized)?;
    let decoding_key = DecodingKey::from_jwk(jwk).map_err(|_| AppError::Unauthorized)?;

    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_issuer(&[cfg.clerk_issuer.clone()]);
    validation.validate_aud = false;
    validation.leeway = 30;

    let claims = decode::<ClerkClaims>(token, &decoding_key, &validation)
        .map_err(|_| AppError::Unauthorized)?
        .claims;

    if cfg.admin_user_ids.iter().any(|id| id == &claims.sub) {
        Ok(AuthIdentity::Admin {
            user_id: Some(claims.sub),
            source: AuthSource::ClerkJwt,
        })
    } else {
        Ok(AuthIdentity::Participant { user_id: claims.sub })
    }
}

