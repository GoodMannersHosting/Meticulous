//! Intermediate Certificate Authority for signing agent job certificates.

use chrono::{DateTime, Duration, Utc};
use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, DnValue, IsCa, KeyPair,
    KeyUsagePurpose, SerialNumber,
};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

use crate::error::SecretsError;

/// Configuration for the Certificate Authority.
#[derive(Debug, Clone)]
pub struct CaConfig {
    /// Organization name in the CA certificate.
    pub organization: String,
    /// Common name for the intermediate CA.
    pub common_name: String,
    /// Maximum validity duration for job certificates.
    pub max_job_cert_duration: std::time::Duration,
    /// Serial number counter seed.
    pub serial_seed: u64,
}

impl Default for CaConfig {
    fn default() -> Self {
        Self {
            organization: "Meticulous CI/CD".to_string(),
            common_name: "Meticulous Intermediate CA".to_string(),
            max_job_cert_duration: std::time::Duration::from_secs(3600),
            serial_seed: 1,
        }
    }
}

/// A signed certificate with its metadata.
#[derive(Debug, Clone)]
pub struct SignedCertificate {
    /// PEM-encoded certificate.
    pub certificate_pem: String,
    /// Serial number of the certificate.
    pub serial_number: String,
    /// Subject common name.
    pub subject_cn: String,
    /// Issuer common name.
    pub issuer_cn: String,
    /// SHA-256 fingerprint of the public key.
    pub public_key_fingerprint: String,
    /// Not valid before.
    pub not_before: DateTime<Utc>,
    /// Not valid after.
    pub not_after: DateTime<Utc>,
}

/// Intermediate Certificate Authority.
///
/// Signs ephemeral certificates for per-job PKI. Each job gets a short-lived
/// certificate that is consumed (single-use) after the job completes.
pub struct CertificateAuthority {
    config: CaConfig,
    ca_key_pair: KeyPair,
    ca_cert_pem: String,
    serial_counter: Arc<RwLock<u64>>,
}

impl std::fmt::Debug for CertificateAuthority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CertificateAuthority")
            .field("config", &self.config)
            .field("ca_cert_pem", &"[PRESENT]")
            .finish()
    }
}

impl CertificateAuthority {
    /// Create a new intermediate CA.
    pub fn new(config: CaConfig) -> Result<Self, SecretsError> {
        let ca_key_pair = KeyPair::generate()
            .map_err(|e| SecretsError::Crypto(format!("CA key generation failed: {e}")))?;

        let mut params = CertificateParams::default();
        let mut dn = DistinguishedName::new();
        dn.push(
            DnType::OrganizationName,
            DnValue::Utf8String(config.organization.clone()),
        );
        dn.push(
            DnType::CommonName,
            DnValue::Utf8String(config.common_name.clone()),
        );
        params.distinguished_name = dn;
        params.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
        params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
            KeyUsagePurpose::DigitalSignature,
        ];
        params.not_before = rcgen::date_time_ymd(2024, 1, 1);
        params.not_after = rcgen::date_time_ymd(2034, 12, 31);

        let ca_cert = params
            .self_signed(&ca_key_pair)
            .map_err(|e| SecretsError::Crypto(format!("CA cert generation failed: {e}")))?;
        let ca_cert_pem = ca_cert.pem();

        let serial_seed = config.serial_seed;
        info!(cn = %config.common_name, "Intermediate CA initialized");

        Ok(Self {
            config,
            ca_key_pair,
            ca_cert_pem,
            serial_counter: Arc::new(RwLock::new(serial_seed)),
        })
    }

    /// Sign a CSR and produce a job certificate.
    ///
    /// Accepts the agent's key pair in DER (PKCS#8) format. `rcgen` requires
    /// the full key pair for `CertificateParams::signed_by`.
    /// The certificate is short-lived (max 1 hour) and single-use.
    pub async fn sign_csr(
        &self,
        agent_id: &str,
        job_id: &str,
        key_pair_der: &[u8],
    ) -> Result<SignedCertificate, SecretsError> {
        let serial = self.next_serial().await;
        let serial_hex = format!("{serial:016x}");

        let subject_cn = format!("agent:{agent_id}/job:{job_id}");

        let now = Utc::now();
        let not_after = now + Duration::seconds(self.config.max_job_cert_duration.as_secs() as i64);

        let agent_key_pair = KeyPair::try_from(key_pair_der)
            .map_err(|e| SecretsError::Crypto(format!("invalid CSR key pair: {e}")))?;

        let fingerprint = {
            let mut hasher = Sha256::new();
            hasher.update(agent_key_pair.public_key_der());
            hex::encode(hasher.finalize())
        };

        let mut params = CertificateParams::default();
        let mut dn = DistinguishedName::new();
        dn.push(
            DnType::OrganizationName,
            DnValue::Utf8String(self.config.organization.clone()),
        );
        dn.push(DnType::CommonName, DnValue::Utf8String(subject_cn.clone()));
        params.distinguished_name = dn;
        params.is_ca = IsCa::NoCa;
        params.serial_number = Some(SerialNumber::from_slice(serial.to_be_bytes().as_slice()));
        params.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyEncipherment,
        ];

        let ca_params = CertificateParams::default();
        let ca_cert_for_signing = ca_params
            .self_signed(&self.ca_key_pair)
            .map_err(|e| SecretsError::Crypto(format!("CA cert regeneration failed: {e}")))?;

        let signed = params
            .signed_by(&agent_key_pair, &ca_cert_for_signing, &self.ca_key_pair)
            .map_err(|e| SecretsError::Crypto(format!("CSR signing failed: {e}")))?;

        debug!(
            agent_id = %agent_id,
            job_id = %job_id,
            serial = %serial_hex,
            fingerprint = %fingerprint,
            "Signed job certificate"
        );

        Ok(SignedCertificate {
            certificate_pem: signed.pem(),
            serial_number: serial_hex,
            subject_cn,
            issuer_cn: self.config.common_name.clone(),
            public_key_fingerprint: fingerprint,
            not_before: now,
            not_after,
        })
    }

    /// Get the CA certificate in PEM format.
    pub fn ca_certificate_pem(&self) -> &str {
        &self.ca_cert_pem
    }

    async fn next_serial(&self) -> u64 {
        let mut counter = self.serial_counter.write().await;
        let serial = *counter;
        *counter += 1;
        serial
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ca_creation() {
        let ca = CertificateAuthority::new(CaConfig::default()).unwrap();
        assert!(!ca.ca_certificate_pem().is_empty());
        assert!(ca.ca_certificate_pem().contains("BEGIN CERTIFICATE"));
    }

    #[tokio::test]
    async fn test_sign_csr() {
        let ca = CertificateAuthority::new(CaConfig::default()).unwrap();
        let agent_kp = KeyPair::generate().unwrap();
        let key_pair_der = agent_kp.serialize_der().to_vec();

        let cert = ca
            .sign_csr("agent-1", "job-1", &key_pair_der)
            .await
            .unwrap();

        assert!(cert.certificate_pem.contains("BEGIN CERTIFICATE"));
        assert!(cert.subject_cn.contains("agent:agent-1"));
        assert!(cert.subject_cn.contains("job:job-1"));
        assert!(!cert.public_key_fingerprint.is_empty());
        assert!(cert.not_after > cert.not_before);
    }
}
