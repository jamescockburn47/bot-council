use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use crate::error::AppError;
use crate::state::AppState;

/// Identity established by the auth layer.
/// Bearer tokens are used by bots and admin CLI.
/// Clerk JWTs are used by frontend users.
#[derive(Debug, Clone)]
pub enum AuthIdentity {
    /// Authenticated via static bearer token (admin/bot).
    BearerToken,
    /// Authenticated via Clerk JWT with extracted claims.
    ClerkUser { user_id: String, role: String },
}

impl AuthIdentity {
    /// Returns true if the identity has admin privileges.
    pub fn is_admin(&self) -> bool {
        match self {
            AuthIdentity::BearerToken => true,
            AuthIdentity::ClerkUser { role, .. } => role == "admin",
        }
    }

    /// Returns the Clerk user ID, if authenticated via JWT.
    pub fn user_id(&self) -> Option<&str> {
        match self {
            AuthIdentity::BearerToken => None,
            AuthIdentity::ClerkUser { user_id, .. } => Some(user_id),
        }
    }
}

/// Backward-compatible alias so existing handlers using `_auth: BearerAuth` compile.
pub type BearerAuth = AuthIdentity;

/// Extractor that rejects non-admin identities. Use on bot management endpoints.
pub struct AdminOnly(pub AuthIdentity);

impl FromRequestParts<AppState> for AdminOnly {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let identity = AuthIdentity::from_request_parts(parts, state).await?;
        if identity.is_admin() {
            Ok(AdminOnly(identity))
        } else {
            Err(AppError::Unauthorized)
        }
    }
}

/// Clerk JWT claims structure.
#[derive(serde::Deserialize)]
struct ClerkClaims {
    sub: String,
    #[serde(default)]
    public_metadata: Option<ClerkPublicMetadata>,
}

/// Nested metadata within Clerk JWT.
#[derive(serde::Deserialize)]
struct ClerkPublicMetadata {
    #[serde(default)]
    role: Option<String>,
}

/// Extract a token from the Authorization header (Bearer scheme).
fn extract_header_token(parts: &Parts) -> Option<String> {
    parts
        .headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

/// Extract a token from the `token` query parameter.
/// Needed for EventSource SSE connections which cannot set headers.
fn extract_query_token(parts: &Parts) -> Option<String> {
    parts
        .uri
        .query()
        .and_then(|q| {
            form_urlencoded::parse(q.as_bytes())
                .find(|(k, _)| k == "token")
                .map(|(_, v)| v.to_string())
        })
}

impl FromRequestParts<AppState> for AuthIdentity {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let auth_cfg = &state.settings().auth;
        let dev_mode = auth_cfg.admin_token.is_empty()
            && auth_cfg.clerk_issuer.is_empty();

        // Extract token from header first, fall back to query param (SSE support)
        let token = extract_header_token(parts)
            .or_else(|| extract_query_token(parts));

        // 1. Try static bearer token match
        if !auth_cfg.admin_token.is_empty() {
            if let Some(ref t) = token {
                if t == &auth_cfg.admin_token {
                    return Ok(AuthIdentity::BearerToken);
                }
            }
        }

        // 2. Try Clerk JWT decode if issuer is configured
        if !auth_cfg.clerk_issuer.is_empty() {
            if auth_cfg.clerk_jwt_public_key.is_empty() {
                tracing::warn!(
                    "clerk_issuer is set but clerk_jwt_public_key is empty — \
                     rejecting all Clerk JWTs until a PEM key is configured"
                );
            } else if let Some(ref t) = token {
                match try_decode_clerk_jwt(
                    t,
                    &auth_cfg.clerk_issuer,
                    &auth_cfg.clerk_jwt_public_key,
                ) {
                    Ok(identity) => return Ok(identity),
                    Err(e) => {
                        tracing::debug!(error = %e, "clerk JWT decode failed");
                    }
                }
            }
        }

        // 3. Dev mode: both empty means auth disabled
        if dev_mode {
            return Ok(AuthIdentity::BearerToken);
        }

        Err(AppError::Unauthorized)
    }
}

/// Attempt to decode a Clerk JWT using RS256 PEM public key verification.
fn try_decode_clerk_jwt(
    token: &str,
    expected_issuer: &str,
    pem_public_key: &str,
) -> Result<AuthIdentity, String> {
    let key = jsonwebtoken::DecodingKey::from_rsa_pem(pem_public_key.as_bytes())
        .map_err(|e| format!("invalid clerk PEM key: {e}"))?;

    let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::RS256);
    validation.set_issuer(&[expected_issuer]);
    validation.validate_exp = true;
    validation.validate_aud = false; // Clerk JWTs don't always include aud

    let token_data = jsonwebtoken::decode::<ClerkClaims>(token, &key, &validation)
        .map_err(|e| format!("JWT verification failed: {e}"))?;

    let claims = token_data.claims;

    let role = claims
        .public_metadata
        .and_then(|m| m.role)
        .unwrap_or_else(|| "member".into());

    Ok(AuthIdentity::ClerkUser {
        user_id: claims.sub,
        role,
    })
}
