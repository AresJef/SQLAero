"""Real result order and trusted ACK facts survive later local save failures.

The fixed relay only enlarges the informational text of a real final SELECT
terminator. Its status, row packets and sequence are retained. This is a valid
complete ACK followed by a local ownership-quota failure, not malformed wire
or evidence of a naturally occurring server failure.
"""

from collections.abc import Generator
from concurrent.futures import ThreadPoolExecutor
from contextlib import contextmanager
from dataclasses import dataclass, field
import select
import socket
from threading import Event
from typing import TypeAlias

import pytest
from sqlaero import Connection, Cursor, DictCursor, SSCursor, SSDictCursor, SSL
from sqlaero.errors import OperationalError, SQLAeroError, SQLAeroErrorCode
from test_public_connection import ROOT, fixture_port, live, requires_mysql


CursorType: TypeAlias = Cursor | DictCursor | SSCursor | SSDictCursor
READER_LIMIT = 52 * 1024
INFO_BYTES = 96 * 1024
COPY_LIMIT = 208 * 1024
FIRST_SQL = "SELECT 10 AS a_first UNION ALL SELECT 11"
FINAL_SQL = "SELECT 20 AS a_final UNION ALL SELECT 21 UNION ALL SELECT 22 UNION ALL SELECT 23 UNION ALL SELECT 24"


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("dictionary", [False, True])
@pytest.mark.parametrize("streaming", [False, True])
def test_nextset_reader_quota_failure_keeps_an_explicit_failure_boundary(
    label: str, dictionary: bool, streaming: bool,
) -> None:
    # The second set's real metadata/one row fits native reception; preparing
    # its per-column Reader plans needs additional result-owned capacity.
    wide = ",".join(f"{value} AS c{value}" for value in range(256))
    sql = f"SELECT %s AS a_first UNION ALL SELECT 70;SELECT {wide};SELECT 99 AS a_third"
    with live(label, result_memory_limit=READER_LIMIT,
              total_result_memory_limit=4 * 1024 * 1024) as connection:
        current = (connection.cursor(dict, streaming=streaming) if dictionary
                   else connection.cursor(streaming=streaming))
        try:
            if streaming:
                # Only the last request is streamed. The failure concerns the
                # already buffered prefix and must not steal its wire owner.
                current.executemany(sql, [(7,), (8,)])
            else:
                assert current.execute(sql, (7,)) == 2
            assert current.fetchone() == ({"a_first": 7} if dictionary else (7,))
            with pytest.raises(OperationalError) as immediate:
                current.nextset()
            assert immediate.value.code == SQLAeroErrorCode.RESOURCE_LIMIT
            assert immediate.value.is_server_error is False
            assert "native result requires" in immediate.value.message
            assert str(READER_LIMIT) in immediate.value.message
            # Entering nextset explicitly abandons the unread old row even
            # when the next set's local Reader preparation cannot finish.
            assert current.fetchone() is None
            assert current.fetchall() == ()
            # A permanent saved boundary is sufficient; recovery of the old
            # rows or the rejected set is not required. Neither EOF nor the
            # third result may silently replace the failed switch.
            for _ in range(2):
                with pytest.raises(SQLAeroError) as repeated:
                    current.nextset()
                assert repeated.value.code == immediate.value.code
                assert repeated.value.is_server_error is False
        finally:
            current.close()
        assert connection.closed is False and connection.ping() is True


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("dictionary", [False, True])
def test_first_reader_publication_failure_cannot_expose_later_buffered_sets(
    label: str, dictionary: bool,
) -> None:
    wide = ",".join(f"{value} AS c{value}" for value in range(256))
    with live(label, result_memory_limit=READER_LIMIT,
              total_result_memory_limit=4 * 1024 * 1024) as connection:
        current = connection.cursor(dict) if dictionary else connection.cursor()
        try:
            with pytest.raises(OperationalError) as immediate:
                current.execute(f"SELECT {wide};SELECT 99 AS a_later")
            assert immediate.value.code == SQLAeroErrorCode.RESOURCE_LIMIT
            assert immediate.value.is_server_error is False
            assert "native result requires" in immediate.value.message
            assert str(READER_LIMIT) in immediate.value.message
            notes = "\n".join(getattr(immediate.value, "__notes__", ()))
            # Both native sets finished before the first Reader's local plans
            # failed. Keep that real request contribution without publishing
            # the unrelated second set as if the first had succeeded.
            assert "confirmed 1 commands" in notes
            assert "actual cumulative row count 1," in notes
            assert current.batch_row_count == 1
            for _ in range(2):
                with pytest.raises(SQLAeroError) as repeated:
                    current.nextset()
                assert repeated.value.code == immediate.value.code
                assert repeated.value.is_server_error is False
        finally:
            current.close()
        assert connection.closed is False and connection.ping() is True


@dataclass
class _AckTrace:
    queries: list[bytes] = field(default_factory=lambda: list[bytes]())
    rows: list[bytes] = field(default_factory=lambda: list[bytes]())
    rewritten: int = 0
    info_bytes: int = 0
    status: int | None = None


def _receive(peer: socket.socket, size: int) -> bytes:
    """Reuse the existing lifecycle relay's bounded complete-packet read."""
    data = bytearray()
    while len(data) < size:
        block = peer.recv(size - len(data))
        if not block:
            raise EOFError("the owned ACK relay ended before a complete packet")
        data.extend(block)
    return bytes(data)


@contextmanager
def _completion_copy_failure(
    label: str, sql: str,
) -> Generator[tuple[Connection, _AckTrace], None, None]:
    """Copy the existing narrow real-server relay for one fixed final ACK."""
    target = sql.encode("ascii")
    trace = _AckTrace()
    stop = Event()
    real_rows = {b"\x02" + str(value).encode("ascii") for value in (10, 11, 20, 21, 22, 23, 24)}
    with socket.socket() as listener, ThreadPoolExecutor(max_workers=1) as executor:
        listener.bind(("127.0.0.1", 0))
        endpoint = listener.getsockname()
        listener.listen(1)
        listener.settimeout(5)

        def relay() -> None:
            with listener.accept()[0] as client, socket.create_connection(
                ("127.0.0.1", fixture_port(label)), timeout=5,
            ) as server:
                client.settimeout(5)
                server.settimeout(5)
                active = False
                while not stop.is_set():
                    readable, _, _ = select.select([client, server], [], [], 0.05)
                    for peer in readable:
                        try:
                            header = _receive(peer, 4)
                            size = int.from_bytes(header[:3], "little")
                            assert size < 1024 * 1024, "fixed ACK fixture packet exceeded its bound"
                            payload = _receive(peer, size)
                        except (EOFError, ConnectionResetError, ConnectionAbortedError):
                            return
                        destination = server if peer is client else client
                        if peer is client:
                            if header[3] == 0 and payload[:1] == b"\x03":
                                trace.queries.append(payload[1:])
                                active = payload[1:] == target
                        elif active:
                            if payload in real_rows:
                                trace.rows.append(payload)
                            if payload[:1] == b"\xfe" and len(payload) >= 7:
                                status = int.from_bytes(payload[3:5], "little")
                                if not status & 0x0008:
                                    assert not status & 0x4000, "fixed SELECT unexpectedly changed session state"
                                    assert payload[1:3] == b"\x00\x00"
                                    assert payload[7:] in (b"", b"\x00"), "fixed SELECT returned unexpected Info"
                                    # CLIENT_SESSION_TRACK informational text
                                    # uses a length-encoded string. The 0xfd
                                    # prefix carries this fixed 24-bit length.
                                    payload = (payload[:7] + b"\xfd"
                                               + INFO_BYTES.to_bytes(3, "little")
                                               + b"I" * INFO_BYTES)
                                    header = len(payload).to_bytes(3, "little") + header[3:]
                                    trace.rewritten += 1
                                    trace.info_bytes = INFO_BYTES
                                    trace.status = status
                                    active = False
                        try:
                            destination.sendall(header + payload)
                        except (BrokenPipeError, ConnectionResetError, ConnectionAbortedError):
                            return

        relayed = executor.submit(relay)
        connection: Connection | None = None
        try:
            connection = live(
                label, unix_socket=None, host=endpoint[0], port=endpoint[1],
                ssl=SSL(mode="DISABLED"), compression=None,
                server_public_key=ROOT / label / "data/public_key.pem",
                max_received_packet=256 * 1024, result_memory_limit=COPY_LIMIT,
                total_result_memory_limit=4 * 1024 * 1024, operation_timeout=5,
            )
            yield connection, trace
        finally:
            if connection is not None:
                connection.close()
            stop.set()
            relayed.result(timeout=6)


def _trusted_copy_error(error: OperationalError, trace: _AckTrace, sql: str) -> str:
    """Require actual terminal confirmation before checking recovered counts."""
    assert error.code == SQLAeroErrorCode.RESOURCE_LIMIT
    assert error.is_server_error is False
    assert "native result requires" in error.message
    assert str(COPY_LIMIT) in error.message
    notes = "\n".join(getattr(error, "__notes__", ()))
    assert "final server response was fully confirmed" in notes
    assert trace.rewritten == 1 and trace.info_bytes == INFO_BYTES
    assert trace.status is not None and not trace.status & 0x0008
    assert trace.queries.count(sql.encode("ascii")) == 1, "the failed request was replayed"
    return notes


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("streaming", [False, True])
def test_confirmed_later_set_copy_failure_keeps_the_first_set_contribution(
    label: str, streaming: bool,
) -> None:
    sql = FIRST_SQL + ";" + FINAL_SQL
    with _completion_copy_failure(label, sql) as (connection, trace):
        current: CursorType = connection.cursor(streaming=streaming)
        try:
            if streaming:
                assert current.execute(sql) is None
                assert current.fetchall() == ((10,), (11,))
                assert current.nextset() is True
                with pytest.raises(OperationalError) as caught:
                    current.fetchall()
            else:
                with pytest.raises(OperationalError) as caught:
                    current.execute(sql)
                assert current.fetchall() == ((10,), (11,))
                assert current.row_count == 2
            notes = _trusted_copy_error(caught.value, trace, sql)
            assert trace.rows == [b"\x02" + str(value).encode("ascii")
                                  for value in (10, 11, 20, 21, 22, 23, 24)]
            assert "confirmed 1 commands" in notes
            assert "actual cumulative row count 2," in notes
            assert current.batch_row_count == 2
            assert connection.closed is True
        finally:
            current.close()


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("streaming", [False, True])
def test_first_and_final_ack_copy_failure_preserves_the_reliable_batch_count(
    label: str, streaming: bool,
) -> None:
    with _completion_copy_failure(label, FINAL_SQL) as (connection, trace):
        current: CursorType = connection.cursor(streaming=streaming)
        try:
            if streaming:
                assert current.execute(FINAL_SQL) is None
                assert current.columns.names == ("a_final",)
                with pytest.raises(OperationalError) as caught:
                    current.fetchall()
            else:
                with pytest.raises(OperationalError) as caught:
                    current.execute(FINAL_SQL)
            notes = _trusted_copy_error(caught.value, trace, FINAL_SQL)
            assert trace.rows == [b"\x02" + str(value).encode("ascii") for value in range(20, 25)]
            assert "confirmed 1 commands" in notes
            assert "actual cumulative row count 5," in notes
            assert current.batch_row_count == 5
            # Receipt of a count does not invent a readable row owner, metadata
            # for an unpublished result, or a transaction-commit confirmation.
            assert connection.closed is True
        finally:
            current.close()


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("close", [False, True], ids=["nextset", "close"])
def test_discard_ack_copy_failure_preserves_the_reliable_batch_count(
    label: str, close: bool,
) -> None:
    with _completion_copy_failure(label, FINAL_SQL) as (connection, trace):
        current = connection.cursor(streaming=True)
        try:
            assert current.execute(FINAL_SQL) is None
            assert current.columns.names == ("a_final",)
            # No fetch precedes the drain. Both exits count and validate the
            # real five rows without converting the discarded public values.
            with pytest.raises(OperationalError) as caught:
                if close:
                    current.close()
                else:
                    current.nextset()
            notes = _trusted_copy_error(caught.value, trace, FINAL_SQL)
            assert trace.rows == [b"\x02" + str(value).encode("ascii") for value in range(20, 25)]
            assert "confirmed 1 commands" in notes
            assert "actual cumulative row count 5," in notes
            assert current.batch_row_count == 5
            assert current.closed is close
            assert connection.closed is True
        finally:
            current.close()
