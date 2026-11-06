"""EX-035-Q13 prepared CALL contracts, sharing the N07 owned routines.

The direct controls run independently of managed OUT support. Unsupported
managed cases remain real failures when selected; no capability skip hides them.
"""

from collections.abc import Mapping
from datetime import date
from decimal import Decimal
from functools import partial
from typing import Any, Callable, TypeAlias, cast
from uuid import uuid4

import pytest
import sqlaero
from sqlaero.errors import InterfaceBusyError, OperationalTimeoutError, SQLAeroError
from test_callproc_cursor import (
    IDS, PREFIX, Routines, drain, formatted, quote, readonly_outputs, scalar,
    session_variables, streaming,
)
from test_callproc_cursor import routines as routines
from test_public_connection import live, requires_mysql


CursorType: TypeAlias = (
    type[sqlaero.Cursor] | type[sqlaero.DictCursor]
    | type[sqlaero.SSCursor] | type[sqlaero.SSDictCursor]
)
CursorValue: TypeAlias = sqlaero.Cursor | sqlaero.DictCursor | sqlaero.SSCursor | sqlaero.SSDictCursor
KINDS: tuple[CursorType, ...] = (sqlaero.Cursor, sqlaero.DictCursor, sqlaero.SSCursor, sqlaero.SSDictCursor)
pytestmark = requires_mysql


@pytest.mark.parametrize('kind', KINDS)
def test_prepared_in_business_sets_tail_and_confirmed_empty_outputs(routines: Routines, kind: CursorType) -> None:
    with live(routines.label) as connection, connection.cursor(kind, prepared=True) as cur:
        assert cur.prepared and cur.call_output_params is None
        count = cur.callproc(routines.names['pure'], (4, 8))
        assert count is None if streaming(kind) else count == 2
        assert drain(cur) == [(('value',), formatted(kind, 'value', 12, 13), 2),
                              (('second',), formatted(kind, 'second', 8), 1), (None, None, 0)]
        assert cur.batch_row_count == 2
        previous = readonly_outputs(cur, {})
        cur.callproc(routines.names['null'], None)
        assert drain(cur) == [(('value',), formatted(kind, 'value', 1), 1), (None, None, 0)]
        readonly_outputs(cur, {})
        cur.callproc(routines.names['zero'])
        assert drain(cur)[0][1] == formatted(kind, 'value', 42)
        readonly_outputs(cur, {})
        cur.execute('SELECT 9 AS value')
        assert cur.call_output_params is None and dict(previous) == {}
        drain(cur)


@pytest.mark.parametrize('kind', KINDS)
def test_prepared_name_and_explicit_quoted_database(routines: Routines, kind: CursorType) -> None:
    assert routines.quoted_database is not None, routines.database_error
    with live(routines.label) as connection, connection.cursor(kind, prepared=True) as cur:
        before = scalar(connection, 'SELECT DATABASE()')
        cur.callproc(routines.names['quoted'], database=routines.database)
        assert drain(cur)[0][1] == formatted(kind, 'value', 73)
        readonly_outputs(cur, {})
        cur.callproc(routines.names['zero'], database=routines.quoted_database)
        assert drain(cur)[0][1] == formatted(kind, 'value', 42)
        assert scalar(connection, 'SELECT DATABASE()') == before


@pytest.mark.parametrize('kind', KINDS)
def test_prepared_execute_call_exposes_real_out_set_without_output_mapping(routines: Routines, kind: CursorType) -> None:
    with live(routines.label) as connection, connection.cursor(kind, prepared=True) as cur:
        cur.execute(f'CALL {routines.qualified("adjust")}(%s,%s,%s)', (5, None, 10))
        assert cur.call_output_params is None
        sets = drain(cur)
        assert sets[:2] == [(('value',), formatted(kind, 'value', 5), 1),
                            (('second',), formatted(kind, 'second', 11), 1)]
        assert len(sets) == 4 and sets[-1] == (None, None, 0)
        names, rows, count = sets[2]
        assert names == ('result', 'counter') and count == 1
        expected = ({'result': 10, 'counter': 11},) if kind in (sqlaero.DictCursor, sqlaero.SSDictCursor) else ((10, 11),)
        assert rows == expected and cur.call_output_params is None
        assert connection.ping()


def test_prepared_execute_call_actual_output_fields(
    routines: Routines, record_property: Callable[[str, object], None],
) -> None:
    with live(routines.label) as connection, connection.cursor(prepared=True) as cur:
        cur.use_decimal = True
        cur.decode_json = True
        cur.execute(f'CALL {routines.qualified("typed")}(%s,%s,%s,%s,%s)', (None,) * 5)
        codes = tuple(column.type_code for column in cur.columns)
        sets = drain(cur)
        assert len(sets) == 2 and sets[-1] == (None, None, 0)
        values = cast(tuple[object, ...], sets[0][1][0])
        assert values[0] == Decimal('12.50') and values[3:] == (b'\x00\xff', None)
        if codes[1] == 245:
            assert values[1] == {'v': 1}
        if codes[2] == 10:
            assert values[2] == date(2024, 2, 29)
        assert cur.call_output_params is None
        record_property('prepared_output_field_codes', repr(codes))
        record_property('prepared_output_python_types', repr(tuple(type(value).__name__ for value in values)))


@pytest.mark.parametrize('kind', KINDS)
def test_prepared_local_direction_validation_preserves_prior_results(routines: Routines, kind: CursorType) -> None:
    with live(routines.label) as connection, connection.cursor(kind, prepared=True) as cur:
        cur.callproc(routines.names['zero'])
        drain(cur)
        old = readonly_outputs(cur, {})
        cur.execute('SELECT 91 AS untouched')
        columns = cur.columns
        invalid: list[tuple[object, dict[str, object]]] = [
            ((5, None, 10), {'parameter_modes': ('IN',)}),
            ((5, 7, 10), {'parameter_modes': ('IN', 'OUT', 'INOUT')}),
            ((5, None, 10), {'parameter_modes': ('IN', 'BAD', 'INOUT')}),
            ((5, None, 10), {'parameter_modes': 'IN,OUT,INOUT'}),
        ]
        for args, keywords in invalid:
            with pytest.raises((TypeError, ValueError, SQLAeroError)) as caught:
                cast(Any, cur).callproc(routines.names['adjust'], args, **keywords)
            assert getattr(caught.value, 'is_server_error', None) is not True
            assert cur.columns is columns and cur.call_output_params is None
        assert cur.fetchone() == ({'untouched': 91} if kind in (sqlaero.DictCursor, sqlaero.SSDictCursor) else (91,))
        assert dict(old) == {}
        drain(cur)


@pytest.mark.parametrize('kind', KINDS)
@pytest.mark.parametrize('multi', [False, True])
def test_managed_prepared_positions_hidden_out_tail_busy_and_no_user_slots(routines: Routines, kind: CursorType, multi: bool) -> None:
    with live(routines.label, multi_statements=multi) as connection, connection.cursor(kind, prepared=True) as cur:
        before = session_variables(connection)
        args = [5, None, 10]
        count = cur.callproc(routines.names['adjust'], args, parameter_modes=('in', 'Out', 'InOuT'))
        assert args == [5, None, 10] and count == cur.row_count
        assert count in (None, 1)
        if streaming(kind):
            assert cur.call_output_params is None
            with connection.cursor() as other, pytest.raises(InterfaceBusyError):
                other.execute('SELECT 100')
        assert drain(cur) == [(('value',), formatted(kind, 'value', 5), 1),
                              (('second',), formatted(kind, 'second', 11), 1), (None, None, 0)]
        assert cur.batch_row_count == 1
        outputs = readonly_outputs(cur, {1: 10, 2: 11})
        assert session_variables(connection) == before
        connection.close()
        assert cur.call_output_params is outputs


@pytest.mark.parametrize('kind', KINDS)
def test_managed_prepared_actual_fields_decode_policy_and_closed_first_getter(routines: Routines, kind: CursorType) -> None:
    with live(routines.label) as connection:
        expected: tuple[object, ...] | None = None
        with connection.cursor(prepared=True) as reference:
            reference.use_decimal = True
            reference.decode_json = True
            reference.execute(f'CALL {routines.qualified("typed")}(%s,%s,%s,%s,%s)', (None,) * 5)
            sets = drain(reference)
            assert len(sets) == 2 and sets[-1] == (None, None, 0)
            assert sets[0][0] is not None and sets[0][2] == 1
            expected = cast(tuple[object, ...], sets[0][1][0])
            assert expected[3:] == (b'\x00\xff', None)
            # The direct protocol fields supply this oracle, without a guessed type.
        assert expected is not None
        with connection.cursor(kind, prepared=True) as cur:
            cur.use_decimal = True
            cur.decode_json = True
            count = cur.callproc(routines.names['typed'], (None,) * 5, parameter_modes=('OUT',) * 5)
            assert count == 0 and cur.row_count == 0 and cur.batch_row_count == 0
            cur.use_decimal = False
            cur.decode_json = False
            assert drain(cur) == [(None, None, 0)]
            connection.close()
            outputs = cast(Mapping[int, object], readonly_outputs(cur, dict(enumerate(expected))))
            assert tuple(type(outputs[i]) for i in range(5)) == tuple(type(value) for value in expected)


@pytest.mark.parametrize('kind', KINDS)
def test_managed_prepared_null_and_old_native_snapshots_survive_reuse(routines: Routines, kind: CursorType) -> None:
    with live(routines.label) as connection:
        first = connection.cursor(kind, prepared=True)
        second = connection.cursor(kind, prepared=True)
        try:
            first.callproc(routines.names['adjust'], (None, None, None), parameter_modes=('IN', 'OUT', 'INOUT'))
            drain(first)
            second.callproc(routines.names['adjust'], (7, None, 3), parameter_modes=('IN', 'OUT', 'INOUT'))
            drain(second)
            connection.close()
            readonly_outputs(first, {1: None, 2: 1})
            readonly_outputs(second, {1: 14, 2: 4})
        finally:
            first.close()
            second.close()


@pytest.mark.parametrize('kind', KINDS)
@pytest.mark.parametrize('business_error', [False, True])
def test_managed_prepared_close_and_business_primary_keep_outputs(routines: Routines, kind: CursorType, business_error: bool) -> None:
    with live(routines.label) as connection:
        cur = connection.cursor(kind, prepared=True)
        original = ValueError('prepared CALL business primary')
        if business_error:
            with pytest.raises(ValueError) as caught:
                with cur:
                    cur.callproc(routines.names['adjust'], (5, None, 10), parameter_modes=('IN', 'OUT', 'INOUT'))
                    raise original
            assert caught.value is original
        else:
            cur.callproc(routines.names['adjust'], (5, None, 10), parameter_modes=('IN', 'OUT', 'INOUT'))
            cur.close()
        assert cur.closed and connection.ping()
        connection.close()
        readonly_outputs(cur, {1: 10, 2: 11})


@pytest.mark.parametrize('kind', KINDS)
@pytest.mark.parametrize('rollback', [False, True])
def test_managed_prepared_transaction_output_and_receipt_are_separate(routines: Routines, kind: CursorType, rollback: bool) -> None:
    with live(routines.label) as connection:
        value = next(IDS)
        cur: CursorValue | None = None
        original = ValueError('prepared rollback primary')
        if rollback:
            with pytest.raises(ValueError) as caught:
                with connection.transaction(kind, prepared=True) as cur:
                    cur.callproc(routines.names['receipt'], (value, None), parameter_modes=('IN', 'OUT'))
                    raise original
            assert caught.value is original
        else:
            with connection.transaction(kind, prepared=True) as cur:
                cur.callproc(routines.names['receipt'], (value, None), parameter_modes=('IN', 'OUT'))
        assert cur is not None and cur.closed
        readonly_outputs(cur, {1: value * 2})
        with connection.cursor() as observer:
            observer.execute(f'SELECT v FROM {quote(routines.database)}.{quote(routines.table)} WHERE v=%s', value)
            assert observer.fetchall() == (() if rollback else ((value,),))


@pytest.mark.parametrize('kind', KINDS)
def test_managed_prepared_failed_call_keeps_original_error_and_trusted_prefix(routines: Routines, kind: CursorType) -> None:
    with live(routines.label) as connection, connection.cursor(kind, prepared=True) as cur:
        if streaming(kind):
            cur.callproc(routines.names['failure'], (23, None), parameter_modes=('IN', 'OUT'))
            assert cur.fetchall() == formatted(kind, 'value', 23)
            with pytest.raises(SQLAeroError) as caught:
                cur.nextset()
        else:
            with pytest.raises(SQLAeroError) as caught:
                cur.callproc(routines.names['failure'], (23, None), parameter_modes=('IN', 'OUT'))
            assert cur.fetchall() == formatted(kind, 'value', 23)
        assert caught.value.code == 1644 and caught.value.is_server_error
        assert cur.call_output_params is None
        with pytest.raises(type(caught.value)) as repeated:
            cur.nextset()
        assert repeated.value.code == 1644


@pytest.mark.parametrize('kind', KINDS)
def test_managed_prepared_all_in_declaration_detects_later_native_out(routines: Routines, kind: CursorType) -> None:
    with live(routines.label) as connection, connection.cursor(kind, prepared=True) as cur:
        if streaming(kind):
            cur.callproc(routines.names['adjust'], (5, None, 10))
        else:
            with pytest.raises(SQLAeroError) as initial:
                cur.callproc(routines.names['adjust'], (5, None, 10))
            assert initial.value.is_server_error is False
        assert cur.fetchall() == formatted(kind, 'value', 5)
        assert cur.nextset() is True
        assert cur.fetchall() == formatted(kind, 'second', 11)
        if streaming(kind):
            # All-IN keeps Header return; PS_OUT_PARAMS is unavailable until
            # the real row End, so no column-name/count guess may reject here.
            assert cur.nextset() is True
            with pytest.raises(SQLAeroError) as caught:
                cur.fetchall()
        else:
            with pytest.raises(SQLAeroError) as caught:
                cur.nextset()
        assert caught.value.is_server_error is False and cur.call_output_params is None
        diagnostic = (caught.value.message + ' ' + ' '.join(getattr(caught.value, '__notes__', ()))).lower()
        assert 'parameter_modes' in diagnostic and 'call' in diagnostic
        assert 'returned native out' in diagnostic and 'not replayed' in diagnostic
        assert 'call sending started=true' in diagnostic


@pytest.mark.parametrize('kind', KINDS)
def test_managed_prepared_does_not_guess_business_columns_as_output(routines: Routines, kind: CursorType) -> None:
    with live(routines.label) as connection, connection.cursor(kind, prepared=True) as cur:
        # This actual IN procedure has one business column, matching the one
        # declared output position. Only the real native flag may identify OUT.
        if streaming(kind):
            cur.callproc(routines.names['null'], None, parameter_modes=('OUT',))
        else:
            with pytest.raises(SQLAeroError) as initial:
                cur.callproc(routines.names['null'], None, parameter_modes=('OUT',))
            assert initial.value.is_server_error is False
        assert cur.fetchall() == formatted(kind, 'value', 1)
        with pytest.raises(SQLAeroError) as caught:
            while cur.nextset() is True:
                assert cur.description is None and cur.row_count == 0
        assert caught.value.is_server_error is False and cur.call_output_params is None
        diagnostic = (caught.value.message + ' ' + ' '.join(getattr(caught.value, '__notes__', ()))).lower()
        assert 'parameter_modes' in diagnostic and 'call' in diagnostic


@pytest.mark.parametrize('kind', KINDS)
def test_managed_prepared_saved_mapping_survives_later_server_failure(routines: Routines, kind: CursorType) -> None:
    with live(routines.label) as connection, connection.cursor(kind, prepared=True) as cur:
        cur.callproc(routines.names['adjust'], (5, None, 10), parameter_modes=('IN', 'OUT', 'INOUT'))
        drain(cur)
        saved = readonly_outputs(cur, {1: 10, 2: 11})
        with pytest.raises(SQLAeroError) as caught:
            cur.execute(f'SELECT * FROM {quote(routines.database)}.{quote(PREFIX + "_never_created")}')
        assert caught.value.code == 1146 and caught.value.is_server_error
        assert cur.call_output_params is None
        connection.close()
        assert dict(saved) == {1: 10, 2: 11}


@pytest.mark.parametrize('kind', [sqlaero.Cursor, sqlaero.SSDictCursor])
def test_managed_prepared_atomic_getter_allocation_retry(routines: Routines, kind: CursorType) -> None:
    from importlib import import_module
    capi = cast(Any, import_module('_testcapi'))
    failures = 0
    for offset in range(24):
        with live(routines.label) as connection, connection.cursor(kind, prepared=True) as cur:
            cur.callproc(routines.names['typed'], (None,) * 5, parameter_modes=('OUT',) * 5)
            drain(cur)
            connection.close()
            operation = partial(getattr, cur, 'call_output_params')
            bounded = getattr(capi, 'call_with_nomemory', None)
            try:
                if bounded is not None:
                    outputs = bounded(offset, offset + 1, operation)
                else:
                    capi.set_nomemory(offset, offset + 1)
                    try:
                        outputs = operation()
                    finally:
                        capi.remove_mem_hooks()
            except MemoryError:
                failures += 1
                outputs = operation()
            assert outputs is not None and tuple(outputs) == (0, 1, 2, 3, 4)
            assert outputs[3] == b'\x00\xff' and outputs[4] is None
            assert operation() is outputs
    assert failures > 0


@pytest.mark.parametrize('kind', [sqlaero.SSCursor, sqlaero.SSDictCursor])
def test_managed_prepared_stream_uses_one_remaining_deadline(routines: Routines, kind: CursorType) -> None:
    name = PREFIX + '_deadline_' + uuid4().hex[:8]
    qualified = quote(routines.database) + '.' + quote(name)
    with live(routines.label) as setup, setup.cursor() as create:
        create.execute(f'CREATE PROCEDURE {qualified}(IN n INT, OUT result INT) '
                       'BEGIN SELECT 1 AS value UNION ALL SELECT 2 UNION ALL SELECT 3; '
                       'DO SLEEP(0.15); SELECT 4 AS later; DO SLEEP(0.15); SET result=n*2; END')
        try:
            connection = live(routines.label, operation_timeout=0.25)
            try:
                cur = connection.cursor(kind, prepared=True)
                cur.callproc(name, (5, None), parameter_modes=('IN', 'OUT'))
                assert cur.fetchall() == formatted(kind, 'value', 1, 2, 3)
                assert cur.nextset() is True
                assert cur.fetchall() == formatted(kind, 'later', 4)
                with pytest.raises(OperationalTimeoutError):
                    cur.nextset()
                assert cur.call_output_params is None
            finally:
                connection.close()
        finally:
            create.execute(f'DROP PROCEDURE IF EXISTS {qualified}')
