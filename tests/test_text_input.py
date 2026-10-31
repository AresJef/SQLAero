"""SQL quoting and strict character conversion through actual native readers."""

# pyright: reportPrivateUsage=false

import pytest
from sqlaero import _sqlaero
from sqlaero._sqlaero import OperationalError

# Independently inspected differences from both MySQL conversion outputs.
# These are server semantics, not a fallback to the PSF codec at runtime.
MYSQL_DIFFERENCES: dict[str, dict[int, str | None]] = {
    "cp1256": {0x8A: None, 0x8F: None, 0x98: None, 0x9A: None,
               0x9F: None, 0xAA: None, 0xC0: None, 0xFF: None},
    "cp866": {0xFC: "ⁿ", 0xFD: "²"},
    "koi8u": {0x95: "•"},
}


class UnsupportedBytearray(bytearray):
    def __bytes__(self) -> bytes:
        raise AssertionError('Escape must not implicitly convert unsupported input')

    def __repr__(self) -> str:
        raise AssertionError('diagnostics must not format unsupported mutable input')


@pytest.mark.parametrize('source', [bytearray(), bytearray(range(256)),
                                  bytearray(b'x' * 262145), UnsupportedBytearray(b'private')])
def test_bytearray_is_rejected_at_every_scalar_encoding_entry(source: bytearray) -> None:
    for encode in (_sqlaero._encode_scalar, _sqlaero._encode_value,
                   _sqlaero._prepare_encoder().encode):
        with pytest.raises(TypeError, match='unsupported exact escape type'):
            encode(source)


@pytest.mark.parametrize('many', [False, True])
@pytest.mark.parametrize('itemize', [False, True])
@pytest.mark.parametrize('nested', [False, True])
def test_bytearray_rejection_uses_the_shared_binding_path(
    many: bool, itemize: bool, nested: bool,
) -> None:
    source = bytearray(b'private payload')
    value: object = (1, [source]) if nested else source
    args = [[value]] if many else [value] if itemize else value
    with pytest.raises(TypeError, match='unsupported exact escape type: bytearray') as caught:
        _sqlaero._bind_sql('SELECT %s', args, many=many, itemize=itemize)
    notes = '\n'.join(getattr(caught.value, '__notes__', []))
    assert 'while binding args' in notes and 'current request was not sent' in notes
    assert 'private payload' not in notes


@pytest.mark.parametrize("value", [b"", b"12", b"\x00\xff'\\"])
def test_binary_values_use_ordinary_binary_string_semantics(value: bytes) -> None:
    assert _sqlaero._encode_scalar(value) == b"CAST(X'" + value.hex().upper().encode("ascii") + b"' AS BINARY)"


@pytest.mark.parametrize("length", [0, 1, 31, 32, 33, 262143, 262144, 262145, 524305])
def test_bytes_keep_binary_work_window_contents(length: int) -> None:
    source = (bytes(range(256)) * ((length + 255) // 256))[:length]
    expected = b"CAST(X'" + source.hex().upper().encode("ascii") + b"' AS BINARY)"
    assert _sqlaero._encode_scalar(source) == expected
    assert _sqlaero._bind_sql("SELECT %s", [source]) == [b"\x03SELECT " + expected]


@pytest.mark.parametrize("length", [0, 1, 31, 32, 33, 262145])
def test_bytes_limit_uses_complete_binary_width_and_current_request(length: int) -> None:
    source = b"\x00" * length
    expected = b"CAST(X'" + b"00" * length + b"' AS BINARY)"
    assert len(expected) == 2 * length + 19
    assert _sqlaero._encode_scalar(source, limit=len(expected)) == expected
    with pytest.raises(OperationalError) as scalar:
        _sqlaero._encode_scalar(source, limit=len(expected) - 1)
    assert scalar.value.code == 65538
    request = b"\x03SELECT " + expected
    assert _sqlaero._bind_sql("SELECT %s", [source], limit=len(request)) == [request]
    with pytest.raises(OperationalError) as binding:
        _sqlaero._bind_sql("SELECT %s", [source], limit=len(request) - 1)
    assert binding.value.code == 65538


def test_completed_binary_owner_keeps_its_independent_contents() -> None:
    source = bytes(range(256)) * 1025
    expected = b"CAST(X'" + source.hex().upper().encode("ascii") + b"' AS BINARY)"
    owner = _sqlaero._prepare_encoder().encode(source)
    del source
    assert owner.to_bytes() == expected


def test_shared_stable_nested_input_can_use_one_encoder_concurrently() -> None:
    from concurrent.futures import ThreadPoolExecutor
    from threading import Barrier

    source = bytes(range(256)) * 1025
    value = (True, [source, ("中", None)])
    expected = b"(1,(CAST(X'" + source.hex().upper().encode("ascii") + b"' AS BINARY),('" + "中".encode() + b"',NULL)))"
    encoder = _sqlaero._prepare_encoder()
    start = Barrier(4)

    def encode() -> None:
        start.wait(timeout=10)
        for _ in range(8):
            assert encoder.encode(value).to_bytes() == expected

    with ThreadPoolExecutor(max_workers=4) as workers:
        futures = [workers.submit(encode) for _ in range(4)]
        for future in futures:
            future.result(timeout=20)



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
