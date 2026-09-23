use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SIGV4_SCHEMA_V1: &str = "aethra.sigv4.v1";

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum SigV4Error {
    #[error("Incomplete credential set")]
    IncompleteCredentials,
    #[error("Session token requires static credentials")]
    OrphanSessionToken,
    #[error("Invalid request: {0}")]
    InvalidRequest(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AwsCredentials {
    pub schema: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub session_token: Option<String>,
    pub expires_at: Option<u64>,
}

impl AwsCredentials {
    pub fn validate(&self) -> Result<(), SigV4Error> {
        if self.schema != SIGV4_SCHEMA_V1 {
            return Err(SigV4Error::InvalidRequest("unsupported schema".into()));
        }
        if self.access_key_id.is_empty() || self.secret_access_key.is_empty() {
            return Err(SigV4Error::IncompleteCredentials);
        }
        Ok(())
    }

    pub fn is_expired(&self, now_millis: u64) -> bool {
        self.expires_at.is_some_and(|expires| expires <= now_millis)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SigV4Request {
    pub method: String,
    pub host: String,
    pub path: String,
    pub query_params: Vec<(String, String)>,
    pub headers: Vec<(String, String)>,
    pub payload_hash: String,
    pub amz_date: String,
    pub region: String,
    pub service: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedRequest {
    pub authorization: String,
    pub amz_date: String,
    pub security_token: Option<String>,
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("hmac accepts any key size");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn sha256_hex(data: &[u8]) -> String {
    hex(&Sha256::digest(data))
}

fn signing_key(secret: &str, date: &str, region: &str, service: &str) -> Vec<u8> {
    let k_date = hmac_sha256(format!("AWS4{secret}").as_bytes(), date.as_bytes());
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, service.as_bytes());
    hmac_sha256(&k_service, b"aws4_request")
}

pub fn empty_payload_hash() -> String {
    sha256_hex(b"")
}

pub fn sign_v4(
    creds: &AwsCredentials,
    request: &SigV4Request,
) -> Result<SignedRequest, SigV4Error> {
    creds.validate()?;
    if request.method.is_empty() || request.host.is_empty() || request.path.is_empty() {
        return Err(SigV4Error::InvalidRequest(
            "method, host and path are required".into(),
        ));
    }
    if request.amz_date.len() < 8 {
        return Err(SigV4Error::InvalidRequest("amz_date is required".into()));
    }
    let date = request.amz_date[..8].to_owned();

    let mut query = request.query_params.clone();
    query.sort();
    let canonical_query = query
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&");

    let mut signed: Vec<(String, String)> = vec![
        ("host".to_owned(), request.host.clone()),
        ("x-amz-date".to_owned(), request.amz_date.clone()),
    ];
    for (name, value) in &request.headers {
        signed.push((name.to_lowercase(), value.trim().to_owned()));
    }
    if let Some(token) = &creds.session_token {
        signed.push(("x-amz-security-token".to_owned(), token.clone()));
    }
    signed.sort();
    let canonical_headers = signed
        .iter()
        .map(|(k, v)| format!("{k}:{v}\n"))
        .collect::<String>();
    let signed_headers = signed
        .iter()
        .map(|(k, _)| k.clone())
        .collect::<Vec<_>>()
        .join(";");

    let canonical_request = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        request.method,
        request.path,
        canonical_query,
        canonical_headers,
        signed_headers,
        request.payload_hash
    );
    let scope = format!("{date}/{}/{}/aws4_request", request.region, request.service);
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{}\n{}\n{}",
        request.amz_date,
        scope,
        sha256_hex(canonical_request.as_bytes())
    );
    let key = signing_key(
        &creds.secret_access_key,
        &date,
        &request.region,
        &request.service,
    );
    let signature = hex(&hmac_sha256(&key, string_to_sign.as_bytes()));

    Ok(SignedRequest {
        authorization: format!(
            "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
            creds.access_key_id, scope, signed_headers, signature
        ),
        amz_date: request.amz_date.clone(),
        security_token: creds.session_token.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example_creds() -> AwsCredentials {
        AwsCredentials {
            schema: SIGV4_SCHEMA_V1.into(),
            access_key_id: "AKIDEXAMPLE".into(),
            secret_access_key: "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY".into(),
            session_token: None,
            expires_at: None,
        }
    }

    fn example_request() -> SigV4Request {
        SigV4Request {
            method: "GET".into(),
            host: "iam.amazonaws.com".into(),
            path: "/".into(),
            query_params: vec![
                ("Action".into(), "ListUsers".into()),
                ("Version".into(), "2010-05-08".into()),
            ],
            headers: vec![(
                "Content-Type".into(),
                "application/x-www-form-urlencoded; charset=utf-8".into(),
            )],
            payload_hash: empty_payload_hash(),
            amz_date: "20150830T123600Z".into(),
            region: "us-east-1".into(),
            service: "iam".into(),
        }
    }

    #[test]
    fn aws_reference_vector_matches() {
        let signed = sign_v4(&example_creds(), &example_request()).unwrap();
        assert_eq!(
            signed.authorization,
            "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/iam/aws4_request, SignedHeaders=content-type;host;x-amz-date, Signature=5d672d79c15b13162d9279b0855cfba6789a8edb4c82c400e06b5924a6f2b5d7"
        );
    }

    #[test]
    fn session_token_signed_and_returned() {
        let mut creds = example_creds();
        creds.session_token = Some("session123".into());
        let signed = sign_v4(&creds, &example_request()).unwrap();
        assert!(signed
            .authorization
            .contains("SignedHeaders=content-type;host;x-amz-date;x-amz-security-token"));
        assert_eq!(signed.security_token.as_deref(), Some("session123"));
    }

    #[test]
    fn no_session_token_no_security_header() {
        let signed = sign_v4(&example_creds(), &example_request()).unwrap();
        assert!(!signed.authorization.contains("x-amz-security-token"));
        assert!(signed.security_token.is_none());
    }

    #[test]
    fn incomplete_credentials_rejected() {
        let mut creds = example_creds();
        creds.secret_access_key.clear();
        assert_eq!(
            sign_v4(&creds, &example_request()),
            Err(SigV4Error::IncompleteCredentials)
        );
    }

    #[test]
    fn empty_method_rejected() {
        let mut request = example_request();
        request.method.clear();
        assert!(matches!(
            sign_v4(&example_creds(), &request),
            Err(SigV4Error::InvalidRequest(_))
        ));
    }

    #[test]
    fn expiry_detected() {
        let mut creds = example_creds();
        creds.expires_at = Some(1000);
        assert!(creds.is_expired(2000));
        assert!(!creds.is_expired(500));
    }

    #[test]
    fn query_params_sorted_before_signing() {
        let mut request = example_request();
        request.query_params.reverse();
        let ordered = sign_v4(&example_creds(), &example_request()).unwrap();
        let reversed = sign_v4(&example_creds(), &request).unwrap();
        assert_eq!(ordered.authorization, reversed.authorization);
    }
}
