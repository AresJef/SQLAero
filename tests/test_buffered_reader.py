"""Atomic tuple fetches from the same Reader that production Cursor embeds."""

# pyright: reportPrivateUsage=false

import sys
from types import FrameType
from datetime import time
from typing import Any, cast
import pytest
from _native_signal import NativeAlarm
from sqlaero._sqlaero import _Rows, _rows_from_packets, DataError, InterfaceBusyError, InterfaceError

# Hosted macOS may deliver a real timer after its requested 1 ms. Keep both
# native signal tests long enough to exercise interruption inside the fetch.
_SIGNAL_FETCH_ROWS: int = 1_000_000


def length(data: bytes) -> bytes:
    count = len(data)
    if count < 251:
        return bytes([count]) + data
    if count < 65536:
        return b"\xfc" + count.to_bytes(2, "little") + data
    if count < 1 << 24:
        return b"\xfd" + count.to_bytes(3, "little") + data
    return b"\xfe" + count.to_bytes(8, "little") + data


def column(name: str, type_code: int = 3, collation_id: int = 63) -> bytes:
    labels = (b"def", b"db", b"t", b"t", name.encode(), name.encode())
    return b"".join(length(item) for item in labels) + b"\x0c" + collation_id.to_bytes(2, "little") + (255).to_bytes(4, "little") + bytes([type_code, 0, 0, 0, 0, 0])


def packets(columns: list[bytes], rows: list[tuple[bytes | None, ...]]) -> list[bytes]:
    return [bytes([len(columns)]), *columns,
            *(b"".join(b"\xfb" if value is None else length(value) for value in row) for row in rows),
            b"\xfe\x00\x00\x02\x00\x00\x00"]


def reader(rows: list[tuple[bytes | None, ...]]) -> _Rows:
    return _rows_from_packets(packets([column("x")], rows))


def test_tuple_fetch_and_iteration_share_one_position_and_keep_metadata() -> None:
    value = reader([(b"1",), (None,), (b"3",), (b"4",)])
    assert value.names == ("x",)
    assert value.fetchone() == (1,)
    assert value.fetchmany(0) == ()
    assert value.position == 1
    assert value.fetchmany() == ((None,),)
    assert next(value) == (3,)
    assert value.fetchall() == ((4,),)
    assert value.fetchone() is None
    assert value.fetchall() == ()
    assert value.names == ("x",)
    with pytest.raises(StopIteration):
        next(value)


def test_failed_whole_fetch_keeps_its_original_start_and_next_row() -> None:
    source = packets([column("duration", 11)], [(b"12:00:00",), (b"13:00:00",), (b"25:00:00",)])
    value = _rows_from_packets(source, use_timedelta=False)
    with pytest.raises(DataError) as caught:
        value.fetchmany(3)
    assert value.position == 0
    assert "row 2, column 0" in "\n".join(getattr(caught.value, "__notes__", []))
    assert value.fetchone() == (time(12),)
    assert value.fetchone() == (time(13),)
    with pytest.raises(DataError):
        value.fetchone()
    assert value.position == 2
    another = _rows_from_packets(source, use_timedelta=False)
    with pytest.raises(DataError):
        another.fetchall()
    assert another.position == 0


@pytest.mark.parametrize(('type_code', 'valid', 'invalid'), [
    (3, b'12', b"bad'\n\\\x00value"),
    (10, b'2024-02-29', b'2026-02-31'),
    (11, b'12:00:00', b'-25:00:00'),
    (245, b'{}', b'{"bad":' + b'x' * 100000),
], ids=['integer', 'date', 'duration', 'long-json'])
def test_failed_field_has_one_bounded_raw_preview_and_absolute_location(
    type_code: int, valid: bytes, invalid: bytes,
) -> None:
    value = _rows_from_packets(
        packets([column('bad_field', type_code)], [(valid,), (valid,), (invalid,)]),
        decode_json=True, use_timedelta=False,
    )
    assert value.columns.keys == ('bad_field',)
    first = value.fetchone()
    with pytest.raises(DataError) as caught:
        value.fetchmany(2)
    assert value.position == 1
    notes = '\n'.join(getattr(caught.value, '__notes__', []))
    assert 'row 2, column 0' in notes and 'bad_field' in notes
    assert "field b'" in notes
    preview = notes.split('; field ', 1)[1]
    assert len(preview) <= 160 and '\x00' not in preview and '\n' not in preview
    assert f'original length {len(invalid)}' in preview
    if len(invalid) > 24:
        assert '[truncated]' in preview
    assert value.fetchone() == first and value.position == 2


def test_unicode_field_error_keeps_original_object_without_duplicate_preview() -> None:
    value = _rows_from_packets(packets([column('text', 253, 255)], [(b'A\xffB',)]))
    with pytest.raises(UnicodeDecodeError) as caught:
        value.fetchone()
    assert caught.value.object == b'A\xffB' and caught.value.start == 1
    assert "field b'" not in '\n'.join(getattr(caught.value, '__notes__', []))
    assert value.position == 0


def test_results_remain_independent_and_close_preserves_local_information() -> None:
    first = reader([(b"1",), (b"2",)])
    second = reader([(b"7",)])
    assert first.fetchone() == (1,)
    assert second.fetchall() == ((7,),)
    first.close()
    first.close()
    assert first.closed
    assert first.position == 1
    assert first.names == ("x",)
    with pytest.raises(InterfaceError, match="closed"):
        first.fetchone()
    assert second.names == ("x",)


def test_zero_rows_and_no_row_result_are_distinct() -> None:
    empty = reader([])
    assert empty.fetchone() is None
    assert empty.fetchmany(0) == ()
    assert empty.names == ("x",)
    no_result = _rows_from_packets([b"\x00\x00\x00\x02\x00\x00\x00"])
    with pytest.raises(InterfaceError):
        no_result.fetchmany(0)
    assert no_result.names == ()


@pytest.mark.parametrize("size", [-1, True, 1.5, "1"])
def test_fetch_size_validation_never_changes_position(size: object) -> None:
    value = reader([(b"1",)])
    with pytest.raises((TypeError, ValueError)):
        value.fetchmany(size)  # type: ignore[arg-type]
    assert value.position == 0
    assert value.fetchmany(10**10000) == ((1,),)


class Interrupted(Exception):
    """An original user exception that must survive output cancellation."""


@pytest.mark.parametrize('shape', ['array', 'object'])
@pytest.mark.parametrize('interrupt', [True, False], ids=['interrupt', 'continue'])
def test_single_large_json_checks_signals_before_finishing_the_field(shape: str, interrupt: bool) -> None:
    """Observe partial native work, rather than imposing a timer precision SLA."""
    import gc

    count = 1_000_000
    document = (b'[' + b'123456789,' * (count - 1) + b'123456789]' if shape == 'array'
                else b'{' + b','.join(f'"k{index}":123456789'.encode('ascii')
                                     for index in range(count)) + b'}')
    value = _rows_from_packets(packets([column('payload', 245, 255)], [(document,)]), decode_json=True)
    original = Interrupted('stop one JSON field')
    observed: list[int] = []
    baseline = sys.getallocatedblocks()

    def check_progress(signum: int, frame: FrameType | None) -> None:
        if observed:
            return
        # Noncached integers create real CPython objects. Ignore a signal that
        # precedes decoding; an old field-end-only check observes the full set.
        allocated = sys.getallocatedblocks() - baseline
        if allocated < 1024:
            return
        observed.append(allocated)
        alarm.start(0)
        # Arbitrary application handlers may inspect GC containers. Every
        # visible tuple must already have initialized slots, including while
        # the enclosing row is still decoding this one long JSON field.
        for temporary in gc.get_objects():
            if type(temporary) is tuple:
                candidate = cast(tuple[object, ...], temporary)
                if len(candidate) == 1:
                    _ = candidate[0]
        if interrupt:
            raise original

    alarm = NativeAlarm(check_progress)
    # Sender construction is preparation, not allocation progress in the field.
    baseline = sys.getallocatedblocks()
    result: tuple[Any, ...] | None = None
    try:
        alarm.start(0.001, 0.001, wait_ready=True)
        if interrupt:
            with pytest.raises(Interrupted) as caught:
                value.fetchone()
            assert caught.value is original and not hasattr(original, 'error_info')
            assert value.position == 0
        else:
            result = cast(tuple[Any, ...], value.fetchone())
    finally:
        alarm.close()
    assert observed and 1024 <= observed[0] < count // 2, observed
    if interrupt:
        result = cast(tuple[Any, ...], value.fetchone())
    assert result is not None and len(result[0]) == count and value.position == 1


@pytest.mark.parametrize('width', [1, 4, 32], ids=['scalar-row', 'narrow-row', 'wide-row'])
def test_interrupted_output_releases_temporaries_and_does_not_commit(width: int) -> None:
    import gc

    marker = 1472853690
    value = _rows_from_packets(packets(
        [column(f'value_{index}') for index in range(width)],
        [(str(marker).encode(),) * width] * (_SIGNAL_FETCH_ROWS // width),
    ))
    original = Interrupted("stop this fetch")
    observed: list[bool] = []

    def interrupt(signum: int, frame: FrameType | None) -> None:
        alarm.start(0)
        # Observe an actual output tuple, rather than accepting an exception
        # delivered before fetch entry or after the result has committed.
        for temporary in gc.get_objects():
            if type(temporary) is tuple:
                candidate = cast(tuple[object, ...], temporary)
                if len(candidate) == width and type(candidate[0]) is int and candidate[0] == marker:
                    observed.append(True)
                    del candidate, temporary
                    raise original
        alarm.start(0.0001)

    alarm = NativeAlarm(interrupt)
    try:
        alarm.start(0.0001)
        with pytest.raises(Interrupted) as caught:
            value.fetchall()
        assert caught.value is original
    finally:
        alarm.close()
    assert observed == [True]
    assert value.position == 0
    assert value.fetchone() == (marker,) * width


def test_reentrant_mutation_is_immediately_busy_while_metadata_remains_readable() -> None:
    value = reader([(b"1",)] * _SIGNAL_FETCH_ROWS)
    observed: list[str] = []

    def overlap(signum: int, frame: FrameType | None) -> None:
        assert value.names == ("x",)
        try:
            value.close()
        except InterfaceBusyError:
            observed.append("busy")

    alarm = NativeAlarm(overlap)
    try:
        alarm.start(0.001)
        assert len(value.fetchall()) == _SIGNAL_FETCH_ROWS
    finally:
        alarm.close()
    assert observed == ["busy"]
    assert not value.closed


def test_failed_decode_reuses_prepared_unique_key_without_advancing_position() -> None:
    source = packets([column('duration', 11), column('duration', 11)],
                     [(b'12:00:00', b'25:00:00')])
    value = _rows_from_packets(source, use_timedelta=False)
    assert value.columns.keys == ('duration.1', 'duration.2')
    with pytest.raises(DataError) as caught:
        value.fetchone()
    notes = '\n'.join(getattr(caught.value, '__notes__', []))
    assert 'row 0, column 1' in notes and "key='duration.2'" in notes
    assert value.position == 0 and caught.value.error_info.stage == 'cursor.fetchone'


@pytest.mark.parametrize('width', [5, 8, 32])
def test_wide_mixed_rows_preserve_fetch_shapes_and_atomic_failure(width: int) -> None:
    import json

    columns = [column(f'v{index}', 3 if index == width - 1 or index % 3 == 0
                      else 253 if index % 3 == 1 else 245,
                      63 if index == width - 1 or index % 3 == 0 else 255)
               for index in range(width)]
    encoded: list[tuple[bytes | None, ...]] = []
    expected: list[tuple[object, ...]] = []
    for row_index in range(4):
        raw: list[bytes | None] = []
        final: list[object] = []
        for index in range(width):
            if index == width - 1 or index % 3 == 0:
                number = 1000 + row_index * width + index
                raw.append(str(number).encode())
                final.append(number)
            elif row_index == 1 and index == 1:
                raw.append(None)
                final.append(None)
            elif index % 3 == 1:
                text = f'row {row_index}, column {index} 中'
                raw.append(text.encode())
                final.append(text)
            else:
                document = {'row': row_index, 'column': index, 'values': [None, row_index]}
                raw.append(json.dumps(document).encode())
                final.append(document)
        encoded.append(tuple(raw))
        expected.append(tuple(final))
    source = packets(columns, encoded)

    value = _rows_from_packets(source, decode_json=True)
    assert value.fetchmany(0) == () and value.position == 0
    assert value.fetchone() == expected[0] and value.position == 1
    assert value.fetchmany(1) == (expected[1],) and value.position == 2
    assert value.fetchall() == tuple(expected[2:]) and value.position == 4
    assert value.fetchone() is None

    value = _rows_from_packets(source, decode_json=True)
    assert value.fetchmany(3) == tuple(expected[:3]) and value.position == 3
    assert value.fetchall() == (expected[3],) and value.position == 4
    whole = _rows_from_packets(source, decode_json=True).fetchall()
    assert whole == tuple(expected)
    for actual_row, expected_row in zip(whole, expected):
        assert type(actual_row) is tuple
        assert all(type(actual) is type(wanted) for actual, wanted in zip(actual_row, expected_row))

    broken = list(encoded)
    broken[2] = (*broken[2][:-1], b'invalid integer')
    value = _rows_from_packets(packets(columns, broken), decode_json=True)
    assert value.fetchone() == expected[0] and value.position == 1
    with pytest.raises(DataError) as caught:
        value.fetchmany(2)
    assert value.position == 1
    assert f'row 2, column {width - 1}' in '\n'.join(getattr(caught.value, '__notes__', []))
    assert value.fetchone() == expected[1] and value.position == 2
    with pytest.raises(DataError):
        value.fetchone()
    assert value.position == 2


def test_wide_nested_tuple_allocation_failure_keeps_the_original_start() -> None:
    """Fail while inner and outer final tuples contain completed values."""
    from pathlib import Path
    import subprocess

    source = r'''
import _testcapi
import sys
from collections.abc import Callable
sys.path.insert(0, sys.argv[1])
from test_buffered_reader import column, packets
from sqlaero._sqlaero import _Rows, _rows_from_packets

width: int = 32
count: int = 32
expected: tuple[tuple[int, ...], ...] = tuple(
    tuple(1000 + row * width + index for index in range(width)) for row in range(count))
source: list[bytes] = packets([column(f'v{index}') for index in range(width)],
    [tuple(str(value).encode() for value in row) for row in expected])
enable, disable = _testcapi.set_nomemory, _testcapi.remove_mem_hooks

def probe(offset: int) -> tuple[bool, bool, bool]:
    value: _Rows = _rows_from_packets(source)
    fetch: Callable[[], object] = value.fetchall
    partial_inner = partial_outer = False
    try:
        enable(offset, offset + 1)
        try:
            result: object = fetch()
        finally:
            disable()
    except MemoryError as error:
        assert not hasattr(error, 'error_info')
        assert value.position == 0
        notes = '\n'.join(getattr(error, '__notes__', []))
        partial_inner = any(f'row 0, column {index};' in notes for index in range(1, width))
        partial_outer = any(f'row {row}, column ' in notes for row in range(1, count))
        assert fetch() == expected
        failed = True
    else:
        assert result == expected
        failed = False
    assert value.position == count
    return failed, partial_inner, partial_outer

observed: list[tuple[bool, bool, bool]] = [probe(offset) for offset in range(96)]
assert any(failed for failed, _, _ in observed)
assert any(inner for _, inner, _ in observed), 'no failure inside the first wide row'
assert any(outer for _, _, outer in observed), 'no failure after an earlier row completed'
print('wide inner and outer allocation failures preserved position and recovered')
'''
    result = subprocess.run([sys.executable, '-', str(Path(__file__).resolve().parent)],
                            input=source, text=True, capture_output=True, timeout=20, check=False)
    assert result.returncode == 0, result.stdout + result.stderr
    assert 'allocation failures preserved position and recovered' in result.stdout


def test_wide_partial_row_and_batch_stay_private_during_signal_and_parallel_gc() -> None:
    """Inspect from another thread while JSON is still inside a wide row."""
    from pathlib import Path
    import os
    import subprocess

    source = r'''
import gc
import sys
from threading import Event, Thread
from types import FrameType
from typing import Any, cast

def gil_enabled() -> bool:
    check = getattr(sys, '_is_gil_enabled', None)
    return True if check is None else bool(check())

initial_gil: bool = gil_enabled()
sys.path.insert(0, sys.argv[1])
from _native_signal import NativeAlarm
from test_buffered_reader import Interrupted, column, packets
from sqlaero._sqlaero import _rows_from_packets
assert gil_enabled() is initial_gil, 'extension import changed the GIL state'

width: int = 5
count: int = 1_000_000
marker: int = 8712394560432
document: bytes = (b'[' + str(marker).encode() + b','
                   + b'123456789,' * (count - 2) + b'123456789]')
columns: list[bytes] = [column('id'), column('label', 253, 255),
                       column('payload', 245, 255), column('left'), column('right')]
value = _rows_from_packets(packets(columns, [
    (b'1001', b'first', b'[0]', b'2001', b'3001'),
    (b'1002', b'second', document, b'2002', b'3002'),
]), decode_json=True)
inspect_now, inspected, ready = Event(), Event(), Event()
failures: list[BaseException] = []
observed: list[int] = []
completed_rows: list[bool] = []
original = Interrupted('stop inside a wide row and unfinished batch')

def inspect_gc() -> None:
    ready.set()
    try:
        assert inspect_now.wait(15), 'no in-field signal observation'
        gc.collect()
        for temporary in gc.get_objects():
            if type(temporary) is tuple:
                candidate = cast(tuple[object, ...], temporary)
                if len(candidate) in (2, width):
                    for index in range(len(candidate)):
                        _ = candidate[index]
                if len(candidate) == width and type(candidate[0]) is int:
                    assert candidate[0] != 1002, 'GC exposed an unfinished wide row'
                    if candidate[0] == 1001:
                        assert candidate == (1001, 'first', [0], 2001, 3001)
                        completed_rows.append(True)
    except BaseException as error:
        failures.append(error)
    finally:
        inspected.set()

def interrupt(_signum: int, _frame: FrameType | None) -> None:
    if observed:
        return
    for temporary in gc.get_objects():
        if type(temporary) is list:
            candidate = cast(list[object], temporary)
            length = len(candidate)
            if 1024 <= length < count // 2 and type(candidate[0]) is int and candidate[0] == marker:
                observed.append(length)
                alarm.start(0)
                del candidate, temporary
                inspect_now.set()
                assert inspected.wait(10), 'parallel GC did not finish'
                if failures:
                    raise failures[0]
                raise original

inspector = Thread(target=inspect_gc, daemon=True)
inspector.start()
assert ready.wait(5)
alarm = NativeAlarm(interrupt)
caught: BaseException | None = None
try:
    alarm.start(0.001, 0.001, wait_ready=True)
    try:
        value.fetchall()
    except BaseException as error:
        caught = error
finally:
    alarm.close()
    inspect_now.set()
    inspector.join(15)
assert observed and not failures and completed_rows
assert caught is original and not hasattr(original, 'error_info')
assert value.position == 0
rows = value.fetchall()
assert len(rows) == 2 and rows[0] == (1001, 'first', [0], 2001, 3001)
assert rows[1][:2] == (1002, 'second') and rows[1][3:] == (2002, 3002)
payload = cast(list[Any], rows[1][2])
assert len(payload) == count and payload[0] == marker and payload[-1] == 123456789
assert value.position == 2 and gil_enabled() is initial_gil
print('wide partial row and batch survived signal and parallel GC inspection')
'''
    environment = dict(os.environ)
    creationflags = 0
    if os.name == 'nt':
        # Give this signal child its own console, as the existing fixture does;
        # sharing the pytest console would deliver its event to the parent too.
        environment['SQLAERO_TEST_PRIVATE_CONSOLE'] = '1'
        creationflags = subprocess.CREATE_NEW_CONSOLE
    result = subprocess.run([sys.executable, '-', str(Path(__file__).resolve().parent)],
                            input=source, text=True, capture_output=True, timeout=45,
                            env=environment, creationflags=creationflags, check=False)
    assert result.returncode == 0, result.stdout + result.stderr
    assert 'survived signal and parallel GC inspection' in result.stdout
