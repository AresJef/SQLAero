"""A-lane value-contract, SQL-planning and session-helper regressions."""

# pyright: reportPrivateUsage=false

from datetime import date, datetime, time, timedelta, timezone
from decimal import Decimal
from typing import Any, cast

import pytest
from sqlaero import _sqlaero


def native_scalar(
    data: object, *, utc_confirmed: bool = True, limit: int = 67_108_864,
    charset: str | None = None, no_backslash: bool = False,
) -> bytes:
    """Access the private installation probe without extending the public stub."""
    return cast(bytes, getattr(_sqlaero, "_encode_native_scalar")(
        data, utc_confirmed=utc_confirmed, limit=limit, charset=charset,
        no_backslash=no_backslash,
    ))


@pytest.mark.parametrize(("value", "expected"), [
    (None, b"NULL"),
    (True, b"1"),
    (False, b"0"),
    (-17, b"-17"),
    (2**64 - 1, b"18446744073709551615"),
    (10**64, b"1" + b"0" * 64),
    (-0.0, b"-0.0e0"),
    (float("inf"), b"NULL"),
    (float("nan"), b"NULL"),
    (Decimal("-0.000"), b"-0"),
    (Decimal("12.34500"), b"12.345"),
    (Decimal("1e-65"), b"0." + b"0" * 64 + b"1"),
    ("O'Reilly", b"'O\\'Reilly'"),
    (b"\x00\xff", b"CAST(X'00FF' AS BINARY)"),
    (date(2024, 2, 29), b"'2024-02-29'"),
    (datetime(2024, 2, 29, 12, 13, 14, 123456), b"'2024-02-29 12:13:14.123456'"),
    (datetime(2024, 1, 1, tzinfo=timezone(timedelta(hours=1))), b"'2023-12-31 23:00:00'"),
    (time(12, 13, 14, 123456), b"'12:13:14.123456'"),
    (timedelta(days=-1, microseconds=1), b"'-23:59:59.999999'"),
])
def test_native_scalar_sink_uses_the_same_exact_input_contract(value: object, expected: bytes) -> None:
    assert native_scalar(value) == expected
    assert _sqlaero._encode_scalar(value) == expected


@pytest.mark.parametrize("no_backslash", [False, True])
def test_native_scalar_sink_keeps_long_text_and_binary_work_windows(no_backslash: bool) -> None:
    text = "中文'\\" * 60_000
    binary = bytes(range(256)) * 1200
    for value in (text, binary):
        assert native_scalar(value, no_backslash=no_backslash) == _sqlaero._encode_scalar(
            value, no_backslash=no_backslash,
        )


def test_native_scalar_sink_retains_approved_subclass_protocols() -> None:
    class Integer(int):
        def __int__(self) -> int:
            return 42

    class Float(float):
        def __float__(self) -> float:
            return 1.25

    class Text(str):
        def __str__(self) -> str:
            return "converted"

    for value, expected in [(Integer(1), b"42"), (Float(9), b"1.25e0"), (Text("base"), b"'converted'")]:
        assert native_scalar(value) == expected


def test_native_scalar_sink_preserves_numpy_temporal_precision_and_nulls() -> None:
    np: Any = pytest.importorskip("numpy")
    for value, expected in [
        (np.int64(-17), b"-17"),
        (np.uint64(2**64 - 1), b"18446744073709551615"),
        (np.datetime64(-1, "ns"), b"'1969-12-31 23:59:59.999999999'"),
        (np.timedelta64(-1, "ns"), b"'-00:00:00.000000001'"),
        (np.datetime64("NaT"), b"NULL"),
    ]:
        assert native_scalar(value) == expected


def test_native_scalar_sink_preserves_original_unicode_error_positions() -> None:
    with pytest.raises(UnicodeEncodeError) as caught:
        native_scalar("é中文", charset="ascii")
    assert (caught.value.encoding, caught.value.start, caught.value.end) == ("ascii", 0, 1)


@pytest.mark.parametrize("native", [False, True])
def test_utf8mb3_reports_original_characters_across_a_long_value_checkpoint(native: bool) -> None:
    encode = native_scalar if native else _sqlaero._encode_scalar
    source = "é" * 131_073 + "😀"
    with pytest.raises(UnicodeEncodeError) as caught:
        encode(source, charset="utf8mb3")
    assert (caught.value.encoding, caught.value.object, caught.value.start, caught.value.end) == (
        "utf8mb3", source, 131_073, 131_074,
    )
    assert encode("中文\uffff", charset="utf8mb3") == "'中文\uffff'".encode()


def test_utf8mb3_output_retains_strict_and_surrogateescape_policies() -> None:
    data = b"A" + "😀".encode() + b"B"
    with pytest.raises(UnicodeDecodeError) as caught:
        _sqlaero._decode_field(253, data, collation_id=33, decode_text_strict=True)
    assert caught.value.encoding == "utf8mb3" and caught.value.start == 1
    assert _sqlaero._decode_field(253, data, collation_id=33, decode_text_strict=False) == (
        "A\udcf0\udc9f\udc98\udc80B"
    )
    assert _sqlaero._decode_field(253, "中文".encode(), collation_id=33) == "中文"
