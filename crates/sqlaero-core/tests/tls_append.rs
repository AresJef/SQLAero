//! Optional outbound trust-composition probe; it sends only a TLS handshake.

use sqlaero_core::transport::tls::{self, CaMode, Mode, Version};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

#[cfg(not(any(target_vendor = "apple", windows)))]
#[test]
fn unavailable_system_source_cannot_fall_back_to_explicit_roots() {
    const CHILD: &str = "SQLAERO_TLS_EMPTY_SYSTEM_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let missing =
            std::env::temp_dir().join(format!("sqlaero-tls-empty-system-{}", std::process::id()));
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "unavailable_system_source_cannot_fall_back_to_explicit_roots",
            ])
            .env(CHILD, "1")
            .env("SSL_CERT_FILE", &missing)
            .env("SSL_CERT_DIR", &missing)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "isolated native root-source check failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let roots = tls::certificates(include_bytes!("fixtures/tls-diagnostics/anchors.pem")).unwrap();
    for mode in [Mode::VerifyCa, Mode::VerifyIdentity] {
        let error = tls::client_config(
            mode,
            Version::Tls12,
            None,
            CaMode::Append,
            Some(roots.clone()),
            None,
        )
        .unwrap_err();
        assert!(matches!(
            &error,
            tls::ConfigError::SystemTrustUnavailable {
                ca_mode: CaMode::Append,
                cause: Some(_)
            }
        ));
        assert!(std::error::Error::source(&error).is_some());
        assert!(error.to_string().contains("sqlaero-tls-empty-system-"));
        assert!(
            tls::client_config(
                mode,
                Version::Tls12,
                None,
                CaMode::Replace,
                Some(roots.clone()),
                None
            )
            .unwrap()
            .is_some()
        );
    }
    assert!(
        tls::client_config(
            Mode::Required,
            Version::Tls12,
            None,
            CaMode::Append,
            None,
            None
        )
        .unwrap()
        .is_some()
    );
    assert!(
        tls::client_config(
            Mode::Disabled,
            Version::Tls12,
            None,
            CaMode::Append,
            None,
            None
        )
        .unwrap()
        .is_none()
    );
}

#[test]
#[ignore = "requires outbound public TLS and the real native system trust service"]
fn appended_private_roots_retain_a_real_public_ca_path() {
    let roots = tls::certificates(include_bytes!("fixtures/tls-diagnostics/anchors.pem")).unwrap();
    for mode in [Mode::VerifyCa, Mode::VerifyIdentity] {
        let start = Instant::now();
        let address = ("pypi.org", 443)
            .to_socket_addrs()
            .unwrap()
            .find(|address| address.is_ipv4())
            .unwrap();
        let mut socket = TcpStream::connect_timeout(&address, Duration::from_secs(5)).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let configuration = tls::client_config(
            mode,
            Version::Tls12,
            None,
            CaMode::Append,
            Some(roots.clone()),
            None,
        )
        .unwrap()
        .unwrap();
        let mut session =
            rustls::ClientConnection::new(configuration, "pypi.org".to_owned().try_into().unwrap())
                .unwrap();
        while session.is_handshaking() {
            assert!(start.elapsed() < Duration::from_secs(10));
            session.complete_io(&mut socket).unwrap();
        }
        assert!(session.protocol_version().is_some());
        assert!(
            session
                .peer_certificates()
                .is_some_and(|chain| !chain.is_empty())
        );
    }
}
