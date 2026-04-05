//! GitHub App authentication: JWT + installation access tokens.
//!
//! Private keys and credential JSON are decrypted only on the control plane.
//! Call [`installation_access_token`] to obtain a short-lived token for Git / API calls.

use std::time::Duration;

use chrono::Utc;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Parsed credential JSON stored in `builtin_secrets` with `kind: github_app`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GithubAppCredentials {
    /// GitHub App numeric ID.
    pub app_id: u64,
    /// Installation numeric ID for the target org/user.
    pub installation_id: u64,
    /// PEM PKCS#8 or PKCS#1 private key content.
    pub private_key_pem: String,
    /// Optional API base (e.g. `https://api.github.com` or GHE root + `/api/v3`).
    #[serde(default)]
    pub github_api_base: Option<String>,
}

#[derive(Debug, Error)]
pub enum GithubAppError {
    #[error(
        "github_app secret must be JSON with app_id, installation_id, and private_key_pem (legacy raw PEM is not supported)"
    )]
    LegacyPemNotSupported,

    #[error("invalid github_app credentials JSON: {0}")]
    InvalidCredentials(String),

    #[error("failed to sign GitHub App JWT: {0}")]
    JwtSign(String),

    #[error("GitHub installation token request failed: {0}")]
    Request(String),

    #[error("GitHub API error {status}: {body}")]
    Api { status: u16, body: String },

    #[error("GitHub API returned no token in response body")]
    MissingToken,
}

#[derive(Serialize)]
struct AppJwtClaims {
    iat: i64,
    exp: i64,
    iss: u64,
}

fn default_api_base(creds: &GithubAppCredentials) -> String {
    creds
        .github_api_base
        .as_deref()
        .unwrap_or("https://api.github.com")
        .trim_end_matches('/')
        .to_string()
}

/// Build a JWT suitable for `Authorization: Bearer` when calling GitHub App endpoints.
pub fn create_app_jwt(creds: &GithubAppCredentials) -> Result<String, GithubAppError> {
    let now = Utc::now().timestamp();
    // GitHub allows at most ~10 minutes; stay under 9 for clock skew.
    let exp = now + Duration::from_secs(8 * 60).as_secs() as i64;
    let claims = AppJwtClaims {
        iat: now - 60,
        exp,
        iss: creds.app_id,
    };
    let key = EncodingKey::from_rsa_pem(creds.private_key_pem.as_bytes())
        .map_err(|e| GithubAppError::JwtSign(e.to_string()))?;
    let mut header = Header::new(Algorithm::RS256);
    header.typ = Some("JWT".to_string());
    encode(&header, &claims, &key).map_err(|e| GithubAppError::JwtSign(e.to_string()))
}

/// Parse the decrypted payload for a stored `github_app` secret.
pub fn parse_github_app_credentials(plaintext: &str) -> Result<GithubAppCredentials, GithubAppError> {
    let t = plaintext.trim();
    match serde_json::from_str::<GithubAppCredentials>(t) {
        Ok(creds) => Ok(creds),
        Err(e) => {
            if looks_like_legacy_pem_only(t) {
                Err(GithubAppError::LegacyPemNotSupported)
            } else {
                Err(GithubAppError::InvalidCredentials(e.to_string()))
            }
        }
    }
}

/// Legacy `github_app` rows stored the PEM file alone (no JSON wrapper).
fn looks_like_legacy_pem_only(t: &str) -> bool {
    let s = t.trim_start();
    s.starts_with("-----BEGIN") && s.contains("PRIVATE KEY")
}

/// Exchange JWT for a repository-scoped installation access token (Bearer usable for HTTPS Git / API).
pub async fn installation_access_token(creds: &GithubAppCredentials) -> Result<String, GithubAppError> {
    let jwt = create_app_jwt(creds)?;
    let base = default_api_base(creds);
    let url = format!(
        "{base}/app/installations/{}/access_tokens",
        creds.installation_id
    );

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| GithubAppError::Request(e.to_string()))?;

    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {jwt}"))
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header(reqwest::header::USER_AGENT, "meticulous-control-plane")
        .send()
        .await
        .map_err(|e| GithubAppError::Request(e.to_string()))?;

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(GithubAppError::Api {
            status: status.as_u16(),
            body: body.chars().take(512).collect(),
        });
    }

    let v: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| GithubAppError::Request(e.to_string()))?;
    v.get("token")
        .and_then(|t| t.as_str())
        .map(std::string::ToString::to_string)
        .ok_or(GithubAppError::MissingToken)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reject_legacy_pem() {
        let pem = "-----BEGIN RSA PRIVATE KEY-----\nabc\n-----END RSA PRIVATE KEY-----";
        assert!(matches!(
            parse_github_app_credentials(pem),
            Err(GithubAppError::LegacyPemNotSupported)
        ));
    }

    #[test]
    fn parse_json_roundtrip() {
        let j = r#"{"app_id":1,"installation_id":2,"private_key_pem":"x"}"#;
        let c = parse_github_app_credentials(j).unwrap();
        assert_eq!(c.app_id, 1);
        assert_eq!(c.installation_id, 2);
    }

    #[test]
    fn parse_json_with_pem_body_is_not_legacy() {
        let pem = "-----BEGIN RSA PRIVATE KEY-----\nabc\n-----END RSA PRIVATE KEY-----";
        let j = serde_json::json!({
            "app_id": 1u64,
            "installation_id": 2u64,
            "private_key_pem": pem,
        })
        .to_string();
        let c = parse_github_app_credentials(&j).expect("JSON with PEM in field must parse");
        assert_eq!(c.app_id, 1);
        assert_eq!(c.installation_id, 2);
        assert_eq!(c.private_key_pem, pem);
    }

    #[test]
    fn create_app_jwt_uses_rs256() {
        use jsonwebtoken::decode_header;
        use rand::thread_rng;
        use rsa::pkcs8::{EncodePrivateKey, LineEnding};
        use rsa::RsaPrivateKey;

        let mut rng = thread_rng();
        let key = RsaPrivateKey::new(&mut rng, 2048).expect("rsa key");
        let pem = key.to_pkcs8_pem(LineEnding::LF).expect("pem").to_string();
        let creds = GithubAppCredentials {
            app_id: 42,
            installation_id: 99,
            private_key_pem: pem,
            github_api_base: None,
        };
        let jwt = create_app_jwt(&creds).expect("jwt");
        let h = decode_header(&jwt).expect("header");
        assert_eq!(h.alg, jsonwebtoken::Algorithm::RS256);
    }

    #[tokio::test]
    async fn installation_access_token_posts_to_mock_github() {
        use rand::thread_rng;
        use rsa::pkcs8::{EncodePrivateKey, LineEnding};
        use rsa::RsaPrivateKey;
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/app/installations/99/access_tokens"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "token": "ghs_mock_install_token" })),
            )
            .mount(&server)
            .await;

        let mut rng = thread_rng();
        let key = RsaPrivateKey::new(&mut rng, 2048).expect("rsa key");
        let pem = key.to_pkcs8_pem(LineEnding::LF).expect("pem").to_string();
        let creds = GithubAppCredentials {
            app_id: 42,
            installation_id: 99,
            private_key_pem: pem,
            github_api_base: Some(server.uri()),
        };

        let tok = installation_access_token(&creds).await.expect("install token");
        assert_eq!(tok, "ghs_mock_install_token");
    }
}
