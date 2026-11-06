"""Public §28 budgets across one real streamed response and transaction exit.

The fixed local relay follows test_public_connection's packet relay. It delays
one known row and control replies; its COMMIT-error case asks the owned server
to return a real SIGNAL ERR instead of executing COMMIT. It is not evidence of
a naturally occurring server COMMIT failure or a transaction rollback oracle.
"""

from collections.abc import Callable, Generator
from concurrent.futures import ThreadPoolExecutor
from contextlib import contextmanager
from dataclasses import dataclass, field
import select
import socket
import sys
from threading import Event
import time

import pytest
from sqlaero import Connection, SSL
from sqlaero.errors import OperationalTimeoutError, SQLAeroError
from test_public_connection import ROOT, fixture_port, live, requires_mysql


QUERY = b"SELECT 7 AS d_budget_value"
COMMIT = b"COMMIT AND NO CHAIN NO RELEASE"
ROLLBACK = b"ROLLBACK AND NO CHAIN NO RELEASE"
COMMIT_ERROR = (b"SIGNAL SQLSTATE '45000' SET MYSQL_ERRNO=1644, "
                b"MESSAGE_TEXT='d budget controlled COMMIT failure'")


@dataclass
class _Trace:
    start_exit: Event = field(default_factory=Event)
    events: list[tuple[str, float]] = field(default_factory=lambda: list[tuple[str, float]]())

    def mark(self, name: str) -> None:
        self.events.append((name, time.monotonic()))

    def names(self) -> list[str]:
        return [name for name, _ in self.events]


def _receive(peer: socket.socket, size: int) -> bytes:
    result = bytearray()
    while len(result) < size:
        block = peer.recv(size - len(result))
        if not block:
            raise EOFError('the owned test connection ended')
        result.extend(block)
    return bytes(result)


@contextmanager
def _delayed_connection(label: str, *, row_delay: float, reply_delay: float,
                        commit_error_delay: float | None = None
                        ) -> Generator[tuple[Connection, _Trace], None, None]:
    """Control only the three fixed packets needed for these budget tests."""
    trace = _Trace()
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
                row_packets: int | None = None
                control: bytes | None = None
                while not stop.is_set():
                    readable, _, _ = select.select([client, server], [], [], 0.05)
                    for peer in readable:
                        try:
                            header = _receive(peer, 4)
                            size = int.from_bytes(header[:3], 'little')
                            assert size < 1024 * 1024, 'fixed fixture packet exceeded relay bound'
                            payload = _receive(peer, size)
                        except EOFError:
                            return
                        if peer is client:
                            if header[3] == 0 and payload[:1] == b'\x03':
                                sql = payload[1:]
                                if sql == QUERY:
                                    trace.mark('SELECT sent')
                                    row_packets = 0
                                elif sql in (COMMIT, ROLLBACK):
                                    control = sql
                                    trace.mark('COMMIT sent' if sql == COMMIT else 'ROLLBACK sent')
                                    if sql == COMMIT and commit_error_delay is not None:
                                        payload = b'\x03' + COMMIT_ERROR
                                        header = len(payload).to_bytes(3, 'little') + header[3:]
                            server.sendall(header + payload)
                        else:
                            delay = 0.0
                            if row_packets is not None:
                                row_packets += 1
                                if row_packets == 3:
                                    assert payload == b'\x017', 'expected the fixed first row after its header'
                                    trace.mark('row held')
                                    assert trace.start_exit.wait(5), 'stream Header did not return before the row'
                                    delay = row_delay
                                    row_packets = None
                            if control is not None:
                                if control == COMMIT and commit_error_delay is not None:
                                    assert payload[:3] == b'\xffl\x06', 'expected the real server 1644 ERR'
                                    trace.mark('COMMIT ERR held')
                                    delay = commit_error_delay
                                else:
                                    assert payload[:1] == b'\x00', 'expected the real control OK'
                                    trace.mark('control OK held')
                                    delay = reply_delay
                                control = None
                            if stop.wait(delay):
                                return
                            try:
                                client.sendall(header + payload)
                            except (BrokenPipeError, ConnectionResetError):
                                return
                            if delay:
                                trace.mark('delayed packet delivered')

        future = executor.submit(relay)
        connection: Connection | None = None
        try:
            connection = live(label, unix_socket=None, host=endpoint[0], port=endpoint[1],
                              ssl=SSL(mode='DISABLED'), compression=None,
                              server_public_key=ROOT / label / 'data/public_key.pem',
                              read_timeout=None, write_timeout=None, operation_timeout=5)
            yield connection, trace
        finally:
            stop.set()
            trace.start_exit.set()
            if connection is not None:
                connection.close()
            future.result(timeout=6)


def _record(record_property: Callable[[str, object], None], trace: _Trace,
            elapsed: float) -> None:
    record_property('actual_gil', getattr(sys, '_is_gil_enabled', lambda: True)())
    record_property('exit_elapsed_seconds', elapsed)
    record_property('packet_events', trace.events)


@requires_mysql
@pytest.mark.parametrize('label', ['84', '97'])
def test_old_stream_remaining_cap_cannot_be_extended_by_transaction_exit(
    label: str, record_property: Callable[[str, object], None],
) -> None:
    with _delayed_connection(label, row_delay=1.5, reply_delay=0) as (connection, trace):
        scope = connection.transaction(streaming=True)
        cursor = scope.__enter__()
        connection.operation_timeout = 0.6
        assert cursor.execute(QUERY.decode('ascii')) is None
        connection.operation_timeout = 2.4
        before = time.monotonic()
        trace.start_exit.set()
        with pytest.raises(OperationalTimeoutError) as caught:
            scope.__exit__(None, None, None)
        elapsed = time.monotonic() - before
        _record(record_property, trace, elapsed)
        assert elapsed < 1.3, 'exit must not replace the old 0.6-second command allowance'
        assert caught.value.is_server_error is False
        assert cursor.closed and connection.closed
        assert trace.names() == ['SELECT sent', 'row held']
        assert 'COMMIT was not attempted' in '\n'.join(caught.value.__notes__)


@requires_mysql
@pytest.mark.parametrize('label', ['84', '97'])
@pytest.mark.parametrize('rollback', [False, True])
def test_drain_and_control_share_the_single_exit_allowance(
    label: str, rollback: bool, record_property: Callable[[str, object], None],
) -> None:
    with _delayed_connection(label, row_delay=0.9, reply_delay=1.0) as (connection, trace):
        scope = connection.transaction(streaming=True)
        cursor = scope.__enter__()
        assert cursor.execute(QUERY.decode('ascii')) is None
        connection.operation_timeout = 1.5
        primary = RuntimeError('d budget original business failure') if rollback else None
        before = time.monotonic()
        trace.start_exit.set()
        if primary is None:
            with pytest.raises(OperationalTimeoutError) as caught:
                scope.__exit__(None, None, None)
            notes = '\n'.join(caught.value.__notes__)
            assert caught.value.error_info.stage == 'transaction.exit'
            assert 'commit outcome is unknown' in notes.lower()
        else:
            assert scope.__exit__(type(primary), primary, None) is False
            notes = '\n'.join(primary.__notes__)
            assert 'transaction.rollback' in notes
            assert 'timeout' in notes.lower() or 'deadline' in notes.lower()
        elapsed = time.monotonic() - before
        _record(record_property, trace, elapsed)
        assert 1.2 < elapsed < 2.6
        assert cursor.closed and connection.closed
        names = trace.names()
        assert names == ['SELECT sent', 'row held', 'delayed packet delivered',
                         'ROLLBACK sent' if rollback else 'COMMIT sent', 'control OK held']
        assert cursor.row_count == cursor.batch_row_count == 1


@requires_mysql
@pytest.mark.parametrize('label', ['84', '97'])
def test_confirmed_commit_err_and_needed_rollback_share_exit_remaining_budget(
    label: str, record_property: Callable[[str, object], None],
) -> None:
    with _delayed_connection(label, row_delay=0.45, reply_delay=1.1,
                             commit_error_delay=0.7) as (connection, trace):
        scope = connection.transaction(streaming=True)
        cursor = scope.__enter__()
        assert cursor.execute(QUERY.decode('ascii')) is None
        connection.operation_timeout = 1.8
        before = time.monotonic()
        trace.start_exit.set()
        with pytest.raises(SQLAeroError) as caught:
            scope.__exit__(None, None, None)
        elapsed = time.monotonic() - before
        _record(record_property, trace, elapsed)
        primary = caught.value
        assert primary.code == 1644 and primary.is_server_error is True
        assert primary.message == 'd budget controlled COMMIT failure'
        assert primary.error_info.stage == 'transaction.exit'
        rollback_errors = [error for error in primary.error_info.additional_errors
                           if error.stage == 'transaction.rollback']
        assert len(rollback_errors) == 1
        assert rollback_errors[0].is_server_error is False
        assert rollback_errors[0].exception_type is OperationalTimeoutError
        assert 1.5 < elapsed < 2.8
        assert cursor.closed and connection.closed
        assert trace.names() == ['SELECT sent', 'row held', 'delayed packet delivered',
                                 'COMMIT sent', 'COMMIT ERR held', 'delayed packet delivered',
                                 'ROLLBACK sent', 'control OK held']
        assert cursor.row_count == cursor.batch_row_count == 1
