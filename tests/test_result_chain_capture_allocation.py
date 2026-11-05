"""Catchable allocation failures in the original encoding-error boundary."""

import os
import subprocess
import sys

import pytest


@pytest.mark.skipif('SQLAERO_TEST_MYSQL_ROOT' not in os.environ,
                    reason='requires explicitly selected owned MySQL fixtures')
def test_capture_allocation_never_replaces_immediate_unicode_failure() -> None:
    source = r'''
import _testcapi
from functools import partial
import json
import os
from pathlib import Path
import sqlaero
import sys
import sysconfig

if sysconfig.get_config_var('Py_GIL_DISABLED'):
    assert not sys._is_gil_enabled()

root = Path(os.environ['SQLAERO_TEST_MYSQL_ROOT'])
state = json.loads((root/'84/state.json').read_text())
enable = _testcapi.set_nomemory
disable = _testcapi.remove_mem_hooks
bounded_call = getattr(_testcapi, 'call_with_nomemory', None)
with sqlaero.connect(host='127.0.0.1', port=int(state['port']), user='sqlaero',
                     password=state['test_password'], database='sqlaero_test',
                     charset='latin1', ssl=sqlaero.SSL(ca_file=root/'tls/ca.pem')) as connection:
    original_failures = 0
    resource_failures = 0
    for offset in range(64):
        with connection.cursor() as cur:
            operation = partial(cur.executemany, 'SELECT %s AS trusted', [('ok',), ('\U0001f600',)])
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
                resource_failures += 1
            except UnicodeEncodeError as original:
                original_failures += 1
                assert (original.encoding, original.object, original.start, original.end) == ('latin1', '\U0001f600', 0, 1)
            else:
                raise AssertionError('encoding failure claimed success')
    assert original_failures > 0 and resource_failures > 0
print('capture keeps the immediate original cause', original_failures, resource_failures)
if sysconfig.get_config_var('Py_GIL_DISABLED'):
    assert not sys._is_gil_enabled()
'''
    result = subprocess.run([sys.executable, '-'], input=source, text=True,
                            capture_output=True, timeout=30, check=False)
    assert result.returncode == 0, result.stdout + result.stderr
    assert 'capture keeps the immediate original cause' in result.stdout
    print(result.stdout.strip())
