"""Single Python allocator faults preserve local SQL projection and live results."""

import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
from typing import Any

import pytest
import sqlaero
from test_public_connection import requires_mysql


CHILD = r'''
import _testcapi
from functools import partial
import gc
import hashlib
import json
from pathlib import Path
import sys
import sysconfig
from typing import Any, Callable

free_threaded = bool(sysconfig.get_config_var('Py_GIL_DISABLED'))
gil_enabled = getattr(sys, '_is_gil_enabled', lambda: True)
assert bool(gil_enabled()) is (not free_threaded)
import sqlaero
native = sys.modules['sqlaero._sqlaero']
from sqlaero.errors import InterfaceBusyError
from test_public_connection import live
assert bool(gil_enabled()) is (not free_threaded)
kind, expected_hash = sys.argv[1:3]
native_path = Path(native.__file__).resolve()
before_hash = hashlib.sha256(native_path.read_bytes()).hexdigest()
assert before_hash == expected_hash
before_gil = bool(gil_enabled())
enable, disable = _testcapi.set_nomemory, _testcapi.remove_mem_hooks
bounded_call = getattr(_testcapi, 'call_with_nomemory', None)
failures: list[int] = []
successes: list[int] = []
source = [(1024, "中'\\"), (1025, 'plain'), (1026, None)]
literal = "'中\\'\\\\'"

with live('84') as connection, connection.cursor(prepared=kind == 'prepared') as business:
    with connection.cursor() as setup:
        setup.execute("SET SESSION sql_mode=''")
    business.execute('SELECT 41 AS v UNION ALL SELECT 42')
    columns, description = business.columns, business.description
    counts = (business.row_count, business.rowcount, business.affected_rows,
              business.batch_row_count, business.warning_count, business.lastrowid)
    operation: Callable[[], Any]
    if kind == 'escape':
        operation = partial(connection.escape, source, many=True)
        expected: Any = [('1024', literal), ('1025', "'plain'"), ('1026', 'NULL')]
    else:
        operation = partial(business.mogrify, 'SELECT %s,%s AS v', source, many=True)
        expected = ['SELECT 1024,' + literal + ' AS v', "SELECT 1025,'plain' AS v", 'SELECT 1026,NULL AS v']
    assert operation() == expected
    with connection.cursor(streaming=True) as pending, connection.cursor() as blocked:
        pending.execute('SELECT 51 AS pending_v UNION ALL SELECT 52 UNION ALL SELECT 53')
        pending_columns, pending_description = pending.columns, pending.description
        pending_counts = (pending.row_count, pending.batch_row_count, pending.warning_count)
        for offset in range(64):
            # Clear CPython preparation free lists before one exact allocator
            # fault; the same operation and owners survive every attempt.
            gc.collect()
            try:
                if bounded_call is not None:
                    result = bounded_call(offset, offset + 1, operation)
                else:
                    enable(offset, offset + 1)
                    try:
                        result = operation()
                    finally:
                        disable()
            except MemoryError:
                failures.append(offset)
                assert operation() == expected
            except BaseException:
                print('unhandled allocation failure:', kind, offset, flush=True)
                print(json.dumps({'kind': kind, 'offset': offset, 'failed_offsets': failures,
                                  'gil_before': before_gil, 'gil_after': bool(gil_enabled()),
                                  'native_sha256_before': before_hash,
                                  'native_sha256_after': hashlib.sha256(native_path.read_bytes()).hexdigest()}), flush=True)
                raise
            else:
                successes.append(offset)
                assert result == expected
            assert not connection.closed
            assert business.columns is columns and business.description == description
            assert (business.row_count, business.rowcount, business.affected_rows,
                    business.batch_row_count, business.warning_count, business.lastrowid) == counts
            assert business.fetchone() == (41,)
            business.scroll(0, mode='absolute')
            assert pending.columns is pending_columns and pending.description == pending_description
            assert (pending.row_count, pending.batch_row_count, pending.warning_count) == pending_counts
            try:
                blocked.execute('SELECT 99')
            except InterfaceBusyError:
                pass
            else:
                raise AssertionError('local auxiliary work consumed or released the unread response')
        assert failures, (kind, failures, successes)
        assert pending.fetchall() == ((51,), (52,), (53,))
        assert business.fetchall() == ((41,), (42,))
        assert operation() == expected
        assert blocked.execute('SELECT 99') == 1 and blocked.fetchone() == (99,)
after_hash = hashlib.sha256(native_path.read_bytes()).hexdigest()
after_gil = bool(gil_enabled())
assert after_hash == before_hash and after_gil is before_gil
print(json.dumps({'kind': kind, 'pid': __import__('os').getpid(), 'free_threaded': free_threaded,
                  'gil_before': before_gil, 'gil_after': after_gil, 'native': str(native_path),
                  'native_sha256_before': before_hash, 'native_sha256_after': after_hash,
                  'allocator_hook': 'call_with_nomemory' if bounded_call is not None else 'set_nomemory',
                  'failed_offsets': failures, 'successful_offsets': successes}))
'''


@requires_mysql
@pytest.mark.parametrize("kind", ["escape", "text", "prepared"])
def test_python_output_allocation_faults_remain_retryable_and_preserve_unread_wire(kind: str) -> None:
    """Exercise 64 single faults with the same owner in an isolated interpreter."""
    native: Any = sys.modules["sqlaero._sqlaero"]
    native_hash = hashlib.sha256(Path(native.__file__).read_bytes()).hexdigest()
    environment = os.environ.copy()
    environment["PYTHONPATH"] = os.pathsep.join((str(Path(sqlaero.__file__).resolve().parent.parent),
                                                str(Path(__file__).resolve().parent)))
    environment["PYTHONIOENCODING"] = "utf-8"
    result = subprocess.run([sys.executable, "-", kind, native_hash], input=CHILD, text=True,
                            encoding="utf-8", capture_output=True, timeout=45, check=False,
                            env=environment)
    assert result.returncode == 0, result.stdout + result.stderr
    record: dict[str, Any] = json.loads(result.stdout.splitlines()[-1])
    assert record["kind"] == kind and record["failed_offsets"]
    assert record["native_sha256_before"] == record["native_sha256_after"] == native_hash
    assert record["gil_before"] == record["gil_after"] == bool(getattr(sys, "_is_gil_enabled", lambda: True)())
    assert len(record["failed_offsets"]) + len(record["successful_offsets"]) == 64
    print(result.stdout.strip())
