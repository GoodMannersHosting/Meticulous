//! NATS client URL normalization and TLS options for the agent.

#![allow(clippy::result_large_err)]

use std::path::PathBuf;
use std::sync::Arc;

use met_agent::config::AgentIdentity;
use met_agent::error::AgentError;
use nkeys::KeyPair;
use rustls::ClientConfig;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::CryptoProvider;
use rustls::crypto::{WebPkiSupportedAlgorithms, verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, Error, SignatureScheme};
use url::Url;

pub async fn connect_nats(identity: &AgentIdentity) -> Result<async_nats::Client, AgentError> {
    let url = normalize_nats_url(&identity.nats_url)?;
    let opts = base_connect_options(identity)?;
    let opts = apply_nats_tls_env(opts, &url)?;
    opts.connect(&url)
        .await
        .map_err(|e| AgentError::Internal(format!("NATS connect: {e}")))
}

fn normalize_nats_url(raw: &str) -> Result<String, AgentError> {
    let t = raw.trim();
    if t.is_empty() {
        return Err(AgentError::Config("NATS URL is empty".into()));
    }
    if !t.contains("://") {
        return Ok(t.to_string());
    }
    let mut u: Url = t
        .parse()
        .map_err(|e| AgentError::Config(format!("invalid NATS URL {t:?}: {e}")))?;
    match u.scheme() {
        "http" => {
            u.set_scheme("ws").map_err(|_| {
                AgentError::Config(format!("cannot rewrite NATS URL scheme to ws: {t}"))
            })?;
        }
        "https" => {
            u.set_scheme("wss").map_err(|_| {
                AgentError::Config(format!("cannot rewrite NATS URL scheme to wss: {t}"))
            })?;
        }
        _ => {}
    }
    Ok(u.into())
}

fn env_truthy(name: &str) -> bool {
    std::env::var(name)
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("yes"))
        .unwrap_or(false)
}

fn base_connect_options(
    identity: &AgentIdentity,
) -> Result<async_nats::ConnectOptions, AgentError> {
    match (&identity.nats_user_jwt, &identity.nats_user_seed) {
        (Some(jwt), Some(seed)) if !jwt.trim().is_empty() && !seed.trim().is_empty() => {
            let kp = Arc::new(KeyPair::from_seed(seed.trim()).map_err(|e| {
                AgentError::Config(format!("invalid NATS user seed in identity: {e}"))
            })?);
            let jwt = jwt.clone();
            Ok(async_nats::ConnectOptions::with_jwt(jwt, move |nonce| {
                let kp = kp.clone();
                async move { kp.sign(&nonce).map_err(async_nats::AuthError::new) }
            }))
        }
        _ => Ok(async_nats::ConnectOptions::new()),
    }
}

fn apply_nats_tls_env(
    mut opts: async_nats::ConnectOptions,
    url: &str,
) -> Result<async_nats::ConnectOptions, AgentError> {
    let tls_insecure = env_truthy("MET_AGENT_NATS_TLS_INSECURE");
    let ca_file = std::env::var("MET_AGENT_NATS_CA_FILE")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let force_require_tls = env_truthy("MET_AGENT_NATS_REQUIRE_TLS");

    if tls_insecure && ca_file.is_some() {
        tracing::warn!(
            "MET_AGENT_NATS_TLS_INSECURE is set; ignoring MET_AGENT_NATS_CA_FILE \
             (mount a CA bundle and omit insecure mode when possible)"
        );
    }

    if tls_insecure {
        opts = opts.tls_client_config(nats_tls_insecure_client_config()?);
    } else if let Some(path) = ca_file {
        opts = opts.add_root_certificates(PathBuf::from(path));
    }

    let scheme_requires_tls = Url::parse(url).ok().is_some_and(|u| {
        u.scheme().eq_ignore_ascii_case("tls") || u.scheme().eq_ignore_ascii_case("wss")
    });

    if force_require_tls || scheme_requires_tls {
        opts = opts.require_tls(true);
    }

    Ok(opts)
}

#[derive(Debug)]
struct SkipServerCertVerification {
    supported: WebPkiSupportedAlgorithms,
}

impl ServerCertVerifier for SkipServerCertVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        verify_tls12_signature(message, cert, dss, &self.supported)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        verify_tls13_signature(message, cert, dss, &self.supported)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.supported.supported_schemes()
    }
}

fn nats_tls_insecure_client_config() -> Result<ClientConfig, AgentError> {
    let provider = CryptoProvider::get_default().ok_or_else(|| {
        AgentError::Internal(
            "NATS TLS: rustls CryptoProvider not installed (internal error)".into(),
        )
    })?;
    let verifier = Arc::new(SkipServerCertVerification {
        supported: provider.signature_verification_algorithms,
    });
    Ok(ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth())
}

#[cfg(test)]
mod tests {
    use super::normalize_nats_url;

    #[test]
    fn rewrites_http_https_to_ws_wss() {
        assert_eq!(
            normalize_nats_url("https://nats.example/nats").unwrap(),
            "wss://nats.example/nats"
        );
        assert_eq!(
            normalize_nats_url("http://nats.example/nats").unwrap(),
            "ws://nats.example/nats"
        );
    }

    #[test]
    fn leaves_nats_and_tls_unchanged() {
        assert_eq!(
            normalize_nats_url("nats://host:4222").unwrap(),
            "nats://host:4222"
        );
        assert_eq!(
            normalize_nats_url("tls://host:443").unwrap(),
            "tls://host:443"
        );
    }
}
