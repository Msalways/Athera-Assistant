use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const OAUTH_PKCE_SCHEMA_V1: &str = "aethra.oauth-pkce.v1";

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum OAuthError {
    #[error("Invalid redirect: state mismatch or replay")]
    InvalidRedirect,
    #[error("Verifier too short")]
    WeakVerifier,
    #[error("Token expired")]
    Expired,
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PkceChallenge {
    pub schema: String,
    pub verifier: String,
    pub challenge: String,
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OAuthTokenSet {
    pub schema: String,
    pub access_token_ref: String,
    pub refresh_token_ref: Option<String>,
    pub expires_at: Option<u64>,
    pub scope: String,
}

impl OAuthTokenSet {
    pub fn is_expired(&self, now_millis: u64) -> bool {
        self.expires_at.is_some_and(|expires| expires <= now_millis)
    }
}

pub fn new_verifier() -> Result<String, OAuthError> {
    let mut bytes = [0u8; 48];
    getrandom::fill(&mut bytes).map_err(|_| OAuthError::WeakVerifier)?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}

pub fn challenge_for(verifier: &str) -> Result<String, OAuthError> {
    if verifier.len() < 43 {
        return Err(OAuthError::WeakVerifier);
    }
    let digest = Sha256::digest(verifier.as_bytes());
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest))
}

pub fn new_challenge() -> Result<PkceChallenge, OAuthError> {
    let verifier = new_verifier()?;
    let challenge = challenge_for(&verifier)?;
    let mut state_bytes = [0u8; 24];
    getrandom::fill(&mut state_bytes).map_err(|_| OAuthError::WeakVerifier)?;
    let state = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(state_bytes);
    Ok(PkceChallenge {
        schema: OAUTH_PKCE_SCHEMA_V1.into(),
        verifier,
        challenge,
        state,
    })
}

pub fn authorization_url(
    base: &str,
    client_id: &str,
    redirect_uri: &str,
    scope: &str,
    challenge: &PkceChallenge,
) -> Result<String, OAuthError> {
    if base.is_empty() || client_id.is_empty() || redirect_uri.is_empty() {
        return Err(OAuthError::InvalidConfig(
            "base, client_id and redirect_uri are required".into(),
        ));
    }
    Ok(format!(
        "{base}?response_type=code&client_id={client_id}&redirect_uri={redirect_uri}&scope={scope}&state={}&code_challenge={}&code_challenge_method=S256",
        challenge.state, challenge.challenge
    ))
}

pub fn validate_redirect(
    expected_state: &str,
    returned_state: &str,
    code: &str,
) -> Result<String, OAuthError> {
    if expected_state.is_empty() || expected_state != returned_state {
        return Err(OAuthError::InvalidRedirect);
    }
    if code.is_empty() {
        return Err(OAuthError::InvalidRedirect);
    }
    Ok(code.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifier_has_proper_length() {
        let verifier = new_verifier().unwrap();
        assert!(verifier.len() >= 43);
        assert!(verifier.len() <= 128);
    }

    #[test]
    fn challenge_matches_rfc7636_vector() {
        let challenge = challenge_for("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk").unwrap();
        assert_eq!(challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn short_verifier_rejected() {
        assert_eq!(challenge_for("short"), Err(OAuthError::WeakVerifier));
    }

    #[test]
    fn state_mismatch_rejected() {
        assert_eq!(
            validate_redirect("abc", "xyz", "code123"),
            Err(OAuthError::InvalidRedirect)
        );
    }

    #[test]
    fn empty_code_rejected() {
        assert_eq!(
            validate_redirect("abc", "abc", ""),
            Err(OAuthError::InvalidRedirect)
        );
    }

    #[test]
    fn valid_redirect_returns_code() {
        assert_eq!(
            validate_redirect("abc", "abc", "code123"),
            Ok("code123".into())
        );
    }

    #[test]
    fn auth_url_contains_pkce_params() {
        let challenge = new_challenge().unwrap();
        let url = authorization_url(
            "https://auth.example.com/authorize",
            "client",
            "aethra://oauth/callback",
            "read",
            &challenge,
        )
        .unwrap();
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains(&challenge.state));
    }

    #[test]
    fn token_expiry_detected() {
        let set = OAuthTokenSet {
            schema: OAUTH_PKCE_SCHEMA_V1.into(),
            access_token_ref: "ref-1".into(),
            refresh_token_ref: None,
            expires_at: Some(1000),
            scope: "read".into(),
        };
        assert!(set.is_expired(2000));
        assert!(!set.is_expired(500));
    }
}
