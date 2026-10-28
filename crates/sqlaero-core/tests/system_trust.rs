//! Read-only system-trust probe; no account data or HTTP request is transmitted.

use sqlaero_core::transport::tls::{self, MinimumVersion, Mode};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

#[test]
#[ignore = "requires outbound public TLS and the real operating-system trust service"]
fn system_ca_and_identity_policies_accept_the_real_pypi_certificate() {
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
        let config = tls::client_config(mode, MinimumVersion::Tls12, None, None)
            .unwrap()
            .unwrap();
        let mut tls =
            rustls::ClientConnection::new(config, "pypi.org".to_owned().try_into().unwrap())
                .unwrap();
        while tls.is_handshaking() {
            assert!(start.elapsed() < Duration::from_secs(10));
            tls.complete_io(&mut socket).unwrap();
        }
        assert!(tls.protocol_version().is_some());
        println!(
            "system {mode:?} validated pypi.org; negotiated {:?}",
            tls.protocol_version()
        );
    }
}
