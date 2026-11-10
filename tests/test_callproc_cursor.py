"""N07 text-callproc acceptance; pre-API builds expose one explicit RED only.

The direct-CALL control verifies these task-owned routines and server field
behavior. It is fixture evidence, not evidence that callproc is implemented.
"""

from collections.abc import Callable, Iterator, Mapping
from dataclasses import dataclass
from decimal import Decimal
from functools import partial
from itertools import count
import os
from typing import Any, TypeAlias, cast
from uuid import uuid4

import pytest
import sqlaero
from sqlaero.errors import InterfaceBusyError, SQLAeroError
from test_public_connection import live, requires_mysql


CursorType: TypeAlias = (
    type[sqlaero.Cursor] | type[sqlaero.DictCursor]
    | type[sqlaero.SSCursor] | type[sqlaero.SSDictCursor]
)
CursorValue: TypeAlias = sqlaero.Cursor | sqlaero.DictCursor | sqlaero.SSCursor | sqlaero.SSDictCursor
KINDS: tuple[CursorType, ...] = (sqlaero.Cursor, sqlaero.DictCursor, sqlaero.SSCursor, sqlaero.SSDictCursor)
MISSING_API = tuple(
    f'{kind.__name__}.{name}' for kind in KINDS
    for name in ('callproc', 'call_output_params') if not hasattr(kind, name)
)
requires_callproc = pytest.mark.skipif(
    bool(MISSING_API), reason='pre-API installed build: callproc/output property not registered',
)
PREFIX = os.environ.get('SQLAERO_TEST_CALLPROC_PREFIX') or ('dcpd_' + uuid4().hex[:12])
IDS = count(1000)


def quote(name: str) -> str:
    return '`' + name.replace('`', '``') + '`'


def scalar(connection: Any, sql: str) -> Any:
    with connection.cursor() as cur:
        cur.execute(sql)
        return cur.fetchone()[0]


def drain(cursor: Any) -> list[tuple[tuple[str, ...] | None, Any, int | None]]:
    result: list[tuple[tuple[str, ...] | None, Any, int | None]] = []
    while True:
        names = None if cursor.description is None else cursor.columns.names
        rows = None if names is None else cursor.fetchall()
        result.append((names, rows, cursor.row_count))
        if cursor.nextset() is None:
            return result


def streaming(kind: CursorType) -> bool:
    return kind in (sqlaero.SSCursor, sqlaero.SSDictCursor)


def formatted(kind: CursorType, name: str, *values: Any) -> tuple[Any, ...]:
    if kind in (sqlaero.DictCursor, sqlaero.SSDictCursor):
        return tuple({name: value} for value in values)
    return tuple((value,) for value in values)


def readonly_outputs(cursor: Any, expected: dict[int, Any]) -> Mapping[int, Any]:
    outputs = cast(Mapping[int, Any], cursor.call_output_params)
    assert isinstance(outputs, Mapping)
    assert tuple(outputs) == tuple(expected)
    assert dict(outputs) == expected
    assert cursor.call_output_params is outputs
    with pytest.raises(TypeError):
        outputs[999] = 'cannot mutate'  # type: ignore[index]
    with pytest.raises(AttributeError):
        cursor.call_output_params = {}  # type: ignore[misc]
    return outputs


def session_variables(connection: Any) -> dict[str, Any]:
    """Observe this session only, without guessing any driver's private names."""
    with connection.cursor() as cur:
        cur.execute('SELECT v.VARIABLE_NAME,v.VARIABLE_VALUE '
                    'FROM performance_schema.user_variables_by_thread AS v '
                    'JOIN performance_schema.threads AS t ON t.THREAD_ID=v.THREAD_ID '
                    'WHERE t.PROCESSLIST_ID=CONNECTION_ID() ORDER BY v.VARIABLE_NAME')
        return dict(cur.fetchall())


@dataclass
class Routines:
    label: str
    database: str
    names: dict[str, str]
    table: str
    quoted_database: str | None
    database_error: str | None
    typed: tuple[object, ...] | None = None
    variables_error: str | None = None

    def qualified(self, name: str) -> str:
        return quote(self.database) + '.' + quote(self.names[name])


@pytest.fixture(scope='module', params=('84', '97'))
def routines(request: pytest.FixtureRequest) -> Iterator[Routines]:
    if 'SQLAERO_TEST_MYSQL_ROOT' not in os.environ:
        pytest.skip('requires explicitly selected owned MySQL fixtures')
    label = str(request.param)
    names = {key: PREFIX + '_' + key for key in (
        'pure', 'zero', 'null', 'echo', 'adjust', 'typed', 'sparse', 'failure', 'receipt', 'quoted',
    )}
    names['quoted'] += '`name.with.dot'
    table = PREFIX + '_receipts'
    created: list[str] = []
    quoted_database = None
    database_error = None
    with live(label) as setup, setup.cursor() as cur:
        database = scalar(setup, 'SELECT DATABASE()')
        prefix = quote(database) + '.'
        cur.execute(f'DROP TABLE IF EXISTS {prefix}{quote(table)}')
        cur.execute(f'CREATE TABLE {prefix}{quote(table)} (v INT PRIMARY KEY) ENGINE=InnoDB')
        bodies = {
            'pure': '(IN a BIGINT, IN b BIGINT) BEGIN SELECT a+b AS value UNION ALL SELECT a+b+1; SELECT b AS second; END',
            'zero': '() BEGIN SELECT 42 AS value; END',
            'null': '(IN a BIGINT) BEGIN SELECT a IS NULL AS value; END',
            'echo': '(IN a VARBINARY(64)) BEGIN SELECT a AS value; END',
            'adjust': '(IN n BIGINT, OUT result BIGINT, INOUT counter BIGINT) BEGIN SELECT n AS value; SET result=n*2; SET counter=COALESCE(counter,0)+1; SELECT counter AS second; END',
            'typed': "(OUT d DECIMAL(12,2), OUT j JSON, OUT day_value DATE, OUT binary_value VARBINARY(4), OUT nullable_value INT) BEGIN SET d=12.50; SET j=JSON_OBJECT('v',1); SET day_value='2024-02-29'; SET binary_value=X'00FF'; SET nullable_value=NULL; END",
            'sparse': '(IN assign_value INT, OUT result BIGINT) BEGIN IF assign_value THEN SET result=99; END IF; SELECT assign_value AS value; END',
            'failure': "(IN n INT, OUT result INT) BEGIN SET result=444; SELECT n AS value; SIGNAL SQLSTATE '45000' SET MYSQL_ERRNO=1644, MESSAGE_TEXT='dcpd original CALL failure'; END",
            'receipt': f'(IN n INT, OUT result INT) BEGIN INSERT INTO {prefix}{quote(table)} VALUES(n); SET result=n*2; SELECT n AS value; END',
            'quoted': '() BEGIN SELECT 73 AS value; END',
        }
        try:
            for key, body in bodies.items():
                name = prefix + quote(names[key])
                cur.execute(f'DROP PROCEDURE IF EXISTS {name}')
                cur.execute(f'CREATE PROCEDURE {name} {body}')
                created.append(name)
            candidate = PREFIX + '`database'
            try:
                cur.execute(f'CREATE DATABASE {quote(candidate)}')
            except SQLAeroError as error:
                if error.code not in (1044, 1142, 1227):
                    raise
                database_error = f'CREATE DATABASE unavailable: {type(error).__name__} code={error.code}'
            else:
                quoted_database = candidate
                name = quote(candidate) + '.' + quote(names['zero'])
                cur.execute(f'CREATE PROCEDURE {name} {bodies["zero"]}')
            bundle = Routines(label, database, names, table, quoted_database, database_error)
            slots = [f'@{quote(PREFIX + "_control_" + str(i))}' for i in range(5)]
            cur.execute('SET ' + ','.join(f'{slot}=NULL' for slot in slots))
            cur.execute(f'CALL {bundle.qualified("typed")}({",".join(slots)})')
            drain(cur)
            cur.decode_json = True
            cur.execute('SELECT ' + ','.join(slots))
            bundle.typed = cur.fetchone()
            cur.execute('SET ' + ','.join(f'{slot}=NULL' for slot in slots))
            try:
                session_variables(setup)
            except SQLAeroError as error:
                if error.code not in (1044, 1142, 1227):
                    raise
                bundle.variables_error = f'performance_schema variable inspection unavailable: {type(error).__name__} code={error.code}'
            yield bundle
        finally:
            for name in reversed(created):
                cur.execute(f'DROP PROCEDURE IF EXISTS {name}')
            if quoted_database is not None:
                cur.execute(f'DROP DATABASE IF EXISTS {quote(quoted_database)}')
            cur.execute(f'DROP TABLE IF EXISTS {prefix}{quote(table)}')


def test_public_callproc_surface_is_registered_on_all_four_cursor_classes() -> None:
    assert not MISSING_API, 'missing confirmed public callproc surface: ' + ', '.join(MISSING_API)


@requires_mysql
def test_fixture_direct_call_controls_and_actual_text_output_fields(routines: Routines,
                                                                  record_property: Callable[[str, object], None]) -> None:
    with live(routines.label) as connection, connection.cursor() as cur:
        cur.execute(f'CALL {routines.qualified("pure")}(4,8)')
        assert drain(cur) == [(('value',), ((12,), (13,)), 2),
                              (('second',), ((8,),), 1), (None, None, 0)]
        assert routines.typed is not None
        assert routines.typed[0] == Decimal('12.50')
        assert routines.typed[3:] == (b'\x00\xff', None)
        cur.execute(f'CALL {routines.qualified("echo")}(%s)', b'one value')
        assert drain(cur)[0][1] == ((b'one value',),)
        for sql, expected_code in (
            (f'CALL {routines.qualified("zero")}(NULL)', 1318),
            (f'CALL {routines.qualified("adjust")}(5,NULL,10)', 1414),
        ):
            with pytest.raises(SQLAeroError) as caught:
                cur.execute(sql)
            assert caught.value.code == expected_code and caught.value.is_server_error
        if routines.quoted_database is not None:
            cur.execute(f'CALL {quote(routines.quoted_database)}.{quote(routines.names["zero"])}()')
            assert drain(cur)[0][1] == ((42,),)
        record_property('actual_text_output_types', repr(tuple(type(value).__name__ for value in routines.typed)))
        record_property('quoted_database', routines.quoted_database or routines.database_error)
        record_property('server_variable_inspection', routines.variables_error or 'available')


@requires_mysql
@requires_callproc
@pytest.mark.parametrize('kind', KINDS)
def test_pure_in_results_tail_and_confirmed_empty_mapping(routines: Routines, kind: CursorType) -> None:
    with live(routines.label) as connection, connection.cursor(kind) as cur:
        assert cur.call_output_params is None
        result = cur.callproc(routines.names['pure'], (4, 8))
        assert result is None if streaming(kind) else result == 2
        assert drain(cur) == [(('value',), formatted(kind, 'value', 12, 13), 2),
                              (('second',), formatted(kind, 'second', 8), 1), (None, None, 0)]
        outputs = readonly_outputs(cur, {})
        cur.execute('SELECT 1 AS value')
        assert cur.call_output_params is None
        drain(cur)
        assert dict(outputs) == {}


@requires_mysql
@requires_callproc
@pytest.mark.parametrize('kind', KINDS)
def test_explicit_out_inout_positions_snapshot_and_no_hidden_sets(routines: Routines, kind: CursorType) -> None:
    with live(routines.label) as connection, connection.cursor(kind) as cur:
        args = [5, None, 10]
        result = cur.callproc(routines.names['adjust'], args, parameter_modes=['in', 'Out', 'InOuT'])
        assert result is None if streaming(kind) else result == 1
        assert args == [5, None, 10]
        if streaming(kind):
            for _ in range(2):
                assert cur.call_output_params is None
            with connection.cursor() as other, pytest.raises(InterfaceBusyError):
                other.execute('SELECT 9')
        assert drain(cur) == [(('value',), formatted(kind, 'value', 5), 1),
                              (('second',), formatted(kind, 'second', 11), 1), (None, None, 0)]
        first = readonly_outputs(cur, {1: 10, 2: 11})
        with pytest.raises(KeyError):
            first[0]
        cur.callproc(routines.names['adjust'], (7, None, 3), parameter_modes=('IN', 'OUT', 'INOUT'))
        drain(cur)
        second = readonly_outputs(cur, {1: 14, 2: 4})
        assert second is not first and dict(first) == {1: 10, 2: 11}


@requires_mysql
@requires_callproc
@pytest.mark.parametrize('kind', KINDS)
def test_none_is_input_without_inferred_direction_and_inout_accepts_null(routines: Routines, kind: CursorType) -> None:
    with live(routines.label) as connection, connection.cursor(kind) as cur:
        cur.callproc(routines.names['null'], None)
        assert drain(cur)[0][1] == formatted(kind, 'value', 1)
        readonly_outputs(cur, {})
        cur.callproc(routines.names['adjust'], (None, None, None), parameter_modes=('IN', 'OUT', 'INOUT'))
        drain(cur)
        readonly_outputs(cur, {1: None, 2: 1})
        with pytest.raises(SQLAeroError) as caught:
            cur.callproc(routines.names['adjust'], (5, None, 10))
        assert caught.value.is_server_error is True
        assert caught.value.code == 1414


@requires_mysql
@requires_callproc
@pytest.mark.parametrize('kind', KINDS)
def test_omitted_args_are_zero_positions_and_text_bytes_are_single_values(routines: Routines, kind: CursorType) -> None:
    with live(routines.label) as connection, connection.cursor(kind) as cur:
        cur.callproc(routines.names['zero'])
        assert drain(cur) == [(('value',), formatted(kind, 'value', 42), 1), (None, None, 0)]
        readonly_outputs(cur, {})
        for value in ('one value', b'bytes value'):
            cur.callproc(routines.names['echo'], value)
            expected = value.encode('ascii') if isinstance(value, str) else value
            assert drain(cur)[0][1] == formatted(kind, 'value', expected)
            readonly_outputs(cur, {})
        with pytest.raises(SQLAeroError) as caught:
            cur.callproc(routines.names['zero'], None)
        assert caught.value.is_server_error is True and caught.value.code == 1318


@requires_mysql
@requires_callproc
@pytest.mark.parametrize('kind', KINDS)
def test_direct_execute_call_does_not_create_managed_outputs(routines: Routines, kind: CursorType) -> None:
    with live(routines.label) as connection, connection.cursor(kind) as cur:
        cur.callproc(routines.names['zero'])
        drain(cur)
        saved = readonly_outputs(cur, {})
        cur.execute(f'CALL {routines.qualified("pure")}(4,8)')
        assert cur.call_output_params is None
        assert len(drain(cur)) == 3
        assert cur.call_output_params is None and dict(saved) == {}


@requires_mysql
@requires_callproc
@pytest.mark.parametrize('kind', KINDS)
@pytest.mark.parametrize('multi', [False, True])
def test_initialization_reuse_and_old_native_outputs_are_independent(routines: Routines, kind: CursorType, multi: bool) -> None:
    connection = live(routines.label, multi_statements=multi)
    first = connection.cursor(kind)
    second = connection.cursor(kind)
    try:
        first.callproc(routines.names['adjust'], (5, None, 10), parameter_modes=('IN', 'OUT', 'INOUT'))
        drain(first)
        second.callproc(routines.names['adjust'], (7, None, 3), parameter_modes=('IN', 'OUT', 'INOUT'))
        drain(second)
        connection.close()
        readonly_outputs(first, {1: 10, 2: 11})
        readonly_outputs(second, {1: 14, 2: 4})
    finally:
        first.close()
        second.close()
        connection.close()


@requires_mysql
@requires_callproc
@pytest.mark.parametrize('kind', KINDS)
def test_unassigned_out_is_null_instead_of_previous_slot_value(routines: Routines, kind: CursorType) -> None:
    with live(routines.label) as connection, connection.cursor(kind) as cur:
        for flag, expected in ((1, 99), (0, None), (1, 99)):
            cur.callproc(routines.names['sparse'], (flag, None), parameter_modes=('IN', 'OUT'))
            drain(cur)
            readonly_outputs(cur, {1: expected})


@requires_mysql
@requires_callproc
@pytest.mark.parametrize('kind', KINDS)
def test_actual_output_decode_policy_is_captured_before_later_option_changes(routines: Routines, kind: CursorType) -> None:
    assert routines.typed is not None, 'direct text-field fixture control must run first'
    with live(routines.label) as connection, connection.cursor(kind) as cur:
        cur.decode_json = True
        cur.use_decimal = True
        cur.callproc(routines.names['typed'], (None,) * 5, parameter_modes=('OUT',) * 5)
        cur.use_decimal = False
        cur.decode_json = False
        drain(cur)
        connection.close()
        outputs = cast(Mapping[int, object], readonly_outputs(cur, dict(enumerate(routines.typed))))
        assert tuple(type(outputs[index]) for index in range(5)) == tuple(type(value) for value in routines.typed)


@requires_mysql
@requires_callproc
@pytest.mark.parametrize('kind', KINDS)
def test_close_and_business_exception_preserve_confirmed_outputs(routines: Routines, kind: CursorType) -> None:
    with live(routines.label) as connection:
        cur = connection.cursor(kind)
        original = ValueError('dcpd business primary')
        with pytest.raises(ValueError) as caught:
            with cur:
                cur.callproc(routines.names['adjust'], (5, None, 10), parameter_modes=('IN', 'OUT', 'INOUT'))
                raise original
        assert caught.value is original and cur.closed
        assert connection.ping() is True
        connection.close()
        readonly_outputs(cur, {1: 10, 2: 11})


@requires_mysql
@requires_callproc
@pytest.mark.parametrize('kind', KINDS)
@pytest.mark.parametrize('rollback', [False, True])
def test_transaction_receipt_and_closed_output_are_separate_facts(routines: Routines, kind: CursorType, rollback: bool) -> None:
    with live(routines.label) as connection:
        cur: CursorValue | None = None
        value = next(IDS)
        original = ValueError('dcpd rollback primary')
        if rollback:
            with pytest.raises(ValueError) as caught:
                with connection.transaction(kind) as cur:
                    cur.callproc(routines.names['receipt'], (value, None), parameter_modes=('IN', 'OUT'))
                    assert cur.fetchone() == ({'value': value} if kind in (sqlaero.DictCursor, sqlaero.SSDictCursor) else (value,))
                    raise original
            assert caught.value is original
        else:
            with connection.transaction(kind) as cur:
                cur.callproc(routines.names['receipt'], (value, None), parameter_modes=('IN', 'OUT'))
        assert cur is not None and cur.closed
        readonly_outputs(cur, {1: value * 2})
        with connection.cursor() as check:
            check.execute(f'SELECT v FROM {quote(routines.database)}.{quote(routines.table)} WHERE v=%s', value)
            assert check.fetchall() == (() if rollback else ((value,),))


@requires_mysql
@requires_callproc
@pytest.mark.parametrize('kind', KINDS)
def test_failed_call_keeps_real_error_prefix_and_never_publishes_output(routines: Routines, kind: CursorType) -> None:
    with live(routines.label) as connection, connection.cursor(kind) as cur:
        if streaming(kind):
            cur.callproc(routines.names['failure'], (23, None), parameter_modes=('IN', 'OUT'))
            assert cur.fetchall() == formatted(kind, 'value', 23)
            with pytest.raises(SQLAeroError) as caught:
                cur.nextset()
        else:
            with pytest.raises(SQLAeroError) as caught:
                cur.callproc(routines.names['failure'], (23, None), parameter_modes=('IN', 'OUT'))
            assert cur.fetchall() == formatted(kind, 'value', 23)
        assert caught.value.code == 1644 and caught.value.is_server_error is True
        assert cur.call_output_params is None
        with pytest.raises(type(caught.value)) as boundary:
            cur.nextset()
        assert boundary.value.code == 1644


@requires_mysql
@requires_callproc
@pytest.mark.parametrize('kind', [sqlaero.SSCursor, sqlaero.SSDictCursor])
def test_business_primary_survives_later_call_error_on_exit(routines: Routines, kind: CursorType) -> None:
    connection = live(routines.label)
    cur = connection.cursor(kind)
    original = ValueError('business before CALL tail')
    try:
        with pytest.raises(ValueError) as caught:
            with cur:
                cur.callproc(routines.names['failure'], (23, None), parameter_modes=('IN', 'OUT'))
                raise original
        assert caught.value is original
        assert any('1644' in note or 'dcpd original CALL failure' in note for note in original.__notes__)
        assert cur.closed and cur.call_output_params is None
    finally:
        connection.close()


@requires_mysql
@requires_callproc
@pytest.mark.parametrize('kind', KINDS)
def test_raw_routine_name_and_explicit_database_are_quoted_separately(routines: Routines, kind: CursorType) -> None:
    class Name(str):
        def __str__(self) -> str:
            raise AssertionError('native string subclasses must not call __str__')

    with live(routines.label) as connection, connection.cursor(kind) as cur:
        before = scalar(connection, 'SELECT DATABASE()')
        cur.callproc(Name(routines.names['quoted']), database=Name(routines.database))
        assert drain(cur)[0][1] == formatted(kind, 'value', 73)
        assert scalar(connection, 'SELECT DATABASE()') == before


@requires_mysql
@requires_callproc
def test_database_identifier_with_backtick_when_fixture_privilege_allows(routines: Routines) -> None:
    if routines.quoted_database is None:
        pytest.skip(routines.database_error or 'fixture cannot create quoted database')
    with live(routines.label) as connection, connection.cursor() as cur:
        before = scalar(connection, 'SELECT DATABASE()')
        cur.callproc(routines.names['zero'], database=routines.quoted_database)
        assert drain(cur)[0][1] == ((42,),)
        assert scalar(connection, 'SELECT DATABASE()') == before


@requires_mysql
@requires_callproc
def test_strict_preparation_failure_preserves_old_mapping_and_results(routines: Routines) -> None:
    class NotName:
        def __str__(self) -> str:
            raise AssertionError('name validation must not stringify arbitrary objects')

    with live(routines.label) as connection, connection.cursor() as cur:
        cur.callproc(routines.names['adjust'], (5, None, 10), parameter_modes=('IN', 'OUT', 'INOUT'))
        old = readonly_outputs(cur, {1: 10, 2: 11})
        columns = cur.columns
        invalid: list[tuple[object, object, dict[str, object]]] = [
            (None, (), {}), (42, (), {}), (NotName(), (), {}),
            ('', (), {}), ('a\x00b', (), {}), (routines.names['zero'], (), {'database': ''}),
            (routines.names['zero'], (), {'database': 42}),
            (routines.names['zero'], (), {'database': NotName()}),
            (routines.names['zero'], (), {'database': 'a\x00b'}),
            (routines.names['adjust'], (5, None, 10), {'parameter_modes': ('IN',)}),
            (routines.names['adjust'], (5, 7, 10), {'parameter_modes': ('IN', 'OUT', 'INOUT')}),
            (routines.names['adjust'], (5, None, 10), {'parameter_modes': ('IN', 'BAD', 'INOUT')}),
            (routines.names['adjust'], (5, None, 10), {'parameter_modes': 'IN,OUT,INOUT'}),
            (routines.names['adjust'], (5, None, 10), {'parameter_modes': ('IN', 1, 'INOUT')}),
        ]
        for name, args, keywords in invalid:
            with pytest.raises((TypeError, ValueError, SQLAeroError)) as caught:
                cast(Any, cur).callproc(name, args, **keywords)
            assert getattr(caught.value, 'is_server_error', None) is not True
            assert cur.call_output_params is old and cur.columns is columns
        with pytest.raises(TypeError):
            cast(Any, cur).callproc(routines.names['zero'], many=True)
        with pytest.raises(TypeError):
            cast(Any, cur).callproc(routines.names['zero'], itemize=True)
        assert cur.fetchone() == (5,)


@requires_mysql
@requires_callproc
def test_multirow_table_is_rejected_before_call(routines: Routines) -> None:
    np = pytest.importorskip('numpy')
    with live(routines.label) as connection, connection.cursor() as cur:
        cur.execute('SELECT 91 AS untouched')
        columns = cur.columns
        with pytest.raises(Exception) as caught:
            cur.callproc(routines.names['null'], np.array([[1], [2]], dtype=np.int64))
        assert getattr(caught.value, 'is_server_error', None) is not True
        assert cur.columns is columns and cur.fetchone() == (91,)


def out_placeholder_arguments(source: str, receipt: int) -> object:
    """Build independent source-level NULL cases; none is a literal None slot."""
    if source.startswith('decimal-'):
        return (receipt, Decimal(source.removeprefix('decimal-')))
    if source == 'float-nan':
        return (receipt, float('nan'))
    if source == 'integer':
        return (receipt, 7)
    if source.startswith('numpy-'):
        np = pytest.importorskip('numpy')
        if source == 'numpy-NaT':
            return (receipt, np.datetime64('NaT', 'ns'))
        if source == 'numpy-timedelta-NaT':
            return (receipt, np.timedelta64('NaT', 'ns'))
        if source == 'numpy-object-NaT':
            return np.array([receipt, np.datetime64('NaT', 'ns')], dtype=object)
        return np.array((receipt, 'NaT'), dtype=[('receipt', 'i8'), ('out', 'M8[ns]')])[()]
    if source.startswith('pandas-'):
        pd = pytest.importorskip('pandas')
        if source == 'pandas-NA':
            return (receipt, pd.NA)
        if source == 'pandas-NaT':
            return (receipt, pd.NaT)
        return pd.DataFrame({'receipt': [receipt], 'out': pd.array([None], dtype='Float64')})
    if source in ('arrow-null', 'arrow-list-containing-null'):
        pa = pytest.importorskip('pyarrow')
        out = (pa.array([None], type=pa.int64()) if source == 'arrow-null'
               else pa.array([[None]], type=pa.list_(pa.int64())))
        return pa.table({'receipt': [receipt], 'out': out})
    if source == 'polars-null':
        pl = pytest.importorskip('polars')
        return pl.DataFrame({'receipt': [receipt], 'out': pl.Series([None], dtype=pl.Int64)})
    raise AssertionError(f'Unknown test source: {source}')


def target_command_counts(connection: Any) -> dict[str, int]:
    """Observe real target commands using a separate ordinary text Cursor."""
    with connection.cursor() as observer:
        observer.execute("SHOW SESSION STATUS WHERE Variable_name IN "
                         "('Com_stmt_prepare','Com_call_procedure')")
        result = {name: int(value) for name, value in observer.fetchall()}
        assert set(result) == {'Com_stmt_prepare', 'Com_call_procedure'}
        return result


@requires_mysql
@requires_callproc
@pytest.mark.parametrize('kind', KINDS)
@pytest.mark.parametrize('prepared', [False, True])
@pytest.mark.parametrize('source', [
    'decimal-NaN', 'decimal-Infinity', 'decimal--Infinity', 'decimal-sNaN',
    'float-nan', 'integer', 'numpy-NaT', 'numpy-timedelta-NaT', 'numpy-object-NaT',
    'numpy-record-NaT', 'pandas-NA', 'pandas-NaT', 'pandas-nullable',
    'arrow-null', 'arrow-list-containing-null', 'polars-null',
])
def test_out_requires_literal_none_before_target_send(
    routines: Routines, kind: CursorType, prepared: bool, source: str,
) -> None:
    receipt = next(IDS)
    args = out_placeholder_arguments(source, receipt)
    with live(routines.label) as connection, connection.cursor(kind, prepared=prepared) as cur:
        cur.callproc(routines.names['zero'])
        drain(cur)
        saved_output = readonly_outputs(cur, {})
        columns = cur.columns
        commands = target_command_counts(connection)
        variables = session_variables(connection)
        with pytest.raises(ValueError, match=r'args\[1\].*must be None'):
            cast(Any, cur).callproc(routines.names['receipt'], args,
                                   parameter_modes=('IN', 'OUT'))
        assert target_command_counts(connection) == commands
        assert session_variables(connection) == variables
        assert scalar(connection, f'SELECT COUNT(*) FROM {quote(routines.table)} '
                      f'WHERE v={receipt}') == 0
        assert cur.columns is columns and cur.call_output_params is saved_output
        assert cur.nextset() is None
        assert connection.ping()


@requires_mysql
@requires_callproc
@pytest.mark.parametrize('kind', KINDS)
@pytest.mark.parametrize('prepared', [False, True])
@pytest.mark.parametrize('source', [
    'tuple', 'list', 'numpy-object', 'numpy-record', 'pandas-object', 'polars-object',
])
def test_literal_none_out_survives_shared_source_preparation(
    routines: Routines, kind: CursorType, prepared: bool, source: str,
) -> None:
    receipt = next(IDS)
    args: object = (receipt, None)
    if source == 'list':
        args = [receipt, None]
    elif source.startswith('numpy-'):
        np = pytest.importorskip('numpy')
        args = (np.array([receipt, None], dtype=object) if source == 'numpy-object'
                else np.array((receipt, None), dtype=[('receipt', 'i8'), ('out', object)])[()])
    elif source == 'pandas-object':
        pd = pytest.importorskip('pandas')
        args = pd.DataFrame({'receipt': [receipt], 'out': pd.Series([None], dtype=object)})
    elif source == 'polars-object':
        pl = pytest.importorskip('polars')
        args = pl.DataFrame({'receipt': [receipt], 'out': pl.Series([None], dtype=pl.Object)})
    with live(routines.label) as connection, connection.cursor(kind, prepared=prepared) as cur:
        cur.callproc(routines.names['receipt'], args, parameter_modes=('IN', 'OUT'))
        drain(cur)
        readonly_outputs(cur, {1: receipt * 2})
        assert scalar(connection, f'SELECT COUNT(*) FROM {quote(routines.table)} '
                      f'WHERE v={receipt}') == 1


@requires_mysql
@requires_callproc
def test_output_mapping_allocation_failure_is_atomic_and_retryable(routines: Routines) -> None:
    from importlib import import_module
    capi = cast(Any, import_module('_testcapi'))
    assert routines.typed is not None
    expected = dict(enumerate(routines.typed))
    failures = 0
    for offset in range(24):
        with live(routines.label) as connection, connection.cursor() as cur:
            cur.use_decimal = True
            cur.decode_json = True
            cur.callproc(routines.names['typed'], (None,) * 5, parameter_modes=('OUT',) * 5)
            connection.close()
            operation = partial(getattr, cur, 'call_output_params')
            bounded = getattr(capi, 'call_with_nomemory', None)
            try:
                if bounded is not None:
                    result = bounded(offset, offset + 1, operation)
                else:
                    capi.set_nomemory(offset, offset + 1)
                    try:
                        result = operation()
                    finally:
                        capi.remove_mem_hooks()
            except MemoryError:
                failures += 1
                result = operation()
            assert dict(result) == expected
            assert operation() is result
    assert failures > 0


@requires_mysql
@requires_callproc
@pytest.mark.parametrize('multi', [False, True])
def test_server_slot_values_are_cleared_and_names_reused_without_touching_user_vars(routines: Routines, multi: bool) -> None:
    if routines.variables_error is not None:
        pytest.skip(routines.variables_error)
    with live(routines.label, multi_statements=multi) as connection, connection.cursor() as cur:
        cur.execute('SET @dcpd_user_sentinel=123')
        before = session_variables(connection)
        cur.callproc(routines.names['adjust'], (5, None, 10), parameter_modes=('IN', 'OUT', 'INOUT'))
        drain(cur)
        after = session_variables(connection)
        new_names = set(after) - set(before)
        assert new_names, 'the text OUT path must have observable output slots'
        assert all(after[name] is None for name in new_names)
        assert scalar(connection, 'SELECT @dcpd_user_sentinel') == 123
        saved = readonly_outputs(cur, {1: 10, 2: 11})
        cur.callproc(routines.names['adjust'], (7, None, 3), parameter_modes=('IN', 'OUT', 'INOUT'))
        drain(cur)
        repeated = session_variables(connection)
        assert set(repeated) == set(after), 'repeated calls reuse this session slot names'
        assert all(repeated[name] is None for name in new_names)
        assert dict(saved) == {1: 10, 2: 11}
