//! The same CA-only bundle policy applies before trust-source composition.

use sqlaero_core::transport::tls::{self, MaterialError};

const CA: &[u8] = include_bytes!("fixtures/tls-diagnostics/anchors.pem");

#[test]
fn all_ca_blocks_are_required_and_foreign_pem_is_never_partially_accepted() {
    for label in ["PUBLIC KEY", "PRIVATE KEY", "UNRECOGNIZED TEST MATERIAL"] {
        let block =
            format!("-----BEGIN {label}-----\nU0VDUkVUX01BUktFUg==\n-----END {label}-----\n");
        for bundle in [
            [CA, block.as_bytes()].concat(),
            [block.as_bytes(), CA].concat(),
        ] {
            let error = tls::ca_certificates(&bundle).unwrap_err();
            assert!(matches!(error, MaterialError::Certificates));
            let message = error.to_string();
            assert!(!message.contains("U0VDUkVUX01BUktFUg"));
            assert!(!message.contains(label));
            // The explicitly unchanged client identity entry point retains its
            // existing mixed-block behavior; the stricter policy belongs to CA.
            assert_eq!(tls::certificates(&bundle).unwrap().len(), 1);
        }
    }
}

#[test]
fn explanation_text_and_multiple_valid_cas_are_accepted_without_deduplication() {
    let bundle = [
        &b"# CA deployment note\nordinary explanation text\n"[..],
        CA,
        &b"\n# A second copy is intentional.\r\n"[..],
        CA,
        &b"\nEnd of deployment notes.\n"[..],
    ]
    .concat();
    let roots = tls::ca_certificates(&bundle).unwrap();
    assert_eq!(roots.len(), 2);
    assert_eq!(roots[0].as_ref(), roots[1].as_ref());
    let text = std::str::from_utf8(CA).unwrap();
    for replacement in ["\r\n", "\r", " \r\n"] {
        let native_line_endings = text.replace('\n', replacement);
        assert_eq!(
            tls::ca_certificates(native_line_endings.as_bytes())
                .unwrap()
                .len(),
            1
        );
    }
}

#[test]
fn empty_broken_and_incomplete_ca_sections_fail_the_complete_bundle() {
    for suffix in [
        &b"-----BEGIN CERTIFICATE-----\n-----END CERTIFICATE-----\n"[..],
        &b"-----BEGIN CERTIFICATE-----\nnot base64!\n-----END CERTIFICATE-----\n"[..],
        &b"-----BEGIN CERTIFICATE-----\nQQ==\n"[..],
        &b"-----BEGIN CERTIFICATE-----\nQQ==\n-----END PUBLIC KEY-----\n"[..],
        &b"-----BEGIN CERTIFICATE-----\nQQ==\n-----BEGIN CERTIFICATE-----\nQQ==\n-----END CERTIFICATE-----\n"[..],
        &b"-----END CERTIFICATE-----\n"[..],
        &b"  -----BEGIN PUBLIC KEY-----\nQQ==\n  -----END PUBLIC KEY-----\n"[..],
    ] {
        assert!(tls::ca_certificates(&[CA, suffix].concat()).is_err());
    }
    for empty in [&b""[..], &b"# only comments\nexplanation\n"[..]] {
        assert!(tls::ca_certificates(empty).is_err());
    }
}

#[test]
fn directory_members_consume_the_same_complete_ca_policy() {
    struct Remove(std::path::PathBuf);
    impl Drop for Remove {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = Remove(
        std::env::temp_dir().join(format!("sqlaero-ca-policy-{}-{unique}", std::process::id())),
    );
    std::fs::create_dir(&directory.0).unwrap();
    let member = directory.0.join("active.pem");
    std::fs::write(&member, [CA, CA].concat()).unwrap();
    assert_eq!(
        tls::materials::directory_certificates(&directory.0)
            .unwrap()
            .len(),
        2
    );
    for label in ["PUBLIC KEY", "PRIVATE KEY", "UNRECOGNIZED TEST MATERIAL"] {
        let suffix =
            format!("-----BEGIN {label}-----\nU0VDUkVUX01BUktFUg==\n-----END {label}-----\n");
        std::fs::write(&member, [CA, suffix.as_bytes()].concat()).unwrap();
        let error = tls::materials::directory_certificates(&directory.0).unwrap_err();
        match &error {
            tls::materials::DirectoryError::Material { path, source } => {
                assert_eq!(path.as_ref(), member);
                assert!(matches!(source, MaterialError::Certificates));
            }
            _ => panic!("the complete member should fail its material policy"),
        }
        assert!(!error.to_string().contains("U0VDUkVUX01BUktFUg"));
    }
}
