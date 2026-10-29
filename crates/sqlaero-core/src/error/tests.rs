//! Independent semantic anchors and maintained-table identity checks.

use super::{
    CLIENT_CODES, ExceptionClass as C,
    catalog::{SERVER_CODES, classify},
};

#[test]
fn every_maintained_code_matches_the_independent_semantic_audit() {
    let oracle = include_str!("../../../../tests/data/error-classification-audit-20261026.tsv");
    let mut counts = [0usize; 2];
    let mut seen = std::collections::HashSet::new();
    for line in oracle.lines().filter(|line| !line.starts_with('#')) {
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 5);
        let code: u32 = fields[1].parse().unwrap();
        assert!(seen.insert((fields[0], code)));
        let expected = match fields[2] {
            "DatabaseError" => C::Database,
            "OperationalError" => C::Operational,
            "ProgrammingError" => C::Programming,
            "DataError" => C::Data,
            "IntegrityError" => C::Integrity,
            "InternalError" => C::Internal,
            "NotSupportedError" => C::NotSupported,
            "InterfaceError" => C::Interface,
            "InterfaceBusyError" => C::InterfaceBusy,
            "OperationalTimeoutError" => C::OperationalTimeout,
            other => panic!("unrecognized audit class {other}"),
        };
        if fields[0] == "server" {
            counts[0] += 1;
            let code = u16::try_from(code).unwrap();
            for state in [None, Some(*b"HY000"), Some(*b"23000"), Some(*b"42000")] {
                assert_eq!(classify(code, state), expected, "reviewed server {code}");
            }
        } else {
            assert_eq!(fields[0], "client");
            counts[1] += 1;
            let entry = CLIENT_CODES
                .iter()
                .find(|entry| entry.code as u32 == code)
                .unwrap();
            assert_eq!(entry.class, expected, "reviewed client {code}");
        }
    }
    assert_eq!(counts, [2046, 19]);
    assert_eq!(SERVER_CODES.len(), counts[0]);
    assert_eq!(CLIENT_CODES.len(), counts[1]);
}

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
