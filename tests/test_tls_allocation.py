"""EX-083 single Python allocation faults must not poison owned TLS diagnostics."""

import os
import subprocess
import sys

import pytest


@pytest.mark.skipif('SQLAERO_TEST_MYSQL_ROOT' not in os.environ,
                    reason='requires explicitly selected owned MySQL fixtures')
@pytest.mark.parametrize('kind', [
    'info', 'chain', 'subject', 'serial', 'datetime', 'dns', 'der', 'pem', 'fingerprint',
    'info-str', 'certificate-str', 'certificate-repr',
])
def test_single_allocation_fault_propagates_and_cold_property_remains_retryable(kind: str) -> None:
    source = r'''
import _testcapi
from functools import partial
import gc
import json
import os
from pathlib import Path
import sys
import sysconfig
from sqlaero import connect, SSL, TLSInfo, TLSCertificate

kind = KIND
if sysconfig.get_config_var('Py_GIL_DISABLED'):
    assert not sys._is_gil_enabled(), 'TLS allocation evidence must retain real NoGIL mode'
root = Path(os.environ['SQLAERO_TEST_MYSQL_ROOT'])
state = json.loads((root / '84/state.json').read_text())
tls = SSL(ca_file=root / 'tls/ca.pem')
enable = _testcapi.set_nomemory
disable = _testcapi.remove_mem_hooks
call_with_nomemory = getattr(_testcapi, 'call_with_nomemory', None)

def connection():
    result = connect(user='sqlaero', password=state['test_password'], database='sqlaero_test',
                     host='127.0.0.1', port=int(state['port']), ssl=tls, operation_timeout=5)
    result.close()
    return result

def prepare():
    client = connection()
    if kind == 'info':
        return partial(getattr, client, 'tls_info')
    info = client.tls_info
    if kind == 'chain':
        return partial(getattr, info, 'peer_certificates')
    if kind == 'info-str':
        return partial(str, info)
    cert = info.peer_certificates[0]
    if kind == 'certificate-str':
        return partial(str, cert)
    if kind == 'certificate-repr':
        return partial(repr, cert)
    if kind == 'der':
        return cert.to_der
    if kind == 'pem':
        return cert.to_pem
    attribute = {'subject': 'subject', 'serial': 'serial_number', 'datetime': 'not_before',
                 'dns': 'dns_names', 'fingerprint': 'fingerprint_sha256'}[kind]
    return partial(getattr, cert, attribute)

def verify(result):
    if kind == 'info':
        assert type(result) is TLSInfo and result.version in ('TLSv1.2', 'TLSv1.3')
    elif kind == 'chain':
        assert type(result) is tuple and result and type(result[0]) is TLSCertificate
    elif kind == 'subject':
        assert 'SQLAero server' in result
    elif kind == 'serial':
        assert type(result) is int and result.bit_length() > 64
    elif kind == 'datetime':
        from datetime import datetime, timezone
        assert type(result) is datetime and result.tzinfo is timezone.utc
    elif kind == 'dns':
        assert result == ('localhost',)
    elif kind == 'der':
        assert type(result) is bytes and result.startswith(b'0')
    elif kind == 'pem':
        assert result.startswith('-----BEGIN CERTIFICATE-----\n') and result.endswith('\n')
    elif kind == 'fingerprint':
        assert len(result) == 64 and result == result.lower()
    else:
        assert type(result) is str and result.startswith('TLS')
        assert '\n' not in result if kind.endswith('repr') else '\n' in result
    if kind not in ('info', 'chain', 'info-str'):
        assert result == expected

# Import/prepare real datetime and display providers outside injected failures;
# each operation below still owns a fresh, cold native and Python cache.
expected = prepare()()
verify(expected)
failures = 0
successes = 0
for offset in range(64):
    gc.collect()
    operation = prepare()
    try:
        if call_with_nomemory is not None:
            result = call_with_nomemory(offset, offset + 1, operation)
        else:
            enable(offset, offset + 1)
            try:
                result = operation()
            finally:
                disable()
    except MemoryError:
        failures += 1
        # The same immutable owner must remain usable after the original
        # resource exception. No failed Python value or PyErr may be cached.
        verify(operation())
    else:
        successes += 1
        verify(result)
assert failures > 0, (kind, failures, successes)
if sysconfig.get_config_var('Py_GIL_DISABLED'):
    assert not sys._is_gil_enabled(), 'TLS allocation work changed the NoGIL mode'
print('TLS allocation boundary preserved:', kind, failures, successes)
'''.replace('KIND', repr(kind))
    result = subprocess.run([sys.executable, '-'], input=source, text=True,
                            capture_output=True, timeout=45, check=False)
    assert result.returncode == 0, result.stdout + result.stderr
    assert 'TLS allocation boundary preserved:' in result.stdout
    print(result.stdout.strip())
