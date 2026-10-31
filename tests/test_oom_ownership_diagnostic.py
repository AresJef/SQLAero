"""Isolated causal check of a known CPython allocation cleanup path."""

import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import pytest


def test_untraced_oom_ownership_controls(capsys: pytest.CaptureFixture[str]) -> None:
    """Compare unchanged failures with a debugger-only constructor correction."""
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
    native = original.replace(legacy, '''        result = _testcapi.call_with_nomemory(offset, offset + 1, operation)
''').replace('for offset in range(48):', 'for offset in range(256):')
    stdlib = "\nimport decimal\nimport _testcapi\nimport gc\nimport sys\nimport tokenize\nimport traceback\nassert not any(name == 'sqlaero' or name.startswith('sqlaero.') for name in sys.modules)\nfailures = 0\nsuccesses = 0\nfor offset in range(256):\n    gc.collect()\n    try:\n        raise ValueError('allocation boundary')\n    except ValueError as error:\n        stack = traceback.StackSummary.extract(traceback.walk_tb(error.__traceback__),\n                                               capture_locals=False, lookup_lines=False)\n    operation = stack.format\n    try:\n        _testcapi.set_nomemory(offset, offset + 1)\n        try:\n            result = operation()\n        finally:\n            _testcapi.remove_mem_hooks()\n    except MemoryError:\n        failures += 1\n    else:\n        assert isinstance(result, list)\n        successes += 1\nassert failures and successes\nprint('stdlib completed', failures, successes)\n"
    stdlib = stdlib.replace('''        _testcapi.set_nomemory(offset, offset + 1)
        try:
            result = operation()
        finally:
            _testcapi.remove_mem_hooks()
''', '''        result = _testcapi.call_with_nomemory(offset, offset + 1, operation)
''')
    full = stdlib.replace('import traceback\n', 'import traceback\nfrom functools import partial\n')
    full = full.replace('''    except ValueError as error:
        stack = traceback.StackSummary.extract(traceback.walk_tb(error.__traceback__),
                                               capture_locals=False, lookup_lines=False)
    operation = stack.format''', '''    except ValueError as error:
        saved = error
    operation = partial(traceback.format_exception, saved)''')
    assert hashlib.sha256(full.encode()).hexdigest() == "3704f31cf850f897874564684da8987fca3ed62fa8e1d70abb7f43de237bdeb2"
    # Verified disassembly for this exact Linux 3.15.2t SDK: rbx owns the
    # unpublished PyList_New object; size/item/allocated are +32/+40/+48.
    # Only the inferior process is changed. No SDK file or product is patched.
    guard = r'''
set pagination off
set confirm off
if *(unsigned short *)((char *)PyList_New+240) != 0x438b
  echo UNSUPPORTED_DEBUG_SDK\n
  quit 3
end
if *(unsigned int *)((char *)PyList_New+85) != 0x284b8948
  echo UNSUPPORTED_LIST_ITEM_LAYOUT\n
  quit 3
end
set $guard_hits = 0
break *((char *)PyList_New+240)
commands
  silent
  set $guard_hits = $guard_hits + 1
  set {long long}($rbx+32) = 0
  set {long long}($rbx+40) = 0
  set {long long}($rbx+48) = 0
  continue
end
echo HOST_LIST_GUARD_READY\n
run
printf "HOST_LIST_GUARD_HITS=%ld\n", $guard_hits
'''
    debugger = shutil.which('gdb')
    assert debugger is not None, 'This bounded check needs the existing Linux debugger'
    records: list[dict[str, object]] = []
    with tempfile.TemporaryDirectory(prefix='sqlaero-list-cleanup-') as directory:
        commands = Path(directory) / 'guard.gdb'
        commands.write_text(guard)
        for name, program, corrected in (
            ('gdb-native-original-unpatched', original, False),
            ('gdb-stdlib-full-unpatched', full, False),
            ('gdb-native-original-host-corrected', original, True),
            ('gdb-native-scoped256-host-corrected', native, True),
            ('gdb-stdlib-full-host-corrected', full, True),
        ):
            assert 'faulthandler' not in program
            options = ['-x', str(commands)] if corrected else [
                '-ex', 'run', '-ex', 'thread apply all bt 30', '-ex', 'frame 2',
                '-ex', 'info registers rbx r12 r13', '-ex', 'x/8gx $rbx']
            result = subprocess.run([debugger, '-q', '--batch', *options,
                                     '--args', sys.executable, '-'],
                                    input=program.encode(), capture_output=True,
                                    timeout=90, check=False)
            records.append({'variant': name, 'repeat': 0, 'exit': result.returncode,
                            'source_sha256': hashlib.sha256(program.encode()).hexdigest(),
                            'python_executable_sha256': hashlib.sha256(Path(sys.executable).read_bytes()).hexdigest(),
                            'stdout': result.stdout.decode(errors='replace')[-45000:],
                            'stderr': result.stderr.decode(errors='replace')[-3000:]})
    with capsys.disabled():
        print('Untraced ownership diagnosis: ' + json.dumps(records), flush=True)

