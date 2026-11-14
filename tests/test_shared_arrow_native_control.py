"""Native import preserves offsets, ownership and bounded controllable work."""

from decimal import Decimal
from datetime import datetime, timedelta, tzinfo
import gc
import json
import struct
import weakref

import pytest
from sqlaero import _sqlaero, json_dumps
from _native_signal import NativeAlarm

pa = pytest.importorskip("pyarrow")


def unaligned(data: bytes):
    # The public slice intentionally starts one byte after a real allocation.
    # It is an Arrow source buffer, not an Escape memoryview input.
    return pa.py_buffer(b"!" + data).slice(1)


@pytest.mark.parametrize(("dtype", "payload", "expected"), [
    (pa.int64(), struct.pack("=qqq", -1, 0, 7), [-1, 0, 7]),
    (pa.float64(), struct.pack("=ddd", 0.5, -0.0, 1.5), [0.5, -0.0, 1.5]),
    (pa.date32(), struct.pack("=iii", 0, -1, 1), ["1970-01-01", "1969-12-31", "1970-01-02"]),
    (pa.decimal128(10, 3), b"".join(n.to_bytes(16, "little", signed=True) for n in [12500, -12500, 0]), ["12.500", "-12.500", "0.000"]),
    (pa.decimal256(76, 3), b"".join(n.to_bytes(32, "little", signed=True) for n in [12500, -12500, 0]), ["12.500", "-12.500", "0.000"]),
])
@pytest.mark.parametrize("sliced", [False, True])
def test_unaligned_primitive_buffers_keep_actual_values(dtype, payload: bytes, expected: list, sliced: bool) -> None:
    source = pa.Array.from_buffers(dtype, 3, [None, unaligned(payload)])
    if sliced:
        source, expected = source.slice(1), expected[1:]
    assert json.loads(json_dumps(source)) == expected
    # Decimal SQL is a numeric value, while JSON preserves its scale as text.
    sql = _sqlaero._escape_projection(source)
    if pa.types.is_decimal(dtype):
        assert [Decimal(item) for item in sql] == [Decimal(item) for item in expected]
    elif pa.types.is_integer(dtype) or pa.types.is_floating(dtype):
        assert [float(item) for item in sql] == expected


@pytest.mark.parametrize("wide", [False, True])
def test_unaligned_byte_and_list_offsets_keep_empty_and_unicode_values(wide: bool) -> None:
    code = "q" if wide else "i"
    dtype = pa.large_string() if wide else pa.string()
    source = pa.Array.from_buffers(dtype, 3, [None, unaligned(struct.pack("=" + code * 4, 0, 1, 3, 3)), pa.py_buffer(b"a\xc3\xa9")])
    assert json_dumps(source) == b'["a","\xc3\xa9",""]'
    assert _sqlaero._escape_projection(source) == ("'a'", "'\u00e9'", "''")
    dtype = pa.large_list(pa.int64()) if wide else pa.list_(pa.int64())
    source = pa.Array.from_buffers(dtype, 3, [None, unaligned(struct.pack("=" + code * 4, 0, 2, 2, 3))], children=[pa.array([7, 8, 9])])
    assert json_dumps(source) == b"[[7,8],[],[9]]"
    assert json_dumps(source.slice(1)) == b"[[],[9]]"


@pytest.mark.parametrize("fixed", [False, True])
def test_nested_struct_slice_preserves_child_offsets_and_validity(fixed: bool) -> None:
    sequence_type = pa.list_(pa.int64(), 2) if fixed else pa.list_(pa.int64())
    sequence = pa.array([[0, 0], [1, 2], [3, 4], [5, 6], [7, 8]], type=sequence_type).slice(1, 4)
    child = pa.StructArray.from_arrays([sequence, pa.array([0, 1, 2, 3])], names=["items", "n"], mask=pa.array([False, True, False, False]))
    outer = pa.StructArray.from_arrays([child], names=["record"]).slice(1, 2)
    assert json_dumps(outer) == b'[{"record":null},{"record":{"items":[5,6],"n":2}}]'
    assert _sqlaero._escape_projection(outer) == ("(NULL)", "(((5,6),2))")


@pytest.mark.parametrize("fails", [False, True])
def test_unaligned_import_releases_original_owner_on_success_and_failure(fails: bool) -> None:
    np = pytest.importorskip("numpy")
    owner = np.empty(1 + 4096 * 8, dtype=np.uint8)
    owner[1:] = np.arange(4096, dtype=np.int64).view(np.uint8)
    source = pa.Array.from_buffers(pa.int64(), 4096, [None, pa.py_buffer(owner).slice(1)])
    reference = weakref.ref(owner)
    del owner
    if fails:
        with pytest.raises(_sqlaero.OperationalError):
            _sqlaero._bind_sql("SELECT %s", (source,), limit=20)
    else:
        assert json.loads(json_dumps(source)) == list(range(4096))
    del source
    gc.collect()
    assert reference() is None


@pytest.mark.parametrize("kind", ["unaligned-copy", "wide-schema"])
def test_long_native_preparation_preserves_original_signal(kind: str) -> None:
    if kind == "unaligned-copy":
        source = pa.Array.from_buffers(pa.int64(), 4 * 1024 * 1024, [None, unaligned(bytes(32 * 1024 * 1024))])
    else:
        # No result rows can account for the work: schema/native preparation
        # itself must return through the shared control boundary.
        source = pa.StructArray.from_arrays([pa.array([], type=pa.int64())] * 16384, names=[f"f{i}" for i in range(16384)])
    original = RuntimeError("original signal during native source preparation")
    deliveries = 0

    def interrupt(number, frame) -> None:
        nonlocal deliveries
        if frame is None or frame.f_code is not test_long_native_preparation_preserves_original_signal.__code__:
            return
        deliveries += 1
        alarm.cancel()
        raise original

    class Arm(tzinfo):
        def utcoffset(self, value):
            alarm.start(0.001, 0.001, wait_ready=True)
            return timedelta(0)

    alarm = NativeAlarm(interrupt)
    try:
        with pytest.raises(RuntimeError) as caught:
            _sqlaero._bind_sql("SELECT %s,%s", (datetime(2024, 1, 1, tzinfo=Arm()), source))
        assert caught.value is original and deliveries == 1
    finally:
        alarm.close()
