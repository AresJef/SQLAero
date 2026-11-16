"""Schema storage cannot turn an empty document into recursive native stack work."""

import os
import pathlib
import subprocess
import sys

import pytest

from _native_signal import private_console_options


@pytest.mark.parametrize(("depth", "empty"), [(64, False), (65, False), (256, True), (4096, True)])
def test_source_schema_depth_is_distinct_from_actual_output_depth(depth: int, empty: bool) -> None:
    script = f"depth={depth!r}\nempty={empty!r}\n" + """
import numpy as np

from sqlaero import json_dumps
dtype=np.dtype('i8')
for _ in range(depth): dtype=np.dtype([('x',dtype)])
value=np.empty(0,dtype=dtype) if empty else np.zeros((),dtype=dtype)
print('SOURCE_READY',np.__version__,flush=True)
if empty:
    assert json_dumps(value)==b'[]'
elif depth==64:
    assert json_dumps(value)==b'{"x":'*64+b'0'+b'}'*64
else:
    try: json_dumps(value)
    except ValueError as error: assert '64' in str(error)
    else: raise AssertionError('actual output nesting was not limited')
print('DELIVERED',flush=True)
"""
    result = subprocess.run([sys.executable, "-c", script], capture_output=True,
                            text=True, timeout=30, check=False)
    assert result.returncode == 0, (result.returncode, result.stdout, result.stderr)
    assert "SOURCE_READY" in result.stdout and "DELIVERED" in result.stdout


@pytest.mark.parametrize("kind", ["schema", "records", "single_field"])
def test_record_schema_and_payload_work_preserve_original_repeated_signal(kind: str) -> None:
    script = f"kind={kind!r}\n" + """
import numpy as np
from _native_signal import NativeAlarm, native_call_offsets
from sqlaero import json_dumps
if kind=='schema':
    dtype=np.dtype('i8')
    for _ in range(4096): dtype=np.dtype([('x',dtype)])
    value=np.empty(0,dtype=dtype)
elif kind=='records':
    value=np.zeros(1000000,dtype=[('id','i8'),('name','U1')])
else:
    value=np.array([('a'*2000000,)],dtype=[('text','U2000000')])[0]
json_dumps(np.zeros(1,dtype=[('id','i8')]))
original=MemoryError('original growing record work error')
count=0
def serialize():
    return json_dumps(value)
offsets=native_call_offsets(serialize.__code__)
def interrupt(number,frame):
    global count
    if frame is None or frame.f_code is not serialize.__code__ or frame.f_lasti not in offsets:
        return
    count+=1
    if count==3:
        alarm.cancel()
        raise original
alarm=NativeAlarm(interrupt)
try:
    alarm.start(0.0001,0.0001,wait_ready=True)
    try: serialize()
    except MemoryError as error:
        assert error is original and count==3
        assert 'while serializing JSON at data' in '\\n'.join(error.__notes__)
    else: raise AssertionError('record work did not observe repeated signals')
finally:
    alarm.close()
"""
    environment = dict(os.environ)
    environment["PYTHONPATH"] = str(pathlib.Path(__file__).parent) + os.pathsep + environment.get("PYTHONPATH", "")
    result = subprocess.run([sys.executable, "-c", script], **private_console_options(environment),
                            capture_output=True, text=True, timeout=30, check=False)
    assert result.returncode == 0, result.stdout + result.stderr
