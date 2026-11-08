"""Resource pauses retain prepared, compressed and managed CALL ownership."""

# Native artifact identity is checked without adding a public diagnostic API.
# pyright: reportPrivateUsage=false

from collections.abc import Generator
from contextlib import contextmanager
import hashlib
import json
from pathlib import Path
import subprocess
import sys
from typing import Any
from uuid import uuid4

import pytest
from sqlaero import Connection, _sqlaero
from sqlaero.errors import InterfaceBusyError, OperationalError
from test_public_connection import live, requires_mysql
from test_streaming_resource_recovery import (
    PACKET_LIMIT, PAYLOAD_BYTES, RESULT_LIMIT, ROWS,
    assert_one_execution, assert_resource_error, consume_small_batches,
)


PRIVATE_CALL_PREFIX = "_sqlaero_call_"


def assert_wire_busy(connection: Connection) -> None:
    """A resource pause does not transfer the response lease to a new command."""
    with connection.cursor() as blocked:
        with pytest.raises(InterfaceBusyError):
            blocked.execute("SELECT 99")


def private_slot_nullness(observer: Connection, target: int) -> dict[str, bool]:
    """Read only private slot names and NULL flags, never user variable values."""
    result: dict[str, bool] = {}
    with observer.cursor() as cursor:
        cursor.execute(
            "SELECT v.VARIABLE_NAME,v.VARIABLE_VALUE IS NULL "
            "FROM performance_schema.user_variables_by_thread AS v "
            "JOIN performance_schema.threads AS t ON t.THREAD_ID=v.THREAD_ID "
            "WHERE t.PROCESSLIST_ID=%s AND LEFT(v.VARIABLE_NAME,14)=%s "
            "ORDER BY v.VARIABLE_NAME", (target, PRIVATE_CALL_PREFIX),
        )
        for name, null in cursor.fetchall():
            assert isinstance(name, str) and name.startswith(PRIVATE_CALL_PREFIX)
            assert type(null) is int and null in (0, 1)
            result[name] = bool(null)
    return result


@contextmanager
def owned_resource_routine(label: str) -> Generator[str, None, None]:
    """Create and finally remove only this case's uniquely named procedure."""
    name = "a_resource_combo_" + uuid4().hex
    qualified = "`sqlaero_test`.`" + name + "`"
    with live(label) as setup, setup.cursor() as create:
        create.execute(
            f"CREATE PROCEDURE {qualified}(IN row_total INT,OUT doubled INT,INOUT counter INT) "
            "BEGIN SET @a_combo_calls=COALESCE(@a_combo_calls,0)+1; "
            "WITH RECURSIVE seq(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM seq WHERE n<row_total) "
            f"SELECT n AS ordinal,REPEAT('x',{PAYLOAD_BYTES}) AS payload,"
            "(@a_resource_visits:=@a_resource_visits+1) AS visits FROM seq ORDER BY n; "
            "SET doubled=row_total*2; SET counter=counter+1; END"
        )
        try:
            yield name
        finally:
            create.execute(f"DROP PROCEDURE IF EXISTS {qualified}")


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize(("compression", "dictionary", "finish"), [
    pytest.param("zlib", False, "read", id="binary-zlib-read"),
    pytest.param("zstd", True, "close", id="binary-zstd-dict-close"),
])
def test_prepared_compressed_pause_resumes_or_drains_without_reexecution(
    label: str, compression: str, dictionary: bool, finish: str,
) -> None:
    with live(label, compression=compression, result_memory_limit=RESULT_LIMIT,
              total_result_memory_limit=512 * 1024, max_received_packet=PACKET_LIMIT) as connection:
        with connection.cursor() as setup:
            setup.execute("SET @a_resource_visits=0")
        cursor = (connection.cursor(dict, streaming=True, prepared=True) if dictionary
                  else connection.cursor(streaming=True, prepared=True))
        cursor.execute(
            "WITH RECURSIVE seq(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM seq WHERE n<%s) "
            "SELECT n AS ordinal,REPEAT('x',%s) AS payload,"
            "(@a_resource_visits:=@a_resource_visits+1) AS visits FROM seq ORDER BY n",
            (ROWS, PAYLOAD_BYTES),
        )
        columns = cursor.columns
        with pytest.raises(OperationalError) as failure:
            cursor.fetchall()
        assert_resource_error(failure.value, RESULT_LIMIT)
        assert not connection.closed and cursor.prepared
        assert_wire_busy(connection)
        assert cursor.scroll(0, mode="absolute") is None
        if finish == "read":
            consume_small_batches(cursor, dictionary)
        cursor.close()
        assert cursor.closed and cursor.columns is columns
        assert cursor.row_count == cursor.batch_row_count == ROWS
        assert_one_execution(connection)


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("multi_statements", [False, True], ids=["split", "combined"])
@pytest.mark.parametrize(("prepared", "compression", "dictionary", "finish"), [
    pytest.param(False, None, False, "read", id="text-raw-read"),
    pytest.param(True, "zstd", True, "read", id="binary-zstd-dict-read"),
    pytest.param(False, "zlib", True, "close", id="text-zlib-dict-close"),
    pytest.param(True, None, False, "close", id="binary-raw-close"),
])
def test_call_resource_pause_keeps_outputs_pending_and_private_stages_ordered(
    label: str, prepared: bool, compression: str | None, dictionary: bool, finish: str,
    multi_statements: bool,
) -> None:
    with owned_resource_routine(label) as name, live(
        label, compression=compression, result_memory_limit=RESULT_LIMIT,
        total_result_memory_limit=512 * 1024, max_received_packet=PACKET_LIMIT,
        multi_statements=multi_statements,
    ) as connection, live(label) as observer:
        with connection.cursor() as setup:
            setup.execute("SET @a_resource_visits=0,@a_combo_calls=0")
        before = private_slot_nullness(observer, connection.connection_id)
        assert before == {}
        cursor = (connection.cursor(dict, streaming=True, prepared=prepared) if dictionary
                  else connection.cursor(streaming=True, prepared=prepared))
        args = [ROWS, None, 41]
        assert cursor.callproc(name, args, database="sqlaero_test",
                               parameter_modes=("IN", "OUT", "INOUT")) is None
        assert args == [ROWS, None, 41]
        columns = cursor.columns
        with pytest.raises(OperationalError) as failure:
            cursor.fetchall()
        assert_resource_error(failure.value, RESULT_LIMIT)
        assert not connection.closed
        assert cursor.call_output_params is None and cursor.call_output_params is None
        assert_wire_busy(connection)
        paused = private_slot_nullness(observer, connection.connection_id)
        if prepared:
            assert paused == before
        else:
            assert len(paused) == 2
            # A combined text request already contains initialization, CALL,
            # output SELECT and cleanup in that order. The server can complete
            # all four while the client retains unread packets. NULL private
            # slots do not imply ready public output or a released wire lease.
            # A split request has not sent its output SELECT/cleanup yet.
            if not multi_statements:
                assert any(null is False for null in paused.values())
        assert cursor.columns is columns and cursor.scroll(0, mode="absolute") is None
        if finish == "read":
            consume_small_batches(cursor, dictionary)
            assert cursor.call_output_params is None
            assert cursor.nextset() is True
            assert cursor.description is None
            assert cursor.nextset() is None
            assert cursor.columns.names == ()
        else:
            cursor.close()
            assert cursor.closed
        outputs = cursor.call_output_params
        assert outputs is not None and dict(outputs) == {1: ROWS * 2, 2: 42}
        assert cursor.call_output_params is outputs
        after = private_slot_nullness(observer, connection.connection_id)
        if prepared:
            assert after == before
        else:
            assert after.keys() == paused.keys() and all(after.values())
        with connection.cursor() as check:
            check.execute("SELECT @a_combo_calls,@a_resource_visits")
            assert check.fetchone() == (1, ROWS)
        assert columns.names == ("ordinal", "payload", "visits")
        cursor.close()
        assert cursor.call_output_params is outputs and dict(outputs) == {1: ROWS * 2, 2: 42}


NOGIL_OWNER = r'''from __future__ import annotations
from concurrent.futures import ThreadPoolExecutor
import decimal
from decimal import Decimal as NativeDecimal
import gc
import hashlib
import json
from pathlib import Path
import sys
from threading import Event
from typing import Callable

before = sys._is_gil_enabled()
assert before is False
action: Callable[[str], None] | None = None

class GateDecimal:
    def __new__(cls: type[GateDecimal], value: str) -> NativeDecimal:
        callback = action
        if callback is not None:
            callback(value)
        return NativeDecimal(value)

decimal.Decimal = GateDecimal
import sqlaero
from sqlaero import _sqlaero
decimal.Decimal = NativeDecimal
assert sys._is_gil_enabled() is False
sys.path.insert(0, sys.argv[1])
from test_public_connection import live
from sqlaero.errors import InterfaceBusyError, OperationalError, SQLAeroErrorCode

label = sys.argv[2]
native = Path(_sqlaero.__file__)
native_before = hashlib.sha256(native.read_bytes()).hexdigest()
entered, release = Event(), Event()
original = MemoryError('original post-resource-pause Decimal output failure')

def fail_second(value: str) -> None:
    if NativeDecimal(value) == 2:
        entered.set()
        assert release.wait(5), 'observer did not release real second-field conversion'
        raise original

with live(label, compression='zstd', result_memory_limit=65536,
          total_result_memory_limit=524288, max_received_packet=16384) as connection:
    with connection.cursor() as setup:
        setup.execute('SET @a_resource_visits=0')
    cursor = connection.cursor(streaming=True, prepared=True)
    cursor.execute('WITH RECURSIVE seq(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM seq WHERE n<%s) '
                   'SELECT 975312000+n AS marker,CAST(n AS DECIMAL(10,0)) AS d,'
                   "REPEAT('x',%s) AS payload,(@a_resource_visits:=@a_resource_visits+1) AS visits "
                   'FROM seq ORDER BY n', (128,4096))
    columns = cursor.columns
    try:
        cursor.fetchall()
    except OperationalError as error:
        assert error.code == SQLAeroErrorCode.RESOURCE_LIMIT and error.is_server_error is False
    else:
        raise AssertionError('the large native accumulation must hit its configured quota')
    assert not connection.closed and cursor.scroll(0, mode='absolute') is None
    action = fail_second
    busy = 0
    with ThreadPoolExecutor(max_workers=1) as executor:
        future = executor.submit(cursor.fetchmany, 2)
        try:
            assert entered.wait(5), 'no overlap with the actual second Decimal conversion'
            gc.collect()
            for candidate in gc.get_objects():
                if type(candidate) is tuple and len(candidate) == 4:
                    if type(candidate[0]) is int and candidate[0] == 975312002:
                        raise AssertionError('unfinished native row became visible to concurrent GC')
                if type(candidate) is tuple and len(candidate) == 2:
                    first = candidate[0]
                    if type(first) is tuple and len(first) == 4 and first[0] == 975312001:
                        raise AssertionError('unfinished outer result became visible to concurrent GC')
            for operation in (cursor.fetchone, lambda: cursor.fetchmany(1), cursor.close):
                try:
                    operation()
                except InterfaceBusyError:
                    busy += 1
                else:
                    raise AssertionError('same-Cursor operation entered during atomic conversion')
            with live(label) as independent, independent.cursor() as observer:
                observer.execute('SELECT 41')
                assert observer.fetchone() == (41,)
            assert cursor.columns is columns
        finally:
            action = None
            release.set()
        try:
            future.result(timeout=5)
        except MemoryError as error:
            assert error is original
            assert any('row 1' in note and 'position remains 0' in note
                       for note in getattr(error, '__notes__', ()))
        else:
            raise AssertionError('the original output MemoryError was lost')
    assert busy == 3 and cursor.scroll(0, mode='absolute') is None
    for ordinal in range(1,129):
        assert cursor.fetchmany(1) == ((975312000+ordinal,NativeDecimal(ordinal),'x'*4096,ordinal),)
    assert cursor.fetchone() is None and cursor.row_count == 128
    with connection.cursor() as observer:
        observer.execute('SELECT @a_resource_visits')
        assert observer.fetchone() == (128,), 'quota or output recovery replayed the SQL'
    cursor.close()
assert sys._is_gil_enabled() is False
native_after = hashlib.sha256(native.read_bytes()).hexdigest()
assert native_after == native_before
print(json.dumps({'label':label,'gil_before':before,'gil_after':sys._is_gil_enabled(),
                  'sha256':native_after,'same_cursor_busy':busy}))
'''


@requires_mysql
@pytest.mark.skipif(getattr(sys, "_is_gil_enabled", lambda: True)(),
                    reason="requires a verified GIL-disabled package")
@pytest.mark.parametrize("label", ["84", "97"])
def test_nogil_paused_prepared_rows_keep_atomic_ownership_during_concurrent_gc(label: str) -> None:
    result = subprocess.run(
        [sys.executable, "-c", NOGIL_OWNER, str(Path(__file__).resolve().parent), label],
        text=True, capture_output=True, timeout=25, check=False,
    )
    assert result.returncode == 0, result.stdout + result.stderr
    report: dict[str, Any] = json.loads(result.stdout.strip().splitlines()[-1])
    assert report["label"] == label and report["gil_before"] is report["gil_after"] is False
    assert report["same_cursor_busy"] == 3
    assert report["sha256"] == hashlib.sha256(Path(_sqlaero.__file__).read_bytes()).hexdigest()
