//! Vérificateurs de certificat rustls sur mesure (ADR-0005, BR-CONN-001, 002, 003).
//!
//! Le serveur présente un certificat auto-signé : aucune chaîne à vérifier, aucun nom à
//! comparer. Ce qui compte, c'est l'empreinte (SHA-256 du DER). Deux modes :
//! - **sonde** : accepte tout certificat et note son empreinte. Uniquement pour le `hello` de
//!   première prise de contact, avant que l'utilisateur ne confirme l'empreinte ;
//! - **épinglé** : refuse la poignée de main si l'empreinte diffère de celle mémorisée.
//!
//! Dans les deux modes la **signature de la poignée de main** est vérifiée avec la clé du
//! certificat : sans cela, quiconque connaît le certificat (public) pourrait se faire passer pour
//! le serveur. TLS 1.3 seul.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use hearth_proto::fingerprint::Fingerprint;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, ring};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{CertificateError, ClientConfig, DigitallySignedStruct, SignatureScheme};
use thiserror::Error;

use crate::domain::pinning::{PinDecision, decide};
use crate::ports::transport::Pin;

#[derive(Debug, Error)]
#[error("configuration TLS impossible : {0}")]
pub struct TlsError(String);

/// Ce que la poignée de main a révélé : l'empreinte présentée, et si elle a été refusée.
#[derive(Debug, Default)]
pub struct PinState {
    presented: Mutex<Option<Fingerprint>>,
    mismatch: AtomicBool,
}

impl PinState {
    pub fn presented(&self) -> Option<Fingerprint> {
        *self
            .presented
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Le certificat a été refusé parce que son empreinte diffère de celle mémorisée.
    pub fn mismatched(&self) -> bool {
        self.mismatch.load(Ordering::SeqCst)
    }

    fn record(&self, fingerprint: Fingerprint) {
        *self
            .presented
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(fingerprint);
    }
}

#[derive(Debug)]
struct PinVerifier {
    provider: Arc<CryptoProvider>,
    /// `None` : mode sonde.
    expected: Option<Fingerprint>,
    state: Arc<PinState>,
}

impl ServerCertVerifier for PinVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let presented = Fingerprint::of_certificate_der(end_entity.as_ref());
        self.state.record(presented);
        match self.expected {
            None => Ok(ServerCertVerified::assertion()),
            Some(_) => match decide(self.expected.as_ref(), &presented) {
                PinDecision::Match => Ok(ServerCertVerified::assertion()),
                _ => {
                    self.state.mismatch.store(true, Ordering::SeqCst);
                    Err(rustls::Error::InvalidCertificate(
                        CertificateError::ApplicationVerificationFailure,
                    ))
                }
            },
        }
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Err(rustls::Error::General("TLS 1.2 refusé".into()))
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Configuration TLS 1.3 pour ce mode d'épinglage, et l'état où lire l'empreinte présentée.
pub fn client_config(pin: Pin) -> Result<(Arc<ClientConfig>, Arc<PinState>), TlsError> {
    let provider = Arc::new(ring::default_provider());
    let state = Arc::new(PinState::default());
    let verifier = PinVerifier {
        provider: provider.clone(),
        expected: match pin {
            Pin::Probe => None,
            Pin::Pinned(fingerprint) => Some(fingerprint),
        },
        state: state.clone(),
    };
    let mut config = ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|error| TlsError(error.to_string()))?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth();
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok((Arc::new(config), state))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn certificate() -> Vec<u8> {
        rcgen::generate_simple_self_signed(vec!["localhost".to_owned()])
            .unwrap()
            .cert
            .der()
            .to_vec()
    }

    fn verify(pin: Pin, der: &[u8]) -> (Result<ServerCertVerified, rustls::Error>, Arc<PinState>) {
        let provider = Arc::new(ring::default_provider());
        let state = Arc::new(PinState::default());
        let verifier = PinVerifier {
            provider,
            expected: match pin {
                Pin::Probe => None,
                Pin::Pinned(fingerprint) => Some(fingerprint),
            },
            state: state.clone(),
        };
        let result = verifier.verify_server_cert(
            &CertificateDer::from(der.to_vec()),
            &[],
            &ServerName::try_from("whatever.example").unwrap(),
            &[],
            UnixTime::now(),
        );
        (result, state)
    }

    #[test]
    fn the_probe_accepts_any_certificate_and_reports_its_fingerprint() {
        let der = certificate();
        let (result, state) = verify(Pin::Probe, &der);
        assert!(result.is_ok());
        assert_eq!(
            state.presented(),
            Some(Fingerprint::of_certificate_der(&der))
        );
        assert!(!state.mismatched());
    }

    #[test]
    fn the_pinned_mode_accepts_the_matching_fingerprint_whatever_the_name() {
        let der = certificate();
        let pin = Pin::Pinned(Fingerprint::of_certificate_der(&der));
        let (result, state) = verify(pin, &der);
        assert!(result.is_ok());
        assert!(!state.mismatched());
    }

    #[test]
    fn the_pinned_mode_refuses_any_other_certificate() {
        let pinned = certificate();
        let other = certificate();
        let pin = Pin::Pinned(Fingerprint::of_certificate_der(&pinned));
        let (result, state) = verify(pin, &other);
        assert!(matches!(
            result,
            Err(rustls::Error::InvalidCertificate(
                CertificateError::ApplicationVerificationFailure
            ))
        ));
        assert!(state.mismatched());
        assert_eq!(
            state.presented(),
            Some(Fingerprint::of_certificate_der(&other))
        );
    }

    #[test]
    fn a_config_is_built_for_both_modes() {
        assert!(client_config(Pin::Probe).is_ok());
        assert!(client_config(Pin::Pinned(Fingerprint::from_bytes([1; 32]))).is_ok());
    }
}
