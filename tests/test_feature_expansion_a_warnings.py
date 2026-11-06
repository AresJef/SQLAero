"""Diagnostic rows decode from actual metadata without a name snapshot."""

# pyright: reportPrivateUsage=false

from typing import Any

import pytest
from sqlaero import _sqlaero
from sqlaero.errors import DataError, DatabaseError, NotSupportedError, OperationalError
from test_buffered_reader import column, packets


def decode(source: list[bytes], **options: Any) -> tuple[tuple[str, int, str], ...]:
    return getattr(_sqlaero, "_warnings_from_packets")(source, **options)


def columns(collation: int = 255) -> list[bytes]:
    return [column("Level", 253, collation), column("Code", 3, 63), column("Message", 253, collation)]


def test_warning_rows_keep_exact_types_order_and_independent_final_ownership() -> None:
    source = packets(columns(), [(b"Warning", b"1292", b"number"), (b"Error", b"1644", "诊断消息".encode())])
    result = decode(source)
    source.clear()
    assert result == (("Warning", 1292, "number"), ("Error", 1644, "诊断消息"))
    assert type(result) is tuple
    assert all(type(row) is tuple and tuple(map(type, row)) == (str, int, str) for row in result)
    assert decode(packets(columns(), [])) == ()


def test_unknown_or_binary_value_encoding_is_never_guessed() -> None:
    for collation in (65534, 63):
        source = packets(columns(collation), [(b"Warning", b"1292", b"plain ASCII")])
        for strict in (False, True):
            with pytest.raises(NotSupportedError):
                decode(source, strict=strict)
    assert decode(packets(columns(65534), [])) == ()


def test_strict_text_policy_preserves_existing_unicode_error_or_surrogateescape() -> None:
    source = packets(columns(), [(b"Warning", b"1292", b"A\xffB")])
    with pytest.raises(UnicodeDecodeError) as failure:
        decode(source)
    assert failure.value.object == b"A\xffB" and (failure.value.start, failure.value.end) == (1, 2)
    assert decode(source, strict=False) == (("Warning", 1292, "A\udcffB"),)


def test_non_utf8_metadata_uses_the_actual_column_codec() -> None:
    assert decode(packets(columns(8), [(b"Warning", b"1292", b"caf\xe9")])) == (("Warning", 1292, "café"),)


def test_diagnostic_shape_null_and_integer_content_errors_are_not_silenced() -> None:
    with pytest.raises(OperationalError):
        decode(packets(columns()[:2], [(b"Warning", b"1292")]))
    with pytest.raises(OperationalError):
        decode(packets(columns(), [(b"Warning", None, b"message")]))
    with pytest.raises(DataError):
        decode(packets(columns(), [(b"Warning", b"not-an-integer", b"message")]))


def test_original_server_err_keeps_its_code_and_origin() -> None:
    with pytest.raises(DatabaseError) as failure:
        decode([b"\xff" + (1644).to_bytes(2, "little") + b"#45000original primary"])
    assert failure.value.code == 1644 and failure.value.is_server_error is True
