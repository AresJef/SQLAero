//! LOCAL classification shares the SQL lexer and preserves the actual request.

use sqlaero_core::bind::template::Compiler;
use sqlaero_core::charset::{codec::Codec, resolve};

fn classify(request: Vec<u8>, charset: &str, no_backslash: bool, ansi: bool) -> (Vec<u8>, bool) {
    let codec = Codec::new(resolve(Some(charset), None).unwrap().charset);
    let mut compiler = Compiler::for_local_query(request, codec, no_backslash, ansi);
    loop {
        if let Some(template) = compiler.advance().unwrap() {
            let local = template.is_single_local_infile();
            return (template.into_source(), local);
        }
    }
}

#[test]
fn ordinary_local_load_uses_protected_statement_boundaries() {
    for sql in [
        "LOAD DATA LOCAL INFILE 'label' INTO TABLE t",
        "/* before ; */ load /* gap */ data local infile 'x; %s ? %%' into table t; -- ;\n",
        "LOAD DATA LOW_PRIORITY LOCAL INFILE 'label' INTO TABLE t; # ;",
        "LOAD DATA CONCURRENT LOCAL INFILE 'label' INTO TABLE `t;name`",
        "LOAD DATA LOCAL INFILE 'a\\';still-label' INTO TABLE t",
        "LOAD DATA LOCAL INFILE 'label' INTO TABLE t SET a=1%2, b='%(name)s', c=%%",
    ] {
        let request = [b"\x03".as_slice(), sql.as_bytes()].concat();
        let pointer = request.as_ptr();
        let (retained, local) = classify(request.clone(), "utf8mb4", false, false);
        assert!(local, "{sql}");
        assert_eq!(retained, request);
        let (retained, _) = classify(request, "utf8mb4", false, false);
        assert_eq!(
            retained.as_ptr(),
            pointer,
            "the SQL allocation must be retained"
        );
    }
}

#[test]
fn unsupported_or_unreliable_forms_do_not_authorize_a_source() {
    for sql in [
        "SELECT 1",
        "LOAD DATA INFILE 'label' INTO TABLE t",
        "LOAD XML LOCAL INFILE 'x' INTO TABLE t",
        "LOAD DATA LOCAL INFILE 'x' INTO TABLE t; SELECT 1",
        "LOAD DATA LOCAL INFILE 'x' INTO TABLE t;;",
        "; LOAD DATA LOCAL INFILE 'x' INTO TABLE t",
        "/*!80000 LOAD DATA LOCAL INFILE 'x' INTO TABLE t */",
        "LOAD /*+ hint */ DATA LOCAL INFILE 'x' INTO TABLE t",
        "LOAD DATA LOCAL INFILE 'x' INTO TABLE t /*! SET a=1 */",
        "LOAD DATA LOCAL INFILE 'unterminated",
        "LOAD DATA LOCAL INFILE 'x' /* unterminated",
        "`LOAD` DATA LOCAL INFILE 'x' INTO TABLE t",
        "'LOAD' DATA LOCAL INFILE 'x' INTO TABLE t",
        "LOAD 'DATA' LOCAL INFILE 'x' INTO TABLE t",
        "LOAD DATA `LOCAL` INFILE 'x' INTO TABLE t",
    ] {
        let request = [b"\x03".as_slice(), sql.as_bytes()].concat();
        let (retained, local) = classify(request.clone(), "utf8mb4", false, false);
        assert!(!local, "{sql}");
        assert_eq!(retained, request);
    }
    for request in [
        Vec::new(),
        b"\x16LOAD DATA LOCAL INFILE 'x'".to_vec(),
        b"LOAD DATA LOCAL INFILE 'x'".to_vec(),
    ] {
        assert!(!classify(request, "utf8mb4", false, false).1);
    }
}

#[test]
fn actual_sql_modes_and_multibyte_trail_bytes_control_classification() {
    let sql = b"\x03LOAD DATA LOCAL INFILE 'a\\'; SELECT 1; -- '";
    assert!(classify(sql.to_vec(), "utf8mb4", false, false).1);
    assert!(!classify(sql.to_vec(), "utf8mb4", true, false).1);
    let quoted = b"\x03\"LOAD\" DATA LOCAL INFILE 'x'";
    assert!(!classify(quoted.to_vec(), "utf8mb4", false, false).1);
    assert!(!classify(quoted.to_vec(), "utf8mb4", false, true).1);
    let codec = Codec::new(resolve(Some("sjis"), None).unwrap().charset);
    let mut request = vec![3];
    codec
        .encode(
            "LOAD DATA LOCAL INFILE '表' INTO TABLE t;",
            &mut request,
            usize::MAX,
        )
        .unwrap();
    assert!(classify(request, "sjis", false, false).1);
}

#[test]
fn large_protected_source_yields_without_reinterpreting_binding_directives() {
    let mut request = b"\x03LOAD DATA LOCAL INFILE 'label' INTO TABLE t /*".to_vec();
    request.extend(std::iter::repeat_n(b';', 600_000));
    request.extend_from_slice(b"*/ SET a=%(name)s, b=%s, c=%%;");
    let pointer = request.as_ptr();
    let codec = Codec::new(resolve(None, None).unwrap().charset);
    let mut compiler = Compiler::for_local_query(request, codec, false, false);
    assert!(compiler.advance().unwrap().is_none());
    let template = loop {
        if let Some(template) = compiler.advance().unwrap() {
            break template;
        }
    };
    assert!(template.is_single_local_infile());
    assert_eq!(template.slots(), 0);
    let request = template.into_source();
    assert_eq!(request.as_ptr(), pointer);
    assert!(request.ends_with(b"SET a=%(name)s, b=%s, c=%%;"));
}

#[test]
fn a_non_load_first_keyword_returns_before_scanning_a_long_sql_tail() {
    let codec = Codec::new(resolve(None, None).unwrap().charset);
    for prefix in [
        b"\x03SELECT ".as_slice(),
        b"\x03/* ordinary */ SHOW ",
        b"\x03/*!80000 ",
        b"\x03'not LOAD'",
    ] {
        let mut request = prefix.to_vec();
        request.extend(std::iter::repeat_n(b'\'', 2_000_000));
        let pointer = request.as_ptr();
        let mut compiler = Compiler::for_local_query(request, codec, false, false);
        let template = compiler
            .advance()
            .unwrap()
            .expect("a rejected prefix must not scan the long tail");
        assert!(!template.is_single_local_infile());
        let request = template.into_source();
        assert_eq!(request.as_ptr(), pointer);
    }
}
