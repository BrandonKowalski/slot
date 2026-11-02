use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature, CryptoProvider};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, ClientConnection, DigitallySignedStruct, SignatureScheme, StreamOwned};
use sha2::{Digest, Sha256};
use ureq::unversioned::transport::{
    Buffers, ConnectionDetails, Connector, Either, LazyBuffers, NextTimeout, Transport,
    TransportAdapter,
};

pub type Pin = [u8; 32];

pub fn fingerprint(der: &[u8]) -> Pin {
    Sha256::digest(der).into()
}

pub fn pin_text(pin: &Pin) -> String {
    pin.iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

pub fn pin_hex(pin: &Pin) -> String {
    pin.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn parse_pin(hex: &str) -> Option<Pin> {
    let clean: String = hex.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if clean.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&clean[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

#[derive(Debug)]
struct Pinned {
    pin: Option<Pin>,
    seen: Arc<Mutex<Option<Pin>>>,
    provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for Pinned {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let got = fingerprint(end_entity);
        if let Ok(mut seen) = self.seen.lock() {
            *seen = Some(got);
        }
        match self.pin == Some(got) {
            true => Ok(ServerCertVerified::assertion()),
            false => Err(rustls::Error::InvalidCertificate(
                rustls::CertificateError::ApplicationVerificationFailure,
            )),
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(
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

#[derive(Debug)]
pub struct PinnedConnector {
    config: Arc<ClientConfig>,
}

impl PinnedConnector {
    fn new(pin: Option<Pin>, seen: Arc<Mutex<Option<Pin>>>) -> PinnedConnector {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let verifier = Pinned {
            pin,
            seen,
            provider: provider.clone(),
        };
        let config = ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .expect("ring supports the default TLS versions")
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(verifier))
            .with_no_client_auth();
        PinnedConnector {
            config: Arc::new(config),
        }
    }
}

pub struct PinnedTransport {
    buffers: LazyBuffers,
    stream: StreamOwned<ClientConnection, TransportAdapter>,
}

impl std::fmt::Debug for PinnedTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PinnedTransport").finish()
    }
}

impl<In: Transport> Connector<In> for PinnedConnector {
    type Out = Either<In, PinnedTransport>;

    fn connect(
        &self,
        details: &ConnectionDetails,
        chained: Option<In>,
    ) -> Result<Option<Self::Out>, ureq::Error> {
        let Some(transport) = chained else {
            return Ok(None);
        };
        if !details.needs_tls() || transport.is_tls() {
            return Ok(Some(Either::A(transport)));
        }
        let host = details
            .uri
            .host()
            .ok_or(ureq::Error::Tls("no host to connect to"))?;
        let host = host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .to_string();
        let name =
            ServerName::try_from(host).map_err(|_| ureq::Error::Tls("invalid server name"))?;
        let mut conn = ClientConnection::new(self.config.clone(), name)?;
        let mut sock = TransportAdapter::new(transport.boxed(), details.current_time.clone());
        sock.set_timeout(details.timeout);
        conn.complete_io(&mut sock)?;
        let buffers = LazyBuffers::new(
            details.config.input_buffer_size(),
            details.config.output_buffer_size(),
        );
        Ok(Some(Either::B(PinnedTransport {
            buffers,
            stream: StreamOwned { conn, sock },
        })))
    }
}

impl Transport for PinnedTransport {
    fn buffers(&mut self) -> &mut dyn Buffers {
        &mut self.buffers
    }

    fn transmit_output(&mut self, amount: usize, timeout: NextTimeout) -> Result<(), ureq::Error> {
        self.stream.get_mut().set_timeout(timeout);
        let output = &self.buffers.output()[..amount];
        self.stream.write_all(output)?;
        Ok(())
    }

    fn await_input(&mut self, timeout: NextTimeout) -> Result<bool, ureq::Error> {
        self.stream.get_mut().set_timeout(timeout);
        let input = self.buffers.input_append_buf();
        let amount = self.stream.read(input)?;
        self.buffers.input_appended(amount);
        Ok(amount > 0)
    }

    fn is_open(&mut self) -> bool {
        self.stream.get_mut().get_mut().is_open()
    }

    fn is_tls(&self) -> bool {
        true
    }
}

pub fn pinned_agent(
    config: ureq::config::Config,
    pin: Option<Pin>,
    seen: Arc<Mutex<Option<Pin>>>,
) -> ureq::Agent {
    use ureq::unversioned::resolver::DefaultResolver;
    use ureq::unversioned::transport::TcpConnector;
    let connector = TcpConnector::default().chain(PinnedConnector::new(pin, seen));
    ureq::Agent::with_parts(config, connector, DefaultResolver::default())
}
