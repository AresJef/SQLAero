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
    count = 1_000_000
    document = (b'[' + b'123456789,' * (count - 1) + b'123456789]' if shape == 'array'
                else b'{' + b','.join(f'"k{index}":123456789'.encode('ascii')
                                     for index in range(count)) + b'}')
    value = _rows_from_packets(packets([column('payload', 245, 255)], [(document,)]), decode_json=True)
    original = Interrupted('stop one JSON field')
    observed: list[int] = []
    baseline = sys.getallocatedblocks()

    def check_progress(signum: int, frame: FrameType | None) -> None:
        # Noncached integers create real CPython objects. Ignore a signal that
        # precedes decoding; an old field-end-only check observes the full set.
        allocated = sys.getallocatedblocks() - baseline
        if allocated < 1024:
            return
        if not observed:
            observed.append(allocated)
        if interrupt:
            raise original

    alarm = NativeAlarm(check_progress)
    result: tuple[Any, ...] | None = None
    try:
        alarm.start(0.001, 0.001)
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


def test_interrupted_output_releases_temporaries_and_does_not_commit() -> None:
    value = reader([(b"1",)] * _SIGNAL_FETCH_ROWS)
    original = Interrupted("stop this fetch")

    def interrupt(signum: int, frame: FrameType | None) -> None:
        raise original

    alarm = NativeAlarm(interrupt)
    try:
        alarm.start(0.001)
        with pytest.raises(Interrupted) as caught:
            value.fetchall()
        assert caught.value is original
    finally:
        alarm.close()
    assert value.position == 0
    assert value.fetchone() == (1,)


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
