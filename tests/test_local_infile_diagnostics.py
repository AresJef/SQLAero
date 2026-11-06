"""Public LOCAL failure provenance through a finite owned upload relay.

The existing native short-read control opens 100000 bytes, then truncates to
50000. This relay applies that mutation only after seeing the actual LOAD
request, so the public Source has already opened and fixed its original length.
It forwards the real server's request/ACK and records counts, never file data.
"""

from collections.abc import Generator
from concurrent.futures import ThreadPoolExecutor
from contextlib import contextmanager
from dataclasses import dataclass, field
from pathlib import Path
import select
import socket
from threading import Event
from typing import cast

import pytest
from sqlaero import Connection, SSL
from test_local_infile import load_sql, local_server as local_server
# Deliberately reuse the existing finite test helper rather than copy its I/O.
from test_prepared_lifecycle_context import _receive  # pyright: ignore[reportPrivateUsage]
from test_public_connection import ROOT, fixture_port, live, requires_mysql


CANARY = 'D_LOCAL_CANARY_17'
ROW = b'1\tD_LOCAL_CANARY_17\n'


@dataclass
class _Trace:
    query: bool = False
    requested: bool = False
    bytes: int = 0
    eof: bool = False
    boundary: Event = field(default_factory=Event)


@contextmanager
def _upload_connection(
    label: str, source: Path, *, truncate: bool,
) -> Generator[tuple[Connection, _Trace], None, None]:
    """Intercept only one fixed LOAD, without guessing a file from its label."""
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
                uploading = False
                while not stop.is_set():
                    readable, _, _ = select.select([client, server], [], [], 0.05)
                    for peer in readable:
                        try:
                            header = _receive(peer, 4)
                            size = int.from_bytes(header[:3], 'little')
                            assert size < 1024 * 1024, 'fixed LOCAL packet exceeded the relay bound'
                            payload = _receive(peer, size)
                        except (EOFError, ConnectionResetError):
                            trace.boundary.set()
                            return
                        if peer is client:
                            if uploading:
                                if size == 0:
                                    uploading = False
                                    trace.eof = True
                                    trace.boundary.set()
                                else:
                                    trace.bytes += size
                            elif header[3] == 0 and payload.startswith(b'\x03LOAD DATA LOCAL INFILE '):
                                assert not trace.query, 'this peer controls exactly one LOAD'
                                trace.query = True
                                if truncate:
                                    with source.open('r+b') as changed:
                                        changed.truncate(50_000)
                            target = server
                        else:
                            if trace.query and not trace.requested and payload[:1] == b'\xfb':
                                trace.requested = True
                                uploading = True
                            target = client
                        try:
                            target.sendall(header + payload)
                        except (BrokenPipeError, ConnectionResetError):
                            trace.boundary.set()
                            return

        future = executor.submit(relay)
        connection: Connection | None = None
        try:
            connection = live(
                label, unix_socket=None, host=endpoint[0], port=endpoint[1],
                ssl=SSL(mode='DISABLED'), compression=None,
                server_public_key=ROOT / label / 'data/public_key.pem',
                local_infile=True, max_allowed_packet=128 * 1024,
                operation_timeout=5,
            )
            yield connection, trace
        finally:
            if connection is not None:
                connection.close()
            stop.set()
            future.result(timeout=6)


def _source(tmp_path: Path) -> Path:
    source = tmp_path / 'authorized-诊断.tsv'
    source.write_bytes(ROW * 5_000)
    assert source.stat().st_size == 100_000
    return source


def _table(connection: Connection) -> None:
    with connection.cursor() as setup:
        setup.execute('CREATE TEMPORARY TABLE local_diagnostic_rows(id INT, txt VARCHAR(80))')


@requires_mysql
def test_local_short_read_preserves_source_and_known_transfer_facts(
    local_server: str, tmp_path: Path,
) -> None:
    source = _source(tmp_path)
    with _upload_connection(local_server, source, truncate=True) as (connection, trace):
        _table(connection)
        with connection.cursor() as current:
            with pytest.raises(OSError) as caught:
                current.execute(load_sql('local_diagnostic_rows'), ('unrelated-label',),
                                local_file=source)
            assert 'opening length' in str(caught.value)
            assert connection.closed
            assert trace.boundary.wait(5), 'the retired upload did not reach the peer boundary'
            assert trace.query and trace.requested
            assert trace.bytes == 50_000 and not trace.eof
            notes = cast(list[str], getattr(caught.value, '__notes__', []))
            diagnostic = '\n'.join([str(caught.value), *notes])
            assert 'outcome is unknown' in diagnostic
            compact = diagnostic.lower().replace(' ', '')
            expected = ('opening_length=100000', 'read_bytes=50000',
                        'flushed_bytes=50000', 'eof_flushed=false')
            missing = [fact for fact in expected if fact not in compact]
            if str(source) not in diagnostic and str(source).replace('\\', '\\\\') not in diagnostic:
                missing.append('actual local_file source path')
            assert not missing, f'missing known LOCAL failure facts: {missing}'
            assert CANARY not in diagnostic


@requires_mysql
def test_local_success_keeps_real_ack_and_complete_eof_with_the_same_peer(
    local_server: str, tmp_path: Path,
) -> None:
    source = _source(tmp_path)
    with _upload_connection(local_server, source, truncate=False) as (connection, trace):
        _table(connection)
        with connection.cursor() as current:
            assert current.execute(load_sql('local_diagnostic_rows'), ('unrelated-label',),
                                   local_file=source) == 5_000
            assert current.row_count == current.batch_row_count == 5_000
            assert current.warning_count == 0
            assert trace.boundary.wait(5)
            assert trace.query and trace.requested
            assert trace.bytes == 100_000 and trace.eof
            assert not connection.closed
            current.execute('SELECT COUNT(*) FROM local_diagnostic_rows')
            assert current.fetchone() == (5_000,)
