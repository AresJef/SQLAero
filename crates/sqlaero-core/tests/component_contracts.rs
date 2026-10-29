//! Runtime checks of the selected public APIs against the isolated TLS fixture.

use rustls::RootCertStore;
use rustls::client::WebPkiServerVerifier;
use sqlaero_core::transport::tls;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

fn fixture() -> PathBuf {
    std::env::var_os("SQLAERO_TEST_TLS")
        .map(PathBuf::from)
        .expect("set SQLAERO_TEST_TLS to the isolated fixture certificate directory")
}

#[test]
#[ignore = "requires the isolated TLS fixture; run explicitly with SQLAERO_TEST_TLS"]
fn encrypted_pkcs8_and_positive_key_matching_use_public_interfaces() {
    let directory = fixture();
    let provider = tls::provider();
    let cert = std::fs::read(directory.join("client.pem")).unwrap();
    let key = std::fs::read(directory.join("client-key-encrypted.pem")).unwrap();
    let password = std::fs::read(directory.join("key-password")).unwrap();
    let parsed = tls::private_key(&key, Some(&password)).unwrap();
    tls::signing_identity(tls::certificates(&cert).unwrap(), parsed, &provider).unwrap();
    assert!(matches!(
        tls::private_key(&key, None),
        Err(tls::MaterialError::PasswordRequired)
    ));
    assert!(matches!(
        tls::private_key(&key, Some(b"incorrect")),
        Err(tls::MaterialError::Decryption)
    ));
    let other = std::fs::read(directory.join("server-key.pem")).unwrap();
    assert!(
        tls::signing_identity(
            tls::certificates(&cert).unwrap(),
            tls::private_key(&other, None).unwrap(),
            &provider
        )
        .is_err()
    );
}

#[test]
#[ignore = "requires the isolated TLS fixture and its private OpenSSL test files"]
fn declared_pem_formats_and_modern_encryption_are_decoded_natively() {
    let directory = fixture();
    let output = directory.join(format!("key-formats-{}", std::process::id()));
    std::fs::create_dir(&output).unwrap();
    let openssl = std::env::var_os("SQLAERO_OPENSSL").unwrap_or_else(|| "openssl".into());
    let run = |arguments: &[&str]| {
        let result = Command::new(&openssl)
            .current_dir(&output)
            .args(arguments)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "test certificate generation failed"
        );
    };
    std::fs::copy(directory.join("client-key.pem"), output.join("rsa.pem")).unwrap();
    std::fs::copy(directory.join("key-password"), output.join("password")).unwrap();
    run(&[
        "pkey",
        "-in",
        "rsa.pem",
        "-traditional",
        "-out",
        "rsa-pkcs1.pem",
    ]);
    run(&[
        "ecparam",
        "-name",
        "prime256v1",
        "-genkey",
        "-noout",
        "-out",
        "ec-sec1.pem",
    ]);
    for name in ["rsa.pem", "rsa-pkcs1.pem", "ec-sec1.pem"] {
        let parsed = tls::private_key(&std::fs::read(output.join(name)).unwrap(), None).unwrap();
        tls::provider()
            .key_provider
            .load_private_key(parsed)
            .unwrap();
    }
    let password = std::fs::read(output.join("password")).unwrap();
    for cipher in ["aes-128-cbc", "aes-192-cbc", "aes-256-cbc"] {
        for digest in ["hmacWithSHA256", "hmacWithSHA384", "hmacWithSHA512"] {
            run(&[
                "pkcs8",
                "-topk8",
                "-in",
                "rsa.pem",
                "-out",
                "encrypted.pem",
                "-v2",
                cipher,
                "-v2prf",
                digest,
                "-passout",
                "file:password",
            ]);
            let parsed = tls::private_key(
                &std::fs::read(output.join("encrypted.pem")).unwrap(),
                Some(&password),
            )
            .unwrap();
            tls::provider()
                .key_provider
                .load_private_key(parsed)
                .unwrap();
        }
    }
    run(&[
        "pkcs8",
        "-topk8",
        "-in",
        "rsa.pem",
        "-out",
        "encrypted.pem",
        "-scrypt",
        "-v2",
        "aes-256-cbc",
        "-passout",
        "file:password",
    ]);
    let parsed = tls::private_key(
        &std::fs::read(output.join("encrypted.pem")).unwrap(),
        Some(&password),
    )
    .unwrap();
    tls::provider()
        .key_provider
        .load_private_key(parsed)
        .unwrap();
    std::fs::remove_dir_all(output).unwrap();
}

#[test]
#[ignore = "requires the isolated TLS fixture; run explicitly with SQLAERO_TEST_TLS"]
fn explicit_roots_replace_the_store_and_system_verifier_constructs() {
    let provider = tls::provider();
    let ca = std::fs::read(fixture().join("ca.pem")).unwrap();
    let mut roots = RootCertStore::empty();
    for certificate in tls::certificates(&ca).unwrap() {
        roots.add(certificate).unwrap();
    }
    assert_eq!(roots.len(), 1);
    WebPkiServerVerifier::builder_with_provider(Arc::new(roots), Arc::clone(&provider))
        .build()
        .unwrap();
    rustls_platform_verifier::Verifier::new(provider).unwrap();
}

#[cfg(target_vendor = "apple")]
#[test]
#[ignore = "requires the isolated TLS fixture; run explicitly with SQLAERO_TEST_TLS"]
fn apple_ca_only_policy_validates_without_a_hostname() {
    use security_framework::certificate::SecCertificate;
    use security_framework::policy::SecPolicy;
    use security_framework::secure_transport::SslProtocolSide;
    use security_framework::trust::SecTrust;

    let directory = fixture();
    let cert = tls::certificates(&std::fs::read(directory.join("server.pem")).unwrap()).unwrap();
    let ca = tls::certificates(&std::fs::read(directory.join("ca.pem")).unwrap()).unwrap();
    let cert = SecCertificate::from_der(cert[0].as_ref()).unwrap();
    let ca = SecCertificate::from_der(ca[0].as_ref()).unwrap();
    let policy = SecPolicy::create_ssl(SslProtocolSide::SERVER, None);
    let mut trust = SecTrust::create_with_certificates(&[cert], &[policy]).unwrap();
    trust.set_anchor_certificates(&[ca]).unwrap();
    trust.set_trust_anchor_certificates_only(true).unwrap();
    trust.set_network_fetch_allowed(false).unwrap();
    trust.evaluate_with_error().unwrap();
}
