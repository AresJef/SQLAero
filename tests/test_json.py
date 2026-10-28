"""Validate the actual native JSON path before result-set integration."""

# pyright: reportPrivateUsage=false

import math
from typing import Any, cast

import pytest
from _native_signal import NativeAlarm

from sqlaero import _sqlaero


def decode(data: bytes) -> Any:
    """Call the private native output adapter, without a Python JSON parser."""
    return _sqlaero._decode_json(data)


def test_json_exact_integers_and_signed_float_underflow() -> None:
    values = decode(b'[9007199254740993,9007199254740993.0,-0,-0.0,-1e-400]')
    assert values[:3] == [9007199254740993, 9007199254740992.0, 0]
    assert math.copysign(1.0, values[3]) == -1.0
    assert math.copysign(1.0, values[4]) == -1.0
    text = b'9' * 1000
    assert decode(text) == int(text)


def test_json_final_objects_and_duplicate_key_order() -> None:
    result = decode(b'{"a":1,"b":[true,false,null,"\\u4e2d"],"a":2}')
    assert result == {'a': 2, 'b': [True, False, None, '中']}
    assert list(result) == ['a', 'b']
    assert decode(b'""') == ''
    assert decode(b'{"":1,"k":2,"":3}') == {'': 3, 'k': 2}


@pytest.mark.parametrize('data', [
    b'1e400', b'-1e400', b'NaN', b'Infinity', b'-Infinity', b'',
    b'{}[]', b'[1,]', b'{"a":}', b'/*x*/1', b'"\\ud800"', b'"\xff"',
])
def test_json_rejects_invalid_or_out_of_range_documents(data: bytes) -> None:
    with pytest.raises(ValueError):
        decode(data)


def test_json_preserves_upstream_resource_bounds() -> None:
    assert decode(b'[' * 200 + b'0' + b']' * 200) is not None
    with pytest.raises(ValueError, match='recursion'):
        decode(b'[' * 201 + b'0' + b']' * 201)
    with pytest.raises(ValueError):
        decode(b'9' * 4301)


def test_json_bigint_keeps_its_bound_independent_of_python_text_conversion_limit() -> None:
    """Fallible native construction must retain the existing JSON number policy."""
    import sys

    previous = sys.get_int_max_str_digits()
    try:
        sys.set_int_max_str_digits(640)
        assert decode(b'9' * 1000) == 10 ** 1000 - 1
        assert decode(b'-' + b'9' * 1000) == -(10 ** 1000 - 1)
    finally:
        sys.set_int_max_str_digits(previous)


@pytest.mark.parametrize('escaped', [False, True])
def test_json_checks_signals_before_a_single_large_value_completes(escaped: bool) -> None:
    """Observe a real handler inside one value, without a timing-based SLA."""
    import gc
    from types import FrameType

    marker = 8712394560123 + int(escaped)
    text = b'\\u0001' * (1024 * 1024) if escaped else b'x' * (16 * 1024 * 1024)
    payload = b'[' + str(marker).encode() + b',"' + text + b'"]'
    original = ValueError('original single-value signal')
    observed: list[int] = []

    def inspect_progress(_signum: int, _frame: FrameType | None) -> None:
        alarm.start(0)
        if observed:
            return
        for value in gc.get_objects():
            if type(value) is not list:
                continue
            candidate = cast(list[object], value)
            if len(candidate) in (1, 2) and type(candidate[0]) is int and candidate[0] == marker:
                observed.append(len(candidate))
                alarm.start(0)
                raise original
        alarm.start(0.0001)

    alarm = NativeAlarm(inspect_progress)
    try:
        with pytest.raises(ValueError) as caught:
            alarm.start(0.0001)
            decode(payload)
        assert caught.value is original
        assert observed == [1], 'the handler ran only after the whole value was constructed'
        assert not hasattr(original, 'error_info')
    finally:
        alarm.close()


@pytest.mark.parametrize('offset', [0, 1, 2, 3, 4, 5, 6])
@pytest.mark.parametrize('ascii_escapes', [False, True])
def test_json_string_and_both_key_positions_cross_work_windows(offset: int, ascii_escapes: bool) -> None:
    """Keep UTF-8, escapes, surrogate pairs and repeated keys exact across windows."""
    import json

    value = 'a' * (65536 - offset) + 'é中😀\\"\n\x00' + 'z' * 65540
    expected = [value, {value: 3, 'other': value}]
    encoded = json.dumps(value, ensure_ascii=ascii_escapes).encode()
    document = b'[' + encoded + b',{' + encoded + b':1,"other":' + encoded + b',' + encoded + b':3}]'
    assert decode(document) == expected


@pytest.mark.parametrize('point', ['a', '\x7f', '\x80', 'é', 'Ā', '中', '\uffff', '\U00010000', '\U0010ffff'])
@pytest.mark.parametrize('offset', [-1, 0, 1, 2])
def test_json_long_unicode_storage_kinds_keep_every_scalar(point: str, offset: int) -> None:
    """Cover ASCII, Latin-1, two-byte and four-byte Python strings at the boundary."""
    value = point * (65536 // len(point.encode()) + offset)
    assert decode(b'"' + value.encode() + b'"') == value


@pytest.mark.parametrize('ending', [b'\\uD800"', b'\\uDC00"', b'\\u12xz"', b'\xff"', b'\x01"', b'\\', b''])
def test_json_invalid_string_after_work_window_preserves_error(ending: bytes) -> None:
    """A cooperative boundary must not enable partial JSON or hide invalid text."""
    with pytest.raises(ValueError):
        decode(b'"' + b'a' * 65535 + ending)


@pytest.mark.parametrize('kind', ['unicode', 'containers', 'numbers'])
def test_json_native_unicode_allocation_failure_is_fallible(kind: str) -> None:
    """Fail native Python allocations in an isolated child, never exhaust RAM."""
    import subprocess
    import sys

    source = r'''
import _testcapi
from sqlaero import _sqlaero

payload: bytes = PAYLOAD
failures: int = 0
for offset in range(30):
    failed: bool = False
    try:
        _testcapi.set_nomemory(offset, offset + 1)
        try:
            _sqlaero._decode_json(payload)
        finally:
            _testcapi.remove_mem_hooks()
    except MemoryError:
        failed = True
    failures += int(failed)
assert failures > 0
assert _sqlaero._decode_json(b'"ready"') == 'ready'
print('native allocation failures propagated as MemoryError:', failures)
'''
    payload = {
        'unicode': b'"' + b'a' * 131072 + b'"',
        'containers': b'[' + b'[],{},' * 100 + b'[]]',
        'numbers': b'[0.1,1234567,1000000000000000000000000]',
    }[kind]
    source = source.replace('PAYLOAD', repr(payload))
    # Large fault-injection inputs must not depend on OS command-line limits.
    result = subprocess.run([sys.executable, '-'], input=source, text=True,
                            capture_output=True, timeout=15, check=False)
    assert result.returncode == 0, result.stdout + result.stderr
    assert 'propagated as MemoryError' in result.stdout
