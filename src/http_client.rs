//! Platform trust for all app HTTPS requests. Never disables TLS validation.
use anyhow::{Context, Result};
use reqwest::blocking::ClientBuilder;
use std::path::{Path, PathBuf};

pub fn custom_ca(configured: Option<&Path>) -> Option<PathBuf> {
    configured.map(Path::to_owned).or_else(|| {
        ["SSL_CERT_FILE", "REQUESTS_CA_BUNDLE", "CURL_CA_BUNDLE"]
            .into_iter()
            .find_map(|key| {
                std::env::var_os(key)
                    .filter(|p| !p.is_empty())
                    .map(PathBuf::from)
            })
    })
}
pub fn builder(configured: Option<&Path>) -> Result<ClientBuilder> {
    let mut builder = reqwest::blocking::Client::builder();
    if let Some(path) = custom_ca(configured) {
        let bytes = std::fs::read(&path)
            .with_context(|| format!("추가 CA 인증서 읽기 실패: {}", path.display()))?;
        let certs = reqwest::Certificate::from_pem_bundle(&bytes)
            .context("추가 CA 파일은 PEM 인증서여야 합니다")?;
        anyhow::ensure!(!certs.is_empty(), "추가 CA 파일에 인증서가 없습니다");
        for cert in certs {
            builder = builder.add_root_certificate(cert);
        }
    }
    Ok(builder)
}
pub fn error(error: reqwest::Error) -> anyhow::Error {
    let message = format!("{error:?}");
    let lower = message.to_ascii_lowercase();
    if [
        "unknownissuer",
        "unknown issuer",
        "certificate",
        "cert_e_untrustedroot",
        "untrustedroot",
        "untrusted root",
    ]
    .iter()
    .any(|s| lower.contains(s))
    {
        anyhow::anyhow!("HTTPS 인증서를 신뢰하지 못했습니다. Windows의 신뢰된 인증서 저장소를 확인하거나 설정의 '추가 CA 인증서'에 보안 프로그램/사내 프록시의 PEM 인증서를 지정해 주세요. 인증서 검증은 유지됩니다: {error:#}")
    } else {
        error.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::Arc,
        time::{Duration, Instant},
    };
    fn endpoint() -> (String, String, std::thread::JoinHandle<()>) {
        let rcgen::CertifiedKey { cert, key_pair } =
            rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let pem = cert.pem();
        let key = rustls::pki_types::PrivatePkcs8KeyDer::from(key_pair.serialize_der());
        let _ = rustls::crypto::ring::default_provider().install_default();
        let config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert.der().clone()], key.into())
            .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!(
            "https://localhost:{}/",
            listener.local_addr().unwrap().port()
        );
        listener.set_nonblocking(true).unwrap();
        let task = std::thread::spawn(move || {
            let start = Instant::now();
            let socket = loop {
                match listener.accept() {
                    Ok((s, _)) => break s,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        if start.elapsed() > Duration::from_secs(10) {
                            panic!("TLS test connection timed out");
                        }
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(e) => panic!("{e}"),
                }
            };
            // Windows accept() can inherit the listener's nonblocking mode;
            // rustls::StreamOwned needs a blocking socket for this test server.
            socket.set_nonblocking(false).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut stream = rustls::StreamOwned::new(
                rustls::ServerConnection::new(Arc::new(config)).unwrap(),
                socket,
            );
            let mut request = [0u8; 4096];
            if stream.read(&mut request).is_ok() {
                let _ = stream.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
                );
                let _ = stream.flush();
            }
        });
        (url, pem, task)
    }
    #[test]
    fn untrusted_certificate_is_rejected_and_explicit_ca_is_accepted() {
        let (url, _, task) = endpoint();
        let client = builder(None)
            .unwrap()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        let rejected = client
            .get(url)
            .send()
            .expect_err("untrusted CA must never be accepted");
        let reason = format!("{rejected:?}").to_ascii_lowercase();
        assert!(
            reason.contains("certificate")
                || reason.contains("unknownissuer")
                || reason.contains("untrustedroot"),
            "Expected a certificate validation failure, got {reason}"
        );
        task.join().unwrap();
        let (url, pem, task) = endpoint();
        let path = std::env::temp_dir().join(format!("syousetu-ca-{}.pem", std::process::id()));
        std::fs::write(&path, pem).unwrap();
        let client = builder(Some(&path))
            .unwrap()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        assert_eq!(client.get(url).send().unwrap().text().unwrap(), "ok");
        task.join().unwrap();
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    #[ignore = "explicit network smoke test for the platform TLS backend"]
    fn pinned_huggingface_config_uses_platform_trust() {
        use sha2::{Digest, Sha256};
        let asset = crate::models::manifest()
            .into_iter()
            .find(|a| a.file == "config.json")
            .unwrap();
        let url = format!(
            "https://huggingface.co/{}/resolve/{}/{}?download=true",
            asset.repo, asset.revision, asset.file
        );
        let bytes = builder(None)
            .unwrap()
            .timeout(Duration::from_secs(45))
            .build()
            .unwrap()
            .get(url)
            .send()
            .map_err(error)
            .unwrap()
            .error_for_status()
            .unwrap()
            .bytes()
            .unwrap();
        assert_eq!(bytes.len() as u64, asset.size);
        assert_eq!(format!("{:x}", Sha256::digest(bytes)), asset.sha256);
    }
    #[test]
    fn invalid_ca_file_is_an_actionable_error() {
        let path =
            std::env::temp_dir().join(format!("syousetu-invalid-ca-{}.pem", std::process::id()));
        std::fs::write(&path, b"not a PEM certificate").unwrap();
        assert!(builder(Some(&path)).is_err());
        std::fs::remove_file(path).unwrap();
    }
}
