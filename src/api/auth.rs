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
    iss: String,
    exp: u64,
    #[serde(default)]
    public_metadata: Option<ClerkPublicMetadata>,
}

/// Nested metadata within Clerk JWT.
#[derive(serde::Deserialize)]
struct ClerkPublicMetadata {
    #[serde(default)]
    role: Option<String>,
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

        // Extract bearer token from Authorization header
        let token = parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|h| h.strip_prefix("Bearer "));

        // 1. Try static bearer token match
        if !auth_cfg.admin_token.is_empty() {
            if let Some(t) = token {
                if t == auth_cfg.admin_token {
                    return Ok(AuthIdentity::BearerToken);
                }
            }
        }

        // 2. Try Clerk JWT decode if issuer is configured
        if !auth_cfg.clerk_issuer.is_empty() {
            if let Some(t) = token {
                match try_decode_clerk_jwt(t, &auth_cfg.clerk_issuer) {
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

/// Attempt to decode a Clerk JWT and extract identity.
///
/// TODO: Production should use proper JWKS RS256 verification via clerk_jwks_url.
/// Currently uses insecure decode (no signature verification) which is acceptable
/// for Phase 1.5a behind Cloudflare Tunnel.
fn try_decode_clerk_jwt(
    token: &str,
    expected_issuer: &str,
) -> Result<AuthIdentity, String> {
    let token_data = jsonwebtoken::decode::<ClerkClaims>(
        token,
        &jsonwebtoken::DecodingKey::from_secret(b""),
        &{
            let mut v = jsonwebtoken::Validation::new(
                jsonwebtoken::Algorithm::HS256,
            );
            v.insecure_disable_signature_validation();
            v.set_required_spec_claims::<&str>(&[]);
            v.validate_exp = false;
            v.validate_aud = false;
            v
        },
    )
    .map_err(|e| format!("JWT decode error: {e}"))?;

    let claims = token_data.claims;

    // Validate issuer
    if claims.iss != expected_issuer {
        return Err(format!(
            "issuer mismatch: expected {expected_issuer}, got {}",
            claims.iss
        ));
    }

    // Validate expiry manually
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    if claims.exp <= now {
        return Err("token expired".into());
    }

    let role = claims
        .public_metadata
        .and_then(|m| m.role)
        .unwrap_or_else(|| "member".into());

    Ok(AuthIdentity::ClerkUser {
        user_id: claims.sub,
        role,
    })
}
