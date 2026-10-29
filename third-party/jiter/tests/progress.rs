use jiter::{
    Jiter,
    progress::{CheckedError, Progress, WORK_WINDOW},
};

#[test]
fn fragmented_work_preserves_cumulative_checkpoint_positions() {
    let mut seen = Vec::new();
    let mut progress = Progress::new(|work| {
        seen.push(work);
        Ok::<(), ()>(())
    });
    for bytes in [0, WORK_WINDOW - 1, 1, 2 * WORK_WINDOW + 3, 0, WORK_WINDOW - 1, 1] {
        progress.checkpoint(bytes).unwrap();
    }
    assert_eq!(seen, [WORK_WINDOW, 3 * WORK_WINDOW + 3, 4 * WORK_WINDOW + 3]);
}

#[test]
fn cancellation_is_typed_and_precedes_completion_of_one_string() {
    let data = format!("\"{}\"", "a".repeat(WORK_WINDOW * 3));
    let mut parser = Jiter::new(data.as_bytes());
    let marker = std::sync::Arc::new(());
    let mut seen = 0;
    let mut progress = Progress::new(|work| {
        seen = work;
        Err(marker.clone())
    });
    match parser.known_str_checked(&mut progress) {
        Err(CheckedError::Interrupted(actual)) => assert!(std::sync::Arc::ptr_eq(&actual, &marker)),
        _ => panic!("the original cancellation must be returned"),
    }
    assert_eq!(seen, WORK_WINDOW);
    assert!(seen < data.len());
}

#[test]
fn first_and_later_keys_share_checked_string_work() {
    for first in [true, false] {
        let key = "é中😀".repeat(WORK_WINDOW);
        let data = if first {
            format!("{{\"{key}\":0}}")
        } else {
            format!("{{\"short\":0,\"{key}\":0}}")
        };
        let mut parser = Jiter::new(data.as_bytes());
        let mut calls = 0;
        let mut progress = Progress::new(|_| {
            calls += 1;
            Ok::<(), ()>(())
        });
        if !first {
            assert_eq!(
                parser.known_object_checked(&mut progress).unwrap(),
                Some("short")
            );
            assert!(matches!(
                parser.next_int().unwrap(),
                jiter::NumberInt::Int(0)
            ));
            assert_eq!(
                parser.next_key_checked(&mut progress).unwrap(),
                Some(key.as_str())
            );
        } else {
            assert_eq!(
                parser.known_object_checked(&mut progress).unwrap(),
                Some(key.as_str())
            );
        }
        assert!(calls > 1);
    }
}

#[test]
fn checked_strings_preserve_strict_results_and_error_positions() {
    for length in [
        0,
        1,
        WORK_WINDOW - 4,
        WORK_WINDOW - 1,
        WORK_WINDOW,
        WORK_WINDOW + 1,
        WORK_WINDOW * 2,
    ] {
        for suffix in [
            b"x\"".as_slice(),
            b"\xc3\xa9\xe4\xb8\xad\xf0\x9f\x98\x80\"",
            b"\\n\\t\\b\\f\\r\\\\\\/\\\"\\u0000\\uD83D\\uDE00\"",
            b"\\uD800\"",
            b"\\uDC00\"",
            b"\\u12xz\"",
            b"\xff\"",
            b"\x01\"",
            b"\\",
            b"",
        ] {
            let mut data = Vec::from(b"\"".as_slice());
            data.extend(std::iter::repeat_n(b'a', length));
            data.extend_from_slice(suffix);
            let mut original = Jiter::new(&data);
            let expected = original
                .known_str()
                .map(str::to_owned)
                .map_err(|error| error.description(&original));
            let mut parser = Jiter::new(&data);
            let mut progress = Progress::new(|_| Ok::<(), ()>(()));
            let actual = match parser.known_str_checked(&mut progress) {
                Ok(text) => Ok(text.to_owned()),
                Err(CheckedError::Json(error)) => Err(error.description(&parser)),
                other => panic!("unexpected checked result: {other:?}"),
            };
            assert_eq!(actual, expected, "length={length}, suffix={suffix:?}");
        }
    }
}
