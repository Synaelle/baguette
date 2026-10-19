// Copyright (C) 2026 Synaelle
// SPDX-License-Identifier: GPL-3.0-or-later
//! FTPS: TLS on the control and data connections, checked against the Windows
//! certificate store. A certificate Windows doesn't trust is refused once with
//! its fingerprint, so the window can ask, and accepted after that when the
//! site lists it as trusted.

use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, ClientConnection, DigitallySignedStruct, SignatureScheme, StreamOwned};
use sha2::{Digest, Sha256};

pub type Stream = StreamOwned<ClientConnection, TcpStream>;

/// the last certificate refused: (SHA-256 fingerprint, why)
pub type Refused = Arc<Mutex<Option<(String, String)>>>;

/// "ab:cd:..." of the DER bytes, as shown to the user
pub fn fingerprint(der: &[u8]) -> String {
    Sha256::digest(der).iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(":")
}

#[derive(Debug)]
struct Verifier {
    inner: Arc<rustls_platform_verifier::Verifier>,
    trusted: Vec<String>,
    refused: Refused,
}

impl ServerCertVerifier for Verifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        match self.inner.verify_server_cert(end_entity, intermediates, server_name, ocsp, now) {
            Ok(v) => Ok(v),
            Err(e) => {
                let fp = fingerprint(end_entity);
                if self.trusted.iter().any(|t| t.eq_ignore_ascii_case(&fp)) {
                    return Ok(ServerCertVerified::assertion());
                }
                *self.refused.lock().unwrap() = Some((fp, e.to_string()));
                Err(e)
            }
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}

/// one config per connection: it keeps the TLS session, which the data
/// connections resume (many servers insist on that)
pub fn config(trusted: Vec<String>, refused: Refused) -> io::Result<Arc<ClientConfig>> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let inner = rustls_platform_verifier::Verifier::new(provider.clone()).map_err(io::Error::other)?;
    let verifier = Arc::new(Verifier { inner: Arc::new(inner), trusted, refused });
    let cfg = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(io::Error::other)?
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();
    Ok(Arc::new(cfg))
}

/// TLS handshake over an open connection
pub fn wrap(cfg: &Arc<ClientConfig>, host: &str, tcp: TcpStream) -> io::Result<Stream> {
    let name = ServerName::try_from(host.to_string()).map_err(io::Error::other)?;
    let conn = ClientConnection::new(cfg.clone(), name).map_err(io::Error::other)?;
    let mut s = StreamOwned::new(conn, tcp);
    while s.conn.is_handshaking() {
        s.conn.complete_io(&mut s.sock)?;
    }
    Ok(s)
}

/// a server closing the data connection without close_notify is common: the
/// final reply on the control connection says whether everything arrived
pub fn read(s: &mut Stream, buf: &mut [u8]) -> io::Result<usize> {
    match s.read(buf) {
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => Ok(0),
        r => r,
    }
}

pub fn finish(s: &mut Stream) {
    s.conn.send_close_notify();
    let _ = s.flush();
}
