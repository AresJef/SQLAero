"""R05 restoring a stored boundary never caches a resource exception."""

import os
import subprocess
import sys

import pytest


@pytest.mark.skipif('SQLAERO_TEST_MYSQL_ROOT' not in os.environ,
                    reason='requires explicitly selected owned MySQL fixtures')
@pytest.mark.parametrize('kind', ['unicode', 'native', 'unavailable'])
def test_retained_failure_allocation_preserves_primary_and_retry(kind: str) -> None:
    source = r'''
import _testcapi
from functools import partial
import json
import os
from pathlib import Path
import sys
import sysconfig
import sqlaero

kind = KIND
if sysconfig.get_config_var('Py_GIL_DISABLED'):
    assert not sys._is_gil_enabled()
root = Path(os.environ['SQLAERO_TEST_MYSQL_ROOT'])
state = json.loads((root/'84/state.json').read_text())
enable = _testcapi.set_nomemory
disable = _testcapi.remove_mem_hooks
bounded_call = getattr(_testcapi, 'call_with_nomemory', None)
failures = 0
with sqlaero.connect(host='127.0.0.1', port=int(state['port']), user='sqlaero',
                     password=state['test_password'], database='sqlaero_test',
                     charset='latin1' if kind != 'native' else 'utf8mb4',
                     ssl=sqlaero.SSL(ca_file=root/'tls/ca.pem')) as connection:
    with connection.cursor() as cur:
        try:
            if kind != 'native':
                text = '\U0001f600' if kind == 'unicode' else 'a' * 2000 + '\U0001f600'
                cur.executemany('SELECT %s AS trusted', [('ok',), (text,)])
            else:
                cur.execute('SELECT 7; SELECT * FROM sqlaero_absent_allocation_boundary')
        except Exception as original:
            expected_type = type(original)
            expected_notes = tuple(original.__notes__)
        cur.fetchall()
        if kind == 'unavailable':
            from sqlaero.errors import InterfaceError
            expected_type = InterfaceError
        operation = cur.nextset
        for offset in range(32):
            try:
                if bounded_call is not None:
                    bounded_call(offset, offset+1, operation)
                else:
                    enable(offset, offset+1)
                    try:
                        operation()
                    finally:
                        disable()
            except MemoryError:
                failures += 1
            except expected_type:
                pass
            else:
                raise AssertionError('failure boundary claimed success')
            try:
                operation()
            except expected_type as restored:
                assert tuple(restored.__notes__) == expected_notes
                if kind == 'unicode':
                    assert (restored.encoding, restored.object, restored.start, restored.end) == ('latin1', '\U0001f600', 0, 1)
                elif kind == 'native':
                    assert restored.code == 1146
                else:
                    assert 'local diagnostic limitation, not a new database failure' in restored.message
            else:
                raise AssertionError('resource failure poisoned the boundary')
    with connection.cursor() as other:
        assert other.execute('SELECT 1') == 1
assert failures > 0, (kind, failures)
if sysconfig.get_config_var('Py_GIL_DISABLED'):
    assert not sys._is_gil_enabled()
print('retained boundary resource exceptions and retry preserved', kind, failures)
'''.replace('KIND', repr(kind))
    result = subprocess.run([sys.executable, '-'], input=source, text=True,
                            capture_output=True, timeout=30, check=False)
    assert result.returncode == 0, result.stdout + result.stderr
    assert 'retained boundary resource exceptions and retry preserved' in result.stdout
    print(result.stdout.strip())

