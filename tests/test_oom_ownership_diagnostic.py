"""Bounded, temporary ownership diagnosis; not a product acceptance waiver."""

import hashlib
import json
import subprocess
import sys
import shutil
import pytest


def test_untraced_oom_ownership_controls(capsys: pytest.CaptureFixture[str]) -> None:
    """Keep the observer disabled while isolating native versus stdlib calls."""
    if not (sys.platform.startswith('linux') and sys.version_info[:2] == (3, 15)
            and not getattr(sys, '_is_gil_enabled', lambda: True)()):
        return
    original = "\nimport _testcapi\nfrom collections.abc import Callable\nfrom functools import partial\nimport gc\nimport tokenize\nimport traceback\nfrom typing import Any\nfrom sqlaero import connect, _sqlaero\nfrom sqlaero.errors import ErrorInfo, SQLAeroError\nfrom sqlaero.retry import Retry, RetryRule\n\nkind: str = 'traceback'\nenable = _testcapi.set_nomemory\ndisable = _testcapi.remove_mem_hooks\nrule = RetryRule(error_types=ValueError)\nencoder = _sqlaero._prepare_encoder()\nunknown = object()\nextract_stack = traceback.StackSummary.extract\n\nclass SmallWidth(int):\n    def bit_length(self) -> int:\n        return 0\n\nlarge_subclass = SmallWidth(1 << 200)\n\ndef prepare() -> Callable[[], Any]:\n    if kind == 'invalid-error':\n        return partial(SQLAeroError, 42)\n    if kind == 'invalid-timeout':\n        return partial(connect, user='unopened', connect_timeout=0)\n    if kind == 'retry-defaults':\n        return partial(Retry, rule)\n    if kind == 'unsupported-input':\n        return partial(encoder.encode, unknown)\n    if kind == 'client-error':\n        return partial(_sqlaero._encode_scalar, 'oversized', limit=1)\n    if kind == 'integer-subclass':\n        return partial(SQLAeroError, code=large_subclass)\n    try:\n        raise SQLAeroError('allocation boundary', code=123)\n    except SQLAeroError as caught:\n        error = caught\n    if kind == 'snapshot-stack':\n        # Isolate our native boundary's allocation failures. Older CPython can\n        # corrupt a Python frame while injecting OOM inside StackSummary.extract\n        # (also reproducible with decimal + traceback and no SQLAero import).\n        # Real stack extraction and original provider errors have separate tests.\n        summary = extract_stack(traceback.walk_tb(error.__traceback__),\n                                capture_locals=False, lookup_lines=False)\n        def prepared_stack(*args: Any, **kwargs: Any) -> traceback.StackSummary:\n            return summary\n        traceback.StackSummary.extract = prepared_stack\n        return partial(getattr, error, 'error_info')\n    info = error.error_info\n    if kind == 'traceback':\n        return partial(getattr, info, 'traceback')\n    return partial(repr if kind == 'repr' else str, info)\n\ndef verify(result: Any) -> None:\n    if kind == 'retry-defaults':\n        assert isinstance(result, Retry) and result.wait_interval == 1.0\n    elif kind == 'integer-subclass':\n        assert isinstance(result, SQLAeroError) and result.code == 1 << 200\n    elif kind == 'snapshot-stack':\n        assert isinstance(result, ErrorInfo) and result.message == 'allocation boundary'\n        assert result.code == 123\n    else:\n        assert isinstance(result, str) and 'allocation boundary' in result\n\nfailures = 0\nfor offset in range(48):\n    gc.collect()\n    operation = prepare()\n    try:\n        enable(offset, offset + 1)\n        try:\n            result = operation()\n        finally:\n            disable()\n    except MemoryError:\n        failures += 1\n    except SQLAeroError as error:\n        assert kind == 'client-error' and error.code == 65538\n        assert error.is_server_error is False\n    except (TypeError, ValueError) as error:\n        if kind in ('invalid-error', 'unsupported-input'):\n            assert type(error) is TypeError\n        elif kind == 'invalid-timeout':\n            assert type(error) is ValueError and 'connect_timeout' in str(error)\n        else:\n            raise\n    else:\n        verify(result)\nassert failures > 0\noperation = prepare()\nif kind in ('invalid-error', 'invalid-timeout', 'unsupported-input', 'client-error'):\n    try:\n        operation()\n    except (TypeError, ValueError, SQLAeroError):\n        pass\n    else:\n        raise AssertionError('invalid input unexpectedly succeeded')\nelse:\n    verify(operation())\nprint('allocation boundary preserved:', kind, failures)\n"
    assert hashlib.sha256(original.encode()).hexdigest() == '54ce821040ce0fef924feb6f9adcd2bb70663dc2999d20ec0eef652e4d2f2f68'
    legacy = '''        enable(offset, offset + 1)
        try:
            result = operation()
        finally:
            disable()
'''
    assert original.count(legacy) == 1
    stdlib = r'''
import decimal
import _testcapi
import gc
import sys
import tokenize
import traceback
assert not any(name == 'sqlaero' or name.startswith('sqlaero.') for name in sys.modules)
failures = 0
successes = 0
for offset in range(256):
    gc.collect()
    try:
        raise ValueError('allocation boundary')
    except ValueError as error:
        stack = traceback.StackSummary.extract(traceback.walk_tb(error.__traceback__),
                                               capture_locals=False, lookup_lines=False)
    operation = stack.format
    try:
        _testcapi.set_nomemory(offset, offset + 1)
        try:
            result = operation()
        finally:
            _testcapi.remove_mem_hooks()
    except MemoryError:
        failures += 1
    else:
        assert isinstance(result, list)
        successes += 1
assert failures and successes
print('stdlib completed', failures, successes)
'''
    stdlib_scoped = stdlib.replace('''        _testcapi.set_nomemory(offset, offset + 1)
        try:
            result = operation()
        finally:
            _testcapi.remove_mem_hooks()
''', '''        result = _testcapi.call_with_nomemory(offset, offset + 1, operation)
''')
    methodcaller = stdlib_scoped.replace('import traceback\n',
        'import traceback\nfrom operator import methodcaller\nfrom functools import partial\n')
    methodcaller = methodcaller.replace('operation = stack.format',
                                        "operation = partial(methodcaller('format'), stack)")
    no_package = "assert not any(name == 'sqlaero' or name.startswith('sqlaero.') for name in sys.modules)"
    imported = methodcaller.replace(no_package, 'import sqlaero')
    enums = methodcaller.replace(no_package, '''from enum import IntEnum
Server = IntEnum('Server', {'ER_TEST_' + str(i): 1000 + i for i in range(2046)})
Client = IntEnum('Client', {'CLIENT_TEST_' + str(i): 65536 + i for i in range(19)})''')
    full = stdlib_scoped.replace('import traceback\n', 'import traceback\nfrom functools import partial\n')
    full = full.replace('''    except ValueError as error:
        stack = traceback.StackSummary.extract(traceback.walk_tb(error.__traceback__),
                                               capture_locals=False, lookup_lines=False)
    operation = stack.format''', '''    except ValueError as error:
        saved = error
    operation = partial(traceback.format_exception, saved)''')
    assert full != stdlib_scoped and 'format_exception' in full
    records: list[dict[str, object]] = []
    for name, program in (('exact-original-untraced', original),
                          ('stdlib-methodcaller-untraced-256', methodcaller),
                          ('stdlib-import-methodcaller-untraced-256', imported),
                          ('stdlib-enum-methodcaller-untraced-256', enums),
                          ('stdlib-full-format-untraced-256', full)):
        assert 'faulthandler' not in program
        for repeat in range(1 if name == 'exact-original-untraced' else 4):
            result = subprocess.run([sys.executable, '-'], input=program.encode(),
                                    capture_output=True, timeout=30, check=False)
            records.append({'variant': name, 'repeat': repeat, 'exit': result.returncode,
                            'source_sha256': hashlib.sha256(program.encode()).hexdigest(),
                            'stdout': result.stdout.decode(errors='replace')[-1500:],
                            'stderr': result.stderr.decode(errors='replace')[-1500:]})
    debugger = shutil.which('gdb')
    if debugger is not None:
        result = subprocess.run([
            debugger, '-q', '--batch', '-ex', 'run', '-ex', 'thread apply all bt 30',
            '-ex', 'frame 2', '-ex', 'info registers rax rbx rbp r12 r13 r14 r15',
            '-ex', 'disassemble /r PyList_New', '-ex', 'x/16gx $rbx',
            '--args', sys.executable, '-',
        ], input=original.encode(), capture_output=True, timeout=60, check=False)
        records.append({'variant': 'gdb-exact-list-allocation', 'repeat': 0,
                        'exit': result.returncode,
                        'stdout': result.stdout.decode(errors='replace')[-40000:],
                        'stderr': result.stderr.decode(errors='replace')[-2000:]})
    # Expected crash observations are recorded, never relabelled as safe.
    with capsys.disabled():
        print('Untraced ownership diagnosis: ' + json.dumps(records), flush=True)

