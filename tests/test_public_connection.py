"""Public connection/cursor composition against the owned MySQL fixtures."""

from collections.abc import Iterator
from datetime import datetime, timedelta, timezone
from decimal import Decimal
import inspect
import json
import os
import sys
from pathlib import Path
from typing import Any

import pytest
from sqlaero import Connection, Cursor, SSL, connect
from sqlaero.errors import InterfaceError, ProgrammingError


class PreparationReached(Exception):
    """Stop accepted configuration at the existing PathLike boundary."""


class StopBeforeNativePreparation:
    """Expose whether memory limits were checked before external preparation."""

    def __fspath__(self) -> str:
        raise PreparationReached('validated configuration reached path preparation')


@pytest.mark.parametrize('name', ['result_memory_limit', 'total_result_memory_limit'])
@pytest.mark.parametrize('value', [sys.maxsize + 1, str(sys.maxsize + 1), 2 * sys.maxsize + 1])
def test_result_memory_limit_rejects_above_signed_platform_bound(name: str, value: object) -> None:
    with pytest.raises(ValueError, match=name):
        connect(user='unopened', server_public_key_path=StopBeforeNativePreparation(),
                **{name: value})  # type: ignore[arg-type]


@pytest.mark.parametrize('name', ['result_memory_limit', 'total_result_memory_limit'])
@pytest.mark.parametrize('value', [sys.maxsize, str(sys.maxsize)])
def test_result_memory_limit_accepts_signed_bound_without_allocating_it(name: str, value: object) -> None:
    with pytest.raises(PreparationReached):
        connect(user='unopened', server_public_key_path=StopBeforeNativePreparation(),
                **{name: value})  # type: ignore[arg-type]


def test_connect_preparation_deadline_uses_the_connect_code(tmp_path: Path) -> None:
    import time as clock_time
    from sqlaero.errors import OperationalTimeoutError, SQLAeroErrorCode

    class SlowPath:
        """Consume the actual establishment budget without opening a socket."""

        def __fspath__(self) -> str:
            clock_time.sleep(1.05)
            return str(tmp_path / 'unopened-key.pem')

    with pytest.raises(OperationalTimeoutError) as caught:
        connect(user='unopened', unix_socket=tmp_path / 'unopened.sock',
                ssl=SSL(mode='DISABLED'), server_public_key_path=SlowPath(), connect_timeout=1)
    assert caught.value.code == SQLAeroErrorCode.CONNECT_DEADLINE
    assert caught.value.is_server_error is False
    assert caught.value.error_info.stage == 'connect'


def test_native_file_preparation_keeps_source_and_connect_stage(tmp_path: Path) -> None:
    import errno
    from sqlaero.errors import OperationalError, SQLAeroErrorCode

    with pytest.raises(OperationalError) as caught:
        connect(user='unopened', unix_socket=tmp_path / 'unopened.sock',
                ssl=SSL(mode='DISABLED'), server_public_key_path=tmp_path / 'missing-key.pem')
    assert caught.value.code == SQLAeroErrorCode.TRANSPORT
    info = caught.value.error_info
    assert info.stage == 'connect' and info.native_type is not None
    current = info.cause
    while current is not None and current.native_type != 'std::io::Error':
        current = current.cause
    assert current is not None and current.os_errno == errno.ENOENT
    assert current.exception_type is None and current.code is None


ROOT = Path(os.environ.get('SQLAERO_TEST_MYSQL_ROOT', '/private/tmp/sqlaero-mysql-501'))
requires_mysql = pytest.mark.skipif('SQLAERO_TEST_MYSQL_ROOT' not in os.environ,
                                   reason='requires explicitly selected owned MySQL fixtures')


def live(label: str, **overrides: Any) -> Connection:
    """Read private fixture credentials without printing or persisting them."""
    secret: dict[str, str] = json.loads((ROOT / label / 'state.json').read_text())
    settings: dict[str, Any] = {
        'user': 'sqlaero', 'password': secret['test_password'], 'database': 'sqlaero_test',
        'unix_socket': ROOT / f'{label}.sock', 'operation_timeout': 5,
    }
    settings.update(overrides)
    return connect(**settings)


@pytest.fixture(params=['84', '97'])
def connection(request: pytest.FixtureRequest) -> Iterator[Connection]:
    value = live(str(request.param))
    try:
        yield value
    finally:
        value.close()


@requires_mysql
@pytest.mark.parametrize('name', ['net_read_timeout', 'net_write_timeout', 'wait_timeout',
                                  'innodb_lock_wait_timeout'])
def test_server_timeout_failure_records_its_public_stage(connection: Connection, name: str) -> None:
    from sqlaero.errors import SQLAeroError

    with pytest.raises(SQLAeroError) as caught:
        getattr(connection, 'set_' + name)(10**100)
    assert caught.value.is_server_error is True
    assert caught.value.error_info.stage == 'connection.set_' + name
    assert name in caught.value.message
    assert connection.ping()


@requires_mysql
def test_closed_autocommit_setter_records_stage_without_rewriting_error(connection: Connection) -> None:
    connection.close()
    with pytest.raises(InterfaceError) as caught:
        connection.set_autocommit(False)
    assert caught.value.error_info.stage == 'connection.set_autocommit'
    assert caught.value.is_server_error is False


def test_public_signatures_and_input_preflight() -> None:
    assert inspect.signature(connect).parameters['host'].default == '127.0.0.1'
    assert inspect.signature(connect).parameters['user'].kind is inspect.Parameter.KEYWORD_ONLY
    with pytest.raises(TypeError):
        connect('root')  # type: ignore[call-arg]
    for option in ({'host': None}, {'port': None}, {'port': True}, {'decode_json': 1},
                   {'max_allowed_packet': None}, {'read_timeout': 0.1}, {'password': ''}):
        with pytest.raises((TypeError, ValueError)):
            connect(user='x', **option)  # type: ignore[arg-type]
    with pytest.raises(ValueError, match='conflicts'):
        connect(user='x', host='127.0.0.1', unix_socket='unopened')
    assert 'AUTO' in repr(SSL())
    for option in ({'mode': True}, {'mode': 'PREFERRED'}, {'min_tls_version': 1.3},
                   {'min_tls_version': 'TLS 1 .3'}, {'server_name': '*.example.com'},
                   {'key_password': 'do-not-print'}, {'cert_file': 'unopened'},
                   {'mode': 'DISABLED', 'server_name': 'example.com'}):
        with pytest.raises((TypeError, ValueError)):
            SSL(**option)  # type: ignore[arg-type]


@requires_mysql
def test_actual_connect_bind_and_independent_buffered_cursors(connection: Connection) -> None:
    assert connection.autocommit and connection.ping()
    assert connection.ssl_mode == 'DISABLED' and connection.tls_version is None
    assert connection.connection_id > 0 and connection.server_version
    first = connection.cursor(tuple)
    second = connection.cursor(Cursor)
    assert first.rowcount == -1 and first.row_count is None and first.description is None
    assert first.execute('SELECT %s AS n, %s AS text', [7, 'é中']) == 1
    assert first.rowcount == first.row_count == first.affected_rows == 1
    assert first.warning_count == 0 and first.lastrowid is None
    saved = first.columns
    assert saved.names == ('n', 'text')
    assert second.execute('SELECT 9 UNION ALL SELECT 10') == 2
    assert first.fetchone() == (7, 'é中')
    assert second.fetchall() == ((9,), (10,))
    assert saved.names == ('n', 'text')
    assert first.execute('SELECT %s', None) == 1
    assert first.fetchone() == (None,)
    assert first.execute("SELECT '%%', '%s'") == 1
    assert first.fetchone() == ('%%', '%s')
    assert first.execute('SELECT 42', [], many=True) == 1
    assert first.fetchall() == ((42,),)


@requires_mysql
def test_old_result_survives_local_failure_but_not_a_started_failed_request(connection: Connection) -> None:
    cursor = connection.cursor()
    cursor.execute('SELECT 1 UNION ALL SELECT 2')
    saved = cursor.columns
    with pytest.raises(ProgrammingError):
        cursor.execute('SELECT %s', [1, 2])
    assert cursor.row_count == 2 and cursor.fetchone() == (1,)
    with pytest.raises(ProgrammingError):
        cursor.execute('THIS IS NOT SQL')
    assert cursor.rowcount == -1 and cursor.row_count is None
    assert cursor.warning_count is None and cursor.description is None
    assert cursor.columns.names == ()
    assert saved.names == ('1',)
    with pytest.raises(InterfaceError):
        cursor.fetchone()
    assert not connection.closed
    cursor.execute('SELECT 3')
    assert cursor.fetchone() == (3,)


@requires_mysql
def test_decode_defaults_and_execution_snapshots_are_independent(connection: Connection) -> None:
    connection.use_decimal = False
    cursor = connection.cursor()
    connection.use_decimal = True
    assert not cursor.use_decimal
    cursor.execute('SELECT CAST(1.25 AS DECIMAL(8,2)), JSON_OBJECT(\'x\',1)')
    cursor.use_decimal = True
    cursor.decode_json = True
    assert cursor.fetchone() == (1.25, '{"x": 1}')
    cursor.execute('SELECT CAST(1.25 AS DECIMAL(8,2)), JSON_OBJECT(\'x\',1)')
    assert cursor.fetchone() == (Decimal('1.25'), {'x': 1})
    for name in ('decode_json', 'decode_set', 'decode_bit', 'use_decimal',
                 'use_timedelta', 'decode_text_strict', 'decode_date_strict'):
        with pytest.raises(TypeError):
            setattr(cursor, name, 1)
        with pytest.raises(TypeError):
            setattr(connection, name, 0)


@requires_mysql
def test_explicit_transactions_native_batching_and_close_information(connection: Connection) -> None:
    cursor = connection.cursor()
    cursor.execute('CREATE TEMPORARY TABLE public_batch (id BIGINT PRIMARY KEY, value VARCHAR(32))')
    connection.begin()
    assert cursor.executemany('INSERT INTO public_batch VALUES (%s,%s)', [(1, 'a'), (2, 'é')]) == 2
    connection.rollback()
    cursor.execute('SELECT COUNT(*) FROM public_batch')
    assert cursor.fetchone() == (0,)
    connection.begin()
    cursor.executemany('INSERT INTO public_batch VALUES (%s,%s)', [(3, 'c'), (4, 'd')])
    connection.commit()
    cursor.execute('SELECT id, value FROM public_batch ORDER BY id')
    saved = cursor.columns
    assert cursor.fetchone() == (3, 'c')
    connection.close()
    assert connection.closed and not connection.ping()
    assert cursor.fetchall() == ((4, 'd'),)
    cursor.arraysize = 10**1000
    assert cursor.arraysize == 10**1000
    cursor.close()
    assert saved.names == ('id', 'value') and cursor.row_count == 2
    assert connection.max_allowed_packet == 64 * 1024 * 1024
    assert connection.server_version and connection.connection_id
    with pytest.raises(InterfaceError, match='Cannot modify Cursor.arraysize'):
        cursor.arraysize = 2
    with pytest.raises(InterfaceError, match='Cannot modify Connection'):
        connection.decode_json = True
    with pytest.raises(InterfaceError):
        cursor.fetchone()


@requires_mysql
def test_aware_datetime_uses_actual_utc_state_and_old_metadata_keeps_encoding(connection: Connection) -> None:
    cursor = connection.cursor()
    cursor.execute('SELECT %s', datetime(2024, 1, 2, 12, tzinfo=timezone(timedelta(hours=8))))
    assert cursor.fetchone() == ('2024-01-02 04:00:00',)
    cursor.execute("SET time_zone='+08:00'")
    with pytest.raises(InterfaceError, match='UTC'):
        cursor.execute('SELECT %s', datetime(2024, 1, 2, 12, tzinfo=timezone.utc))
    cursor.execute("SET time_zone='+00:00'")
    cursor.execute('SELECT 1 AS `é`')
    saved = cursor.columns
    cursor.execute('SET NAMES latin1')
    cursor.execute("SELECT 'é' AS `é`")
    assert cursor.fetchone() == ('é',) and cursor.columns.names == ('é',)
    assert saved.names == ('é',)


@requires_mysql
def test_managed_transaction_commit_rollback_manual_segments_and_nesting(connection: Connection) -> None:
    with connection.cursor() as setup:
        setup.execute('CREATE TEMPORARY TABLE managed_tx (id INT PRIMARY KEY)')
    with connection.transaction(tuple) as cursor:
        cursor.execute('INSERT INTO managed_tx VALUES (%s)', 1)
        with pytest.raises(InterfaceError, match='scope'):
            with connection.transaction():
                pytest.fail('nested scope must not execute')
    assert cursor.closed and not connection.closed
    check = connection.cursor()
    check.execute('SELECT id FROM managed_tx')
    assert check.fetchall() == ((1,),)
    failed: Cursor | None = None
    original = ValueError('business failure identity')
    with pytest.raises(ValueError) as caught:
        with connection.transaction() as failed:
            failed.execute('INSERT INTO managed_tx VALUES (2)')
            raise original
    assert caught.value is original
    assert failed is not None and failed.closed
    check.execute('SELECT id FROM managed_tx')
    assert check.fetchall() == ((1,),)
    with connection.transaction(Cursor) as segmented:
        segmented.execute('INSERT INTO managed_tx VALUES (3)')
        connection.commit()
        connection.begin()
        segmented.execute('INSERT INTO managed_tx VALUES (4)')
    check.execute('SELECT id FROM managed_tx ORDER BY id')
    assert check.fetchall() == ((1,), (3,), (4,))
    connection.begin()
    with pytest.raises(InterfaceError, match='active server transaction'):
        with connection.transaction():
            pytest.fail('existing transaction must not be replaced')
    connection.rollback()
    with connection.transaction():
        pass


@requires_mysql
def test_client_properties_and_remote_timeouts_have_distinct_effects(connection: Connection) -> None:
    for getter, setter in (
        ('net_read_timeout', connection.set_net_read_timeout),
        ('net_write_timeout', connection.set_net_write_timeout),
        ('wait_timeout', connection.set_wait_timeout),
        ('innodb_lock_wait_timeout', connection.set_innodb_lock_wait_timeout),
    ):
        assert isinstance(getattr(connection, getter), int)
        setter(17)
        assert getattr(connection, getter) == 17
        setter(17)
        for bad in (None, True, 1.1, '2'):
            with pytest.raises(TypeError):
                setter(bad)  # type: ignore[arg-type]
        for bad in (0, -1):
            with pytest.raises(ValueError):
                setter(bad)
        assert getattr(Connection, getter).__doc__
    cursor = connection.cursor()
    cursor.execute('SET SESSION net_read_timeout=18')
    assert connection.net_read_timeout == 18
    connection.read_timeout = 2
    connection.write_timeout = 3
    connection.operation_timeout = 4
    assert (connection.read_timeout, connection.write_timeout, connection.operation_timeout) == (2, 3, 4)
    cursor.execute('SELECT @@SESSION.net_read_timeout,@@SESSION.net_write_timeout')
    assert cursor.fetchone() == (18, 17)
    connection.close()
    assert connection.net_read_timeout == 18
    with pytest.raises(InterfaceError):
        connection.set_net_read_timeout(18)


@requires_mysql
@pytest.mark.parametrize('label', ['84', '97'])
def test_public_tcp_and_unix_tls_share_prepared_materials(label: str) -> None:
    configuration = SSL(ca_file=ROOT / 'tls/ca.pem', mode='VERIFY_IDENTITY',
                        min_tls_version='TLS 1.3', server_name='localhost')
    with live(label, unix_socket=None, host='127.0.0.1',
              port=13384 if label == '84' else 13397, ssl=configuration) as tcp:
        assert tcp.ssl_mode == 'VERIFY_IDENTITY' and tcp.tls_version == 'TLSv1.3'
        with tcp.cursor() as cursor:
            cursor.execute('SELECT %s', b'\x00\xff')
            assert cursor.fetchone() == (b'\x00\xff',)
    with live(label, ssl=configuration) as unix:
        assert unix.ssl_mode == 'VERIFY_IDENTITY' and unix.ping()
    assert tcp.closed and unix.closed


@requires_mysql
def test_failed_routine_cannot_leave_a_false_utc_fact(connection: Connection) -> None:
    from sqlaero.errors import SQLAeroError
    cursor = connection.cursor()
    cursor.execute('DROP PROCEDURE IF EXISTS sqlaero_public_set_error')
    cursor.execute("CREATE PROCEDURE sqlaero_public_set_error() BEGIN SET time_zone='+08:00'; SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT='state changed before failure'; END")
    with pytest.raises(SQLAeroError):
        cursor.execute('CALL sqlaero_public_set_error()')
    cursor.execute('DROP PROCEDURE sqlaero_public_set_error')
    cursor.execute('SELECT @@SESSION.time_zone')
    assert cursor.fetchone() == ('+08:00',)
    with pytest.raises(InterfaceError, match='UTC'):
        cursor.execute('SELECT %s', datetime(2024, 1, 1, tzinfo=timezone.utc))
    cursor.execute("SET time_zone='+00:00'")
    cursor.execute('SELECT %s', datetime(2024, 1, 1, tzinfo=timezone.utc))
    assert cursor.fetchone() == ('2024-01-01 00:00:00',)


@requires_mysql
@pytest.mark.parametrize('encoding', ['utf16', 'utf16le', 'ucs2', 'utf32'])
def test_context_confirmation_uses_its_actual_server_only_encoding(connection: Connection, encoding: str) -> None:
    cursor = connection.cursor()
    cursor.execute(f'SET character_set_results={encoding}')
    with pytest.raises(ProgrammingError):
        cursor.execute('THIS IS NOT SQL')
    cursor.execute("SELECT %s AS `é`", 'é中')
    assert cursor.fetchone() == ('é中',)
    assert cursor.columns.names == ('é',)
    assert cursor.columns[0].catalog == 'def'
    cursor.execute('SET character_set_results=NULL')
    with pytest.raises(ProgrammingError):
        cursor.execute('THIS IS STILL NOT SQL')
    cursor.execute('SELECT 1 AS `é`')
    assert cursor.fetchone() == (1,) and cursor.columns.names == ('é',)


@requires_mysql
def test_unexpected_multi_results_are_drained_and_update_actual_session_facts(connection: Connection) -> None:
    from sqlaero.errors import NotSupportedError
    cursor = connection.cursor()
    cursor.execute('DROP PROCEDURE IF EXISTS sqlaero_public_multi')
    cursor.execute("CREATE PROCEDURE sqlaero_public_multi() BEGIN SELECT 1; SET time_zone='+08:00'; SELECT 2; END")
    try:
        with pytest.raises(NotSupportedError):
            cursor.execute('CALL sqlaero_public_multi()')
        assert not connection.closed and cursor.columns.names == ()
        with pytest.raises(InterfaceError, match='UTC'):
            cursor.execute('SELECT %s', datetime(2024, 1, 1, tzinfo=timezone.utc))
        cursor.execute('SELECT @@SESSION.time_zone')
        assert cursor.fetchone() == ('+08:00',)
    finally:
        cursor.execute('DROP PROCEDURE sqlaero_public_multi')


@requires_mysql
@pytest.mark.parametrize('label', ['84', '97'])
def test_native_complete_row_splitting_and_failed_call_prefix(label: str) -> None:
    from sqlaero.errors import SQLAeroError
    with live(label, max_allowed_packet='2KB') as connection:
        cursor = connection.cursor()
        cursor.execute('CREATE TEMPORARY TABLE split_public (id INT PRIMARY KEY,value VARCHAR(128))')
        rows = [(index, 'x' * 100) for index in range(100)]
        assert cursor.executemany('INSERT INTO split_public VALUES (%s,%s)', rows) == 100
        assert cursor.row_count == 100 and cursor.warning_count == 0
        cursor.execute('SELECT COUNT(*) FROM split_public')
        assert cursor.fetchone() == (100,)
        cursor.execute('TRUNCATE TABLE split_public')
        broken: list[object] = [(index, 'x' * 100) for index in range(100)]
        broken[70] = (70, object())
        with pytest.raises(TypeError) as caught:
            cursor.executemany('INSERT INTO split_public VALUES (%s,%s)', broken)  # type: ignore[arg-type]
        assert cursor.row_count is None and cursor.rowcount == -1 and cursor.description is None
        assert any('confirmed ' in note for note in getattr(caught.value, '__notes__', []))
        cursor.execute('SELECT COUNT(*) FROM split_public')
        count = cursor.fetchone()
        assert count is not None and isinstance(count[0], int) and 0 < count[0] < 70
        # A server error also leaves the healthy wire usable, while aggregate
        # failed-call counters do not report the already confirmed prefix.
        with pytest.raises(SQLAeroError):
            cursor.execute('INSERT INTO split_public VALUES (%s,%s)', (0, 'duplicate'))
        assert cursor.row_count is None
        cursor.execute('SELECT 42')
        assert cursor.fetchone() == (42,)


@requires_mysql
def test_network_wait_releases_gil_and_does_not_block_local_other_cursor_reads(connection: Connection) -> None:
    from concurrent.futures import ThreadPoolExecutor
    from threading import Event
    import time as clock_time
    from sqlaero.errors import InterfaceBusyError
    buffered = connection.cursor()
    buffered.execute('SELECT 5 UNION ALL SELECT 6')
    network = connection.cursor()
    started = Event()

    def operation() -> int:
        started.set()
        return network.execute('SELECT SLEEP(1)')

    with ThreadPoolExecutor(max_workers=1) as executor:
        future = executor.submit(operation)
        assert started.wait(2)
        clock_time.sleep(0.03)
        assert buffered.fetchall() == ((5,), (6,))
        with pytest.raises(InterfaceBusyError):
            connection.begin()
        with pytest.raises(InterfaceBusyError):
            network.execute('SELECT 99')
        assert future.result(timeout=3) == 1
    assert network.fetchone() == (0,)
    assert connection.ping()


@requires_mysql
@pytest.mark.parametrize('results', ['utf8mb4', 'latin1', 'utf16', 'utf16le', 'ucs2', 'utf32', 'binary', 'NULL'])
def test_real_error_text_follows_results_encoding_without_replacement(connection: Connection, results: str) -> None:
    from sqlaero.errors import MySQLErrorCode
    cursor = connection.cursor()
    cursor.execute(f'SET character_set_results={results}')
    cursor.execute('SELECT 1 AS `é`')
    assert cursor.columns.names == ('é',)
    with pytest.raises(ProgrammingError) as caught:
        cursor.execute('SELECT `é`')
    assert caught.value.code == MySQLErrorCode.ER_BAD_FIELD_ERROR
    assert type(caught.value.code) is int and caught.value.is_server_error is True
    # Both frozen server lines truncate converted ERR messages at the first
    # NUL byte. Keep the actual received text; never invent the missing bytes
    # or replace ProgrammingError with a diagnostic decoding exception.
    if results in {'utf16', 'ucs2', 'utf32'}:
        assert caught.value.message == ''
    elif results == 'utf16le':
        assert caught.value.message == 'U'
        assert any('received bytes' in note for note in getattr(caught.value, '__notes__', []))
    else:
        assert 'é' in caught.value.message and '\x00' not in caught.value.message
    assert any('42S22' in note for note in getattr(caught.value, '__notes__', []))


@requires_mysql
def test_known_code_priority_unknown_fallback_and_ordinary_warnings(connection: Connection) -> None:
    from sqlaero.errors import DataError, DatabaseError, IntegrityError, OperationalError, SQLAeroError
    cursor = connection.cursor()
    for code, state, expected in [(1062, '45000', IntegrityError), (1265, '45000', DataError),
                                  (65535, '23000', IntegrityError), (65535, '08000', OperationalError),
                                  (65535, '40001', DatabaseError), (65535, 'HY000', DatabaseError)]:
        with pytest.raises(SQLAeroError) as caught:
            cursor.execute(f"SIGNAL SQLSTATE '{state}' SET MYSQL_ERRNO={code},MESSAGE_TEXT='native mapping probe'")
        assert type(caught.value) is expected
        assert caught.value.code == code and caught.value.is_server_error is True
    cursor.execute('DROP TABLE IF EXISTS sqlaero_public_no_such_warning_table')
    assert cursor.warning_count is not None and cursor.warning_count > 0
    assert connection.ping()


@requires_mysql
def test_transaction_keeps_original_primary_refreshes_snapshot_and_captures_rollback_failure(connection: Connection) -> None:
    from sqlaero.errors import SQLAeroError
    primary = SQLAeroError('business failure', code=777777, is_server_error=False)
    caught: pytest.ExceptionInfo[SQLAeroError] | None = None
    previous = None
    label = '84' if connection.server_version.startswith('8.') else '97'
    with live(label) as killer:
        with pytest.raises(SQLAeroError) as caught:
            with connection.transaction():
                try:
                    raise primary
                except SQLAeroError as error:
                    previous = error.error_info
                    with killer.cursor() as control:
                        control.execute(f'KILL CONNECTION {connection.connection_id}')
                    raise
    assert caught is not None and caught.value is primary and previous is not None
    current = primary.error_info
    assert current is not previous and primary.error_info is current
    assert previous.additional_errors == ()
    assert current.message == primary.message and current.code == 777777
    assert current.traceback is not None and previous.traceback is not None
    assert len(current.additional_errors) == 1
    rollback = current.additional_errors[0]
    assert rollback.stage == 'transaction.rollback'
    assert rollback.exception_type is not None and issubclass(rollback.exception_type, SQLAeroError)
    assert rollback.is_server_error in (True, False)
    assert rollback.code is not None and rollback.message
    assert current.additional_errors is current.additional_errors
    assert connection.closed


@requires_mysql
def test_external_business_exception_keeps_identity_and_only_receives_notes(connection: Connection) -> None:
    primary = ValueError('user failure')
    caught: pytest.ExceptionInfo[ValueError] | None = None
    label = '84' if connection.server_version.startswith('8.') else '97'
    with live(label) as killer:
        with pytest.raises(ValueError) as caught:
            with connection.transaction():
                with killer.cursor() as control:
                    control.execute(f'KILL CONNECTION {connection.connection_id}')
                raise primary
    assert caught is not None and caught.value is primary and primary.args == ('user failure',)
    assert not hasattr(primary, 'error_info')
    assert any('transaction.rollback' in note for note in getattr(primary, '__notes__', []))
    assert connection.closed


@requires_mysql
def test_actual_native_socket_error_keeps_os_cause_without_fabricated_python_frames() -> None:
    import socket
    from sqlaero.errors import OperationalError
    # Reserve then release an ephemeral local port. On this macOS host a bound
    # non-listening socket silently drops SYNs (also with Python's socket API),
    # whereas the now-closed port produces the actual refusal tested here.
    with socket.socket() as reserved:
        reserved.bind(('127.0.0.1', 0))
        port = int(reserved.getsockname()[1])
        reserved.close()
        with pytest.raises(OperationalError) as caught:
            connect(host='127.0.0.1', port=port, user='unopened', connect_timeout=1)
    info = caught.value.error_info
    assert info.exception_type is OperationalError and info.is_server_error is False
    assert info.os_errno is None
    current = info.cause
    while current is not None and current.native_type != 'std::io::Error':
        current = current.cause
    assert current is not None and current.os_errno is not None
    assert current.exception_type is None and current.code is None and current.is_server_error is None
    assert current.traceback is None and current.message


@requires_mysql
def test_independent_cursor_then_connection_cleanup_failures_keep_actual_order(connection: Connection) -> None:
    from concurrent.futures import Future, ThreadPoolExecutor
    from threading import Event
    import time as clock_time
    from sqlaero.errors import InterfaceBusyError, SQLAeroError
    primary = SQLAeroError('body failure')
    cursor = connection.cursor()
    started = Event()
    future: Future[int] | None = None

    def occupied() -> int:
        started.set()
        return cursor.execute('SELECT SLEEP(0.3)')

    with ThreadPoolExecutor(max_workers=1) as executor:
        with pytest.raises(SQLAeroError) as caught:
            with connection, cursor:
                future = executor.submit(occupied)
                assert started.wait(2)
                clock_time.sleep(0.03)
                raise primary
        assert caught.value is primary
        later = primary.error_info.additional_errors
        assert tuple(item.stage for item in later) == ('cursor.close', 'connection.close')
        assert all(item.exception_type is InterfaceBusyError for item in later)
        assert len(later) == 2 and later is primary.error_info.additional_errors
        assert future is not None and future.result(timeout=3) == 1
    assert not cursor.closed and not connection.closed and connection.ping()
    cursor.close()


@requires_mysql
def test_fixed_seed_values_round_trip_through_the_public_request_path(connection: Connection) -> None:
    import random
    from datetime import date, time
    seed = 20261026
    generator = random.Random(seed)
    cursor = connection.cursor()
    alphabet = "Az09'\\\x00\n\r\x1a\t%_é中😀"
    texts = [''.join(generator.choices(alphabet, k=generator.randrange(65))) for _ in range(40)]
    integers = [generator.randrange(-(10**65 - 1), 10**65) for _ in range(20)]
    for index, value in enumerate([*texts, *integers, None, True, False, 0.1, -0.0, 1.25e30]):
        cursor.execute('SELECT %s', (value,))
        row = cursor.fetchone()
        assert row is not None and row[0] == value, (seed, index, value, row)
        if type(value) is float:
            assert isinstance(row[0], float) and row[0].hex() == value.hex(), (seed, index, value, row)
    calendar_values = [date(2024, 2, 29), datetime(2024, 2, 29, 23, 59, 59, 123456),
                       datetime(2024, 1, 1, tzinfo=timezone(timedelta(hours=5, minutes=30)))]
    for value in calendar_values:
        kind = 'DATE' if type(value) is date else 'DATETIME(6)'
        cursor.execute(f'SELECT CAST(%s AS {kind})', (value,))
        expected = value
        if isinstance(value, datetime) and value.tzinfo is not None:
            expected = value.astimezone(timezone.utc).replace(tzinfo=None)
        assert cursor.fetchone() == (expected,)
    cursor.use_timedelta = False
    cursor.execute('SELECT CAST(%s AS TIME(6)),CAST(%s AS BINARY),CAST(%s AS BINARY)',
                   (time(3, 4, 5, 123456), b'\x00\xffquote\'\\', bytearray(b'\x80\x00')))
    assert cursor.fetchone() == (time(3, 4, 5, 123456), b'\x00\xffquote\'\\', b'\x80\x00')
    cursor.use_timedelta = True
    cursor.execute('SELECT CAST(%s AS TIME(6)),CAST(%s AS DECIMAL(65,30)),%s,%s',
                   (timedelta(hours=-25, microseconds=1), Decimal('1234.5678901234567890123456789'),
                    float('inf'), Decimal('sNaN')))
    assert cursor.fetchone() == (timedelta(hours=-25, microseconds=1),
                                Decimal('1234.5678901234567890123456789'), None, None)
    for index in range(32):
        rows: list[list[int | str] | tuple[int, str]] = []
        for _ in range(generator.randrange(1, 9)):
            pair = (generator.randrange(-1000, 1000), ''.join(generator.choices(alphabet, k=8)))
            rows.append(list(pair) if generator.randrange(2) else pair)
        rows.append(rows[0])  # Shared identity is a DAG, not a cycle.
        target = tuple(rows[0]) if index % 2 else (-99999999, 'not in these rows')
        outer = tuple(rows) if generator.randrange(2) else rows
        cursor.execute('SELECT (%s,%s) IN %s', (*target, outer))
        assert cursor.fetchone() == (index % 2,), (seed, index, rows, target)


@requires_mysql
def test_real_itemize_many_shapes_preserve_counts_and_final_result(connection: Connection) -> None:
    cursor = connection.cursor()
    assert cursor.execute('SELECT 1 IN %s', [1, 2, 3], itemize=False) == 1
    assert cursor.fetchone() == (1,)
    shared = [1, 2]
    cursor.execute('SELECT 1 IN %s,2 IN %s', (shared, shared))
    assert cursor.fetchone() == (1, 1)
    assert cursor.execute('SELECT %s,%s', [(1, 'a'), [2, 'b'], (3, 'c')], many=True) == 3
    assert cursor.row_count == 3 and cursor.fetchall() == ((3, 'c'),)
    assert cursor.executemany('SELECT 8', [(), ()]) == 2
    assert cursor.row_count == 2 and cursor.fetchone() == (8,)
    assert cursor.execute('SELECT 9', (), many=True, itemize=False) == 1
    assert cursor.fetchone() == (9,)


@requires_mysql
@pytest.mark.parametrize('size', [0xFFFFFF - 10, 0xFFFFFF + 7], ids=['exact-request-frame', 'split-request-and-result'])
def test_actual_public_physical_frame_boundaries(connection: Connection, size: int) -> None:
    # COM_QUERY + SELECT-space + two SQL quotes is 10 bytes, so the first
    # request ends exactly on the real 0xFFFFFF boundary (empty terminator).
    # The second case also requires a physically split result-row message.
    connection.operation_timeout = 20
    value = 'x' * size
    cursor = connection.cursor()
    assert cursor.execute('SELECT %s', (value,)) == 1
    row = cursor.fetchone()
    assert row == (value,) and cursor.fetchone() is None
    assert connection.ping()


@requires_mysql
def test_transactions_are_observed_independently_and_implicit_commit_is_not_undone(connection: Connection) -> None:
    label = '84' if connection.server_version.startswith('8.') else '97'
    table = f'b0_visibility_{connection.connection_id}'
    extra = f'{table}_ddl'
    cursor = connection.cursor()
    cursor.execute(f'CREATE TABLE {table} (id INT PRIMARY KEY) ENGINE=InnoDB')
    try:
        with live(label) as observer:
            check = observer.cursor()
            with connection.transaction() as current:
                current.execute(f'INSERT INTO {table} VALUES (1)')
                check.execute(f'SELECT COUNT(*) FROM {table}')
                assert check.fetchone() == (0,)
            check.execute(f'SELECT id FROM {table}')
            assert check.fetchall() == ((1,),)
            with pytest.raises(ValueError, match='rollback this insert'):
                with connection.transaction() as current:
                    current.execute(f'INSERT INTO {table} VALUES (2)')
                    raise ValueError('rollback this insert')
            check.execute(f'SELECT id FROM {table}')
            assert check.fetchall() == ((1,),)
            with pytest.raises(ValueError, match='DDL already committed'):
                with connection.transaction() as current:
                    current.execute(f'INSERT INTO {table} VALUES (3)')
                    current.execute(f'CREATE TABLE {extra} (id INT) ENGINE=InnoDB')
                    raise ValueError('DDL already committed')
            check.execute(f'SELECT id FROM {table} ORDER BY id')
            assert check.fetchall() == ((1,), (3,))
    finally:
        cursor.execute(f'DROP TABLE IF EXISTS {extra}')
        cursor.execute(f'DROP TABLE IF EXISTS {table}')


@requires_mysql
@pytest.mark.parametrize('kind', ['read', 'operation'])
def test_actual_timeout_uses_entry_snapshot_and_retires_incomplete_response(connection: Connection, kind: str) -> None:
    from concurrent.futures import ThreadPoolExecutor
    from threading import Event
    import time as clock_time
    from sqlaero.errors import OperationalTimeoutError, SQLAeroErrorCode
    connection.read_timeout = 1 if kind == 'read' else None
    connection.operation_timeout = 5 if kind == 'read' else 1
    cursor = connection.cursor()
    started = Event()

    def operation() -> int:
        started.set()
        return cursor.execute('SELECT SLEEP(3)')

    before = clock_time.monotonic()
    with ThreadPoolExecutor(max_workers=1) as executor:
        future = executor.submit(operation)
        assert started.wait(2)
        clock_time.sleep(0.05)
        connection.read_timeout = None
        connection.operation_timeout = None
        with pytest.raises(OperationalTimeoutError) as caught:
            future.result(timeout=4)
    elapsed = clock_time.monotonic() - before
    expected = SQLAeroErrorCode.READ_DEADLINE if kind == 'read' else SQLAeroErrorCode.OPERATION_DEADLINE
    assert caught.value.code == expected and caught.value.is_server_error is False
    assert not isinstance(caught.value, TimeoutError) and 0.8 <= elapsed < 3
    assert connection.closed and not connection.ping()
    assert connection.read_timeout is None and connection.operation_timeout is None
    assert cursor.row_count is None and cursor.description is None


@requires_mysql
@pytest.mark.parametrize('label', ['84', '97'])
@pytest.mark.parametrize('mode', ['default', 'custom'])
def test_real_sigint_in_child_preserves_original_error_and_independent_buffer(label: str, mode: str) -> None:
    import subprocess
    import sys
    code = r'''
import json, os, signal, sys, threading, time
from pathlib import Path
from types import FrameType
from typing import Any
from sqlaero import connect
from sqlaero.errors import OperationalError
root = Path(sys.argv[1]); label = sys.argv[2]; mode = sys.argv[3]
secret: dict[str, str] = json.loads((root / label / 'state.json').read_text())
connection = connect(user='sqlaero', password=secret['test_password'], database='sqlaero_test',
                     unix_socket=root / f'{label}.sock')
retained = connection.cursor(); retained.execute('SELECT 7')
cursor = connection.cursor()
original: OperationalError | None = OperationalError('original signal handler', code=777888, is_server_error=False) if mode == 'custom' else None

def interrupted(signum: int, frame: FrameType | None) -> None:
    assert original is not None
    raise original

signal.signal(signal.SIGINT, interrupted if mode == 'custom' else signal.default_int_handler)
timer = threading.Timer(0.1, os.kill, args=(os.getpid(), signal.SIGINT))
start = time.monotonic(); timer.start()
try:
    cursor.execute('SELECT SLEEP(5)')
except BaseException as error:
    elapsed = time.monotonic() - start
    assert elapsed < 2.0
    if mode == 'custom':
        assert error is original and isinstance(error, OperationalError) and error.code == 777888
    else:
        assert isinstance(error, KeyboardInterrupt)
    assert connection.closed and not connection.ping()
    assert cursor.row_count is None and retained.fetchone() == (7,)
    proof: dict[str, Any] = {'label': label, 'mode': mode, 'elapsed': elapsed,
                             'exception': type(error).__name__, 'closed': connection.closed}
    print(json.dumps(proof))
else:
    raise AssertionError('the actual SIGINT did not interrupt the native wait')
finally:
    timer.cancel(); timer.join(); connection.close()
'''
    completed = subprocess.run([sys.executable, '-c', code, str(ROOT), label, mode],
                               capture_output=True, text=True, timeout=8, check=False)
    assert completed.returncode == 0, (completed.stdout, completed.stderr)
    proof: dict[str, object] = json.loads(completed.stdout)
    assert proof['closed'] is True and proof['mode'] == mode and proof['label'] == label
