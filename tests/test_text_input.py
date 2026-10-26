"""SQL quoting and strict character conversion through actual native readers."""

# pyright: reportPrivateUsage=false

import pytest
from sqlaero import _sqlaero

# Independently inspected differences from both MySQL conversion outputs.
# These are server semantics, not a fallback to the PSF codec at runtime.
MYSQL_DIFFERENCES: dict[str, dict[int, str | None]] = {
    "cp1256": {0x8A: None, 0x8F: None, 0x98: None, 0x9A: None,
               0x9F: None, 0xAA: None, 0xC0: None, 0xFF: None},
    "cp866": {0xFC: "ⁿ", 0xFD: "²"},
    "koi8u": {0x95: "•"},
}


@pytest.mark.parametrize("value", [b"", b"12", b"\x00\xff'\\", bytearray(b"\x00a\xff")])
def test_binary_values_use_ordinary_binary_string_semantics(value: bytes | bytearray) -> None:
    assert _sqlaero._encode_scalar(value) == b"CAST(X'" + value.hex().upper().encode("ascii") + b"' AS BINARY)"


def test_utf8_quoting_keeps_unicode_patterns_and_both_sql_modes() -> None:
    value = "中'\\\0%_\n\r\x1a\""
    assert _sqlaero._encode_scalar(value) == "'中\\'\\\\\\0%_\\n\\r\\Z\\\"'".encode("utf-8")
    assert _sqlaero._encode_scalar(value, no_backslash=True) == ("'" + value.replace("'", "''") + "'").encode("utf-8")


def test_national_single_byte_symbols_are_quoted_after_encoding() -> None:
    assert _sqlaero._encode_scalar("Ö", charset="swe7") == b"'\\\\'"
    assert _sqlaero._encode_scalar("Ö", charset="swe7", no_backslash=True) == b"'\\'"
    assert _sqlaero._decode_text(b"\\", charset="swe7") == "Ö"


def test_encoding_failure_keeps_original_object_and_character_coordinates() -> None:
    source = "Café😀"
    with pytest.raises(UnicodeEncodeError) as caught:
        _sqlaero._encode_scalar(source, charset="latin1")
    assert caught.value.object is source
    assert caught.value.encoding == "latin1"
    assert (caught.value.start, caught.value.end) == (4, 5)
    with pytest.raises(UnicodeEncodeError) as caught:
        _sqlaero._encode_scalar("a\ud800b", charset="latin1")
    assert (caught.value.encoding, caught.value.start, caught.value.end) == ("latin1", 1, 2)


@pytest.mark.parametrize(("encoding", "data", "position"), [
    ("utf8mb4", b"a\xffb", (1, 2)),
    ("utf8mb4", b"a\xe4\xb8", (1, 3)),
    ("ascii", b"a\x80b", (1, 2)),
    ("cp1250", b"a\x81b", (1, 2)),
    ("tis620", b"a\xfcb", (1, 2)),
])
def test_invalid_sequences_do_not_become_replacement_text(encoding: str, data: bytes, position: tuple[int, int]) -> None:
    with pytest.raises(UnicodeDecodeError) as caught:
        _sqlaero._decode_text(data, charset=encoding)
    assert caught.value.object == data
    assert (caught.value.start, caught.value.end) == position


@pytest.mark.parametrize(("mysql_name", "python_name"), [
    ("cp1250", "cp1250"), ("cp1251", "cp1251"), ("cp1256", "cp1256"),
    ("cp1257", "cp1257"), ("cp850", "cp850"), ("cp852", "cp852"),
    ("cp866", "cp866"), ("koi8r", "koi8_r"), ("koi8u", "koi8_u"),
    ("latin2", "iso8859_2"), ("latin5", "iso8859_9"), ("latin7", "iso8859_13"),
    ("macce", "mac_latin2"), ("macroman", "mac_roman"),
])
def test_single_byte_common_reference_points_use_an_independent_psf_codec(mysql_name: str, python_name: str) -> None:
    # This oracle is independent of the MySQL table extraction. Any actual
    # server-specific discrepancy must be identified explicitly, never hidden
    # by calculating the expected result with the product table itself.
    for value in range(256):
        data = bytes([value])
        if value in MYSQL_DIFFERENCES.get(mysql_name, {}):
            expected = MYSQL_DIFFERENCES[mysql_name][value]
            if expected is None:
                with pytest.raises(UnicodeDecodeError):
                    _sqlaero._decode_text(data, charset=mysql_name)
            else:
                assert _sqlaero._decode_text(data, charset=mysql_name) == expected
            continue
        try:
            expected = data.decode(python_name, "strict")
        except UnicodeDecodeError:
            with pytest.raises(UnicodeDecodeError):
                _sqlaero._decode_text(data, charset=mysql_name)
        else:
            assert _sqlaero._decode_text(data, charset=mysql_name) == expected, (mysql_name, value)


@pytest.mark.parametrize(("mysql_name", "python_name", "source"), [
    ("big5", "big5", "繁體中文"),
    ("cp932", "cp932", "日本語ソ漢字ｱ"),
    ("sjis", "shift_jis", "日本語ソ漢字ｱ"),
    ("ujis", "euc_jp", "日本語ソ漢字ｱ"),
    ("eucjpms", "euc_jp", "日本語ソ漢字ｱ"),
    ("euckr", "cp949", "한글가힣똠"),
    ("gb2312", "gb2312", "简体中文"),
    ("gbk", "gbk", "简体中文"),
    ("gb18030", "gb18030", "简体中文😀\uE7C7\ufffd"),
])
def test_multibyte_common_values_use_an_independent_codec(mysql_name: str, python_name: str, source: str) -> None:
    adjusted = source.translate({0x1E3F: 0xE7C7, 0xE7C7: 0x1E3F}) if mysql_name == "gb18030" else source
    expected = adjusted.encode(python_name)
    assert _sqlaero._decode_text(expected, charset=mysql_name) == source
    # Quote one actual character at a time, so a valid trail 0x5C is preserved.
    quoted = bytearray(b"'")
    for point in adjusted:
        encoded = point.encode(python_name)
        quoted.extend(encoded.replace(b"\\", b"\\\\") if len(encoded) == 1 else encoded)
    quoted.extend(b"'")
    assert _sqlaero._encode_scalar(source, charset=mysql_name) == bytes(quoted)


def test_gb18030_full_unicode_scalar_space_matches_independent_revision_reference() -> None:
    # The server matrix checks every BMP four-byte candidate and astral range
    # boundaries. CPython's codec retains the GB18030-2000 two-point mapping.
    # Apply its explicit 2005 U+1E3F/U+E7C7 swap, confirmed independently in both
    # server observation files, before checking the complete scalar space.
    source = "".join(chr(point) for point in range(0x110000) if not 0xD800 <= point <= 0xDFFF)
    encoded = source.translate({0x1E3F: 0xE7C7, 0xE7C7: 0x1E3F}).encode("gb18030")
    actual = _sqlaero._decode_text(encoded, charset="gb18030")
    if actual != source:
        first = next((index for index, (left, right) in enumerate(zip(actual, source)) if left != right), min(len(actual), len(source)))
        pytest.fail(f"GB18030 Unicode mismatch at scalar index {first}")
