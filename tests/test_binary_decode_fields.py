"""Native binary fields reach the existing final Python Decode contract."""

# pyright: reportPrivateUsage=false

from datetime import date, datetime, time, timedelta
from decimal import Decimal
import math
import struct
from typing import Any

import pytest
from sqlaero import _sqlaero
from sqlaero.errors import DataError, NotSupportedError, OperationalError


def binary(kind: int, raw: bytes | None, **options: Any) -> Any:
    return _sqlaero._decode_binary_field(kind, raw, **options)


def calendar(year: int, month: int, day: int, hour: int | None = None,
             minute: int = 0, second: int = 0, microsecond: int | None = None) -> bytes:
    if hour is None:
        return struct.pack('<BHBB', 4, year, month, day)
    if microsecond is None:
        return struct.pack('<BHBBBBB', 7, year, month, day, hour, minute, second)
    return struct.pack('<BHBBBBBI', 11, year, month, day, hour, minute, second, microsecond)


def duration(microseconds: int) -> bytes:
    magnitude = abs(microseconds)
    days, rest = divmod(magnitude, 86_400_000_000)
    seconds, micros = divmod(rest, 1_000_000)
    hours, seconds = divmod(seconds, 3600)
    minutes, seconds = divmod(seconds, 60)
    return struct.pack('<BBIBBBI', 12, int(microseconds < 0), days, hours, minutes, seconds, micros)


@pytest.mark.parametrize(('kind', 'raw', 'options', 'expected'), [
    (1, b'\x80', {}, -128),
    (1, b'\xff', {'flags': 32}, 255),
    (2, struct.pack('<h', -32768), {}, -32768),
    (2, struct.pack('<H', 65535), {'flags': 32}, 65535),
    (3, struct.pack('<i', -(2 ** 31)), {}, -(2 ** 31)),
    (3, struct.pack('<I', 2 ** 32 - 1), {'flags': 32}, 2 ** 32 - 1),
    (8, struct.pack('<q', -(2 ** 63)), {}, -(2 ** 63)),
    (8, struct.pack('<Q', 2 ** 64 - 1), {'flags': 32}, 2 ** 64 - 1),
    (9, struct.pack('<i', -(2 ** 23)), {}, -(2 ** 23)),
    (9, struct.pack('<I', 2 ** 24 - 1), {'flags': 32}, 2 ** 24 - 1),
    (13, struct.pack('<H', 0), {}, 0),
    (13, struct.pack('<H', 1901), {}, 1901),
    (13, struct.pack('<H', 2155), {'flags': 32}, 2155),
    (4, struct.pack('<f', 0.1), {}, 0.10000000149011612),
    (5, struct.pack('<d', 0.1), {}, 0.1),
    (5, struct.pack('<d', 1e-320), {'collation_id': 65535}, 1e-320),
])
def test_binary_integer_widths_and_actual_float_precision(
    kind: int, raw: bytes, options: dict[str, Any], expected: int | float,
) -> None:
    actual = binary(kind, raw, **options)
    assert type(actual) is type(expected) and actual == expected


@pytest.mark.parametrize('kind', [4, 5])
def test_binary_float_negative_zero_is_not_formatted_or_requantized(kind: int) -> None:
    actual = binary(kind, struct.pack('<f' if kind == 4 else '<d', -0.0))
    assert type(actual) is float and actual == 0.0 and math.copysign(1, actual) == -1


@pytest.mark.parametrize(('kind', 'raw', 'options'), [
    (9, struct.pack('<i', 2 ** 23), {}),
    (9, struct.pack('<I', 2 ** 24), {'flags': 32}),
    (13, struct.pack('<H', 1900), {}),
    (13, struct.pack('<H', 2156), {'flags': 32}),
    (13, struct.pack('<H', 65535), {}),
    (4, struct.pack('<f', math.inf), {}),
    (5, struct.pack('<d', math.nan), {}),
    (5, struct.pack('<d', -math.inf), {}),
])
def test_binary_numeric_content_failures_keep_dataerror(
    kind: int, raw: bytes, options: dict[str, Any],
) -> None:
    with pytest.raises(DataError) as caught:
        binary(kind, raw, **options)
    assert caught.value.code == 65545 and caught.value.is_server_error is False


@pytest.mark.parametrize(('kind', 'raw', 'options', 'expected'), [
    (246, b'-1234.5600', {}, Decimal('-1234.5600')),
    (246, b'1.2300', {'use_decimal': False}, 1.23),
    (252, b'\x00\xffA', {'collation_id': 63}, b'\x00\xffA'),
    (255, b'\x00geometry', {}, b'\x00geometry'),
    (16, b'\xff' * 8, {'length': 64, 'decode_bit': True}, 2 ** 64 - 1),
    (248, b'first,first,', {'decode_set': True}, ('first', 'first', '')),
    (245, b'{"n":9007199254740993}', {'decode_json': True}, {'n': 9007199254740993}),
    (253, b'A\xffB', {'decode_text_strict': False}, 'A\udcffB'),
])
def test_binary_original_bytes_reuse_the_existing_output_actions(
    kind: int, raw: bytes, options: dict[str, Any], expected: object,
) -> None:
    actual = binary(kind, raw, **options)
    assert type(actual) is type(expected) and actual == expected
    if isinstance(actual, Decimal) and isinstance(expected, Decimal):
        assert actual.as_tuple() == expected.as_tuple()


def test_binary_null_is_independent_of_every_output_policy() -> None:
    assert binary(245, None, decode_json=True) is None
    assert binary(253, None, collation_id=65535) is None
    assert binary(6, b'') is None


def test_binary_unknown_text_codec_stays_explicit() -> None:
    with pytest.raises(NotSupportedError):
        binary(253, b'', collation_id=65535, decode_text_strict=False)


@pytest.mark.parametrize(('kind', 'raw'), [(8, b'\x00'), (4, b'\x00' * 8), (13, b'')])
def test_binary_invalid_physical_width_is_protocol_failure(kind: int, raw: bytes) -> None:
    with pytest.raises(OperationalError) as caught:
        binary(kind, raw)
    assert caught.value.code == 65539


@pytest.mark.parametrize(('kind', 'raw', 'options', 'expected'), [
    (10, calendar(2024, 2, 29), {}, date(2024, 2, 29)),
    (14, calendar(1, 1, 1), {}, date(1, 1, 1)),
    # The official shared DATE/DATETIME/TIMESTAMP wire layout admits 7/11 too;
    # the declared DATE column still selects a date rather than datetime.
    (10, calendar(2026, 11, 5, 12), {}, date(2026, 11, 5)),
    (14, calendar(2026, 11, 5, 12, 34, 56, 123456), {}, date(2026, 11, 5)),
    (12, calendar(2026, 11, 5), {}, datetime(2026, 11, 5)),
    (7, calendar(2026, 11, 5, 12, 34, 56, 123456), {'decimals': 6},
     datetime(2026, 11, 5, 12, 34, 56, 123456)),
    (12, calendar(2026, 11, 5, 12, 34, 56, 123456), {'decimals': 0},
     datetime(2026, 11, 5, 12, 34, 56, 123456)),
    (12, calendar(2026, 11, 5, 12, 34, 56, 123456), {'decimals': 31},
     datetime(2026, 11, 5, 12, 34, 56, 123456)),
    (11, duration(-1), {}, timedelta(microseconds=-1)),
    (11, duration(-90_000_000_000), {}, timedelta(hours=-25)),
    (11, duration(3_020_399_999_999), {}, timedelta(hours=838, minutes=59, seconds=59, microseconds=999999)),
    (11, b'\x00', {}, timedelta(0)),
    (11, b'\x00', {'use_timedelta': False}, time(0)),
    (11, duration(86_399_999_999), {'use_timedelta': False}, time(23, 59, 59, 999999)),
])
def test_binary_native_calendar_and_duration_produce_direct_final_values(
    kind: int, raw: bytes, options: dict[str, Any], expected: object,
) -> None:
    actual = binary(kind, raw, **options)
    assert type(actual) is type(expected) and actual == expected
    if isinstance(actual, (datetime, time)):
        assert actual.tzinfo is None


@pytest.mark.parametrize(('kind', 'raw', 'options'), [
    (10, b'\x00', {}),
    (10, calendar(1900, 2, 29), {}),
    (12, calendar(2026, 2, 30, 12), {}),
    (7, calendar(2026, 11, 5, 24), {}),
    (11, duration(3_020_400_000_000), {}),
    (11, duration(2 ** 64), {}),
    (11, duration(-(2 ** 64)), {}),
    (11, duration(-1), {'use_timedelta': False}),
    (11, duration(86_400_000_000), {'use_timedelta': False}),
])
def test_binary_calendar_content_keeps_existing_strict_and_clock_policy(
    kind: int, raw: bytes, options: dict[str, Any],
) -> None:
    with pytest.raises(DataError) as caught:
        binary(kind, raw, **options)
    assert caught.value.code == 65545


@pytest.mark.parametrize(('kind', 'raw', 'decimals', 'expected'), [
    (10, b'\x00', 0, '0000-00-00'),
    (10, calendar(2026, 2, 31), 0, '2026-02-31'),
    (10, calendar(2026, 13, 31, 12, 34, 56, 123456), 6, '2026-13-31'),
    (12, b'\x00', 0, '0000-00-00 00:00:00'),
    (12, b'\x00', 6, '0000-00-00 00:00:00.000000'),
    (12, calendar(2026, 0, 15, 12, 34, 56), 3, '2026-00-15 12:34:56.000'),
    (7, calendar(2026, 2, 31, 12, 34, 56, 123000), 4, '2026-02-31 12:34:56.1230'),
    (12, calendar(2026, 11, 5, 12, 34, 60), 6, '2026-11-05 12:34:60.000000'),
])
def test_binary_invalid_calendar_fallback_uses_trusted_declared_precision(
    kind: int, raw: bytes, decimals: int, expected: str,
) -> None:
    actual = binary(kind, raw, decimals=decimals, decode_date_strict=False)
    assert type(actual) is str and actual == expected


@pytest.mark.parametrize(('raw', 'decimals'), [
    (b'\x00', 31),
    (calendar(2026, 0, 15, 12, 34, 56, 123456), 3),
    (calendar(2026, 0, 15, 12, 34, 56, 1), 0),
])
def test_binary_fallback_never_guesses_precision_or_discards_fractional_components(
    raw: bytes, decimals: int,
) -> None:
    with pytest.raises(DataError):
        binary(12, raw, decimals=decimals, decode_date_strict=False)


@pytest.mark.parametrize(('kind', 'raw'), [
    (10, b'\x05' + struct.pack('<HBBB', 2026, 11, 5, 0)),
    (14, b'\x03\x00\x00\x00'),
    (12, b'\x07\x00'),
    (12, calendar(2026, 11, 5, 12, 34, 56, 1_000_000)),
    (11, b'\x08\x00'),
])
def test_binary_protocol_damage_never_becomes_calendar_fallback(kind: int, raw: bytes) -> None:
    with pytest.raises(OperationalError) as caught:
        binary(kind, raw, decode_date_strict=False)
    assert caught.value.code == 65539


@pytest.mark.parametrize(('kind', 'raw', 'component'), [
    (10, calendar(10000, 1, 1), '10000'),
    (10, calendar(2026, 255, 1), '255'),
    (12, calendar(2026, 0, 1, 100), '100'),
    (7, calendar(2026, 0, 1, 12, 255), '255'),
])
def test_binary_fallback_rejects_overwide_components_without_truncating(
    kind: int, raw: bytes, component: str,
) -> None:
    with pytest.raises(DataError) as caught:
        binary(kind, raw, decode_date_strict=False)
    assert caught.value.code == 65545 and component in caught.value.message


def test_binary_concurrent_native_construction_uses_the_same_gil_and_nogil_path() -> None:
    from concurrent.futures import ThreadPoolExecutor
    from threading import Barrier

    barrier = Barrier(8)

    def read(_: int) -> tuple[object, ...]:
        barrier.wait(timeout=10)
        return (
            binary(8, struct.pack('<Q', 2 ** 64 - 1), flags=32),
            binary(7, calendar(2026, 11, 5, 12, 34, 56, 123456), decimals=6),
            binary(11, duration(-90_000_123_456)),
            binary(12, b'\x00', decimals=3, decode_date_strict=False),
            binary(246, b'1234.5600'),
        )

    with ThreadPoolExecutor(max_workers=8) as workers:
        results = list(workers.map(read, range(8)))
    expected = (2 ** 64 - 1, datetime(2026, 11, 5, 12, 34, 56, 123456),
                timedelta(microseconds=-90_000_123_456), '0000-00-00 00:00:00.000', Decimal('1234.5600'))
    assert results == [expected] * 8


@pytest.mark.parametrize('kind', [
    'signed', 'unsigned', 'float', 'date', 'datetime', 'duration', 'clock',
    'fallback', 'decimal', 'set', 'json',
])
def test_binary_single_python_allocation_fault_preserves_exception_and_retry(kind: str) -> None:
    """Inject only one fault; native conversions must never invent DataError."""
    import subprocess
    import sys

    source = r'''
import _testcapi
from functools import partial
import gc
import struct
import sys
import sysconfig
from sqlaero._sqlaero import _decode_binary_field

kind = KIND
if sysconfig.get_config_var('Py_GIL_DISABLED'):
    assert not sys._is_gil_enabled()
cases = {
    'signed': (8, struct.pack('<q', -(2 ** 63)), {}),
    'unsigned': (8, struct.pack('<Q', 2 ** 64 - 1), {'flags': 32}),
    'float': (5, struct.pack('<d', 0.1), {}),
    'date': (10, struct.pack('<BHBB', 4, 2024, 2, 29), {}),
    'datetime': (12, struct.pack('<BHBBBBBI', 11, 2026, 11, 5, 12, 34, 56, 123456), {'decimals': 6}),
    'duration': (11, struct.pack('<BBIBBBI', 12, 1, 1, 1, 0, 0, 1), {}),
    'clock': (11, struct.pack('<BBIBBBI', 12, 0, 0, 23, 59, 59, 999999), {'use_timedelta': False}),
    'fallback': (12, b'\x00', {'decimals': 6, 'decode_date_strict': False}),
    'decimal': (246, b'1234.5600', {}),
    'set': (248, b'aa,' * 31 + b'aa', {'decode_set': True}),
    'json': (245, b'{"n":[9007199254740993,"text"]}', {'decode_json': True}),
}
code, raw, options = cases[kind]
operation = partial(_decode_binary_field, code, raw, **options)
expected = operation()
enable = _testcapi.set_nomemory
disable = _testcapi.remove_mem_hooks
native_call = getattr(_testcapi, 'call_with_nomemory', None)
failures = 0
for offset in range(64):
    gc.collect()
    try:
        if native_call is not None:
            result = native_call(offset, offset + 1, operation)
        else:
            enable(offset, offset + 1)
            try:
                result = operation()
            finally:
                disable()
    except MemoryError:
        failures += 1
        result = operation()
    assert type(result) is type(expected) and result == expected
assert failures > 0
if sysconfig.get_config_var('Py_GIL_DISABLED'):
    assert not sys._is_gil_enabled()
print('binary output MemoryError retained:', kind, failures)
'''.replace('KIND', repr(kind))
    result = subprocess.run([sys.executable, '-'], input=source, text=True,
                            capture_output=True, timeout=20, check=False)
    assert result.returncode == 0, result.stdout + result.stderr
    assert 'binary output MemoryError retained:' in result.stdout
    print(result.stdout.strip())
