"""Checked counter publication preserves MemoryError and real execution facts."""

import os
import subprocess
import sys

import pytest


@pytest.mark.skipif("SQLAERO_TEST_MYSQL_ROOT" not in os.environ,
                    reason="requires the explicitly selected owned MySQL fixture")
@pytest.mark.parametrize("kind", ["execute", "rowcount", "row_count", "affected_rows", "batch_row_count"])
def test_count_allocation_failure_is_not_a_panic_or_connection_poison(kind: str) -> None:
    source = r'''
import _testcapi
from functools import partial
import gc
import json
import os
from pathlib import Path
import sqlaero

kind = KIND
root = Path(os.environ['SQLAERO_TEST_MYSQL_ROOT'])
state = json.loads((root / '84/state.json').read_text())
enable = _testcapi.set_nomemory
disable = _testcapi.remove_mem_hooks
sql = 'WITH RECURSIVE n AS (SELECT 1 AS i UNION ALL SELECT i+1 FROM n WHERE i<300) SELECT i FROM n'
with sqlaero.connect(host='127.0.0.1', port=int(state['port']), user='sqlaero',
                     password=state['test_password'], database='sqlaero_test',
                     ssl=sqlaero.SSL(ca_file=root / 'tls/ca.pem')) as connection:
    with connection.cursor() as cursor:
        if kind == 'execute':
            operation = partial(cursor.execute, sql)
        else:
            assert cursor.execute(sql) == 300
            operation = partial(getattr, cursor, kind)
        # CPython may satisfy a small integer from a thread-local free list.
        # Clear preparation leftovers before the one exact allocation fault;
        # the actual library call and its failures remain fully observed.
        gc.collect()
        failure = None
        try:
            enable(0, 1)
            try:
                operation()
            finally:
                disable()
        except BaseException as error:
            failure = error
        assert type(failure) is MemoryError, type(failure)
        assert cursor.rowcount == cursor.row_count == cursor.affected_rows == 300
        assert cursor.batch_row_count == 300
        assert cursor.fetchone() == (1,)
        if kind == 'execute':
            try:
                cursor.nextset()
            except MemoryError:
                pass
            else:
                raise AssertionError('failed count publication must retain a failure boundary')
        else:
            assert cursor.nextset() is None
        with connection.cursor() as other:
            assert other.execute('SELECT 9') == 1
            assert other.fetchone() == (9,)
'''.replace("KIND", repr(kind))
    result = subprocess.run([sys.executable, "-c", source], capture_output=True, text=True,
                            timeout=45, check=False)
    assert result.returncode == 0, result.stdout + result.stderr
