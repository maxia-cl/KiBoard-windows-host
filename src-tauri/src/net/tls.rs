//! The installation's certificate, and the acceptor built from it (protocol §2.2).
//!
//! A LAN has no certificate authority to appeal to, so this is self-signed and the client pins its
//! fingerprint at pairing. That makes the file **per-installation state**, exactly like `host_id`:
//! regenerating it locks out every paired device, and to each of them it looks like an attack. It
//! is therefore written once, next to `config.json`, and only ever read afterwards.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::TlsAcceptor;

fn paths() -> (PathBuf, PathBuf) {
    let dir = crate::config::config_dir();
    (dir.join("cert.der"), dir.join("key.der"))
}

/// The certificate and its key, generated on first run and read forever after.
///
/// DER rather than PEM because rustls wants DER and nothing else ever reads these; a base64 wrapper
/// would only be there to make them look familiar.
fn load_or_create() -> Result<(CertificateDer<'static>, PrivateKeyDer<'static>), String> {
    let (cert_path, key_path) = paths();
    if let (Ok(cert), Ok(key)) = (std::fs::read(&cert_path), std::fs::read(&key_path)) {
        return Ok((
            CertificateDer::from(cert),
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key)),
        ));
    }
    // The name is cosmetic: the client pins the fingerprint and never checks the subject, because
    // on a LAN the address is a DHCP lease, not an identity.
    let generated = rcgen::generate_simple_self_signed(vec!["kiboard".to_string()])
        .map_err(|e| format!("could not generate a certificate: {e}"))?;
    let cert = generated.cert.der().to_vec();
    let key = generated.signing_key.serialize_der();
    std::fs::write(&cert_path, &cert).map_err(|e| format!("could not write {cert_path:?}: {e}"))?;
    std::fs::write(&key_path, &key).map_err(|e| format!("could not write {key_path:?}: {e}"))?;
    eprintln!("KiBoard: generated this installation's certificate");
    Ok((
        CertificateDer::from(cert),
        PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key)),
    ))
}

static ACCEPTOR: OnceLock<Option<TlsAcceptor>> = OnceLock::new();

/// The acceptor every connection is wrapped in. Built once: the certificate cannot change while
/// the host is running without stranding whoever is already connected.
pub(crate) fn acceptor() -> Option<TlsAcceptor> {
    ACCEPTOR
        .get_or_init(|| match build() {
            Ok(a) => Some(a),
            Err(e) => {
                eprintln!("KiBoard: TLS unavailable — {e}");
                None
            }
        })
        .clone()
}

fn build() -> Result<TlsAcceptor, String> {
    let (cert, key) = load_or_create()?;
    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert], key)
        .map_err(|e| format!("rustls rejected the certificate: {e}"))?;
    Ok(TlsAcceptor::from(Arc::new(config)))
}

/// SHA-256 of the certificate, lowercase hex — what the client pins, and what the host UI shows so
/// the two can be compared by eye when something looks wrong.
pub(crate) fn fingerprint() -> Option<String> {
    use sha2::{Digest, Sha256};
    let cert = std::fs::read(paths().0).ok()?;
    Some(
        Sha256::digest(&cert)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
    )
}
