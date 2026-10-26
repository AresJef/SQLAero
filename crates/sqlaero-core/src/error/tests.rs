//! Independent semantic anchors and maintained-table identity checks.

use super::{
    CLIENT_CODES, ExceptionClass as C,
    catalog::{SERVER_CODES, classify},
};

#[test]
fn known_semantics_precede_even_a_conflicting_received_state() {
    for (code, expected) in [
        (1062, C::Integrity),
        (1048, C::Integrity),
        (1054, C::Programming),
        (1205, C::Operational),
        (1213, C::Operational),
        (1235, C::NotSupported),
        (1265, C::Data),
        (1815, C::Internal),
        (1644, C::Database),
    ] {
        assert_eq!(
            classify(code, Some(*b"HY000")),
            expected,
            "known code {code}"
        );
    }
    assert_eq!(classify(1062, Some(*b"42000")), C::Integrity);
    assert_eq!(classify(1054, Some(*b"23000")), C::Programming);
}

#[test]
fn unknown_codes_have_only_the_approved_sqlstate_fallbacks() {
    for (state, expected) in [
        ("08000", C::Operational),
        ("0A000", C::NotSupported),
        ("22003", C::Data),
        ("23000", C::Integrity),
        ("42000", C::Programming),
        ("HY000", C::Database),
        ("40001", C::Database),
        ("01000", C::Database),
    ] {
        assert_eq!(
            classify(65535, Some(state.as_bytes().try_into().unwrap())),
            expected
        );
    }
    assert_eq!(classify(65535, None), C::Database);
}

#[test]
fn maintained_names_codes_docs_and_runtime_dispatch_share_one_registry() {
    assert!(
        SERVER_CODES
            .windows(2)
            .all(|pair| pair[0].code < pair[1].code)
    );
    for entry in SERVER_CODES {
        assert!(!entry.names.is_empty() && !entry.doc.is_empty());
        assert_eq!(classify(entry.code, None), entry.class);
        for name in entry.names {
            assert!(entry.doc.contains(name));
        }
    }
    assert!(
        CLIENT_CODES
            .windows(2)
            .all(|pair| (pair[0].code as u32) < pair[1].code as u32)
    );
    for entry in CLIENT_CODES {
        assert!(entry.code as u32 > u32::from(u16::MAX));
        assert_eq!(entry.code.entry().name, entry.name);
        assert_eq!(entry.code.entry().class, entry.class);
        assert!(entry.doc.contains(&format!("{:?}Error", entry.class)));
    }
}
