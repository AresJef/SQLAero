"""Public native prepared execution on both owned MySQL servers.

These tests require the real factories and real server responses. They do not
substitute a binder probe or a text-protocol fallback for prepared execution.
"""

from __future__ import annotations

from datetime import date, datetime, time, timedelta, timezone
from decimal import Decimal
from enum import IntEnum
import math
from typing import TYPE_CHECKING
from uuid import uuid4

import pytest
import sqlaero
from sqlaero import Connection, Cursor
from sqlaero.errors import DataError, IntegrityError, InterfaceError, NotSupportedError, ProgrammingError
from test_public_connection import connection as connection, requires_mysql

if TYPE_CHECKING:
    from sqlaero import DictCursor


def buffered(connection: Connection, dictionary: bool, *, prepared: bool = True) -> Cursor | DictCursor:
    """Select the public row family without inventing another Cursor class."""
    return connection.cursor(dict if dictionary else tuple, prepared=prepared)


def row(dictionary: bool, **values: object) -> tuple[object, ...] | dict[str, object]:
    """Spell identical expected data in the two public row representations."""
    return values if dictionary else tuple(values.values())


def statement_counts(connection: Connection) -> dict[str, int]:
    """Keep raw server counts and distinguish client PREPARE from reprepare."""
    counts: dict[str, int] = {}
    with connection.cursor() as observer:
        assert observer.execute(
            "SHOW SESSION STATUS WHERE Variable_name IN "
            "('Com_stmt_prepare','Com_stmt_reprepare','Com_stmt_execute','Com_stmt_close')"
        ) == 4
        for name, value in observer.fetchall():
            assert isinstance(name, str) and isinstance(value, str)
            counts[name] = int(value)
    assert counts.keys() == {'Com_stmt_prepare', 'Com_stmt_reprepare', 'Com_stmt_execute', 'Com_stmt_close'}
    # MySQL increments both raw counters for one internal reprepare. These
    # sessions never issue SQL PREPARE, so their difference counts client work.
    counts['Client_stmt_prepare'] = counts['Com_stmt_prepare'] - counts['Com_stmt_reprepare']
    return counts


class IntegerFlag(IntEnum):
    ONE = 1


class IntegerSubclass(int):
    """An int subclass remains outside an exact-bool factory flag."""


class BooleanTrap:
    """A rejected factory flag must never invoke user truth conversion."""

    def __bool__(self) -> bool:
        raise AssertionError('prepared must be checked without truth conversion')


@requires_mysql
@pytest.mark.parametrize('dictionary', [False, True], ids=['tuple', 'dict'])
@pytest.mark.parametrize('factory_name', ['cursor', 'transaction'])
def test_prepared_factory_is_exact_bool_and_creation_policy_is_read_only(
    connection: Connection, dictionary: bool, factory_name: str,
) -> None:
    factory = getattr(connection, factory_name)
    marker = dict if dictionary else tuple
    expected_class = 'DictCursor' if dictionary else 'Cursor'
    choices: tuple[tuple[dict[str, bool], bool], ...] = (
        ({}, False), ({'prepared': False}, False), ({'prepared': True}, True),
    )
    for options, expected in choices:
        with factory(marker, **options) as current:
            assert type(current) is getattr(sqlaero, expected_class)
            assert current.prepared is expected
            with pytest.raises(AttributeError):
                setattr(current, 'prepared', not expected)
            assert current.execute('SELECT 7 AS value') == 1
            assert current.fetchone() == row(dictionary, value=7)
        assert current.prepared is expected
        with pytest.raises(AttributeError):
            setattr(current, 'prepared', not expected)
    invalid_flags: tuple[object, ...] = (
        None, 0, 1, -1, 1.0, 'true', [], {}, IntegerFlag.ONE, IntegerSubclass(1), BooleanTrap(),
    )
    for invalid in invalid_flags:
        with pytest.raises(TypeError):
            factory(marker, prepared=invalid)


@requires_mysql
@pytest.mark.parametrize('dictionary', [False, True], ids=['tuple', 'dict'])
@pytest.mark.parametrize('sql_mode', ['', 'ANSI_QUOTES,NO_BACKSLASH_ESCAPES'])
def test_prepared_markers_share_the_real_lexer_and_preserve_protected_text(
    connection: Connection, dictionary: bool, sql_mode: str,
) -> None:
    with connection.cursor() as setup:
        setup.execute('SET SESSION sql_mode=%s', (sql_mode,))
    value = "? %s 中'\\\0"
    sql = ('SELECT %s AS "bound", \'? %s %% ;\' AS "literal", 10%%3 AS "remainder" '
           '/* ? %s */ # ? %s\n;')
    with buffered(connection, dictionary) as current:
        assert current.execute(sql, (value,)) == 1
        assert current.columns.names == ('bound', 'literal', 'remainder')
        assert current.fetchone() == row(dictionary, bound=value, literal='? %s %% ;', remainder=1)
        assert current.nextset() is None


@requires_mysql
@pytest.mark.parametrize('dictionary', [False, True], ids=['tuple', 'dict'])
def test_active_question_mark_and_local_binding_failures_preserve_old_result(
    connection: Connection, dictionary: bool,
) -> None:
    with buffered(connection, dictionary) as current:
        for sql, args in [('SELECT ?', (1,)), ('SELECT %s, ?', (1, 2)), ('SELECT %s,%s', (1,))]:
            current.execute('SELECT 41 AS saved')
            columns = current.columns
            before = statement_counts(connection)
            with pytest.raises(ProgrammingError) as caught:
                current.execute(sql, args)
            assert caught.value.is_server_error is False
            assert statement_counts(connection) == before
            assert current.columns is columns
            assert current.fetchone() == row(dictionary, saved=41)
        current.execute('SELECT 42 AS saved')
        columns = current.columns
        before = statement_counts(connection)
        with pytest.raises(NotSupportedError):
            current.execute('SELECT 1 IN %s', ((1, 2),))
        assert statement_counts(connection) == before
        assert current.columns is columns
        assert current.fetchone() == row(dictionary, saved=42)


@requires_mysql
@pytest.mark.parametrize('dictionary', [False, True], ids=['tuple', 'dict'])
def test_prepared_multi_statement_is_a_real_prepare_error_without_text_replay(
    connection: Connection, dictionary: bool,
) -> None:
    with buffered(connection, dictionary) as current:
        before = statement_counts(connection)
        with pytest.raises(ProgrammingError) as caught:
            current.execute('SELECT %s AS first; SELECT %s AS second', (1, 2))
        assert caught.value.is_server_error is True
        assert caught.value.code == 1064
        assert statement_counts(connection)['Com_stmt_execute'] == before['Com_stmt_execute']
        assert current.execute("SELECT '; ? %s' AS literal") == 1
        assert current.fetchone() == row(dictionary, literal='; ? %s')
        assert current.nextset() is None


@requires_mysql
@pytest.mark.parametrize('dictionary', [False, True], ids=['tuple', 'dict'])
@pytest.mark.parametrize(('value', 'expected'), [
    pytest.param(None, None, id='null'),
    pytest.param(True, 1, id='true-numeric'),
    pytest.param(False, 0, id='false-numeric'),
    pytest.param(-(2**63), -(2**63), id='i64-min'),
    pytest.param(2**63 - 1, 2**63 - 1, id='i64-max'),
    pytest.param(2**63, 2**63, id='u64-above-i64'),
    pytest.param(2**64 - 1, 2**64 - 1, id='u64-max'),
    pytest.param(-(2**63) - 1, Decimal(-(2**63) - 1), id='negative-newdecimal'),
    pytest.param(2**64, Decimal(2**64), id='positive-newdecimal'),
    pytest.param(10**65 - 1, Decimal('9' * 65), id='65-digit-exact'),
    pytest.param(Decimal('123456789012345678901234567890.12345000000000000000'),
                 Decimal('123456789012345678901234567890.12345'), id='exact-decimal'),
    pytest.param(Decimal('1e-30'), Decimal('1e-30'), id='small-decimal'),
    pytest.param(1.25, 1.25, id='native-double'),
    pytest.param(float('nan'), None, id='nan-null'),
    pytest.param(float('inf'), None, id='infinity-null'),
    pytest.param(Decimal('NaN'), None, id='decimal-nan-null'),
    pytest.param(b'\0\xff?%s\\\'', b'\0\xff?%s\\\'', id='binary'),
    pytest.param("中😀'\\\0?%s", "中😀'\\\0?%s", id='text'),
    pytest.param('', '', id='empty-text'),
    pytest.param(b'', b'', id='empty-binary'),
    pytest.param(date(2024, 2, 29), date(2024, 2, 29), id='date'),
    pytest.param(datetime(2024, 2, 29, 12, 34, 56, 123456),
                 datetime(2024, 2, 29, 12, 34, 56, 123456), id='datetime'),
    pytest.param(datetime(2024, 2, 29, 12, 34, 56, tzinfo=timezone(timedelta(hours=1))),
                 datetime(2024, 2, 29, 11, 34, 56), id='aware-datetime-utc'),
    pytest.param(time(12, 34, 56, 123456), timedelta(hours=12, minutes=34, seconds=56, microseconds=123456),
                 id='clock'),
    pytest.param(time(0, 30, tzinfo=timezone(timedelta(hours=1))), timedelta(minutes=30),
                 id='aware-clock-ignores-zone'),
    pytest.param(timedelta(days=1, seconds=1, microseconds=123456),
                 timedelta(days=1, seconds=1, microseconds=123456), id='duration-over-24h'),
    pytest.param(timedelta(microseconds=-1), timedelta(microseconds=-1), id='negative-duration'),
])
def test_prepared_scalar_parameters_reach_real_mysql_values(
    connection: Connection, dictionary: bool, value: object, expected: object,
) -> None:
    with buffered(connection, dictionary) as current:
        # A direct SELECT marker derives VARCHAR/collation_connection. Use an
        # explicit binary expression when asserting a bytes result; input BLOB
        # alone does not override the server's result metadata.
        sql = ('SELECT CAST(%s AS BINARY) AS value' if isinstance(expected, bytes)
               else 'SELECT %s AS value')
        assert current.execute(sql, (value,)) == 1
        assert current.row_count == current.rowcount == current.affected_rows == 1
        assert current.batch_row_count == 1
        actual = current.fetchone()
        assert actual == row(dictionary, value=expected)
        assert actual is not None
        result_value = actual['value'] if isinstance(actual, dict) else actual[0]
        if isinstance(expected, Decimal):
            assert isinstance(result_value, Decimal)
        elif isinstance(expected, bytes):
            assert isinstance(result_value, bytes)
            assert current.columns[0].collation_id == 63
            assert current.columns[0].is_binary_string is True
        assert current.fetchone() is None


@requires_mysql
@pytest.mark.parametrize('dictionary', [False, True], ids=['tuple', 'dict'])
def test_prepared_bytes_input_does_not_override_derived_text_result_metadata(
    connection: Connection, dictionary: bool,
) -> None:
    with buffered(connection, dictionary) as current:
        for value, expected in ((b'abc', 'abc'), (b'', '')):
            assert current.execute('SELECT %s AS value', (value,)) == 1
            assert current.columns[0].type_code == 253
            assert current.columns[0].charset_name == 'utf8mb4'
            assert current.columns[0].is_binary_string is False
            assert current.fetchone() == row(dictionary, value=expected)

        # BLOB input retains its original bytes. The independently derived
        # VARCHAR result remains subject to the actual text Decode policy.
        value = b'\0\xff?%s\\\''
        assert current.execute('SELECT %s AS value', (value,)) == 1
        columns = current.columns
        assert columns[0].collation_id == 255
        for _ in range(2):
            with pytest.raises(UnicodeDecodeError) as caught:
                current.fetchone()
            assert caught.value.object == value
            assert caught.value.start == 1
        assert current.batch_row_count == 1
        assert current.execute('SELECT HEX(%s) AS value', (value,)) == 1
        assert current.fetchone() == row(dictionary, value='00FF3F25735C27')

        current.decode_text_strict = False
        assert current.execute('SELECT %s AS value', (value,)) == 1
        assert current.fetchone() == row(dictionary, value=value.decode('utf8', 'surrogateescape'))
        assert current.columns[0].is_binary_string is False
        assert columns[0].collation_id == 255


@requires_mysql
@pytest.mark.parametrize('dictionary', [False, True], ids=['tuple', 'dict'])
def test_prepared_negative_zero_is_preserved_by_the_binary_result(
    connection: Connection, dictionary: bool,
) -> None:
    with buffered(connection, dictionary) as current:
        assert current.execute('SELECT %s AS value', (-0.0,)) == 1
        actual = current.fetchone()
        assert actual is not None
        value = actual['value'] if isinstance(actual, dict) else actual[0]
        assert isinstance(value, float) and value == 0.0
        assert math.copysign(1.0, value) == -1.0


@requires_mysql
@pytest.mark.parametrize('dictionary', [False, True], ids=['tuple', 'dict'])
def test_same_prepared_template_updates_types_null_bitmap_and_current_metadata(
    connection: Connection, dictionary: bool,
) -> None:
    with buffered(connection, dictionary) as current:
        before = statement_counts(connection)
        current.execute('SELECT %s AS value', (7,))
        old_columns = current.columns
        old_type = old_columns[0].type_code
        # String input does not force server reprepare. After unsigned integer
        # inference it is converted to that type; DECIMAL later accepts integer
        # input without changing its result metadata back to LONGLONG.
        cases: tuple[tuple[object, object, int, bool], ...] = (
            (None, None, 8, False),
            (2**64 - 1, 2**64 - 1, 8, True),
            (b'12', 12, 8, True),
            ('13', 13, 8, True),
            (Decimal('1.25'), Decimal('1.25'), 246, False),
            (-7, Decimal('-7'), 246, False),
        )
        for value, expected, type_code, unsigned in cases:
            assert current.execute('SELECT %s AS value', (value,)) == 1
            actual = current.fetchone()
            assert actual == row(dictionary, value=expected)
            assert actual is not None
            result_value = actual['value'] if isinstance(actual, dict) else actual[0]
            if isinstance(expected, Decimal):
                assert isinstance(result_value, Decimal)
            assert current.columns[0].type_code == type_code
            assert current.columns[0].is_unsigned is unsigned
            assert current.columns.names == ('value',)
            assert old_columns[0].type_code == old_type
            assert old_columns[0].is_unsigned is False
        counts = statement_counts(connection)
        assert counts['Client_stmt_prepare'] - before['Client_stmt_prepare'] == 1
        assert counts['Com_stmt_execute'] - before['Com_stmt_execute'] == 7
        sql = 'SELECT ' + ','.join(f'%s AS c{index}' for index in range(16))
        for values in [tuple(None if index % 2 == 0 else index for index in range(16)),
                       tuple(None if index % 2 else index for index in range(16)), tuple(range(16))]:
            assert current.execute(sql, values) == 1
            expected = {f'c{index}': value for index, value in enumerate(values)}
            assert current.fetchone() == row(dictionary, **expected)


@requires_mysql
@pytest.mark.parametrize('dictionary', [False, True], ids=['tuple', 'dict'])
def test_long_prepared_values_cross_native_fragment_boundaries_without_escaping(
    connection: Connection, dictionary: bool,
) -> None:
    text = "中'\\\0?%s" * 60_000
    binary = bytes(range(256)) * 1500
    with buffered(connection, dictionary) as current:
        assert current.execute('SELECT %s AS text_value,CAST(%s AS BINARY) AS binary_value',
                               (text, binary)) == 1
        assert current.fetchone() == row(dictionary, text_value=text, binary_value=binary)


@requires_mysql
@pytest.mark.parametrize('dictionary', [False, True], ids=['tuple', 'dict'])
def test_prepared_handle_slot_reuses_switches_and_closes_on_the_same_connection(
    connection: Connection, dictionary: bool,
) -> None:
    before = statement_counts(connection)
    current = buffered(connection, dictionary)
    assert current.prepared is True
    assert statement_counts(connection) == before
    assert current.execute('SELECT %s AS first', (10,)) == 1
    assert current.fetchone() == row(dictionary, first=10)
    columns = current.columns
    assert current.execute('SELECT %s AS first', (11,)) == 1
    assert current.fetchone() == row(dictionary, first=11)
    same = statement_counts(connection)
    assert same['Client_stmt_prepare'] == before['Client_stmt_prepare'] + 1
    assert same['Com_stmt_execute'] == before['Com_stmt_execute'] + 2
    assert current.execute('SELECT %s AS second', (20,)) == 1
    assert current.fetchone() == row(dictionary, second=20)
    switched = statement_counts(connection)
    assert switched['Client_stmt_prepare'] == same['Client_stmt_prepare'] + 1
    assert switched['Com_stmt_close'] == same['Com_stmt_close'] + 1
    assert columns.names == ('first',)
    assert current.close() is None
    assert current.close() is None
    closed = statement_counts(connection)
    assert closed['Com_stmt_close'] == switched['Com_stmt_close'] + 1
    assert current.prepared is True
    with pytest.raises(InterfaceError):
        current.execute('SELECT 99')
    with buffered(connection, dictionary) as replacement:
        assert replacement.execute('SELECT %s AS replacement', (30,)) == 1
        assert replacement.fetchone() == row(dictionary, replacement=30)
    assert statement_counts(connection)['Client_stmt_prepare'] == closed['Client_stmt_prepare'] + 1


@requires_mysql
@pytest.mark.parametrize('dictionary', [False, True], ids=['tuple', 'dict'])
def test_prepared_reuse_compares_complete_sql_contents_and_confirmed_sql_mode(
    connection: Connection, dictionary: bool,
) -> None:
    with connection.cursor() as setup:
        setup.execute("SET SESSION sql_mode=''")
    with buffered(connection, dictionary) as current:
        sql = "SELECT 'a\\n' AS value /* same template */"
        same = bytearray(sql, 'ascii').decode('ascii')
        assert same == sql and same is not sql
        before = statement_counts(connection)
        for source in (sql, same):
            assert current.execute(source) == 1
            assert current.fetchone() == row(dictionary, value='a\n')
        repeated = statement_counts(connection)
        assert repeated['Client_stmt_prepare'] == before['Client_stmt_prepare'] + 1
        with connection.cursor() as setup:
            setup.execute("SET SESSION sql_mode='NO_BACKSLASH_ESCAPES'")
        assert current.execute(sql) == 1
        assert current.fetchone() == row(dictionary, value='a\\n')
        changed_context = statement_counts(connection)
        assert changed_context['Client_stmt_prepare'] == repeated['Client_stmt_prepare'] + 1
        assert changed_context['Com_stmt_close'] == repeated['Com_stmt_close'] + 1
        assert current.execute(sql + ' ') == 1
        assert current.fetchone() == row(dictionary, value='a\\n')
        changed_sql = statement_counts(connection)
        assert changed_sql['Client_stmt_prepare'] == changed_context['Client_stmt_prepare'] + 1
        assert changed_sql['Com_stmt_close'] == changed_context['Com_stmt_close'] + 1


@requires_mysql
@pytest.mark.parametrize('dictionary', [False, True], ids=['tuple', 'dict'])
@pytest.mark.parametrize('many_method', ['execute', 'executemany'])
def test_prepared_many_keeps_every_result_and_separates_current_and_batch_counts(
    connection: Connection, dictionary: bool, many_method: str,
) -> None:
    values = [(10, 1), (20, -1), (30, 1)]
    sql = 'SELECT %s AS value WHERE %s > 0'
    with buffered(connection, dictionary) as current:
        before = statement_counts(connection)
        result = (current.execute(sql, values, many=True) if many_method == 'execute'
                  else current.executemany(sql, values))
        assert result == 2
        counts = statement_counts(connection)
        assert counts['Client_stmt_prepare'] - before['Client_stmt_prepare'] == 1
        assert counts['Com_stmt_execute'] - before['Com_stmt_execute'] == 3
        expected = [(row(dictionary, value=10),), (), (row(dictionary, value=30),)]
        for index, rows in enumerate(expected):
            assert current.row_count == current.rowcount == current.affected_rows == len(rows)
            assert current.batch_row_count == 2
            assert current.fetchall() == rows
            assert current.nextset() is (True if index < 2 else None)
        assert current.batch_row_count == 2
        assert current.execute('SELECT %s AS replacement', (99,)) == 1
        assert current.batch_row_count == 1
        assert current.fetchall() == (row(dictionary, replacement=99),)


@requires_mysql
@pytest.mark.parametrize('dictionary', [False, True], ids=['tuple', 'dict'])
def test_prepared_zero_parameter_rows_keep_real_empty_input_roles(
    connection: Connection, dictionary: bool,
) -> None:
    with buffered(connection, dictionary) as current:
        assert current.execute('SELECT 7 AS value', [], many=True) == 1
        assert current.batch_row_count == 1
        assert current.fetchall() == (row(dictionary, value=7),)
        assert current.nextset() is None
        before = statement_counts(connection)
        assert current.executemany('SELECT 7 AS value', [(), ()]) == 2
        assert statement_counts(connection)['Com_stmt_execute'] == before['Com_stmt_execute'] + 2
        for index in range(2):
            assert current.row_count == 1 and current.batch_row_count == 2
            assert current.fetchall() == (row(dictionary, value=7),)
            assert current.nextset() is (True if index == 0 else None)


@requires_mysql
@pytest.mark.parametrize('dictionary', [False, True], ids=['tuple', 'dict'])
def test_prepared_many_insert_is_per_row_and_later_error_keeps_the_real_prefix(
    connection: Connection, dictionary: bool,
) -> None:
    with connection.cursor() as setup:
        setup.execute('CREATE TEMPORARY TABLE prepared_prefix (id INT PRIMARY KEY)')
    with buffered(connection, dictionary) as current:
        before = statement_counts(connection)
        with pytest.raises(IntegrityError) as caught:
            current.executemany('INSERT INTO prepared_prefix VALUES (%s)', [(1,), (2,), (1,), (3,)])
        assert caught.value.code == 1062 and caught.value.is_server_error is True
        counts = statement_counts(connection)
        assert counts['Client_stmt_prepare'] - before['Client_stmt_prepare'] == 1
        assert counts['Com_stmt_execute'] - before['Com_stmt_execute'] == 3
        # The failed third and unattempted fourth contributions prevent a
        # complete batch count; the two real successes remain diagnostic facts.
        assert current.batch_row_count is None
        assert any('confirmed 2 commands' in note for note in caught.value.__notes__)
        for index in range(2):
            assert current.row_count == 1
            assert current.description is None
            with pytest.raises(InterfaceError):
                current.fetchall()
            if index == 0:
                assert current.nextset() is True
        with pytest.raises(IntegrityError) as boundary:
            current.nextset()
        assert boundary.value.code == 1062
        with connection.cursor() as check:
            assert check.execute('SELECT id FROM prepared_prefix ORDER BY id') == 2
            assert check.fetchall() == ((1,), (2,))
        # A fresh explicit user execution may reuse this still valid handle;
        # the earlier failed batch must not have sent its fourth source row.
        assert current.execute('INSERT INTO prepared_prefix VALUES (%s)', (3,)) == 1
        reused = statement_counts(connection)
        assert reused['Client_stmt_prepare'] == counts['Client_stmt_prepare']
        assert reused['Com_stmt_execute'] == counts['Com_stmt_execute'] + 1
        with connection.cursor() as check:
            assert check.execute('SELECT id FROM prepared_prefix ORDER BY id') == 3
            assert check.fetchall() == ((1,), (2,), (3,))


@requires_mysql
@pytest.mark.parametrize('dictionary', [False, True], ids=['tuple', 'dict'])
def test_prepared_batch_call_counts_only_first_sets_but_exposes_every_native_set(
    connection: Connection, dictionary: bool,
) -> None:
    name = 'prepared_public_' + uuid4().hex
    with connection.cursor() as setup:
        setup.execute(f'CREATE PROCEDURE {name}(IN n INT) BEGIN SELECT n AS first; SELECT n+10 AS second; END')
    try:
        with buffered(connection, dictionary) as current:
            assert current.executemany(f'CALL {name}(%s)', [(1,), (2,)]) == 2
            expected = [(row(dictionary, first=1),), (row(dictionary, second=11),), (),
                        (row(dictionary, first=2),), (row(dictionary, second=12),), ()]
            for index, rows in enumerate(expected):
                assert current.row_count == len(rows)
                assert current.batch_row_count == 2
                if current.description is None:
                    assert rows == ()
                    with pytest.raises(InterfaceError):
                        current.fetchall()
                else:
                    assert current.fetchall() == rows
                if not rows:
                    assert current.description is None
                assert current.nextset() is (True if index < 5 else None)
    finally:
        with connection.cursor() as cleanup:
            cleanup.execute(f'DROP PROCEDURE IF EXISTS {name}')


@requires_mysql
@pytest.mark.parametrize('dictionary', [False, True], ids=['tuple', 'dict'])
def test_prepared_decode_policies_and_buffered_owners_survive_connection_close(
    connection: Connection, dictionary: bool,
) -> None:
    current = buffered(connection, dictionary)
    current.use_decimal = False
    current.use_timedelta = False
    assert current.execute('SELECT CAST(%s AS DECIMAL(12,3)) AS number,CAST(%s AS TIME(6)) AS clock',
                           (Decimal('12.125'), time(1, 2, 3, 123456))) == 1
    assert current.fetchone() == row(dictionary, number=12.125, clock=time(1, 2, 3, 123456))
    assert current.execute('SELECT %s AS retained', (42,)) == 1
    columns = current.columns
    connection.close()
    assert current.fetchall() == (row(dictionary, retained=42),)
    assert current.columns is columns
    assert current.prepared is True
    assert current.close() is None


@requires_mysql
@pytest.mark.parametrize('dictionary', [False, True], ids=['tuple', 'dict'])
def test_prepared_zero_date_uses_common_strict_and_canonical_fallback_policies(
    connection: Connection, dictionary: bool,
) -> None:
    with connection.cursor() as setup:
        setup.execute("SET SESSION sql_mode=''")
    with buffered(connection, dictionary) as current:
        current.decode_date_strict = False
        current.execute("SELECT CAST('0000-00-00' AS DATE) AS value")
        assert current.fetchone() == row(dictionary, value='0000-00-00')
        current.decode_date_strict = True
        current.execute("SELECT CAST('0000-00-00' AS DATE) AS value")
        with pytest.raises(DataError):
            current.fetchone()
        assert current.row_count == 1 and not connection.closed
