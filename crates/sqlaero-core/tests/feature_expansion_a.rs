//! Independent A-lane encoding and SQL-planning contracts.

use sqlaero_core::charset::{
    codec::{Codec, CodecError},
    resolve,
};

#[test]
fn explicit_utf8mb3_uses_its_canonical_default_and_never_changes_utf8_aliases() {
    let selected = resolve(Some(" UTF8MB3 "), None).unwrap();
    assert_eq!(
        (
            selected.charset.name,
            selected.collation.name,
            selected.collation.id
        ),
        ("utf8mb3", "utf8mb3_general_ci", 33)
    );
    assert_eq!(
        resolve(None, Some("UTF8MB3_BIN")).unwrap().charset.name,
        "utf8mb3"
    );
    assert_eq!(
        resolve(Some("utf8mb3"), Some("utf8mb3_unicode_ci"))
            .unwrap()
            .collation
            .id,
        192
    );
    assert_eq!(
        resolve(Some("utf-8"), None).unwrap().charset.name,
        "utf8mb4"
    );
    assert_eq!(resolve(None, None).unwrap().collation.id, 255);
    assert!(resolve(Some("utf8"), Some("utf8mb3_general_ci")).is_err());
    assert!(resolve(None, Some("utf8_general_ci")).is_err());
}

#[test]
fn utf8mb3_writes_bmp_and_sql_metacharacters_in_both_quoting_modes() {
    let codec = Codec::new(resolve(Some("utf8mb3"), None).unwrap().charset);
    let text = "\0中文'\\\u{ffff}";
    for (no_backslash, expected) in [
        (false, "'\\0中文\\'\\\\\u{ffff}'"),
        (true, "'\0中文''\\\u{ffff}'"),
    ] {
        let mut output = Vec::new();
        codec
            .literal(text, &mut output, no_backslash, 4096)
            .unwrap();
        assert_eq!(output, expected.as_bytes());
        assert_eq!(codec.decode(&output).unwrap(), expected);
    }
}

#[test]
fn utf8mb3_rejects_supplementary_characters_at_the_original_utf8_offset() {
    let codec = Codec::new(resolve(Some("utf8mb3"), None).unwrap().charset);
    for text in ["中\u{10000}", "中😀", "中\u{10ffff}"] {
        let mut output = Vec::new();
        assert!(matches!(
            codec.encode(text, &mut output, 4096),
            Err(CodecError::Encode { offset: 3, .. })
        ));
        let mut output = Vec::new();
        assert!(matches!(
            codec.literal(text, &mut output, false, 4096),
            Err(CodecError::Encode { offset: 3, .. })
        ));
        assert!(codec.decode(text.as_bytes()).is_err());
        assert!(codec.decode_point(text.as_bytes(), 3).is_err());
    }
    let mut output = Vec::new();
    let mb4 = Codec::new(resolve(Some("utf8"), None).unwrap().charset);
    mb4.encode("中😀", &mut output, 4096).unwrap();
    assert_eq!(output, "中😀".as_bytes());
}

fn batch_template(sql: &str, ansi: bool) -> sqlaero_core::bind::template::Template {
    let codec = Codec::new(resolve(None, None).unwrap().charset);
    let mut compiler = Compiler::for_batch(sql.as_bytes().to_vec(), codec, false, ansi);
    loop {
        if let Some(template) = compiler.advance().unwrap() {
            return template;
        }
    }
}

#[test]
fn approved_values_layouts_keep_complete_original_groups_and_fixed_suffixes() {
    for (sql, repeated) in [
        ("INSERT t VALUES (1,%s)", "(1,%s)"),
        (
            "INSERT t VALUES (-.25e+2,%s,NULL,TRUE,FALSE,DEFAULT)",
            "(-.25e+2,%s,NULL,TRUE,FALSE,DEFAULT)",
        ),
        (
            "INSERT t VALUES (X'0aFF',B'0101',0xDEAD,0b101,%s,'literal %s')",
            "(X'0aFF',B'0101',0xDEAD,0b101,%s,'literal %s')",
        ),
        (
            "INSERT t VALUES (%s), /* original group */ (DEFAULT,%s) AS new(x,y) ON DUPLICATE KEY UPDATE y=CONCAT(new.y,'literal %s')",
            "(%s), /* original group */ (DEFAULT,%s)",
        ),
        (
            "REPLACE LOW_PRIORITY INTO db.t PARTITION (p0,`p1`) (a,b) VALUES (DEFAULT,%s),(%s,7);",
            "(DEFAULT,%s),(%s,7)",
        ),
        (
            "INSERT t VALUES ROW(%s,DEFAULT),ROW(2,%s) ON DUPLICATE KEY UPDATE b=b+1",
            "ROW(%s,DEFAULT),ROW(2,%s)",
        ),
        (
            "REPLACE t VALUES ROW(%s,3),ROW(%s,4)",
            "ROW(%s,3),ROW(%s,4)",
        ),
        (
            "INSERT t VALUES %s AS new ON DUPLICATE KEY UPDATE a=new.a+1",
            "%s",
        ),
    ] {
        let mut template = batch_template(sql, false);
        let layout = template
            .prepare_merge()
            .unwrap_or_else(|| panic!("approved: {sql}"));
        assert_eq!(&sql[layout.row], repeated, "{sql}");
    }
}

#[test]
fn values_merge_rejects_unapproved_expressions_and_invalid_tail_combinations() {
    for sql in [
        "INSERT t VALUES (?)",
        "INSERT t VALUES ?",
        "INSERT t VALUES (ABS(%s))",
        "INSERT t VALUES (%s+1)",
        "INSERT t VALUES (DEFAULT(a),%s)",
        "INSERT t VALUES (@v,%s)",
        "INSERT t VALUES ((SELECT 1),%s)",
        "INSERT t VALUES (1e,%s)",
        "INSERT t VALUES (0xZ,%s)",
        "INSERT t VALUES (X'GG',%s)",
        "INSERT t VALUES (B'012',%s)",
        "INSERT t VALUES (10%%3,%s)",
        "INSERT t VALUES (%s),(2) ON DUPLICATE KEY UPDATE a=%s",
        "INSERT t VALUES (%s);SELECT 1",
        "INSERT t VALUES (%s),(%s);;",
        "REPLACE t VALUES (%s) AS new",
        "REPLACE t VALUES (%s) ON DUPLICATE KEY UPDATE a=1",
        "INSERT t VALUES ROW(%s) AS new",
        "INSERT t VALUES ROW(%s),(%s)",
        "INSERT /*+ hint */ t VALUES (%s)",
        "INSERT t PARTITION (%s) VALUES (%s)",
    ] {
        assert!(
            batch_template(sql, false).prepare_merge().is_none(),
            "{sql}"
        );
    }
    assert!(
        batch_template("INSERT t VALUES (\"text\",%s)", false)
            .prepare_merge()
            .is_some()
    );
    assert!(
        batch_template("INSERT t VALUES (\"identifier\",%s)", true)
            .prepare_merge()
            .is_none()
    );
}

#[test]
fn complete_multigroup_input_is_indivisible_at_the_hard_limit() {
    use sqlaero_core::bind::batch::Blocks;
    let mut template = batch_template(
        "INSERT t VALUES (%s),(%s) AS new ON DUPLICATE KEY UPDATE a=new.a+1",
        false,
    );
    let layout = template.prepare_merge().unwrap();
    let one = b"\x03INSERT t VALUES (1),(2) AS new ON DUPLICATE KEY UPDATE a=new.a+1";
    let mut blocks = Blocks::new(&template, &layout, one.len(), one.len()).unwrap();
    assert!(blocks.push(b"(1),(2)").unwrap());
    assert!(!blocks.push(b"(3),(4)").unwrap());
    assert_eq!(blocks.rows(), 1);
    assert_eq!(blocks.take().unwrap().unwrap(), one);
    assert!(blocks.push(b"(333),(444)").is_err());
    assert_eq!(blocks.rows(), 0);
}

fn native_template(
    sql: &str,
    no_backslash: bool,
    ansi: bool,
) -> Result<sqlaero_core::bind::template::Template, sqlaero_core::bind::template::TemplateError> {
    let codec = Codec::new(resolve(None, None).unwrap().charset);
    let mut compiler = Compiler::for_prepared(sql.as_bytes().to_vec(), codec, no_backslash, ansi);
    loop {
        if let Some(template) = compiler.advance()? {
            return Ok(template);
        }
    }
}

#[test]
fn prepared_markers_share_the_original_lexer_and_percent_edit_ranges() {
    let sql = "SELECT %s, '中?%s%%', `?%s`, \"?%s\", 10%%3 /* ? %s %% */ , %s # ? %s %%";
    for ansi in [false, true] {
        let template = native_template(sql, false, ansi).unwrap();
        assert_eq!(template.slots(), 2);
        let mut writer = template.prepare_request(4096).unwrap();
        let request = loop {
            if let Some(request) = writer.advance().unwrap() {
                break request;
            }
        };
        assert_eq!(
            request,
            "\x16SELECT ?, '中?%s%%', `?%s`, \"?%s\", 10%3 /* ? %s %% */ , ? # ? %s %%".as_bytes()
        );
        assert!(writer.advance().is_err());
    }
}

#[test]
fn prepared_active_raw_question_is_rejected_at_original_unicode_position() {
    use sqlaero_core::bind::template::TemplateError;
    for sql in ["SELECT '中', ?", "SELECT %s, ?", "SELECT '😀', ?"] {
        let expected = sql.chars().position(|point| point == '?').unwrap();
        assert!(
            matches!(native_template(sql, false, false), Err(TemplateError::RawQuestion { offset }) if offset == expected),
            "{sql}"
        );
    }
    assert!(matches!(
        native_template("SELECT /*! ? */ 1", false, false),
        Err(TemplateError::CommentBinding { .. })
    ));
    assert!(native_template("SELECT '?' /* ? */ # ?", false, false).is_ok());
}

#[test]
fn prepared_mapping_yields_between_large_native_copies_and_checks_full_limit() {
    let sql = format!("SELECT '{}',%s", "x".repeat(600_000));
    let template = native_template(&sql, false, false).unwrap();
    let expected = format!("\x16SELECT '{}',?", "x".repeat(600_000)).into_bytes();
    let mut writer = template.prepare_request(expected.len()).unwrap();
    let mut yields = 0;
    let actual = loop {
        if let Some(request) = writer.advance().unwrap() {
            break request;
        }
        yields += 1;
    };
    assert!(yields >= 2);
    assert_eq!(actual, expected);
    let mut writer = template.prepare_request(expected.len() - 1).unwrap();
    loop {
        match writer.advance() {
            Err(_) => break,
            Ok(None) => (),
            Ok(Some(_)) => panic!("oversized prepare succeeded"),
        }
    }
}
