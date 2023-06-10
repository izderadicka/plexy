use crate::error::Result;
use rustls::{ClientConfig, OwnedTrustAnchor};
use std::{fs::File, io::BufReader};

use crate::config::Args;

pub fn create_client_config(args: &Args) -> Result<ClientConfig> {
    let mut root_cert_store = rustls::RootCertStore::empty();
    if let Some(cafile) = &args.ca_bundle {
        let mut pem = BufReader::new(File::open(cafile)?);
        let certs = rustls_pemfile::certs(&mut pem)?;
        let trust_anchors = certs
            .iter()
            .map(|cert| {
                webpki::TrustAnchor::try_from_cert_der(&cert[..]).map(|ta| {
                    OwnedTrustAnchor::from_subject_spki_name_constraints(
                        ta.subject,
                        ta.spki,
                        ta.name_constraints,
                    )
                })
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;

        root_cert_store.add_server_trust_anchors(trust_anchors.into_iter());
    } else {
        root_cert_store.add_server_trust_anchors(webpki_roots::TLS_SERVER_ROOTS.0.iter().map(
            |ta| {
                OwnedTrustAnchor::from_subject_spki_name_constraints(
                    ta.subject,
                    ta.spki,
                    ta.name_constraints,
                )
            },
        ));
    }

    let config = rustls::ClientConfig::builder()
        .with_safe_defaults()
        .with_root_certificates(root_cert_store)
        .with_no_client_auth();
    Ok(config)
}
