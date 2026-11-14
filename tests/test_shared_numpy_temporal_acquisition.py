"""Finite-unit protocols are acquired only for actual finite temporal cells."""

import subprocess
import sys

import pytest


@pytest.mark.parametrize("target", ["json", "mysql"])
@pytest.mark.parametrize("unit", ["ns", "ps", "fs", "as", "generic", "0ns"])
def test_missing_and_empty_arrays_do_not_invoke_finite_unit_protocol(target: str, unit: str) -> None:
    script = f"target={target!r}\nunit={unit!r}\n" + """
import numpy as np
from sqlaero import _sqlaero, json_dumps
original = np.datetime_data
calls = []
failure = MemoryError('finite unit API is deliberately unavailable')
def metadata(dtype):
    calls.append(dtype)
    raise failure
np.datetime_data = metadata
# Prepare the public type API, but no finite temporal facts.
assert _sqlaero._encode_value(np.int64(7)).to_bytes() == b'7'
convert = json_dumps if target == 'json' else lambda value: _sqlaero._encode_value(value).to_bytes()
for kind in ['M8', 'm8']:
    dtype = np.dtype(kind if unit == 'generic' else kind + '[' + unit + ']')
    missing = np.full(2, -(1 << 63), dtype='i8').view(dtype)
    assert convert(missing) == (b'[null,null]' if target == 'json' else b'(NULL,NULL)')
    assert convert(missing[:0]) == (b'[]' if target == 'json' else b'()')
    assert calls == []
    finite = np.array([-(1 << 63), 1], dtype='i8').view(dtype)
    try:
        convert(finite)
    except MemoryError as caught:
        assert caught is failure
    else:
        raise AssertionError('finite unit protocol was skipped')
    assert len(calls) == 1
    calls.clear()
"""
    result = subprocess.run([sys.executable, "-c", script], capture_output=True,
                            text=True, timeout=20, check=False)
    assert result.returncode == 0, result.stdout + result.stderr


def test_mysql_rejected_tensor_does_not_run_an_unneeded_finite_unit_protocol() -> None:
    script = """
import numpy as np
from sqlaero import _sqlaero
calls = []
def metadata(dtype):
    calls.append(dtype)
    raise AssertionError('MySQL rejected axes need no finite unit call')
np.datetime_data = metadata
value = np.array([1], dtype='i8').view('M8[ns]').reshape(1, 1, 1)
try:
    _sqlaero._encode_value(value)
except ValueError as caught:
    assert 'axes' in str(caught)
else:
    raise AssertionError('MySQL tensor must be rejected')
assert not calls
"""
    result = subprocess.run([sys.executable, "-c", script], capture_output=True,
                            text=True, timeout=20, check=False)
    assert result.returncode == 0, result.stdout + result.stderr


@pytest.mark.parametrize("target", ["json", "mysql"])
@pytest.mark.parametrize("change", ["dtype", "shape"])
def test_finite_metadata_callback_rechecks_actual_source_layout(target: str, change: str) -> None:
    script = f"target={target!r}\nchange={change!r}\n" + """
import numpy as np
from sqlaero import _sqlaero, json_dumps
source = np.array([1, 2], dtype='i8').view('M8[ns]')
original = np.datetime_data
calls = []
def metadata(dtype):
    calls.append(dtype)
    if change == 'dtype':
        source.dtype = 'i8'
    else:
        source.shape = (1, 2)
    return original(dtype)
np.datetime_data = metadata
convert = json_dumps if target == 'json' else lambda value: _sqlaero._encode_value(value).to_bytes()
try:
    convert(source)
except BufferError:
    pass
else:
    raise AssertionError('changed source layout was accepted after its protocol')
assert len(calls) == 1
"""
    result = subprocess.run([sys.executable, "-c", script], capture_output=True,
                            text=True, timeout=20, check=False)
    assert result.returncode == 0, result.stdout + result.stderr


@pytest.mark.parametrize("target", ["json", "mysql"])
def test_finite_unit_snapshot_is_call_local_and_reused_within_one_array(target: str) -> None:
    script = f"target={target!r}\n" + """
import numpy as np
from sqlaero import _sqlaero, json_dumps
original = np.datetime_data
calls = []
def metadata(dtype):
    calls.append(dtype)
    return original(dtype)
np.datetime_data = metadata
source = np.array([0, 1, 2], dtype='i8').view('m8[s]')
convert = json_dumps if target == 'json' else lambda value: _sqlaero._encode_value(value).to_bytes()
expected = b'["P0D","PT1S","PT2S"]' if target == 'json' else b"('00:00:00','00:00:01','00:00:02')"
assert convert(source) == expected and len(calls) == 1
assert convert(source) == expected and len(calls) == 2
"""
    result = subprocess.run([sys.executable, "-c", script], capture_output=True,
                            text=True, timeout=20, check=False)
    assert result.returncode == 0, result.stdout + result.stderr
