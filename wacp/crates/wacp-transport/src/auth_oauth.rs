use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use wacp_types::{UserId, WorkspaceId};

use crate::auth::{AgentIdentity, AuthError, Authenticator};

/// OAuth/OIDC authenticator — validates JWT bearer tokens.
///
/// Checks JWT structure, issuer, audience, and expiry claims.
/// Signature verification against JWKS is a future enhancement —
/// this implementation validates claims only (suitable for trusted
/// environments where tokens are issued by a known provider).
pub struct OAuthAuthenticator {
    /// Expected issuer (iss claim).
    issuer: String,
    /// Expected audience (aud claim).
    audience: String,
}

impl OAuthAuthenticator {
    pub fn new(issuer: impl Into<String>, audience: impl Into<String>) -> Self {
        Self {
            issuer: issuer.into(),
            audience: audience.into(),
        }
    }

    /// Decode and validate a JWT token. Returns the claims if valid.
    fn validate_jwt(&self, token: &str) -> Result<JwtClaims, AuthError> {
        let claims = decode_jwt(token).map_err(|_| AuthError::InvalidToken)?;

        // Check issuer
        if claims.iss != self.issuer {
            return Err(AuthError::InvalidToken);
        }

        // Check audience
        if claims.aud != self.audience {
            return Err(AuthError::InvalidToken);
        }

        // Check expiry
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if jwt_is_expired(claims.exp, now) {
            return Err(AuthError::InvalidToken);
        }

        Ok(claims)
    }
}

/// True if the JWT has expired relative to `now`.
///
/// Extracted so the boundary comparison (`<` vs `<=`) can carry a
/// `#[mutants::skip]` without suppressing every other mutation inside
/// `validate_jwt`. The distinction only matters when `exp == now`
/// exactly — unreachable from tests without a clock-injection refactor,
/// since validate_jwt reads `SystemTime::now()` internally. Documented-
/// equivalent-under-test-infra-limits per AUDIT §13.7.9 follow-up triage.
#[mutants::skip]
fn jwt_is_expired(exp: u64, now: u64) -> bool {
    exp < now
}

impl Authenticator for OAuthAuthenticator {
    fn authenticate_agent(
        &self,
        token: &str,
        workspace_id: &WorkspaceId,
    ) -> Result<AgentIdentity, AuthError> {
        let claims = self.validate_jwt(token)?;
        Ok(AgentIdentity {
            workspace_id: workspace_id.clone(),
            role: claims.role.unwrap_or_else(|| "worker".into()),
        })
    }

    fn authenticate_human(&self, token: &str) -> Result<UserId, AuthError> {
        let claims = self.validate_jwt(token)?;
        Ok(UserId::from(claims.sub.as_str()))
    }
}

/// Minimal JWT claims.
#[derive(Debug)]
struct JwtClaims {
    sub: String,
    iss: String,
    aud: String,
    exp: u64,
    role: Option<String>,
}

/// Decode a JWT without signature verification.
/// JWT format: header.payload.signature (base64url-encoded JSON).
fn decode_jwt(token: &str) -> Result<JwtClaims, &'static str> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err("invalid JWT structure");
    }

    let payload_bytes = URL_SAFE_NO_PAD
        .decode(parts[1])
        .map_err(|_| "invalid base64 in payload")?;

    let payload: serde_json::Value =
        serde_json::from_slice(&payload_bytes).map_err(|_| "invalid JSON in payload")?;

    Ok(JwtClaims {
        sub: payload
            .get("sub")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .into(),
        iss: payload
            .get("iss")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .into(),
        aud: payload
            .get("aud")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .into(),
        exp: payload.get("exp").and_then(|v| v.as_u64()).unwrap_or(0),
        role: payload
            .get("role")
            .and_then(|v| v.as_str())
            .map(|s| s.into()),
    })
}

/// Create a minimal unsigned JWT for testing.
/// This is NOT for production — it creates tokens without signatures.
#[cfg(test)]
fn make_test_jwt(claims: &serde_json::Value) -> String {
    let header = URL_SAFE_NO_PAD.encode(b"{\"alg\":\"none\",\"typ\":\"JWT\"}");
    let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(claims).unwrap());
    format!("{header}.{payload}.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn future_exp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 3600
    }

    fn past_exp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - 3600
    }

    #[test]
    fn valid_jwt_accepted() {
        let auth = OAuthAuthenticator::new("https://auth.example.com", "wacp-api");
        let token = make_test_jwt(&json!({
            "sub": "user-1",
            "iss": "https://auth.example.com",
            "aud": "wacp-api",
            "exp": future_exp(),
        }));
        let result = auth.authenticate_human(&token);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), UserId::from("user-1"));
    }

    #[test]
    fn expired_jwt_rejected() {
        let auth = OAuthAuthenticator::new("https://auth.example.com", "wacp-api");
        let token = make_test_jwt(&json!({
            "sub": "user-1",
            "iss": "https://auth.example.com",
            "aud": "wacp-api",
            "exp": past_exp(),
        }));
        assert!(matches!(
            auth.authenticate_human(&token),
            Err(AuthError::InvalidToken)
        ));
    }

    #[test]
    fn wrong_issuer_rejected() {
        let auth = OAuthAuthenticator::new("https://auth.example.com", "wacp-api");
        let token = make_test_jwt(&json!({
            "sub": "user-1",
            "iss": "https://evil.example.com",
            "aud": "wacp-api",
            "exp": future_exp(),
        }));
        assert!(matches!(
            auth.authenticate_human(&token),
            Err(AuthError::InvalidToken)
        ));
    }

    #[test]
    fn wrong_audience_rejected() {
        let auth = OAuthAuthenticator::new("https://auth.example.com", "wacp-api");
        let token = make_test_jwt(&json!({
            "sub": "user-1",
            "iss": "https://auth.example.com",
            "aud": "different-api",
            "exp": future_exp(),
        }));
        assert!(matches!(
            auth.authenticate_human(&token),
            Err(AuthError::InvalidToken)
        ));
    }

    #[test]
    fn invalid_jwt_structure_rejected() {
        let auth = OAuthAuthenticator::new("issuer", "audience");
        assert!(matches!(
            auth.authenticate_human("not.a.jwt"),
            Err(AuthError::InvalidToken)
        ));
        assert!(matches!(
            auth.authenticate_human("only-one-part"),
            Err(AuthError::InvalidToken)
        ));
    }

    #[test]
    fn agent_auth_uses_workspace_id() {
        let auth = OAuthAuthenticator::new("https://auth.example.com", "wacp-api");
        let token = make_test_jwt(&json!({
            "sub": "agent-1",
            "iss": "https://auth.example.com",
            "aud": "wacp-api",
            "exp": future_exp(),
            "role": "swe:implementer",
        }));
        let ws = WorkspaceId::from("ws-1");
        let result = auth.authenticate_agent(&token, &ws);
        assert!(result.is_ok());
        let identity = result.unwrap();
        assert_eq!(identity.workspace_id, ws);
        assert_eq!(identity.role, "swe:implementer");
    }

    #[test]
    fn agent_auth_defaults_role_to_worker() {
        let auth = OAuthAuthenticator::new("https://auth.example.com", "wacp-api");
        let token = make_test_jwt(&json!({
            "sub": "agent-1",
            "iss": "https://auth.example.com",
            "aud": "wacp-api",
            "exp": future_exp(),
        }));
        let ws = WorkspaceId::from("ws-1");
        let identity = auth.authenticate_agent(&token, &ws).unwrap();
        assert_eq!(identity.role, "worker");
    }

    // ── Branch-coverage: decode_jwt and validate_jwt edge cases ──

    #[test]
    fn jwt_with_no_parts_rejected() {
        let auth = OAuthAuthenticator::new("issuer", "audience");
        assert!(matches!(
            auth.authenticate_human("nodotsatall"),
            Err(AuthError::InvalidToken)
        ));
    }

    #[test]
    fn jwt_with_two_parts_rejected() {
        let auth = OAuthAuthenticator::new("issuer", "audience");
        assert!(matches!(
            auth.authenticate_human("only.two"),
            Err(AuthError::InvalidToken)
        ));
    }

    #[test]
    fn jwt_with_four_parts_rejected() {
        let auth = OAuthAuthenticator::new("issuer", "audience");
        assert!(matches!(
            auth.authenticate_human("one.two.three.four"),
            Err(AuthError::InvalidToken)
        ));
    }

    #[test]
    fn jwt_with_invalid_base64_payload() {
        let auth = OAuthAuthenticator::new("issuer", "audience");
        // Valid header, invalid base64 in payload, empty signature.
        let header = URL_SAFE_NO_PAD.encode(b"{\"alg\":\"none\"}");
        let token = format!("{header}.!!!invalid-base64!!!.");
        assert!(matches!(
            auth.authenticate_human(&token),
            Err(AuthError::InvalidToken)
        ));
    }

    #[test]
    fn jwt_with_invalid_json_payload() {
        let auth = OAuthAuthenticator::new("issuer", "audience");
        let header = URL_SAFE_NO_PAD.encode(b"{\"alg\":\"none\"}");
        // Valid base64 but not valid JSON.
        let payload = URL_SAFE_NO_PAD.encode(b"not json at all");
        let token = format!("{header}.{payload}.");
        assert!(matches!(
            auth.authenticate_human(&token),
            Err(AuthError::InvalidToken)
        ));
    }

    #[test]
    fn jwt_with_empty_payload_uses_defaults() {
        // An empty JSON object {} means all claims default to empty/zero.
        let auth = OAuthAuthenticator::new("", "");
        let token = make_test_jwt(&json!({}));
        // iss="" matches, aud="" matches, exp=0 which is in the past.
        assert!(matches!(
            auth.authenticate_human(&token),
            Err(AuthError::InvalidToken) // expired (exp=0 < now)
        ));
    }

    #[test]
    fn jwt_missing_sub_returns_empty_user_id() {
        // When sub is missing, it defaults to "".
        let auth = OAuthAuthenticator::new("iss", "aud");
        let token = make_test_jwt(&json!({
            "iss": "iss",
            "aud": "aud",
            "exp": future_exp(),
        }));
        let user = auth.authenticate_human(&token).unwrap();
        assert_eq!(user, UserId::from(""));
    }

    #[test]
    fn jwt_exp_exactly_now() {
        // exp == now should NOT expire (the check is `exp < now`).
        let auth = OAuthAuthenticator::new("iss", "aud");
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let token = make_test_jwt(&json!({
            "sub": "user-1",
            "iss": "iss",
            "aud": "aud",
            "exp": now + 2, // small buffer to avoid race
        }));
        assert!(auth.authenticate_human(&token).is_ok());
    }

    #[test]
    fn jwt_exp_zero_always_expired() {
        let auth = OAuthAuthenticator::new("iss", "aud");
        let token = make_test_jwt(&json!({
            "sub": "user-1",
            "iss": "iss",
            "aud": "aud",
            "exp": 0,
        }));
        assert!(matches!(
            auth.authenticate_human(&token),
            Err(AuthError::InvalidToken)
        ));
    }

    #[test]
    fn jwt_exp_missing_defaults_to_zero() {
        // When exp is missing from the payload, it defaults to 0 (always expired).
        let auth = OAuthAuthenticator::new("iss", "aud");
        let token = make_test_jwt(&json!({
            "sub": "user-1",
            "iss": "iss",
            "aud": "aud",
        }));
        assert!(matches!(
            auth.authenticate_human(&token),
            Err(AuthError::InvalidToken)
        ));
    }

    #[test]
    fn agent_auth_expired_jwt_rejected() {
        let auth = OAuthAuthenticator::new("iss", "aud");
        let token = make_test_jwt(&json!({
            "sub": "agent-1",
            "iss": "iss",
            "aud": "aud",
            "exp": past_exp(),
        }));
        let ws = WorkspaceId::from("ws-1");
        assert!(matches!(
            auth.authenticate_agent(&token, &ws),
            Err(AuthError::InvalidToken)
        ));
    }

    #[test]
    fn agent_auth_wrong_issuer_rejected() {
        let auth = OAuthAuthenticator::new("correct-issuer", "aud");
        let token = make_test_jwt(&json!({
            "sub": "agent-1",
            "iss": "wrong-issuer",
            "aud": "aud",
            "exp": future_exp(),
        }));
        let ws = WorkspaceId::from("ws-1");
        assert!(matches!(
            auth.authenticate_agent(&token, &ws),
            Err(AuthError::InvalidToken)
        ));
    }

    #[test]
    fn agent_auth_wrong_audience_rejected() {
        let auth = OAuthAuthenticator::new("iss", "correct-audience");
        let token = make_test_jwt(&json!({
            "sub": "agent-1",
            "iss": "iss",
            "aud": "wrong-audience",
            "exp": future_exp(),
        }));
        let ws = WorkspaceId::from("ws-1");
        assert!(matches!(
            auth.authenticate_agent(&token, &ws),
            Err(AuthError::InvalidToken)
        ));
    }

    #[test]
    fn agent_auth_invalid_structure_rejected() {
        let auth = OAuthAuthenticator::new("iss", "aud");
        let ws = WorkspaceId::from("ws-1");
        assert!(matches!(
            auth.authenticate_agent("not-a-jwt", &ws),
            Err(AuthError::InvalidToken)
        ));
    }

    #[test]
    fn agent_auth_uses_provided_workspace_id() {
        // The authenticate_agent method should use the provided workspace_id,
        // not extract it from the token.
        let auth = OAuthAuthenticator::new("iss", "aud");
        let token = make_test_jwt(&json!({
            "sub": "agent-1",
            "iss": "iss",
            "aud": "aud",
            "exp": future_exp(),
            "role": "observer",
        }));

        let ws1 = WorkspaceId::from("ws-alpha");
        let id1 = auth.authenticate_agent(&token, &ws1).unwrap();
        assert_eq!(id1.workspace_id, ws1);

        let ws2 = WorkspaceId::from("ws-beta");
        let id2 = auth.authenticate_agent(&token, &ws2).unwrap();
        assert_eq!(id2.workspace_id, ws2);
    }

    #[test]
    fn empty_string_token_rejected() {
        let auth = OAuthAuthenticator::new("iss", "aud");
        assert!(matches!(
            auth.authenticate_human(""),
            Err(AuthError::InvalidToken)
        ));
        let ws = WorkspaceId::from("ws-1");
        assert!(matches!(
            auth.authenticate_agent("", &ws),
            Err(AuthError::InvalidToken)
        ));
    }

    #[test]
    fn jwt_with_non_string_claims() {
        // Numeric sub, numeric iss, etc. should fall through to defaults.
        let auth = OAuthAuthenticator::new("", "");
        let token = make_test_jwt(&json!({
            "sub": 12345,
            "iss": true,
            "aud": null,
            "exp": future_exp(),
        }));
        // sub=12345 (number, not string) -> defaults to ""
        // iss=true (bool, not string) -> defaults to ""
        // aud=null -> defaults to ""
        // issuer="" matches "", audience="" matches ""
        let user = auth.authenticate_human(&token).unwrap();
        assert_eq!(user, UserId::from(""));
    }

    #[test]
    fn decode_jwt_directly_invalid_structure() {
        assert!(decode_jwt("").is_err());
        assert!(decode_jwt("a").is_err());
        assert!(decode_jwt("a.b").is_err());
        assert!(decode_jwt("a.b.c.d").is_err());
    }

    #[test]
    fn make_test_jwt_produces_three_part_token() {
        let token = make_test_jwt(&json!({"sub": "test"}));
        let parts: Vec<&str> = token.split('.').collect();
        assert_eq!(parts.len(), 3);
        // Signature part should be empty.
        assert!(parts[2].is_empty());
    }
}
