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


def native_rows(
    data: object, *, many: bool = False, itemize: bool = True, charset: str | None = None,
) -> list[bytes]:
    """Exercise actual row shape adaptation through an immediate native sink."""
    return cast(list[bytes], getattr(_sqlaero, "_encode_native_rows")(
        data, many=many, itemize=itemize, charset=charset,
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


@pytest.mark.parametrize(("data", "many", "expected"), [
    (42, False, [b"42"]), (42, True, [b"42"]),
    (None, False, [b"NULL"]), ([1, "x", None], False, [b"1'x'NULL"]),
    ([1, "x", None], True, [b"1", b"'x'", b"NULL"]),
    ([(1, "x"), (2, "y")], True, [b"1'x'", b"2'y'"]),
    ([], False, [b""]), ([], True, [b""]), ([(), ()], True, [b"", b""]),
])
def test_native_rows_preserve_original_positions_and_empty_logical_rows(
    data: object, many: bool, expected: list[bytes],
) -> None:
    assert native_rows(data, many=many) == expected


def test_native_rows_reject_fragments_without_parsing_escaped_sql() -> None:
    from sqlaero.errors import NotSupportedError

    for data, itemize in [([1, 2], False), ([[1, 2]], True)]:
        with pytest.raises(NotSupportedError):
            native_rows(data, itemize=itemize)
    with pytest.raises(TypeError, match="bytearray"):
        native_rows([bytearray(b"unsupported")])


def test_native_rows_read_numpy_vectors_tables_records_and_temporal_ticks() -> None:
    np: Any = pytest.importorskip("numpy")
    assert native_rows(np.array([1, 2], dtype="int64")) == [b"12"]
    assert native_rows(np.array([1, 2], dtype="int64"), many=True) == [b"1", b"2"]
    assert native_rows(np.array([[1, 2], [3, 4]], dtype="uint64")) == [b"12", b"34"]
    record = np.array([(1, 2)], dtype=[("a", "i8"), ("b", "i8")]).view(np.recarray)[0]
    assert native_rows(record, many=True) == [b"12"]
    assert native_rows(np.array([-1, 1], dtype="datetime64[ns]"), many=True) == [
        b"'1969-12-31 23:59:59.999999999'", b"'1970-01-01 00:00:00.000000001'",
    ]
    assert native_rows(np.empty((0, 2), dtype="int64")) == [b""]
    assert native_rows(np.empty((2, 0), dtype="int64")) == [b"", b""]


def test_structured_numpy_void_uses_the_final_record_shape_contract() -> None:
    np: Any = pytest.importorskip("numpy")
    record = np.array([(1, 2)], dtype=[("a", "i8"), ("b", "i8")])[0]
    assert native_rows(record, many=True) == [b"12"]


@pytest.mark.parametrize("many", [False, True])
def test_structured_numpy_void_keeps_schema_order_and_parent_ownership(many: bool) -> None:
    import gc

    np: Any = pytest.importorskip("numpy")
    owner = np.array([(7, "ab")], dtype=[("z", "i8"), ("a", "U2")])
    record = owner[0]
    del owner
    gc.collect()
    assert native_rows(record, many=many) == [b"7'ab'"]
    assert _sqlaero._bind_sql("SELECT %s,%s", record, many=many) == [b"\x03SELECT 7,'ab'"]
    assert _sqlaero._bind_sql("VALUES %s", record, many=many, itemize=False) == [b"\x03VALUES (7,'ab')"]


@pytest.mark.parametrize("many", [False, True])
def test_structured_numpy_void_zero_fields_stay_one_real_parameter_row(many: bool) -> None:
    np: Any = pytest.importorskip("numpy")
    record = np.zeros(1, dtype=[])[0]
    assert native_rows(record, many=many) == [b""]
    assert _sqlaero._bind_sql("SELECT 1", record, many=many) == [b"\x03SELECT 1"]
    assert _sqlaero._encode_value(record).to_bytes() == b"()"


def test_structured_numpy_void_field_failure_keeps_original_exception_and_index() -> None:
    np: Any = pytest.importorskip("numpy")
    original = MemoryError("actual void field conversion failure")

    class FailingInt(int):
        def __int__(self) -> int:
            raise original

    record = np.zeros(1, dtype=[("first", "i8"), ("second", object)])[0]
    record["first"] = 7
    record["second"] = FailingInt(9)
    def text_row(data: object) -> list[bytes]:
        return _sqlaero._bind_sql("SELECT %s,%s", data)

    for encode in (native_rows, text_row):
        with pytest.raises(MemoryError) as caught:
            encode(record)
        assert caught.value is original
        assert "args[1]" in "\n".join(getattr(caught.value, "__notes__", []))
    record["second"] = 9
    assert native_rows(record) == [b"79"]
    with pytest.raises(TypeError, match="void"):
        native_rows(np.void(b"raw"))


@pytest.mark.parametrize("provider", ["pandas", "polars", "pyarrow"])
def test_native_rows_consume_provider_columns_directly(provider: str) -> None:
    module: Any = pytest.importorskip(provider)
    if provider == "pandas":
        data = module.DataFrame({"a": [1, 2], "b": ["x", "y"]})
    elif provider == "polars":
        data = module.DataFrame({"a": [1, 2], "b": ["x", "y"]})
    else:
        data = module.table({"a": [1, 2], "b": ["x", "y"]})
    assert native_rows(data) == [b"1'x'", b"2'y'"]


def test_native_rows_keep_dictionary_labels_as_values_and_reject_nested_cells() -> None:
    pa: Any = pytest.importorskip("pyarrow")
    data = pa.DictionaryArray.from_arrays(pa.array([0, 1] * 20), pa.array(["A", "B"]))
    assert native_rows(data, many=True) == [b"'A'", b"'B'"] * 20
    from sqlaero.errors import NotSupportedError

    with pytest.raises(NotSupportedError):
        native_rows(pa.table({"nested": [[1], [2]]}))


@pytest.mark.parametrize("provider", ["numpy", "pyarrow"])
def test_native_provider_text_and_binary_keep_bounded_fragments_and_error_offsets(provider: str) -> None:
    module: Any = pytest.importorskip(provider)
    text = "中文'\\\0" * 50_000 + "X"
    binary = bytes(range(256)) * 1500 + b"\xff"
    bad_text = text + "😀"
    if provider == "numpy":
        text_source = module.array([text], dtype=f"U{len(text)}")
        binary_source = module.array([binary], dtype=f"S{len(binary)}")
        bad_source = module.array([bad_text], dtype=f"U{len(bad_text)}")
    else:
        text_source = module.array([text])
        binary_source = module.array([binary])
        bad_source = module.array([bad_text])
    quoted = "'" + text.replace("\\", "\\\\").replace("\0", "\\0").replace("'", "\\'") + "'"
    assert native_rows(text_source, many=True) == [quoted.encode()]
    assert native_rows(binary_source, many=True) == [b"CAST(X'" + binary.hex().upper().encode() + b"' AS BINARY)"]
    with pytest.raises(UnicodeEncodeError) as caught:
        native_rows(bad_source, many=True, charset="utf8mb3")
    assert (caught.value.object, caught.value.start, caught.value.end) == (bad_text, len(text), len(text) + 1)
