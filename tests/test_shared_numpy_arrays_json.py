"""The same checked ndarray cells feed JSON without Python scalar materialization."""

import json

import pytest
from sqlaero import JSON, _sqlaero, json_dumps

np = pytest.importorskip("numpy")


@pytest.mark.parametrize("dtype", ["i1", "i2", "i4", "i8", "u1", "u2", "u4", "u8", "f2", "f4", "f8", "?", "U8", "O"])
@pytest.mark.parametrize("layout", ["vector", "reverse", "matrix", "transpose", "empty", "zero_columns"])
def test_standard_array_values_and_axes_are_shared(dtype: str, layout: str) -> None:
    value = np.array([[0, 1, 2], [3, 4, 5]], dtype=dtype)
    if layout == "vector":
        value = value[0]
    elif layout == "reverse":
        value = value[0, ::-1]
    elif layout == "transpose":
        value = value.T
    elif layout == "empty":
        value = value[:0]
    elif layout == "zero_columns":
        value = value[:, :0]
    expected = value.tolist()
    assert json.loads(json_dumps(value)) == expected
    # The wrapper remains exactly one SQL text value containing that document.
    assert _sqlaero._encode_value(JSON(value)).to_bytes() == _sqlaero._encode_value(json_dumps(value).decode()).to_bytes()


@pytest.mark.parametrize("dtype", [">i2", ">i4", ">i8", ">u2", ">u4", ">u8", ">f2", ">f4", ">f8"])
def test_non_native_order_arrays_preserve_actual_values(dtype: str) -> None:
    values = np.array([[1, 2], [3, 4]], dtype=dtype)
    assert json.loads(json_dumps(values)) == [[1, 2], [3, 4]]


def test_actual_float_width_nonfinite_and_negative_zero_policies() -> None:
    assert json_dumps(np.array([0.1], dtype="f4")) == b"[0.10000000149011612]"
    for dtype in ["f2", "f4", "f8"]:
        values = np.array([-0.0, float("nan"), float("inf"), -float("inf")], dtype=dtype)
        assert json_dumps(values) == b"[-0.0,null,null,null]"
        with pytest.raises(ValueError, match="allow_nan"):
            json_dumps(values, allow_nan=False)


@pytest.mark.parametrize("dtype", ["S0", "S2", "V2", "c8", "c16"])
def test_unsupported_json_schema_is_not_hidden_by_empty_arrays(dtype: str) -> None:
    for values in [np.empty(0, dtype=dtype), np.zeros(1, dtype=dtype)]:
        with pytest.raises(TypeError):
            json_dumps(values)


def test_array_unicode_nul_emoji_and_actual_surrogate_error() -> None:
    values = np.array(["a\0b", "😀", ""], dtype="U4")
    assert json_dumps(values) == '["a\\u0000b","😀",""]'.encode()
    for value in ["\ud800", "\ud800\udc00"]:
        with pytest.raises(UnicodeEncodeError) as caught:
            json_dumps(np.array([value], dtype="U2"))
        assert "data[0]" in " ".join(caught.value.__notes__)


def test_array_objects_keep_structure_missing_and_true_cycle_boundaries() -> None:
    values = np.empty(2, dtype=object)
    values[:] = [{"key": [7]}, None]
    assert json_dumps(values) == b'[{"key":[7]},null]'
    values[1] = values
    with pytest.raises(ValueError, match="cycle"):
        json_dumps(values)


@pytest.mark.parametrize("unit", ["ps", "fs", "as", "generic", "0ns"])
@pytest.mark.parametrize("tag", ["M8", "m8"])
def test_array_nat_precedes_finite_unit_policy(unit: str, tag: str) -> None:
    suffix = "" if unit == "generic" else "[" + unit + "]"
    values = np.array([np.iinfo("i8").min], dtype="i8").view(tag + suffix)
    assert json_dumps(values) == b"[null]"
    with pytest.raises((TypeError, ValueError)):
        json_dumps(np.array([1], dtype="i8").view(tag + suffix))


def test_temporal_metadata_uses_actual_short_unicode_storage() -> None:
    import pathlib
    import subprocess
    import sys
    script = """
import pathlib, sys
import numpy as np
import sqlaero
assert str(pathlib.Path(sqlaero._sqlaero.__file__).resolve()) == sys.argv[1]
class Unit(str):
    def __len__(self):
        raise AssertionError('must read actual Unicode length')
    def __str__(self):
        raise AssertionError('must read actual Unicode value')
np.datetime_data = lambda dtype: (Unit('D'), 1)
value = np.array([1], dtype='i8').view('M8[D]')
assert sqlaero.json_dumps(value) == b'["1970-01-02T00:00:00"]'
assert sqlaero._sqlaero._encode_value(value).to_bytes() == b"('1970-01-02 00:00:00')"
"""
    result = subprocess.run([sys.executable, "-c", script, str(pathlib.Path(_sqlaero.__file__).resolve())],
                            capture_output=True, text=True, timeout=30, check=False)
    assert result.returncode == 0, result.stdout + result.stderr


def test_typed_array_charges_its_actual_container_depth() -> None:
    source = np.empty(1, dtype=object)
    value: object = 7
    for _ in range(63):
        value = [value]
    source[0] = value
    assert json.loads(json_dumps(source)) == [value]
    source[0] = [value]
    with pytest.raises(ValueError, match="64"):
        json_dumps(source)


def test_stable_typed_arrays_have_independent_concurrent_readers() -> None:
    from concurrent.futures import ThreadPoolExecutor
    values = np.arange(128, dtype=">i8")[::-1]
    expected = list(range(127, -1, -1))
    def read(_index: int) -> None:
        for _ in range(20):
            assert json.loads(json_dumps(values)) == expected
    with ThreadPoolExecutor(max_workers=4) as pool:
        list(pool.map(read, range(4)))


@pytest.mark.parametrize("shape", ["many", "single", "error_payload", "tensor", "empty_tensor"])
def test_typed_array_long_native_work_preserves_original_signal(shape: str) -> None:
    import os
    import pathlib
    import subprocess
    import sys
    script = "shape = " + repr(shape) + "\n" + """
import numpy as np
from _native_signal import NativeAlarm, native_call_offsets
from sqlaero import json_dumps
value = (np.arange(1000000, dtype='i8') if shape == 'many' else np.array(['a' * 2000000]) if shape == 'single'
         else np.array([chr(0xd800) + 'a' * 2000000]) if shape == 'error_payload'
         else np.arange(1000000, dtype='i8').reshape(100, 100, 100) if shape == 'tensor'
         else np.empty((1000000, 0, 2), dtype='i8'))
json_dumps(np.array([1], dtype='i8'))
original = MemoryError('original growing typed-array work error')
count = 0
def serialize():
    return json_dumps(value)
offsets = native_call_offsets(serialize.__code__)
def interrupt(number, frame):
    global count
    if frame is None or frame.f_code is not serialize.__code__ or frame.f_lasti not in offsets:
        return
    count += 1
    if count == 3:
        alarm.cancel()
        raise original
alarm = NativeAlarm(interrupt)
try:
    alarm.start(0.0001, 0.0001, wait_ready=True)
    try:
        serialize()
    except MemoryError as error:
        assert error is original and count == 3
        assert 'while serializing JSON at data' in '\\n'.join(error.__notes__)
    else:
        raise AssertionError('typed native work did not observe repeated signals')
finally:
    alarm.close()
"""
    environment = dict(os.environ)
    environment["PYTHONPATH"] = str(pathlib.Path(__file__).parent) + os.pathsep + environment.get("PYTHONPATH", "")
    result = subprocess.run([sys.executable, "-c", script], env=environment,
                            capture_output=True, text=True, timeout=20, check=False)
    assert result.returncode == 0, result.stdout + result.stderr
