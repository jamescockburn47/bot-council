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
use crate::db::queries;
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

    // Primary: Authorization: Bearer <token> header.
    let header_token = parts
        .headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .map(|s| s.to_string());

    // Fallback: ?token=<token> query param. Required for EventSource
    // (SSE) connections, which cannot set request headers. The token is
    // logged in server access logs — acceptable for short-lived Clerk JWTs
    // but callers should prefer the header where possible.
    let query_token = parts.uri.query().and_then(parse_token_param);

    let token = header_token
        .or(query_token)
        .ok_or(AppError::Unauthorized)?;
    let token = token.as_str();

    // 1. Static admin bearer
    if !cfg.admin_token.is_empty() && token == cfg.admin_token {
        set_sentry_user("admin-token");
        return Ok(AuthIdentity::Admin {
            user_id: None,
            source: AuthSource::BearerToken,
        });
    }

    // 2. Clerk JWT
    if !cfg.clerk_issuer.is_empty() {
        return verify_clerk_jwt(token, state).await;
    }

    // 3. Test-mode backdoor (opt-in via APP__AUTH__TEST_MODE=true). Boot
    //    validation refuses to start when `test_mode` coexists with a real
    //    `clerk_issuer`, so this path cannot be enabled in production.
    //    `Bearer admin:<user_id>` → Admin with that user_id.
    //    Any other bearer value → Participant with `user_id = <token>`.
    //    Each call best-effort seeds the `seen_users` row so /users works.
    if cfg.test_mode {
        if let Some(uid) = token.strip_prefix("admin:") {
            let _ = queries::upsert_seen_user(state.db(), uid).await;
            set_sentry_user(uid);
            return Ok(AuthIdentity::Admin {
                user_id: Some(uid.to_string()),
                source: AuthSource::ClerkJwt,
            });
        }
        let _ = queries::upsert_seen_user(state.db(), token).await;
        set_sentry_user(token);
        return Ok(AuthIdentity::Participant {
            user_id: token.to_string(),
        });
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

    // Best-effort seen_users upsert. A DB failure here must not break auth.
    if let Err(e) = queries::upsert_seen_user(state.db(), &claims.sub).await {
        tracing::warn!(error = %e, user_id = %claims.sub, "seen_users upsert failed");
    }

    let is_admin = queries::is_admin(state.db(), &claims.sub).await.unwrap_or(false);
    set_sentry_user(&claims.sub);

    if is_admin {
        Ok(AuthIdentity::Admin {
            user_id: Some(claims.sub),
            source: AuthSource::ClerkJwt,
        })
    } else {
        Ok(AuthIdentity::Participant { user_id: claims.sub })
    }
}

/// Attach a user identifier to the current Sentry scope so subsequent
/// events on this request are searchable by `user.id`. Safe no-op when
/// Sentry is disabled (no DSN).
fn set_sentry_user(id: &str) {
    sentry::configure_scope(|scope| {
        scope.set_user(Some(sentry::User {
            id: Some(id.to_string()),
            ..Default::default()
        }));
    });
}

/// Extract `token=<value>` from a URL query string. Handles percent-decoding
/// of the value. Returns `None` if the key is absent.
fn parse_token_param(query: &str) -> Option<String> {
    for pair in query.split('&') {
        let mut it = pair.splitn(2, '=');
        let k = it.next()?;
        if k != "token" { continue; }
        let v = it.next().unwrap_or("");
        return Some(percent_decode(v));
    }
    None
}

/// Minimal percent-decoder for query values. Handles `%XX` and `+` as space.
fn percent_decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => { out.push(' '); i += 1; }
            b'%' if i + 2 < bytes.len() => {
                let hi = (bytes[i + 1] as char).to_digit(16);
                let lo = (bytes[i + 2] as char).to_digit(16);
                match (hi, lo) {
                    (Some(h), Some(l)) => {
                        out.push(((h * 16 + l) as u8) as char);
                        i += 3;
                    }
                    _ => { out.push('%'); i += 1; }
                }
            }
            b => { out.push(b as char); i += 1; }
        }
    }
    out
}

#[cfg(test)]
mod query_tests {
    use super::{parse_token_param, percent_decode};

    #[test]
    fn extracts_token() {
        assert_eq!(parse_token_param("token=abc123"), Some("abc123".into()));
    }

    #[test]
    fn extracts_token_among_other_params() {
        assert_eq!(
            parse_token_param("other=x&token=abc123&foo=bar"),
            Some("abc123".into())
        );
    }

    #[test]
    fn percent_decoded_value() {
        assert_eq!(parse_token_param("token=abc%20def"), Some("abc def".into()));
    }

    #[test]
    fn absent_returns_none() {
        assert_eq!(parse_token_param("other=x"), None);
    }

    #[test]
    fn percent_decode_basic() {
        assert_eq!(percent_decode("a%20b"), "a b");
        assert_eq!(percent_decode("a+b"), "a b");
        assert_eq!(percent_decode("plain"), "plain");
    }
}

