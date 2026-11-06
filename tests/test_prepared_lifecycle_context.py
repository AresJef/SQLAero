"""Prepared lifecycle facts on real MySQL and one explicitly controlled ERR.

The narrow relay below follows the existing public packet-relay tests. It
intercepts one EXECUTE before server execution and sends a complete 1243/1615
ERR. Authentication, PREPARE, CLOSE and the next explicit execution still use
the owned server. This is not a naturally occurring MySQL reprepare failure.
"""

from collections.abc import Generator
from concurrent.futures import ThreadPoolExecutor
from contextlib import contextmanager
import select
import socket
from threading import Event

import pytest
from sqlaero import Connection, SSL
from sqlaero.errors import SQLAeroError
from test_prepared_cursor import buffered, row, statement_counts
from test_public_connection import ROOT, fixture_port, live, requires_mysql


@requires_mysql
@pytest.mark.parametrize('label', ['84', '97'])
@pytest.mark.parametrize('dictionary', [False, True])
def test_actual_use_rebuilds_the_same_template_on_the_next_explicit_execution(
    label: str, dictionary: bool,
) -> None:
    with live(label) as connection, connection.cursor() as setup:
        setup.execute('SELECT DATABASE()')
        original = setup.fetchone()
        assert original is not None and isinstance(original[0], str)
        database = original[0]
        sql = 'SELECT DATABASE() AS active_schema, %s AS value'
        with buffered(connection, dictionary) as current:
            before = statement_counts(connection)
            for index, schema in enumerate((database, 'information_schema', database)):
                if index:
                    setup.execute('USE `' + schema.replace('`', '``') + '`')
                assert current.execute(sql, (index,)) == 1
                assert current.fetchone() == row(dictionary, active_schema=schema, value=index)
                counts = statement_counts(connection)
                assert counts['Client_stmt_prepare'] == before['Client_stmt_prepare'] + index + 1
                assert counts['Com_stmt_close'] == before['Com_stmt_close'] + index
                assert counts['Com_stmt_execute'] == before['Com_stmt_execute'] + index + 1
            assert connection.ping()


@requires_mysql
@pytest.mark.parametrize('label', ['84', '97'])
@pytest.mark.parametrize('dictionary', [False, True])
def test_actual_charset_and_collation_changes_rebuild_before_native_binding(
    label: str, dictionary: bool,
) -> None:
    settings = (
        ('SET NAMES utf8mb4 COLLATE utf8mb4_0900_ai_ci', 'utf8mb4', 'utf8mb4_0900_ai_ci'),
        ('SET NAMES latin1 COLLATE latin1_swedish_ci', 'latin1', 'latin1_swedish_ci'),
        ("SET SESSION collation_connection='latin1_bin'", 'latin1', 'latin1_bin'),
        ('SET NAMES utf8mb4 COLLATE utf8mb4_0900_ai_ci', 'utf8mb4', 'utf8mb4_0900_ai_ci'),
    )
    sql = 'SELECT %s AS value, CHARSET(%s) AS charset_name, COLLATION(%s) AS collation_name'
    with live(label) as connection, connection.cursor() as setup:
        with buffered(connection, dictionary) as current:
            before = statement_counts(connection)
            for index, (setting, charset, collation) in enumerate(settings):
                setup.execute(setting)
                assert current.execute(sql, ('é', 'é', 'é')) == 1
                assert current.fetchone() == row(
                    dictionary, value='é', charset_name=charset, collation_name=collation,
                )
                counts = statement_counts(connection)
                assert counts['Client_stmt_prepare'] == before['Client_stmt_prepare'] + index + 1
                assert counts['Com_stmt_close'] == before['Com_stmt_close'] + index
                assert counts['Com_stmt_execute'] == before['Com_stmt_execute'] + index + 1
            assert connection.ping()


@requires_mysql
@pytest.mark.parametrize('label', ['84', '97'])
@pytest.mark.parametrize('dictionary', [False, True])
def test_failed_replacement_prepare_closes_old_handle_but_keeps_its_buffered_result(
    label: str, dictionary: bool,
) -> None:
    sql = 'SELECT %s AS stable UNION ALL SELECT %s'
    with _lifecycle_connection(label, None) as (connection, events), buffered(connection, dictionary) as current:
        current.execute(sql, (11, 12))
        assert current.fetchone() == row(dictionary, stable=11)
        columns = current.columns
        before = statement_counts(connection)
        with pytest.raises(SQLAeroError) as caught:
            current.execute('SELECT FROM')
        assert caught.value.code == 1064 and caught.value.is_server_error is True
        assert caught.value.error_info.stage == 'cursor.execute'
        assert any('target SQL was not executed' in note for note in caught.value.__notes__)
        failed = statement_counts(connection)
        assert failed['Client_stmt_prepare'] == before['Client_stmt_prepare'] + 1
        assert failed['Com_stmt_close'] == before['Com_stmt_close'] + 1
        assert failed['Com_stmt_execute'] == before['Com_stmt_execute']
        assert columns is not None and columns.names == ('stable',)
        assert current.columns is not None and current.columns.names == ('stable',)
        assert current.fetchone() == row(dictionary, stable=12)
        assert not connection.closed
        assert current.execute(sql, (21, 22)) == 2
        assert current.fetchall() == (row(dictionary, stable=21), row(dictionary, stable=22))
        recovered = statement_counts(connection)
        assert recovered['Client_stmt_prepare'] == failed['Client_stmt_prepare'] + 1
        assert recovered['Com_stmt_close'] == failed['Com_stmt_close']
        assert recovered['Com_stmt_execute'] == failed['Com_stmt_execute'] + 1
        assert [event for event, _ in events] == [
            'prepare request', 'prepare', 'execute', 'close', 'prepare request', 'prepare ERR',
            'prepare request', 'prepare', 'execute',
        ]
        old_identifier, new_identifier = events[1][1], events[7][1]
        assert events[2][1] == events[3][1] == old_identifier
        assert events[5][1] == 1064
        assert new_identifier != old_identifier and events[8][1] == new_identifier


def _receive(peer: socket.socket, size: int) -> bytes:
    data = bytearray()
    while len(data) < size:
        block = peer.recv(size - len(data))
        if not block:
            raise EOFError('the owned lifecycle relay ended before a complete packet')
        data.extend(block)
    return bytes(data)


@contextmanager
def _lifecycle_connection(
    label: str, code: int | None,
) -> Generator[tuple[Connection, list[tuple[str, int]]], None, None]:
    """Trace real handle packets, optionally replacing one fixed EXECUTE."""
    assert code in (None, 1243, 1615)
    events: list[tuple[str, int]] = []
    stop = Event()
    with socket.socket() as listener, ThreadPoolExecutor(max_workers=1) as executor:
        listener.bind(('127.0.0.1', 0))
        endpoint = listener.getsockname()
        listener.listen(1)
        listener.settimeout(5)

        def relay() -> None:
            with listener.accept()[0] as client, socket.create_connection(
                ('127.0.0.1', fixture_port(label)), timeout=5,
            ) as server:
                client.settimeout(5)
                server.settimeout(5)
                injected = False
                preparing = False
                while not stop.is_set():
                    readable, _, _ = select.select([client, server], [], [], 0.05)
                    for peer in readable:
                        try:
                            header = _receive(peer, 4)
                            size = int.from_bytes(header[:3], 'little')
                            assert size < 1024 * 1024, 'fixed lifecycle packets exceed relay bound'
                            payload = _receive(peer, size)
                        except EOFError:
                            return
                        if peer is client:
                            if header[3] == 0 and payload[:1] == b'\x16':
                                preparing = True
                                events.append(('prepare request', 0))
                            elif header[3] == 0 and payload[:1] in (b'\x17', b'\x19'):
                                assert len(payload) >= 5
                                identifier = int.from_bytes(payload[1:5], 'little')
                                if payload[0] == 0x19:
                                    events.append(('close', identifier))
                                else:
                                    events.append(('execute', identifier))
                                    if code is not None and not injected:
                                        injected = True
                                        error = (b'\xff' + code.to_bytes(2, 'little')
                                                 + b'#HY000controlled lifecycle execute error')
                                        events.append(('injected', code))
                                        client.sendall(len(error).to_bytes(3, 'little') + b'\x01' + error)
                                        continue
                            server.sendall(header + payload)
                        else:
                            if preparing:
                                assert header[3] == 1
                                if payload[:1] == b'\xff':
                                    assert len(payload) >= 9
                                    events.append(('prepare ERR', int.from_bytes(payload[1:3], 'little')))
                                else:
                                    assert len(payload) >= 12 and payload[:1] == b'\x00'
                                    events.append(('prepare', int.from_bytes(payload[1:5], 'little')))
                                preparing = False
                            client.sendall(header + payload)

        future = executor.submit(relay)
        connection: Connection | None = None
        try:
            connection = live(
                label, unix_socket=None, host=endpoint[0], port=endpoint[1],
                ssl=SSL(mode='DISABLED'), compression=None,
                server_public_key=ROOT / label / 'data/public_key.pem',
                operation_timeout=5,
            )
            yield connection, events
        finally:
            if connection is not None:
                connection.close()
            stop.set()
            future.result(timeout=6)


@requires_mysql
@pytest.mark.parametrize('label', ['84', '97'])
@pytest.mark.parametrize('dictionary', [False, True])
@pytest.mark.parametrize('code', [1243, 1615])
def test_complete_controlled_invalid_handle_err_rebuilds_only_on_explicit_next_call(
    label: str, dictionary: bool, code: int,
) -> None:
    with _lifecycle_connection(label, code) as (connection, events):
        with buffered(connection, dictionary) as current:
            with pytest.raises(SQLAeroError) as caught:
                current.execute('SELECT %s AS value', (7,))
            assert caught.value.code == code and caught.value.is_server_error is True
            assert caught.value.message == 'controlled lifecycle execute error'
            assert not connection.closed
            # The intercepted EXECUTE was never sent to MySQL. A whole ERR
            # synchronizes the client, but must not cause an automatic replay.
            assert len(events) == 4
            assert [event for event, _ in events] == ['prepare request', 'prepare', 'execute', 'injected']
            old_identifier = events[1][1]
            assert events[2][1] == old_identifier
            events.append(('explicit next call', 0))
            assert current.execute('SELECT %s AS value', (8,)) == 1
            assert current.fetchone() == row(dictionary, value=8)
            assert [event for event, _ in events] == [
                'prepare request', 'prepare', 'execute', 'injected', 'explicit next call',
                'close', 'prepare request', 'prepare', 'execute',
            ]
            new_identifier = events[7][1]
            assert events[5][1] == old_identifier
            assert new_identifier != old_identifier and events[8][1] == new_identifier
            assert connection.ping()
