"""Public JSON output and shared input-reading contracts, independent of MySQL."""

import gc
import inspect
import json
import math
import weakref
from concurrent.futures import ThreadPoolExecutor
from typing import Any

import pytest

from sqlaero import JSON, Cursor, DictCursor, SSCursor, SSDictCursor, json_dumps
from sqlaero import _sqlaero
from test_public_connection import live, requires_mysql


@pytest.mark.parametrize(("value", "expected"), [
    (None, b"null"), (True, b"true"), (False, b"false"),
    (7, b"7"), (-(2**63), b"-9223372036854775808"),
    (2**64 - 1, b"18446744073709551615"),
    (1.5, b"1.5"), (-0.0, b"-0.0"),
    ("", b'""'), ("é中文😀", '"é中文😀"'.encode()),
    ('"\\\0\n', b'"\\\"\\\\\\u0000\\n"'),
    ([], b"[]"), ((), b"[]"), ([7, (None, True)], b"[7,[null,true]]"),
])
def test_minimal_public_json_values(value: object, expected: bytes) -> None:
    actual = json_dumps(value)
    assert type(actual) is bytes and actual == expected


@pytest.mark.parametrize("value", [math.nan, math.inf, -math.inf])
def test_nonfinite_is_null_or_an_explicit_strict_error(value: float) -> None:
    assert json_dumps(value) == b"null"
    with pytest.raises(ValueError, match="allow_nan"):
        json_dumps(value, allow_nan=False)


def test_both_public_signatures_show_the_real_default() -> None:
    assert str(inspect.signature(JSON)) == "(obj, *, allow_nan=True)"
    assert str(inspect.signature(json_dumps)) == "(obj, *, allow_nan=True)"


@pytest.mark.parametrize("flag", [0, 1, None, "true"])
def test_allow_nan_is_strict_for_both_entries(flag: Any) -> None:
    with pytest.raises(TypeError, match="allow_nan"):
        json_dumps(7, allow_nan=flag)
    with pytest.raises(TypeError, match="allow_nan"):
        JSON(7, allow_nan=flag)


def test_wrapper_is_lazy_readonly_and_reads_current_payload_each_time() -> None:
    data: list[object] = [7, "ab"]
    wrapped = JSON(data)
    assert wrapped.data is data and wrapped.allow_nan is True
    assert _sqlaero._encode_native_scalar(wrapped) == b"'[7,\\\"ab\\\"]'"
    data.append(8)
    assert _sqlaero._encode_native_scalar(wrapped) == b"'[7,\\\"ab\\\",8]'"
    with pytest.raises(AttributeError):
        wrapped.data = []  # type: ignore[misc]
    with pytest.raises(AttributeError):
        wrapped.allow_nan = False  # type: ignore[misc]
    # Construction does not test the payload's serialization support.
    JSON(object())
    with pytest.raises(TypeError):
        json_dumps(wrapped)
    with pytest.raises(TypeError):
        json_dumps([wrapped])


def test_actual_subclasses_read_base_values_without_replacement_protocols() -> None:
    class Integer(int):
        def __int__(self) -> int:
            raise AssertionError("must read int base storage")
        def __index__(self) -> int:
            raise AssertionError("must read int base storage")
    class Float(float):
        def __float__(self) -> float:
            raise AssertionError("must read float base storage")
    class Text(str):
        def __str__(self) -> str:
            raise AssertionError("must read str base storage")
    class Binary(bytes):
        def __bytes__(self) -> bytes:
            raise AssertionError("must read bytes base storage")
    for value, expected, sql in [
        (Integer(7), b"7", b"7"), (Float(1.5), b"1.5", b"1.5e0"),
        (Integer(2**64 - 1), b"18446744073709551615", b"18446744073709551615"),
        (Text("base"), b'"base"', b"'base'"),
    ]:
        assert json_dumps(value) == expected
        assert _sqlaero._encode_native_scalar(value) == sql
    assert _sqlaero._encode_native_scalar(Binary(b"ab")) == b"CAST(X'6162' AS BINARY)"
    with pytest.raises(TypeError):
        json_dumps(Binary(b"ab"))


@pytest.mark.parametrize("value", ["\ud800", "x\ud800y", "\ud83d\ude00", "😀\udfff"])
def test_actual_surrogates_are_rejected_even_as_a_pair(value: str) -> None:
    with pytest.raises(UnicodeEncodeError) as caught:
        json_dumps(value)
    expected = next(index for index, point in enumerate(value) if 0xD800 <= ord(point) <= 0xDFFF)
    assert caught.value.object is value
    assert (caught.value.start, caught.value.end) == (expected, expected + 1)


def test_long_unicode_and_controls_do_not_change_data() -> None:
    source = ('é中😀"\\\0\n' * 45_000) + "tail"
    assert json.loads(json_dumps(source)) == source
    assert _sqlaero._encode_native_scalar(source) == _sqlaero._encode_scalar(source)


@pytest.mark.parametrize("prefix", ["é" * 65536, "中" * 65535, "😀" * 65537])
def test_unicode_storage_windows_keep_original_source_positions(prefix: str) -> None:
    source = prefix + "\ud800tail"
    for encode in (json_dumps, _sqlaero._encode_native_scalar):
        with pytest.raises(UnicodeEncodeError) as caught:
            encode(source)
        assert caught.value.object is source
        assert (caught.value.start, caught.value.end) == (len(prefix), len(prefix) + 1)


def test_independent_calls_share_no_writer_or_wrapper_output_state() -> None:
    payload: list[object] = [7, "é中😀" * 1000, [None, True, -0.0]]
    wrapped = JSON(payload)
    expected = json_dumps(payload)

    def read(index: int) -> tuple[bytes, bytes]:
        assert index >= 0
        return json_dumps(wrapped.data), _sqlaero._encode_native_scalar(wrapped)

    sql = _sqlaero._encode_native_scalar(wrapped)
    with ThreadPoolExecutor(max_workers=4) as pool:
        assert all(result == (expected, sql) for result in pool.map(read, range(64)))


def test_depth_cycles_and_repeated_independent_references() -> None:
    source: object = None
    for _ in range(64):
        source = [source]
    assert json_dumps(source) == b"[" * 64 + b"null" + b"]" * 64
    with pytest.raises(ValueError, match="64"):
        json_dumps([source])
    cyclic: list[object] = []
    cyclic.append(cyclic)
    with pytest.raises(ValueError, match="cycle"):
        json_dumps(cyclic)
    shared = [7]
    assert json_dumps([shared, shared]) == b"[[7],[7]]"


def test_wrapper_payload_cycle_can_be_collected() -> None:
    class Payload(list[object]):
        pass
    data = Payload()
    reference = weakref.ref(data)
    wrapped = JSON(data)
    data.append(wrapped)
    del data, wrapped
    gc.collect()
    assert reference() is None


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("prepared", [False, True])
@pytest.mark.parametrize("kind", [Cursor, DictCursor, SSCursor, SSDictCursor])
@pytest.mark.parametrize("no_backslash", [False, True])
def test_wrapper_is_a_text_value_through_both_protocols_and_four_cursors(
    label: str,
    prepared: bool,
    kind: type[Cursor] | type[DictCursor] | type[SSCursor] | type[SSDictCursor],
    no_backslash: bool,
) -> None:
    payload = [7, "O'Reilly\\\0😀; SELECT 99", None]
    with live(label) as connection, connection.cursor(kind, prepared=prepared) as cursor:
        cursor.decode_json = True
        if no_backslash:
            cursor.execute("SET SESSION sql_mode='NO_BACKSLASH_ESCAPES'")
        count = cursor.execute("SELECT CAST(%s AS JSON) AS payload", (JSON(payload),))
        assert count is None if kind in (SSCursor, SSDictCursor) else count == 1
        expected = (({"payload": payload},) if kind in (DictCursor, SSDictCursor)
                    else ((payload,),))
        assert cursor.fetchall() == expected
        cursor.execute("SELECT ISNULL(%s) AS n, JSON_TYPE(%s) AS j, JSON_TYPE(%s) AS s",
                       (None, JSON(None), JSON("null")))
        expected_nulls = (({"n": 1, "j": "NULL", "s": "STRING"},)
                          if kind in (DictCursor, SSDictCursor) else ((1, "NULL", "STRING"),))
        assert cursor.fetchall() == expected_nulls
