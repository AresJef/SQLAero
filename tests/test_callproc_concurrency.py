"""Managed CALL ownership and lazy output publication on GIL and NoGIL builds."""

from collections.abc import Callable, Mapping
from concurrent.futures import ThreadPoolExecutor
import sys
import sysconfig
from threading import Barrier, Event, get_ident
from typing import TypeAlias, cast

import pytest
import sqlaero
from sqlaero.errors import InterfaceBusyError
from test_callproc_cursor import Routines, drain, formatted, readonly_outputs
from test_callproc_cursor import routines as routines
from test_public_connection import live, requires_mysql


BufferedType: TypeAlias = type[sqlaero.Cursor] | type[sqlaero.DictCursor]
StreamingType: TypeAlias = type[sqlaero.SSCursor] | type[sqlaero.SSDictCursor]
BUFFERED: tuple[BufferedType, ...] = (sqlaero.Cursor, sqlaero.DictCursor)
STREAMING: tuple[StreamingType, ...] = (sqlaero.SSCursor, sqlaero.SSDictCursor)
THREAD_WAIT = 15.0
pytestmark = requires_mysql


def gil_state() -> bool:
    free_threaded = bool(sysconfig.get_config_var('Py_GIL_DISABLED'))
    check = cast(Callable[[], bool] | None, getattr(sys, '_is_gil_enabled', None))
    if check is None:
        assert not free_threaded, 'free-threaded builds must expose actual GIL state'
        return True  # CPython 3.12 has no optional-GIL runtime.
    enabled = check()
    if free_threaded:
        assert enabled is False, 'the free-threaded acceptance run requires GIL disabled'
    return enabled


def assert_busy(connection: sqlaero.Connection) -> None:
    # The active stream remains owned throughout this call. A new command
    # must fail here rather than wait for another phase to release that lease.
    with connection.cursor() as other, pytest.raises(InterfaceBusyError):
        other.execute('SELECT 999 AS must_not_run')


def expected_fields(routines: Routines, connection: sqlaero.Connection, prepared: bool) -> dict[int, object]:
    if not prepared:
        assert routines.typed is not None
        return dict(enumerate(routines.typed))
    with connection.cursor(prepared=True) as reference:
        reference.use_decimal = True
        reference.decode_json = True
        reference.execute(f'CALL {routines.qualified("typed")}(%s,%s,%s,%s,%s)', (None,) * 5)
        sets = drain(reference)
        assert len(sets) == 2 and sets[-1] == (None, None, 0)
        assert sets[0][0] is not None and sets[0][2] == 1
        values = cast(tuple[object, ...], sets[0][1][0])
        return dict(enumerate(values))
    raise AssertionError('the direct-field reference did not complete')


@pytest.mark.parametrize('prepared', [False, True])
@pytest.mark.parametrize('kind', BUFFERED)
def test_eight_independent_connections_keep_old_and_closed_call_outputs(
    routines: Routines, prepared: bool, kind: BufferedType,
    record_property: Callable[[str, object], None],
) -> None:
    enabled = gil_state()
    record_property('gil_enabled_before', enabled)
    start = Barrier(8, timeout=THREAD_WAIT)

    def worker(index: int) -> tuple[Mapping[int, object], Mapping[int, object]]:
        start.wait()
        assert gil_state() is enabled
        with live(routines.label) as connection, connection.cursor(kind, prepared=prepared) as cur:
            value = 1000 + index
            cur.callproc(routines.names['adjust'], (value, None, index), parameter_modes=('IN', 'OUT', 'INOUT'))
            assert drain(cur) == [(('value',), formatted(kind, 'value', value), 1),
                                  (('second',), formatted(kind, 'second', index + 1), 1), (None, None, 0)]
            old = cast(Mapping[int, object], readonly_outputs(cur, {1: value * 2, 2: index + 1}))
            cur.callproc(routines.names['adjust'], (value + 1, None, index + 10), parameter_modes=('IN', 'OUT', 'INOUT'))
            drain(cur)
            # The second output has not been read as Python before close.
            connection.close()
            newest = cast(Mapping[int, object], readonly_outputs(cur, {1: (value + 1) * 2, 2: index + 11}))
            assert dict(old) == {1: value * 2, 2: index + 1}
            assert gil_state() is enabled
            return old, newest
        raise AssertionError('the independent CALL worker did not complete')

    with ThreadPoolExecutor(max_workers=8) as pool:
        futures = [pool.submit(worker, index) for index in range(8)]
        results = [future.result(timeout=THREAD_WAIT) for future in futures]
    assert len({id(old) for old, _ in results}) == 8
    assert len({id(newest) for _, newest in results}) == 8
    for index, (old, newest) in enumerate(results):
        assert dict(old) == {1: (1000 + index) * 2, 2: index + 1}
        assert dict(newest) == {1: (1001 + index) * 2, 2: index + 11}
    assert gil_state() is enabled
    record_property('gil_enabled_after', gil_state())


@pytest.mark.parametrize('prepared', [False, True])
@pytest.mark.parametrize('kind', STREAMING)
def test_one_ss_call_hands_fetch_nextset_and_close_to_distinct_threads(
    routines: Routines, prepared: bool, kind: StreamingType,
    record_property: Callable[[str, object], None],
) -> None:
    enabled = gil_state()
    record_property('gil_enabled_before', enabled)
    with live(routines.label) as connection, connection.cursor(kind, prepared=prepared) as cur:
        cur.callproc(routines.names['adjust'], (5, None, 10), parameter_modes=('IN', 'OUT', 'INOUT'))
        assert cur.call_output_params is None
        assert_busy(connection)
        ready = Barrier(4, timeout=THREAD_WAIT)
        turns = (Event(), Event(), Event())

        def phase(index: int) -> int:
            ready.wait()
            assert turns[index].wait(THREAD_WAIT), 'a preceding handoff did not finish'
            try:
                assert gil_state() is enabled
                if index == 0:
                    row = cur.fetchone()
                    assert row == ({'value': 5} if kind is sqlaero.SSDictCursor else (5,))
                    assert cur.call_output_params is None
                    assert_busy(connection)
                elif index == 1:
                    assert cur.nextset() is True
                    assert cur.columns.names == ('second',)
                    assert cur.call_output_params is None
                    assert_busy(connection)
                else:
                    # Explicit close drains the unread second business set,
                    # obtains OUT and leaves first Python conversion to main.
                    cur.close()
                    assert cur.closed
                return get_ident()
            finally:
                if index < 2:
                    turns[index + 1].set()

        with ThreadPoolExecutor(max_workers=3) as pool:
            futures = [pool.submit(phase, index) for index in range(3)]
            ready.wait()
            turns[0].set()
            identities = [future.result(timeout=THREAD_WAIT) for future in futures]
        assert len(set(identities)) == 3 and get_ident() not in identities
        assert connection.ping()
        connection.close()
        readonly_outputs(cur, {1: 10, 2: 11})
    assert gil_state() is enabled
    record_property('gil_enabled_after', gil_state())


@pytest.mark.parametrize('prepared', [False, True])
@pytest.mark.parametrize('kind', BUFFERED)
@pytest.mark.parametrize('close_before_read', [False, True])
def test_ready_cursor_concurrent_first_output_read_publishes_one_complete_cache(
    routines: Routines, prepared: bool, kind: BufferedType, close_before_read: bool,
    record_property: Callable[[str, object], None],
) -> None:
    enabled = gil_state()
    record_property('gil_enabled_before', enabled)
    with live(routines.label) as connection:
        expected = expected_fields(routines, connection, prepared)
        with connection.cursor(kind, prepared=prepared) as cur:
            cur.use_decimal = True
            cur.decode_json = True
            cur.callproc(routines.names['typed'], (None,) * 5, parameter_modes=('OUT',) * 5)
            assert drain(cur) == [(None, None, 0)]
            # No getter or readonly_outputs helper has touched this Cursor's
            # output mapping. Closing its Connection must not materialize it.
            if close_before_read:
                connection.close()
            start = Barrier(8, timeout=THREAD_WAIT)

            def first_read() -> Mapping[int, object] | None:
                start.wait()
                assert gil_state() is enabled
                try:
                    value = cur.call_output_params
                except InterfaceBusyError:
                    return None
                assert isinstance(value, Mapping)
                output = cast(Mapping[int, object], value)
                assert tuple(output) == (0, 1, 2, 3, 4)
                assert dict(output) == expected
                return output

            with ThreadPoolExecutor(max_workers=8) as pool:
                futures = [pool.submit(first_read) for _ in range(8)]
                first = [future.result(timeout=THREAD_WAIT) for future in futures]
                successes = [output for output in first if output is not None]
                assert successes, 'at least one first reader must finish materialization'
                cached = successes[0]
                assert all(output is cached for output in successes)

                def cached_read() -> Mapping[int, object]:
                    value = cur.call_output_params
                    assert isinstance(value, Mapping) and value is cached
                    assert dict(value) == expected
                    return cast(Mapping[int, object], value)

                # Only local getter reads follow contention; CALL is never retried.
                later = [pool.submit(cached_read) for _ in range(8)]
                assert all(future.result(timeout=THREAD_WAIT) is cached for future in later)
            record_property('first_read_busy_count', sum(output is None for output in first))
            assert cur.call_output_params is cached
            with pytest.raises(TypeError):
                cached[999] = 'not writable'  # type: ignore[index]
            connection.close()
            cur.close()
            assert cur.call_output_params is cached
    assert gil_state() is enabled
    record_property('gil_enabled_after', gil_state())
