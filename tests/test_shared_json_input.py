"""Public JSON output and shared input-reading contracts, independent of MySQL."""

# pyright: reportPrivateUsage=false


import gc
import inspect
import json
import math
import os
from pathlib import Path
import subprocess
import sys
import textwrap
import weakref
from collections.abc import Callable
from concurrent.futures import ThreadPoolExecutor
from typing import Any

import pytest

from _native_signal import private_console_options

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


@pytest.mark.parametrize("digits", [21, 65, 66, 1000, 6000])
@pytest.mark.parametrize("negative", [False, True])
def test_json_integer_precision_has_no_sql_or_python_string_limit(digits: int, negative: bool) -> None:
    # The expected decimal is constructed independently, without int->str or
    # changing Python's process-global maximum decimal conversion setting.
    value = 10**digits + 123
    expected = b"1" + b"0" * (digits - 3) + b"123"
    if negative:
        value = -value
        expected = b"-" + expected
    assert json_dumps(value) == expected
    assert json_dumps([value]) == b"[" + expected + b"]"


def test_public_json_document_has_no_mysql_packet_quota() -> None:
    # J10/current resource policy: independent JSON is not a COM_QUERY.
    length = 64 * 1024 * 1024 + 1
    source = "a" * length
    result = json_dumps(source)
    assert type(result) is bytes and len(result) == length + 2
    assert result[:1] == result[-1:] == b'"'
    assert result.count(b"a") == length


def test_json_wide_integer_subclass_reads_only_base_storage() -> None:
    class Integer(int):
        def __int__(self) -> int:
            raise AssertionError("must read actual int base storage")
        def __index__(self) -> int:
            raise AssertionError("must read actual int base storage")
        def __str__(self) -> str:
            raise AssertionError("must not create a Python decimal string")
        def bit_length(self) -> int:
            raise AssertionError("must not call an overridden size method")

    value = Integer(10**100 + 7)
    assert json_dumps(value) == b"1" + b"0" * 99 + b"7"


def test_concurrent_integer_exports_keep_independent_native_owners() -> None:
    value = 10**6000 + 123
    expected = b"1" + b"0" * 5997 + b"123"
    def read(index: int) -> bytes:
        return json_dumps(value if index % 2 == 0 else -value)
    with ThreadPoolExecutor(max_workers=4) as pool:
        results = list(pool.map(read, range(16)))
    assert results == [expected if index % 2 == 0 else b"-" + expected for index in range(16)]


def test_large_integer_returns_to_signal_control_during_native_work() -> None:
    # An isolated process makes a missing checkpoint a bounded test failure.
    # Three deliveries inside a frame containing only the json_dumps call
    # distinguish growing native work from one entry/final signal check.
    script = textwrap.dedent("""
        import sys
        from _native_signal import NativeAlarm, native_call_offsets
        from sqlaero import json_dumps
        value = 1 << 2_000_000
        original = RuntimeError('original integer signal')
        count = 0
        def serialize():
            return json_dumps(value)
        offsets = native_call_offsets(serialize.__code__)
        def interrupt(number, frame):
            global count
            if frame is None or frame.f_code is not serialize.__code__ or frame.f_lasti not in offsets:
                return
            count += 1
            if count == 3:
                alarm.cancel()
                raise original
        alarm = NativeAlarm(interrupt)
        try:
            alarm.start(0.001, 0.001, wait_ready=True)
            try:
                serialize()
            except RuntimeError as error:
                assert error is original and count == 3
                assert 'while serializing JSON at data' in '\\n'.join(error.__notes__)
            else:
                raise AssertionError('large integer never observed growing-work signals')
        finally:
            alarm.close()
        assert not hasattr(sys, '_is_gil_enabled') or sys._is_gil_enabled() == (not sysconfig_free)
    """)
    # Import mode must stay unchanged; do not turn the GIL on to pass the probe.
    free = hasattr(sys, "_is_gil_enabled") and not getattr(sys, "_is_gil_enabled")()
    script = "sysconfig_free = " + repr(free) + "\n" + script
    env = dict(os.environ)
    env["PYTHONPATH"] = str(Path(__file__).parent) + os.pathsep + env.get("PYTHONPATH", "")
    result = subprocess.run([sys.executable, "-c", script], **private_console_options(env),
                            capture_output=True, text=True, timeout=15)
    assert result.returncode == 0, (result.stdout, result.stderr)


@pytest.mark.parametrize("value", [math.nan, math.inf, -math.inf])
def test_nonfinite_is_null_or_an_explicit_strict_error(value: float) -> None:
    assert json_dumps(value) == b"null"
    with pytest.raises(ValueError, match="allow_nan"):
        json_dumps(value, allow_nan=False)


def test_both_public_signatures_show_the_real_default() -> None:
    assert str(inspect.signature(JSON)) == "(data, *, allow_nan=True)"
    assert str(inspect.signature(json_dumps)) == "(data, *, allow_nan=True)"


def test_both_public_entries_accept_the_same_data_keyword() -> None:
    payload: list[object] = [7, None]
    wrapped = JSON(data=payload, allow_nan=False)
    assert wrapped.data is payload and wrapped.allow_nan is False
    assert json_dumps(data=payload, allow_nan=False) == b"[7,null]"
    assert _sqlaero._encode_native_scalar(wrapped) == b"'[7,null]'"
    assert JSON(data=None).data is None
    assert json_dumps(data=None) == b"null"
    unsupported = object()
    assert JSON(data=unsupported).data is unsupported


@pytest.mark.parametrize("factory", [JSON, json_dumps])
@pytest.mark.parametrize("shape", ["missing", "extra", "duplicate", "obj", "both_names", "unknown"])
def test_public_json_data_keyword_keeps_normal_argument_errors(
    factory: Callable[..., object], shape: str,
) -> None:
    with pytest.raises(TypeError):
        if shape == "missing":
            factory()
        elif shape == "extra":
            factory(7, False)
        elif shape == "duplicate":
            factory(7, data=8)
        elif shape == "obj":
            factory(obj=7)
        elif shape == "both_names":
            factory(data=7, obj=8)
        else:
            factory(data=7, unknown=8)


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


@pytest.mark.parametrize("many",[False,True])
@pytest.mark.parametrize("itemize",[False,True])
@pytest.mark.parametrize(("payload","literal"),[(None,"'null'"),([7,8],"'[7,8]'"),(float("nan"),"'null'")])
def test_json_wrapper_is_one_text_parameter_for_all_root_flags(payload: object,literal: str,many: bool,itemize: bool) -> None:
    wrapped=JSON(payload)
    assert _sqlaero._escape_projection(wrapped,many=many,itemize=itemize)==literal
    with pytest.raises(TypeError):json_dumps(wrapped)


def test_strict_wrapper_and_nested_wrapper_fail_only_when_serialized() -> None:
    strict=JSON(float("nan"),allow_nan=False)
    assert json_dumps(strict.data)==b"null"
    with pytest.raises(ValueError):json_dumps(strict.data,allow_nan=strict.allow_nan)
    with pytest.raises(ValueError):_sqlaero._escape_projection(strict)
    nested=JSON([JSON(7)])
    with pytest.raises(TypeError):_sqlaero._escape_projection(nested)


def test_json_encoding_and_embedded_temporal_text_do_not_inherit_mysql_value_policy() -> None:
    wrapped=JSON({"value":"🙂"})
    assert json_dumps(wrapped.data)=='{"value":"🙂"}'.encode()
    with pytest.raises(UnicodeEncodeError):
        _sqlaero._bind_sql("SELECT %s",(wrapped,),charset="latin1")
    np=pytest.importorskip("numpy")
    timestamp=JSON(np.datetime64("2026-11-12T08:09:10.123456789","ns"))
    assert json_dumps(timestamp.data)==b'"2026-11-12T08:09:10.123456789"'
    requests=_sqlaero._bind_sql("SELECT %s",(timestamp,))
    assert len(requests)==1 and b"2026-11-12T08:09:10.123456789" in requests[0]


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


@pytest.mark.parametrize("width", [15, 16, 17, 1023, 1024, 1025, 4095, 4096, 4097])
def test_json_string_marker_boundaries_match_independent_standard_output(width: int) -> None:
    for point in range(128):
        left, right = "a" * (width // 2), "z" * (width - width // 2 - 1)
        for text in (left + chr(point) + right, left + chr(point) + "中😀" + right):
            for value in (text, {text: [text]}):
                expected = json.dumps(value, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
                assert json_dumps(value) == expected


@pytest.mark.parametrize("prefix", ["é" * 65536, "中" * 65535, "😀" * 65537],
                         ids=["latin1-65536", "ucs2-65535", "ucs4-65537"])
def test_unicode_storage_windows_keep_original_source_positions(prefix: str) -> None:
    # Pytest includes the parameter ID in PYTEST_CURRENT_TEST; raw long values
    # exceed Windows' environment-variable limit before this test can run.
    source = prefix + "\ud800tail"
    for encode in (json_dumps, _sqlaero._encode_native_scalar):
        with pytest.raises(UnicodeEncodeError) as caught:
            encode(source)
        assert caught.value.object is source
        assert (caught.value.start, caught.value.end) == (len(prefix), len(prefix) + 1)


@pytest.mark.parametrize("width", [0, 1, 255, 256, 257, 65535, 65536, 65537])
def test_bounded_base_text_storage_uses_exact_characters(width: int) -> None:
    class Text(str):
        def __str__(self) -> str:
            raise AssertionError("must read actual base text storage")
        def __len__(self) -> int:
            raise AssertionError("must read actual base text length")

    for pattern in ("a", "a'\\\0\n", "\u00c3\u00a9", "\u00c2\u00a3", "é", "中😀"):
        source = (pattern * (width // len(pattern) + 1))[:width]
        expected = json.dumps(source, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
        assert json_dumps(source) == expected
        assert json_dumps(Text(source)) == expected
        expected_object = json.dumps(
            {source: [source]}, ensure_ascii=False, separators=(",", ":"),
        ).encode("utf-8")
        assert json_dumps({Text(source): [Text(source)]}) == expected_object


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
