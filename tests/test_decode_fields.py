"""The actual prepared output actions, before Cursor integration completes."""

# pyright: reportPrivateUsage=false

from datetime import date, datetime, time, timedelta
from decimal import Decimal, localcontext
import math
from typing import TypedDict, Unpack

import pytest
from sqlaero._sqlaero import _decode_field, DataError, NotSupportedError


class FieldOptions(TypedDict, total=False):
    collation_id: int
    flags: int
    length: int
    decode_json: bool
    decode_set: bool
    decode_bit: bool
    use_decimal: bool
    use_timedelta: bool
    decode_text_strict: bool
    decode_date_strict: bool


def field(type_code: int, data: bytes | None, **options: Unpack[FieldOptions]) -> object:
    return _decode_field(type_code, data, **options)


@pytest.mark.parametrize(("type_code", "data", "options", "expected"), [
    (8, b"-9223372036854775808", {}, -(2**63)),
    (8, b"18446744073709551615", {"flags": 32}, 2**64 - 1),
    (1, b"1", {}, 1),
    (13, b"2026", {}, 2026),
    (13, b"0000", {}, 0),
    (4, b"0.1", {}, 0.1),
    (246, b"1.2300", {}, Decimal("1.2300")),
    (246, b"1.2300", {"use_decimal": False}, 1.23),
    (252, b"\x00A\xff", {"collation_id": 63}, b"\x00A\xff"),
    (255, b"\x01\x02\xff", {}, b"\x01\x02\xff"),
    (170, b"", {}, b""),
    (16, b"\x05", {"length": 3}, b"\x05"),
    (16, b"\x05", {"length": 3, "decode_bit": True}, 5),
    (16, b"\xff" * 8, {"length": 64, "decode_bit": True}, 2**64 - 1),
    (254, "中文".encode(), {"flags": 128, "collation_id": 46}, "中文"),
    (254, b"abc", {"flags": 256}, "abc"),
    (248, b"red,blue", {"decode_set": True}, ("red", "blue")),
    (248, b"red,", {"decode_set": True}, ("red", "")),
    (248, b"", {"decode_set": True}, ()),
    (245, b"null", {"decode_json": True}, None),
    (245, b'{"x":9007199254740993}', {"decode_json": True}, {"x": 9007199254740993}),
    (10, b"2024-02-29", {}, date(2024, 2, 29)),
    (10, b"0000-00-00", {"decode_date_strict": False}, "0000-00-00"),
    (12, b"2026-10-23 12:34:56.000001", {}, datetime(2026, 10, 23, 12, 34, 56, 1)),
    (7, b"2026-10-23 12:34:56.1", {}, datetime(2026, 10, 23, 12, 34, 56, 100000)),
    (11, b"-25:00:00", {}, timedelta(hours=-25)),
    (11, b"-00:00:00.000001", {}, timedelta(microseconds=-1)),
    (11, b"23:59:59.999999", {"use_timedelta": False}, time(23, 59, 59, 999999)),
])
def test_exact_final_values(type_code: int, data: bytes, options: FieldOptions, expected: object) -> None:
    actual = field(type_code, data, **options)
    assert actual == expected
    assert type(actual) is type(expected)


@pytest.mark.parametrize(("type_code", "data", "options"), [
    (1, b"128", {}),
    (8, b"-1", {"flags": 32}),
    (8, b"18446744073709551616", {"flags": 32}),
    (13, b"65535", {}),
    (5, b"NaN", {}),
    (5, b"inf", {}),
    (5, b"1e400", {}),
    (246, b"1e2", {"use_decimal": False}),
    (246, b"Infinity", {}),
    (16, b"\x04", {"length": 2}),
    (16, b"", {"length": 3, "decode_bit": True}),
    (10, b"0000-00-00", {}),
    (10, b"2026-02-31", {}),
    (12, b"2026-10-23 12:34:56.1234567", {}),
    (11, b"-25:00:00", {"use_timedelta": False}),
    (11, b"24:00:00", {"use_timedelta": False}),
    (11, b"839:00:00", {}),
    (245, b"NaN", {"decode_json": True}),
    (245, b"1e400", {"decode_json": True}),
    (245, b"{}{}", {"decode_json": True}),
    (245, b"", {"decode_json": True}),
])
def test_invalid_supported_values_never_fall_back_to_bytes(type_code: int, data: bytes, options: FieldOptions) -> None:
    with pytest.raises(DataError) as caught:
        field(type_code, data, **options)
    assert caught.value.code == 65545
    assert caught.value.is_server_error is False


def test_exact_decimal_ignores_context_and_preserves_received_scale() -> None:
    with localcontext() as context:
        context.prec = 1
        actual = field(246, b"1234.5600")
    assert isinstance(actual, Decimal)
    assert actual.as_tuple() == Decimal("1234.5600").as_tuple()
    negative = field(5, b"-0.0")
    decimal_negative = field(246, b"-0.000", use_decimal=False)
    assert isinstance(negative, float)
    assert isinstance(decimal_negative, float)
    assert math.copysign(1, negative) == -1
    assert math.copysign(1, decimal_negative) == -1


def test_strict_and_recoverable_text_keep_raw_invalid_bytes() -> None:
    with pytest.raises(UnicodeDecodeError) as caught:
        field(253, b"A\xffB")
    assert (caught.value.start, caught.value.end) == (1, 2)
    assert field(253, b"A\xffB", decode_text_strict=False) == "A\udcffB"
    assert field(248, b"A\xff,B,", decode_set=True, decode_text_strict=False) == ("A\udcff", "B", "")
    assert field(253, b"A\x81B", collation_id=11, decode_text_strict=False) == "A\udc81B"
    with pytest.raises(NotSupportedError):
        field(253, b"", collation_id=65535, decode_text_strict=False)


def test_mysql_specific_and_server_only_codecs_use_the_same_native_mappings() -> None:
    assert field(253, b"\\", collation_id=10) == "Ö"
    assert field(253, b"\xfc\xfd", collation_id=36) == "ⁿ²"
    assert field(253, "中文".encode("utf-16-be"), collation_id=54) == "中文"
    assert field(253, "😀".encode("utf-16-le"), collation_id=56) == "😀"
    assert field(253, "😀".encode("utf-32-be"), collation_id=60) == "😀"
    with pytest.raises(UnicodeDecodeError):
        field(253, "😀".encode(), collation_id=33)


def test_json_uses_the_actual_converted_result_encoding_without_python_text() -> None:
    assert field(245, b'{"x":"\xe9??"}', collation_id=8, decode_json=True) == {"x": "é??"}
    assert field(245, '{"x":"中"}'.encode("gb18030"), collation_id=248, decode_json=True) == {"x": "中"}
    assert field(245, '{"x":"中"}'.encode(), collation_id=63, decode_json=True) == {"x": "中"}
    assert field(245, b"-1e-400", decode_json=True) == -0.0
    with pytest.raises(DataError):
        field(245, b'{"x":"\xff"}', decode_json=True, decode_text_strict=False)


def test_protocol_null_is_independent_of_every_output_policy() -> None:
    assert field(245, None, decode_json=True) is None
    assert field(16, None, length=64, decode_bit=True) is None
    assert field(253, None, collation_id=65535) is None


def test_set_tuple_allocation_failure_preserves_memory_error() -> None:
    """Fail the real SET fetch after preparation and preserve its start."""
    from pathlib import Path
    import subprocess
    import sys

    source = r'''
import _testcapi
import sys
from typing import Callable

sys.path.insert(0, sys.argv[1])
from test_buffered_reader import column, packets
from sqlaero import _sqlaero
from sqlaero._sqlaero import _Rows

# One-character strings can be cached; 32 members exceed the tuple freelist.
# Prepare the Reader and bound method before injecting allocation failure.
# Store the result in a fast local so the injected failure cannot hit a new
# globals entry after native fetch has already committed its position.
payload: bytes = b'a,' * 31 + b'a'
expected: tuple[tuple[str, ...]] = (('a',) * 32,)
source: list[bytes] = packets([column('members', 248, 255)], [(payload,)])

def probe(offset: int) -> bool:
    reader: _Rows = _sqlaero._rows_from_packets(source, decode_set=True)
    fetch: Callable[[], object] = reader.fetchone
    try:
        _testcapi.set_nomemory(offset, offset + 1)
        try:
            result: object = fetch()
        finally:
            _testcapi.remove_mem_hooks()
    except MemoryError as error:
        assert not hasattr(error, 'error_info')
        assert reader.position == 0
        assert fetch() == expected
        failed = True
    else:
        assert result == expected
        failed = False
    assert reader.position == 1
    return failed

failures: int = sum(probe(offset) for offset in range(30))
assert failures > 0
print('SET allocation failures propagated as MemoryError:', failures)
'''
    result = subprocess.run([sys.executable, '-', str(Path(__file__).resolve().parent)],
                            input=source, text=True, capture_output=True, timeout=15, check=False)
    assert result.returncode == 0, result.stdout + result.stderr
    assert 'propagated as MemoryError' in result.stdout


def test_private_field_allocation_failure_preserves_memory_error() -> None:
    """Keep the private field entry fallible before final value construction."""
    import subprocess
    import sys

    source = r'''
import _testcapi
from sqlaero import _sqlaero

# Cover both private entry preparation and the final SET tuple allocation.
payload: bytes = b'a,' * 31 + b'a'
expected: tuple[str, ...] = ('a',) * 32
failures: int = 0
for offset in range(30):
    failed: bool = False
    try:
        _testcapi.set_nomemory(offset, offset + 1)
        try:
            _sqlaero._decode_field(248, payload, decode_set=True)
        finally:
            _testcapi.remove_mem_hooks()
    except MemoryError:
        failed = True
    failures += int(failed)
assert failures > 0
assert _sqlaero._decode_field(248, payload, decode_set=True) == expected
print('private field allocation failures propagated as MemoryError:', failures)
'''
    result = subprocess.run([sys.executable, '-'], input=source, text=True,
                            capture_output=True, timeout=15, check=False)
    assert result.returncode == 0, result.stdout + result.stderr
    assert 'propagated as MemoryError' in result.stdout
