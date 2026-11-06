"""Native NumPy input values retain exact source semantics without boxing."""

# pyright: reportPrivateUsage=false

from collections.abc import Callable
import math
import subprocess
import sys

import pytest

from sqlaero import _sqlaero
from sqlaero._sqlaero import DataError, ProgrammingError

np = pytest.importorskip("numpy")


@pytest.mark.parametrize("many", [False, True])
@pytest.mark.parametrize("itemize", [False, True])
def test_zero_field_record_retains_its_one_logical_row(many: bool, itemize: bool) -> None:
    value = np.zeros(1, dtype=[]).view(np.recarray)[0]
    sql, expected = ("SELECT 1", b"\x03SELECT 1") if itemize else ("VALUES %s", b"\x03VALUES ()")
    assert _sqlaero._bind_sql(sql, value, many=many, itemize=itemize) == [expected]
    assert _sqlaero._encode_value(value).to_bytes() == b"()"


@pytest.mark.parametrize("many", [False, True])
@pytest.mark.parametrize("itemize", [False, True])
@pytest.mark.parametrize("levels", [62, 63])
def test_natural_numpy_table_counts_the_root_and_current_row(
    many: bool, itemize: bool, levels: int,
) -> None:
    cell: object = 7
    for _ in range(levels):
        cell = [cell]
    source = np.empty((1, 1), dtype=object)
    source[0, 0] = cell
    if levels == 63:
        with pytest.raises(DataError) as caught:
            _sqlaero._bind_sql("SELECT %s", source, many=many, itemize=itemize)
        assert caught.value.code == 65543
    else:
        count = levels + (not itemize)
        assert _sqlaero._bind_sql("SELECT %s", source, many=many, itemize=itemize) == [
            b"\x03SELECT " + b"(" * count + b"7" + b")" * count,
        ]


def test_numpy_shared_object_input_with_application_lock_keeps_values_and_gil_state() -> None:
    script = r'''
import concurrent.futures
import sys
import threading
before = getattr(sys, '_is_gil_enabled', lambda: True)()
import numpy as np
from sqlaero import _sqlaero
assert getattr(sys, '_is_gil_enabled', lambda: True)() == before
source = np.empty(16, dtype=object)
source[:] = [100003] * 16
lock = threading.Lock()
barrier = threading.Barrier(2)
def writer():
    barrier.wait(timeout=10)
    for number in range(100004, 100068):
        with lock:
            source[:] = [int(str(number))] * 16
def reader():
    encoder = _sqlaero._prepare_encoder()
    barrier.wait(timeout=10)
    for _ in range(64):
        with lock:
            literal = encoder.encode(source).to_bytes()
            expected = b'(' + b','.join([str(source[0]).encode()] * 16) + b')'
            assert literal == expected
with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
    futures = [pool.submit(writer), pool.submit(reader)]
    for future in futures:
        future.result(timeout=20)
assert getattr(sys, '_is_gil_enabled', lambda: True)() == before
'''
    result = subprocess.run([sys.executable, "-c", script], capture_output=True, text=True, timeout=30)
    assert result.returncode == 0, result.stdout + result.stderr


@pytest.mark.parametrize("kind", ["array", "record"])
def test_numpy_allocation_callback_keeps_its_actual_argument_error_and_retry(kind: str) -> None:
    script = r'''
import sys
import weakref
import numpy as np
from sqlaero import _sqlaero as native

state = getattr(sys, '_is_gil_enabled', lambda: True)()
encoder = native._prepare_encoder()
assert encoder.encode(np.int64(7)).to_bytes() == b'7'
if sys.argv[1] == 'array':
    value = np.array([1, 2], dtype=np.int64)
    expected = b'(1,2)'
else:
    value = np.array([(7, 'a')], dtype=[('i', 'i8'), ('s', 'U16')]).view(np.recarray)[0]
    expected = b"(7,'a')"

original = MemoryError('actual allocation-owner callback')
reference = weakref.ref
calls = []
def fail(owner, *args, **kwargs):
    if isinstance(owner, np.ndarray):
        calls.append(owner)
        raise original
    return reference(owner, *args, **kwargs)

weakref.ref = fail
try:
    try:
        encoder.encode(value)
    except MemoryError as caught:
        assert caught is original
    else:
        raise AssertionError('the real owner callback was skipped')
finally:
    weakref.ref = reference
assert len(calls) == 1
assert encoder.encode(value).to_bytes() == expected
assert getattr(sys, '_is_gil_enabled', lambda: True)() == state
'''
    completed = subprocess.run(
        [sys.executable, "-c", script, kind], capture_output=True, text=True, timeout=20,
    )
    assert completed.returncode == 0, completed.stdout + completed.stderr


@pytest.mark.parametrize("dtype", [
    np.int8, np.int16, np.int32, np.int64, np.longlong,
    np.uint8, np.uint16, np.uint32, np.uint64, np.ulonglong,
])
def test_numpy_integer_scalar_preserves_full_logical_range(dtype: type) -> None:
    limits = np.iinfo(dtype)
    for value in (limits.min, 0, limits.max):
        scalar = dtype(value)
        assert _sqlaero._encode_value(scalar).to_bytes() == str(value).encode("ascii")


@pytest.mark.parametrize("dtype", [np.float16, np.float32, np.float64])
@pytest.mark.parametrize("value", [0.0, -0.0, 0.1, 1.0, -1.5, float("nan"), float("inf"), -float("inf")])
def test_numpy_float_scalar_preserves_the_actual_binary_value(dtype: type, value: float) -> None:
    scalar = dtype(value)
    raw = _sqlaero._encode_value(scalar).to_bytes()
    native = float(scalar)
    if not math.isfinite(native):
        assert raw == b"NULL"
    else:
        assert b"e" in raw.lower()
        assert float(raw).hex() == native.hex()


def test_numpy_half_all_bit_patterns_preserve_the_promoted_value() -> None:
    values = np.arange(65536, dtype=np.uint16).view(np.float16)
    encoder = _sqlaero._prepare_encoder()
    for scalar in values:
        raw = encoder.encode(scalar).to_bytes()
        native = float(scalar)
        if math.isfinite(native):
            assert float(raw).hex() == native.hex()
        else:
            assert raw == b"NULL"


@pytest.mark.parametrize(("value", "expected"), [
    (np.bool_(False), b"0"),
    (np.bool_(True), b"1"),
    (np.str_("中文'🙂"), _sqlaero._encode_scalar("中文'🙂")),
    (np.bytes_(b"a\x00\xff\x00"), _sqlaero._encode_scalar(b"a\x00\xff\x00")),
])
def test_numpy_bool_text_and_binary_scalar_values(value: object, expected: bytes) -> None:
    assert _sqlaero._encode_value(value).to_bytes() == expected


@pytest.mark.parametrize(("value", "expected"), [
    (np.datetime64("2024-01-02T03:04:05.123456789", "ns"), b"'2024-01-02 03:04:05.123456789'"),
    (np.datetime64("1969-12-31T23:59:59.999999999", "ns"), b"'1969-12-31 23:59:59.999999999'"),
    (np.datetime64(1, "2D"), b"'1970-01-03 00:00:00'"),
    (np.timedelta64(1, "2D"), b"'48:00:00'"),
    (np.timedelta64(-1, "ns"), b"'-00:00:00.000000001'"),
    (np.timedelta64(1, "Y"), b"'8765:49:12'"),
    (np.timedelta64(1, "M"), b"'730:29:06'"),
])
def test_numpy_temporal_scalar_keeps_units_multipliers_and_nanoseconds(value: object, expected: bytes) -> None:
    assert _sqlaero._encode_value(value).to_bytes() == expected


@pytest.mark.parametrize("constructor", [np.datetime64, np.timedelta64])
@pytest.mark.parametrize("unit", [None, "D", "ns", "ps", "fs", "as"])
def test_numpy_nat_is_null_before_unit_interpretation(constructor: type, unit: str | None) -> None:
    value = constructor("NaT") if unit is None else constructor("NaT", unit)
    assert _sqlaero._encode_value(value).to_bytes() == b"NULL"


@pytest.mark.parametrize("value", [np.complex64(1 + 2j), np.complex128(1 + 2j), np.longdouble("0.1")])
def test_numpy_unlisted_scalar_kinds_remain_rejected(value: object) -> None:
    with pytest.raises(TypeError):
        _sqlaero._encode_value(value)


def test_numpy_cold_scalar_preparation_and_shared_immutable_values_are_thread_safe() -> None:
    script = """
import concurrent.futures
import sys
import threading
before = getattr(sys, '_is_gil_enabled', lambda: True)()
import numpy as np
from sqlaero import _sqlaero
assert getattr(sys, '_is_gil_enabled', lambda: True)() == before
barrier = threading.Barrier(4)
integer = np.uint64(2**64 - 1)
fraction = np.float32(0.1)
def run(index):
    barrier.wait(timeout=10)
    encoder = _sqlaero._prepare_encoder()
    for _ in range(64):
        assert encoder.encode(integer).to_bytes() == b'18446744073709551615'
        assert float(encoder.encode(fraction).to_bytes()).hex() == float(fraction).hex()
    return index
with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
    assert sorted(pool.map(run, range(4))) == list(range(4))
assert getattr(sys, '_is_gil_enabled', lambda: True)() == before
"""
    result = subprocess.run(
        [sys.executable, "-c", script],
        capture_output=True,
        text=True,
        timeout=30,
        check=False,
    )
    assert result.returncode == 0, result.stdout + result.stderr


@pytest.mark.parametrize(("many", "itemize", "sql", "expected"), [
    (False, True, "SELECT %s,%s", [b"\x03SELECT 1,2"]),
    (False, False, "SELECT 1 IN %s", [b"\x03SELECT 1 IN (1,2)"]),
    (True, True, "SELECT %s", [b"\x03SELECT 1", b"\x03SELECT 2"]),
    (True, False, "SELECT %s", [b"\x03SELECT 1", b"\x03SELECT 2"]),
])
def test_numpy_vector_roles_are_explicit(
    many: bool, itemize: bool, sql: str, expected: list[bytes],
) -> None:
    value = np.array([1, 2], dtype=np.int64)
    assert _sqlaero._bind_sql(sql, value, many=many, itemize=itemize) == expected


@pytest.mark.parametrize("many", [False, True])
@pytest.mark.parametrize(("itemize", "sql", "expected"), [
    (True, "SELECT %s,%s", [b"\x03SELECT 1,2", b"\x03SELECT 3,4"]),
    (False, "SELECT 1 IN %s", [b"\x03SELECT 1 IN (1,2)", b"\x03SELECT 1 IN (3,4)"]),
])
def test_numpy_two_dimensional_input_retains_natural_rows(
    many: bool, itemize: bool, sql: str, expected: list[bytes],
) -> None:
    value = np.array([[1, 2], [3, 4]], dtype=np.int64)
    assert _sqlaero._bind_sql(sql, value, many=many, itemize=itemize) == expected


@pytest.mark.parametrize("many", [False, True])
def test_numpy_object_vector_keeps_each_tuple_as_one_logical_cell(many: bool) -> None:
    value = np.empty(2, dtype=object)
    value[0] = (1, 2)
    value[1] = (3, 4)
    if many:
        expected = [b"\x03SELECT (1,2)", b"\x03SELECT (3,4)"]
        sql = "SELECT %s"
    else:
        expected = [b"\x03SELECT (1,2),(3,4)"]
        sql = "SELECT %s,%s"
    assert _sqlaero._bind_sql(sql, value, many=many) == expected


@pytest.mark.parametrize("many", [False, True])
@pytest.mark.parametrize("itemize", [False, True])
def test_numpy_empty_root_keeps_zero_position_execution_rule(many: bool, itemize: bool) -> None:
    value = np.array([], dtype=np.int64)
    assert _sqlaero._bind_sql("SELECT 1", value, many=many, itemize=itemize) == [b"\x03SELECT 1"]
    with pytest.raises(ProgrammingError):
        _sqlaero._bind_sql("SELECT %s", value, many=many, itemize=itemize)


@pytest.mark.parametrize("many", [False, True])
@pytest.mark.parametrize("itemize", [False, True])
def test_numpy_nonempty_zero_column_table_does_not_drop_rows(many: bool, itemize: bool) -> None:
    value = np.empty((3, 0), dtype=np.int64)
    assert _sqlaero._bind_sql("SELECT 1", value, many=many, itemize=itemize) == [b"\x03SELECT 1"] * 3
    with pytest.raises(ProgrammingError):
        _sqlaero._bind_sql("SELECT %s", value, many=many, itemize=itemize)


@pytest.mark.parametrize("dtype", [
    np.int8, np.int16, np.int32, np.int64,
    np.uint8, np.uint16, np.uint32, np.uint64,
])
@pytest.mark.parametrize("layout", ["native", "reverse", "transpose", "big_endian", "zero_stride"])
def test_numpy_integer_array_reads_logical_coordinates(dtype: type, layout: str) -> None:
    data = np.arange(6, dtype=dtype).reshape((2, 3))
    if layout == "reverse":
        data = data[:, ::-1]
    elif layout == "transpose":
        data = data.T
    elif layout == "big_endian":
        data = data.astype(data.dtype.newbyteorder(">"))
    elif layout == "zero_stride":
        data = np.broadcast_to(data[:, :1], (2, 3))
    placeholders = ",".join(["%s"] * data.shape[1])
    expected = [
        b"\x03SELECT " + b",".join(str(int(v)).encode("ascii") for v in row)
        for row in data
    ]
    assert _sqlaero._bind_sql("SELECT " + placeholders, data) == expected


def test_numpy_object_self_reference_uses_original_identity_for_cycle_detection() -> None:
    value = np.empty(1, dtype=object)
    value[0] = value
    with pytest.raises(DataError) as caught:
        _sqlaero._encode_value(value)
    assert caught.value.code == 65543


@pytest.mark.parametrize("value", [bytearray(b"a"), memoryview(b"a")])
def test_numpy_object_cell_does_not_reopen_excluded_binary_inputs(value: object) -> None:
    data = np.empty(1, dtype=object)
    data[0] = value
    with pytest.raises(TypeError):
        _sqlaero._encode_value(data)


@pytest.mark.parametrize("many", [False, True])
@pytest.mark.parametrize("itemize", [False, True])
def test_numpy_record_is_one_schema_ordered_row(many: bool, itemize: bool) -> None:
    record = np.array([(7, "ab")], dtype=[("z", "i8"), ("a", "U2")]).view(np.recarray)[0]
    sql = "SELECT %s,%s" if itemize else "SELECT %s"
    expected = b"\x03SELECT 7,'ab'" if itemize else b"\x03SELECT (7,'ab')"
    assert _sqlaero._bind_sql(sql, record, many=many, itemize=itemize) == [expected]


def test_numpy_record_nested_field_keeps_one_outer_cell() -> None:
    record = np.array([((3, 4), 9)], dtype=[("pair", "i8", (2,)), ("last", "i8")]).view(np.recarray)[0]
    assert _sqlaero._bind_sql("SELECT %s,%s", record) == [b"\x03SELECT (3,4),9"]


def test_numpy_records_as_batch_rows_keep_schema_order() -> None:
    records = np.array([(1, 2), (3, 4)], dtype=[("b", "i8"), ("a", "i8")]).view(np.recarray)
    assert _sqlaero._bind_sql("SELECT %s,%s", [records[0], records[1]], many=True) == [
        b"\x03SELECT 1,2", b"\x03SELECT 3,4",
    ]


def test_numpy_record_public_getter_retains_object_identity_and_cycle_protection() -> None:
    record = np.zeros(1, dtype=[("child", object)]).view(np.recarray)[0]
    child = np.empty(1, dtype=object)
    child[0] = record
    record["child"] = child
    with pytest.raises(DataError) as caught:
        _sqlaero._encode_value(record)
    assert caught.value.code == 65543


@pytest.mark.parametrize("value", [
    np.array([1, 2]).view(np.matrix),
    np.zeros((1,), dtype=[("a", "i8")]),
    np.array(1),
    np.zeros((1, 1, 1)),
])
def test_numpy_unconfirmed_container_shapes_and_subclasses_remain_excluded(value: object) -> None:
    with pytest.raises((TypeError, ValueError)):
        _sqlaero._encode_value(value)


@pytest.mark.parametrize("dtype", ["S1", "S8", "S8192"])
@pytest.mark.parametrize("layout", ["native", "reverse", "transpose"])
def test_numpy_fixed_binary_array_removes_padding_and_preserves_embedded_nul(dtype: str, layout: str) -> None:
    data = np.array([[b"a", b"\x00b"], [b"c\x00d", b""]], dtype=dtype)
    if layout == "reverse":
        data = data[:, ::-1]
    elif layout == "transpose":
        data = data.T
    expected = [
        b"\x03SELECT " + b",".join(_sqlaero._encode_scalar(bytes(cell)) for cell in row)
        for row in data
    ]
    assert _sqlaero._bind_sql("SELECT %s,%s", data) == expected


@pytest.mark.parametrize("dtype", ["U4", "U8192", ">U4"])
@pytest.mark.parametrize("layout", ["native", "reverse", "transpose"])
def test_numpy_fixed_unicode_array_uses_actual_ucs4_and_client_encoding(dtype: str, layout: str) -> None:
    data = np.array([["中文", "a\x00b"], ["🙂'", ""]], dtype=dtype)
    if layout == "reverse":
        data = data[:, ::-1]
    elif layout == "transpose":
        data = data.T
    expected = [
        b"\x03SELECT " + b",".join(_sqlaero._encode_scalar(str(cell)) for cell in row)
        for row in data
    ]
    assert _sqlaero._bind_sql("SELECT %s,%s", data) == expected


def test_numpy_unicode_surrogate_preserves_the_strict_codec_error() -> None:
    data = np.array(["\ud800"], dtype="U1")
    with pytest.raises(UnicodeEncodeError):
        _sqlaero._encode_value(data)


@pytest.mark.parametrize("dtype", [np.float16, np.float32, np.float64])
def test_numpy_float_array_nonfinite_is_per_cell_and_negative_zero_remains_signed(dtype: type) -> None:
    data = np.array([0.1, float("nan"), -0.0, float("inf"), -float("inf")], dtype=dtype)
    rows = _sqlaero._bind_sql("SELECT %s", data, many=True)
    values = [row.removeprefix(b"\x03SELECT ") for row in rows]
    assert float(values[0]).hex() == float(data[0]).hex()
    assert values[1] == values[3] == values[4] == b"NULL"
    assert float(values[2]).hex() == "-0x0.0p+0"


@pytest.mark.parametrize("dtype", ["datetime64[2D]", ">M8[2D]", "timedelta64[2D]", ">m8[2D]"])
def test_numpy_temporal_array_keeps_full_multiplier_endian_and_nat(dtype: str) -> None:
    data = np.array([1, np.iinfo(np.int64).min], dtype=np.int64).astype(dtype)
    first = b"'1970-01-03 00:00:00'" if "M" in dtype or dtype.startswith("datetime") else b"'48:00:00'"
    assert _sqlaero._bind_sql("SELECT %s", data, many=True) == [
        b"\x03SELECT " + first, b"\x03SELECT NULL",
    ]


@pytest.mark.parametrize("dtype", ["i8", "S8", "U2", object])
def test_numpy_out_of_allocation_strides_are_rejected_before_element_read(dtype: object) -> None:
    root = np.zeros(1, dtype=dtype)
    invalid = np.lib.stride_tricks.as_strided(root, shape=(4,), strides=(root.itemsize,))
    with pytest.raises(BufferError, match="physical allocation"):
        _sqlaero._encode_value(invalid)


def test_numpy_negative_stride_cannot_walk_before_the_real_allocation() -> None:
    root = np.arange(3, dtype=np.int64)
    invalid = np.lib.stride_tricks.as_strided(root[1:2], shape=(3,), strides=(-8,))
    with pytest.raises(BufferError, match="physical allocation"):
        _sqlaero._encode_value(invalid)


def test_numpy_valid_zero_stride_can_repeat_more_items_than_the_root_length() -> None:
    root = np.array([7], dtype=np.int64)
    repeated = np.lib.stride_tricks.as_strided(root, shape=(20,), strides=(0,))
    assert _sqlaero._bind_sql("SELECT %s", repeated, many=True) == [b"\x03SELECT 7"] * 20


@pytest.mark.parametrize("owner", [bytes(range(8)), bytearray(range(8)), memoryview(bytes(range(8)))])
def test_numpy_direct_buffer_allocation_owners_remain_supported(owner: object) -> None:
    data = np.frombuffer(owner, dtype=np.uint8)[::-1]
    assert _sqlaero._bind_sql("SELECT %s", data, many=True) == [
        b"\x03SELECT " + str(value).encode("ascii") for value in reversed(range(8))
    ]


@pytest.mark.parametrize("record", [False, True])
def test_numpy_callback_storage_replacement_is_rejected_before_reusing_a_view(record: bool) -> None:
    script = r"""
import datetime
import numpy as np
from sqlaero import _sqlaero
record_mode = RECORD_MODE
calls = []
class ReplaceStorage(datetime.tzinfo):
    def utcoffset(self, value):
        calls.append(1)
        root.__setstate__((1, (2,), np.dtype('i8'), False, bytes(16)))
        return datetime.timedelta(0)
first = datetime.datetime(2024, 1, 1, tzinfo=ReplaceStorage())
if record_mode:
    root = np.zeros(1, dtype=[('first', object), ('second', 'i8')])
    root['first'][0] = first
    value = root.view(np.recarray)[0]
else:
    root = np.empty(2, dtype=object)
    root[0] = first
    root[1] = 2
    value = root
try:
    _sqlaero._encode_value(value)
except BufferError as error:
    assert 'stable' in str(error) or 'physical allocation' in str(error)
else:
    raise AssertionError('changed allocation must not be silently reused')
assert calls == [1]
""".replace("RECORD_MODE", str(record))
    result = subprocess.run([sys.executable, "-c", script], capture_output=True, text=True, timeout=30, check=False)
    assert result.returncode == 0, result.stdout + result.stderr


@pytest.mark.parametrize("kind", ["M8", "m8"])
@pytest.mark.parametrize("unit", [None, "D", "ns", "ps", "fs", "as"])
def test_numpy_nat_array_reaches_null_before_finite_unit_validation(kind: str, unit: str | None) -> None:
    dtype = kind if unit is None else f"{kind}[{unit}]"
    value = np.array([np.iinfo(np.int64).min], dtype=np.int64).view(dtype)
    assert _sqlaero._bind_sql("SELECT %s", value, many=True) == [b"\x03SELECT NULL"]


@pytest.mark.parametrize("kind", ["M8", "m8"])
def test_numpy_finite_generic_array_is_not_silently_treated_as_a_time(kind: str) -> None:
    value = np.array([1], dtype=np.int64).view(kind)
    with pytest.raises(DataError, match="finite temporal unit"):
        _sqlaero._encode_value(value)


@pytest.mark.parametrize("kind", ["M8", "m8"])
@pytest.mark.parametrize("missing", [False, True])
@pytest.mark.parametrize("scalar", [False, True])
def test_numpy_zero_unit_metadata_is_only_valid_for_nat(kind: str, missing: bool, scalar: bool) -> None:
    value = np.array([np.iinfo(np.int64).min if missing else 1], dtype=np.int64).view(f"{kind}[0ns]")
    if scalar:
        value = value[0]
    if missing:
        expected = b"NULL" if scalar else b"(NULL)"
        assert _sqlaero._encode_value(value).to_bytes() == expected
    else:
        with pytest.raises(TypeError, match="multiplier must be positive"):
            _sqlaero._encode_value(value)


def test_numpy_native_capsule_owner_has_a_bounded_valid_region() -> None:
    pa = pytest.importorskip("pyarrow")
    value = pa.table({"x": pa.array([1, 2, 3], type=pa.int64())}).to_pandas().iloc[:, 0].to_numpy(copy=False)
    assert _sqlaero._bind_sql("SELECT %s", value, many=True) == [b"\x03SELECT 1", b"\x03SELECT 2", b"\x03SELECT 3"]
    invalid = np.lib.stride_tricks.as_strided(value, shape=(4,), strides=(8,))
    with pytest.raises(BufferError, match="physical allocation"):
        _sqlaero._encode_value(invalid)


def test_numpy_python_base_cannot_impersonate_a_native_capsule_owner() -> None:
    pa = pytest.importorskip("pyarrow")
    value = pa.table({"x": pa.array([1, 2, 3], type=pa.int64())}).to_pandas().iloc[:, 0].to_numpy(copy=False)
    capsule = value
    while isinstance(capsule, np.ndarray):
        capsule = capsule.base
    assert type(capsule).__name__ == "PyCapsule"
    class Owner:
        __array_interface__ = value.__array_interface__
        base = capsule
    disguised = np.asarray(Owner())
    with pytest.raises(TypeError, match="PyCapsule"):
        _sqlaero._encode_value(disguised)


def test_numpy_arrow_array_direct_zero_copy_view_keeps_its_native_buffer_owner() -> None:
    pa = pytest.importorskip("pyarrow")
    value = pa.array([1, 2, 3], type=pa.int64()).to_numpy(zero_copy_only=True)
    assert _sqlaero._bind_sql("SELECT %s", value, many=True) == [b"\x03SELECT 1", b"\x03SELECT 2", b"\x03SELECT 3"]


def test_numpy_cold_preparation_failure_preserves_the_original_and_can_retry() -> None:
    script = r"""
import numpy as np
from sqlaero import _sqlaero
value=np.int64(7)
saved=np.uint64
original=np.__getattr__
marker=RuntimeError('original NumPy API preparation failure')
def missing(name):
    if name=='uint64': raise marker
    return original(name)
del np.uint64
np.__getattr__=missing
assert _sqlaero._encode_value(1).to_bytes()==b'1'
try:
    _sqlaero._encode_value(value)
except RuntimeError as error:
    assert error is marker
else:
    raise AssertionError('incomplete provider preparation must fail')
finally:
    np.uint64=saved
    np.__getattr__=original
assert _sqlaero._encode_value(value).to_bytes()==b'7'
"""
    result = subprocess.run([sys.executable, "-c", script], capture_output=True, text=True, timeout=30, check=False)
    assert result.returncode == 0, result.stdout + result.stderr


def test_standard_inputs_do_not_import_optional_providers() -> None:
    script = r"""
import importlib.abc,sys
class NoProviders(importlib.abc.MetaPathFinder):
    def find_spec(self,fullname,path=None,target=None):
        if fullname.split('.')[0] in {'numpy','pandas','pyarrow','polars'}:
            raise ModuleNotFoundError('optional provider deliberately unavailable',name=fullname)
sys.meta_path.insert(0,NoProviders())
from sqlaero import _sqlaero
assert _sqlaero._bind_sql('SELECT %s,%s',(1,'x'))==[b"\x03SELECT 1,'x'"]
assert not {'numpy','pandas','pyarrow','polars'}.intersection(sys.modules)
"""
    result = subprocess.run([sys.executable, "-c", script], capture_output=True, text=True, timeout=30, check=False)
    assert result.returncode == 0, result.stdout + result.stderr


def test_numpy_noncallable_cold_api_is_not_cached_and_can_recover() -> None:
    script = r"""
import numpy as np
from sqlaero import _sqlaero
value=np.datetime64(1,'s')
original=np.datetime_data
np.datetime_data=0
assert _sqlaero._encode_value(1).to_bytes()==b'1'
try:
    _sqlaero._encode_value(value)
except TypeError as error:
    assert 'callable' in str(error)
else:
    raise AssertionError('noncallable provider function must be rejected')
finally:
    np.datetime_data=original
assert _sqlaero._encode_value(value).to_bytes()==b"'1970-01-01 00:00:01'"
"""
    result = subprocess.run([sys.executable, "-c", script], capture_output=True, text=True, timeout=30, check=False)
    assert result.returncode == 0, result.stdout + result.stderr


@pytest.mark.parametrize(("unit", "date", "duration"), [
    ("Y", "1971-01-01 00:00:00", "8765:49:12"),
    ("M", "1970-02-01 00:00:00", "730:29:06"),
    ("W", "1970-01-08 00:00:00", "168:00:00"),
    ("D", "1970-01-02 00:00:00", "24:00:00"),
    ("h", "1970-01-01 01:00:00", "01:00:00"),
    ("m", "1970-01-01 00:01:00", "00:01:00"),
    ("s", "1970-01-01 00:00:01", "00:00:01"),
    ("ms", "1970-01-01 00:00:00.001", "00:00:00.001"),
    ("us", "1970-01-01 00:00:00.000001", "00:00:00.000001"),
    ("ns", "1970-01-01 00:00:00.000000001", "00:00:00.000000001"),
])
def test_numpy_each_finite_time_unit_has_the_same_scalar_and_array_meaning(unit: str, date: str, duration: str) -> None:
    for kind, expected in (("M8", date), ("m8", duration)):
        value = np.array([1], dtype=np.int64).view(f"{kind}[{unit}]")
        literal = b"'" + expected.encode("ascii") + b"'"
        assert _sqlaero._encode_value(value[0]).to_bytes() == literal
        assert _sqlaero._bind_sql("SELECT %s", value, many=True) == [b"\x03SELECT " + literal]


@pytest.mark.parametrize(("kind", "ticks", "unit", "expected"), [
    ("M8", -1, "M", "1969-12-01 00:00:00"),
    ("M8", -1971, "Y", "-0001-01-01 00:00:00"),
    ("M8", 9223372036854775807, "Y", "9223372036854777777-01-01 00:00:00"),
    ("M8", -9223372036854775807, "Y", "-9223372036854773837-01-01 00:00:00"),
    ("M8", 2, "2147483647Y", "4294969264-01-01 00:00:00"),
    ("m8", -2, "2147483647ns", "-00:00:04.294967294"),
])
def test_numpy_wide_calendar_and_complete_multiplier_are_not_narrowed(kind: str, ticks: int, unit: str, expected: str) -> None:
    value = np.array([ticks], dtype=np.int64).view(f"{kind}[{unit}]")
    literal = b"'" + expected.encode("ascii") + b"'"
    assert _sqlaero._encode_value(value[0]).to_bytes() == literal
    assert _sqlaero._bind_sql("SELECT %s", value, many=True) == [b"\x03SELECT " + literal]


@pytest.mark.parametrize("dtype", ["i2", ">i8", "f4", ">f8"])
def test_numpy_unaligned_numeric_storage_is_read_without_aligned_pointer_access(dtype: str) -> None:
    itemsize = np.dtype(dtype).itemsize
    value = np.ndarray((2,), dtype=dtype, buffer=bytearray(1 + 2 * itemsize), offset=1)
    value[:] = [1, -2]
    assert not value.flags.aligned
    raw = _sqlaero._bind_sql("SELECT %s", value, many=True)
    if value.dtype.kind == "f":
        assert [float(request.removeprefix(b"\x03SELECT ")).hex() for request in raw] == [1.0.hex(), (-2.0).hex()]
    else:
        assert raw == [b"\x03SELECT 1", b"\x03SELECT -2"]


def test_numpy_boolean_array_uses_its_byte_storage_as_logical_boolean_values() -> None:
    value = np.array([True, False, True], dtype=np.bool_)[::-1]
    assert _sqlaero._bind_sql("SELECT %s", value, many=True) == [b"\x03SELECT 1", b"\x03SELECT 0", b"\x03SELECT 1"]


@pytest.mark.parametrize(("dtype", "literal"), [("S0", b"CAST(X'' AS BINARY)"), ("U0", b"''")])
def test_numpy_actual_zero_width_text_storage_encodes_empty_cells(dtype: str, literal: bytes) -> None:
    value = np.ndarray((2,), dtype=dtype)
    assert value.itemsize == 0
    assert _sqlaero._bind_sql("SELECT %s", value, many=True) == [b"\x03SELECT " + literal] * 2


@pytest.mark.parametrize("many", [False, True])
@pytest.mark.parametrize("itemize", [False, True])
def test_numpy_zero_row_table_retains_the_zero_position_root_rule(many: bool, itemize: bool) -> None:
    value = np.empty((0, 2), dtype=np.int64)
    assert _sqlaero._bind_sql("SELECT 1", value, many=many, itemize=itemize) == [b"\x03SELECT 1"]
    with pytest.raises(ProgrammingError):
        _sqlaero._bind_sql("SELECT %s", value, many=many, itemize=itemize)


def test_numpy_record_as_a_cell_and_repeated_noncyclic_fields_remain_nested() -> None:
    record = np.zeros(1, dtype=[("first", object), ("second", object)]).view(np.recarray)[0]
    shared = (1, 2)
    record["first"] = shared
    record["second"] = shared
    assert _sqlaero._bind_sql("SELECT %s", (record,)) == [b"\x03SELECT ((1,2),(1,2))"]


def test_numpy_record_unsupported_field_keeps_the_field_position() -> None:
    record = np.zeros(1, dtype=[("first", "i8"), ("second", object)]).view(np.recarray)[0]
    record["second"] = object()
    with pytest.raises(TypeError) as caught:
        _sqlaero._bind_sql("SELECT %s,%s", record)
    assert "args[1]" in "\n".join(getattr(caught.value, "__notes__", ()))


def test_numpy_record_callback_failure_preserves_the_original_exception_and_releases_views() -> None:
    from datetime import datetime, tzinfo
    original = RuntimeError("original record field conversion")
    class Zone(tzinfo):
        def utcoffset(self, dt: datetime | None):
            raise original
    record = np.zeros(1, dtype=[("field", object)]).view(np.recarray)[0]
    record["field"] = datetime(2024, 1, 1, tzinfo=Zone())
    with pytest.raises(RuntimeError) as caught:
        _sqlaero._encode_value(record)
    assert caught.value is original
    record["field"] = 7
    assert _sqlaero._encode_value(record).to_bytes() == b"(7)"


@pytest.mark.parametrize("kind", ["S", "U"])
def test_numpy_long_fixed_width_value_keeps_the_original_operation_deadline(
    kind: str, record_property: Callable[[str, object], None],
) -> None:
    from time import perf_counter_ns
    from sqlaero._sqlaero import OperationalTimeoutError
    # Prepare the same provider before starting the tested operation budget.
    _sqlaero._bind_sql("SELECT %s", np.array(["x"], dtype=f"{kind}1"))
    # Expand the target-value window after platform-specific preparation costs.
    # The escaped request is about 48MiB, below the unchanged 64MiB limit.
    # This candidate does not establish the cause of the original smaller RED.
    logical_length = 24 * 1024 * 1024
    budget_cap_ns = 50_000_000
    source = np.array(["'" * logical_length], dtype=f"{kind}{logical_length}")
    record_property("source_logical_length", logical_length)
    record_property("source_byte_length", int(source.nbytes))
    record_property("timeout_budget_cap_ns", budget_cap_ns)
    args = (7, source)
    encoder = _sqlaero._prepare_encoder()
    observations_ns: list[int] = []
    for index in range(2):
        started = perf_counter_ns()
        owner = encoder.encode(args)
        elapsed_ns = perf_counter_ns() - started
        observations_ns.append(elapsed_ns)
        record_property(f"native_observation_{index + 1}_ns", elapsed_ns)
        # Both original payloads expand to at least two bytes per source quote.
        # Length checks and owner destruction stay outside the observation.
        assert len(owner) >= 2 * logical_length
        del owner
    timeout_ns = min(budget_cap_ns, min(observations_ns) // 2)
    record_property("timeout_budget_ns", timeout_ns)
    assert timeout_ns > 0, observations_ns
    actual_elapsed_ns = 0
    with pytest.raises(OperationalTimeoutError) as caught:
        started = perf_counter_ns()
        try:
            _sqlaero._bind_sql("SELECT %s,%s", args, timeout_ns=timeout_ns)
        except BaseException as error:
            actual_elapsed_ns = perf_counter_ns() - started
            record_property("bind_elapsed_ns", actual_elapsed_ns)
            record_property("bind_outcome", type(error).__name__)
            record_property("bind_error_code", getattr(error, "code", None))
            record_property("bind_location", "\n".join(getattr(error, "__notes__", ())))
            raise
        else:
            actual_elapsed_ns = perf_counter_ns() - started
            record_property("bind_elapsed_ns", actual_elapsed_ns)
            record_property("bind_outcome", "success")
            record_property("bind_error_code", None)
            record_property("bind_location", "")
    assert caught.value.code == 65544
    notes = "\n".join(getattr(caught.value, "__notes__", ()))
    diagnostics = (observations_ns, timeout_ns, actual_elapsed_ns, notes)
    assert "args[1]" in notes, diagnostics
    assert "args[1][0]" in notes, diagnostics


@pytest.mark.parametrize("kind", ["S", "U"])
def test_numpy_long_fixed_width_value_preserves_the_actual_signal_exception(kind: str) -> None:
    from datetime import datetime, timedelta, tzinfo
    from types import FrameType
    from _native_signal import NativeAlarm
    class InterruptedArray(Exception):
        pass
    original = InterruptedArray("original NumPy signal")
    source = np.array(["'" * (8 * 1024 * 1024)], dtype=f"{kind}{8 * 1024 * 1024}")
    interrupted = False
    def interrupt(signum: int, frame: FrameType | None) -> None:
        nonlocal interrupted
        if interrupted or frame is None or frame.f_code is not test_numpy_long_fixed_width_value_preserves_the_actual_signal_exception.__code__:
            return
        interrupted = True
        alarm.cancel()
        raise original
    class Zone(tzinfo):
        def utcoffset(self, dt: datetime | None) -> timedelta:
            alarm.start(0.0001, 0.0001, wait_ready=True)
            return timedelta(0)
    alarm = NativeAlarm(interrupt)
    try:
        with pytest.raises(InterruptedArray) as caught:
            _sqlaero._bind_sql("SELECT %s,%s", (datetime(2024, 1, 1, tzinfo=Zone()), source))
        # Delivery can reach a final check or return boundary outside a value;
        # the deadline test separately verifies a known native value path.
        assert caught.value is original
    finally:
        alarm.close()


@pytest.mark.parametrize("size", [31, 32, 1024, 1025])
def test_numpy_primitive_runs_keep_every_value_at_short_and_block_boundaries(size: int) -> None:
    value = np.arange(size, dtype=np.int64) - 17
    expected = [b"\x03SELECT " + str(i - 17).encode("ascii") for i in range(size)]
    assert _sqlaero._bind_sql("SELECT %s", value, many=True) == expected


@pytest.mark.parametrize("layout", ["native", "fortran", "transpose", "reverse", "zero_stride", "unaligned", "big_endian"])
def test_numpy_primitive_runs_preserve_actual_two_dimensional_layout(layout: str) -> None:
    value = np.arange(35 * 41, dtype=np.int64).reshape((35, 41))
    if layout == "fortran":
        value = np.asfortranarray(value)
    elif layout == "transpose":
        value = value.T
    elif layout == "reverse":
        value = value[::-1, ::-1]
    elif layout == "zero_stride":
        value = np.broadcast_to(value[:1, :1], value.shape)
    elif layout == "unaligned":
        original = value
        value = np.ndarray(original.shape, dtype="i8", buffer=bytearray(original.nbytes + 1), offset=1)
        value[:] = original
    elif layout == "big_endian":
        value = value.astype(">i8")
    sql = "SELECT " + ",".join(["%s"] * value.shape[1])
    expected = [b"\x03SELECT " + b",".join(str(int(cell)).encode("ascii") for cell in row) for row in value]
    assert _sqlaero._bind_sql(sql, value) == expected


@pytest.mark.parametrize("dtype", [np.float16, np.float32, np.float64])
def test_numpy_primitive_float_runs_keep_actual_bits_and_per_cell_nonfinite_rules(dtype: type) -> None:
    value = np.resize(np.array([0.1, -0.0, float("nan"), float("inf"), -float("inf"), 1.5], dtype=dtype), 1025)
    actual = _sqlaero._bind_sql("SELECT %s", value, many=True)
    assert len(actual) == len(value)
    for cell, request in zip(value, actual, strict=True):
        raw = request.removeprefix(b"\x03SELECT ")
        native = float(cell)
        if math.isfinite(native):
            assert float(raw).hex() == native.hex()
        else:
            assert raw == b"NULL"


@pytest.mark.parametrize("kind", ["M8", "m8"])
def test_numpy_primitive_time_runs_keep_nat_and_complete_units(kind: str) -> None:
    ticks = np.arange(1025, dtype=np.int64)
    ticks[17] = np.iinfo(np.int64).min
    value = ticks.view(f"{kind}[2D]")
    actual = _sqlaero._bind_sql("SELECT %s", value, many=True)
    assert len(actual) == 1025
    assert actual[17] == b"\x03SELECT NULL"
    if kind == "M8":
        assert actual[0] == b"\x03SELECT '1970-01-01 00:00:00'"
        assert actual[1024] == b"\x03SELECT '1975-08-11 00:00:00'"
    else:
        assert actual[0] == b"\x03SELECT '00:00:00'"
        assert actual[1024] == b"\x03SELECT '49152:00:00'"


def test_numpy_future_out_of_allocation_cell_is_not_reported_at_an_earlier_valid_row() -> None:
    root = np.arange(20, dtype=np.int64)
    invalid = np.lib.stride_tricks.as_strided(root, shape=(64,), strides=(root.itemsize,))
    with pytest.raises(BufferError, match="physical allocation") as caught:
        _sqlaero._bind_sql("SELECT %s", invalid, many=True)
    assert "args[20]" in "\n".join(getattr(caught.value, "__notes__", ()))


def test_numpy_primitive_run_keeps_the_original_operation_deadline() -> None:
    from sqlaero._sqlaero import OperationalTimeoutError
    value = np.arange(1000000, dtype=np.int64)
    with pytest.raises(OperationalTimeoutError) as caught:
        _sqlaero._bind_sql("SELECT %s", (value,), timeout_ns=5_000_000)
    assert caught.value.code == 65544


def test_numpy_shared_primitive_input_has_independent_native_runs_per_thread() -> None:
    script = r"""
import concurrent.futures,sys,threading
before=getattr(sys,'_is_gil_enabled',lambda:True)()
import numpy as np
from sqlaero import _sqlaero
assert getattr(sys,'_is_gil_enabled',lambda:True)()==before
source=np.arange(1025,dtype=np.int64)
expected=b'('+b','.join(str(i).encode('ascii') for i in range(1025))+b')'
barrier=threading.Barrier(4)
def run(index):
    encoder=_sqlaero._prepare_encoder()
    barrier.wait(timeout=10)
    for _ in range(4):
        assert encoder.encode(source).to_bytes()==expected
    return index
with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
    assert sorted(pool.map(run,range(4)))==list(range(4))
assert getattr(sys,'_is_gil_enabled',lambda:True)()==before
"""
    result = subprocess.run([sys.executable, "-c", script], capture_output=True, text=True, timeout=30, check=False)
    assert result.returncode == 0, result.stdout + result.stderr


def test_numpy_primitive_runs_keep_the_arrow_owners_physical_bounds() -> None:
    pa = pytest.importorskip("pyarrow")
    source = pa.array(np.arange(1300, dtype=np.int64)).slice(100, 1025).to_numpy(zero_copy_only=True)
    assert _sqlaero._bind_sql("SELECT %s", source, many=True) == [
        b"\x03SELECT " + str(i).encode("ascii") for i in range(100, 1125)
    ]
    invalid = np.lib.stride_tricks.as_strided(source, shape=(1400,), strides=(source.itemsize,))
    with pytest.raises(BufferError, match="physical allocation") as caught:
        _sqlaero._bind_sql("SELECT %s", invalid, many=True)
    assert "args[1200]" in "\n".join(getattr(caught.value, "__notes__", ()))


@pytest.mark.parametrize(("dtype", "shape", "layout"), [
    ("S16", (33, 1), "native"),
    ("S16", (25, 41), "reverse"),
    ("U16", (27, 19), "reverse"),
    (">U16", (25, 41), "transpose"),
])
def test_numpy_fixed_width_owned_runs_keep_padding_nul_and_logical_layout(
    dtype: str, shape: tuple[int, int], layout: str,
) -> None:
    binary = np.dtype(dtype).kind == "S"
    pattern = [b"", b"a", b"a\x00b", b"'\\"] if binary else ["", "中文", "a\x00b", "'\\🙂"]
    value = np.resize(np.array(pattern, dtype=dtype), shape)
    if layout == "reverse":
        value = value[::-1, ::-1]
    elif layout == "transpose":
        value = value.T
    expected = [
        b"\x03SELECT " + b",".join(
            _sqlaero._encode_scalar(bytes(cell) if binary else str(cell)) for cell in row
        ) for row in value
    ]
    sql = "SELECT " + ",".join(["%s"] * value.shape[1])
    assert _sqlaero._bind_sql(sql, value) == expected


@pytest.mark.parametrize("dtype", ["S8192", "S8193", "U2048", "U2049"])
def test_numpy_fixed_width_runs_and_original_large_fragments_agree_at_byte_cap(dtype: str) -> None:
    binary = np.dtype(dtype).kind == "S"
    count = np.dtype(dtype).itemsize if binary else np.dtype(dtype).itemsize // 4
    content = b"a" * (count - 1) + b"b" if binary else "中" * (count - 1) + "x"
    value = np.full(33, content, dtype=dtype)
    expected = b"\x03SELECT " + _sqlaero._encode_scalar(content)
    assert _sqlaero._bind_sql("SELECT %s", value, many=True) == [expected] * 33


@pytest.mark.parametrize("content", [b"", b"x" * 4096 + b"\x00y"])
def test_numpy_owned_binary_cell_keeps_padding_and_fragment_boundaries(content: bytes) -> None:
    value = np.full(33, content, dtype="S8192")
    literal = b"CAST(X'" + content.hex().upper().encode("ascii") + b"' AS BINARY)"
    assert _sqlaero._bind_sql("SELECT %s", value, many=True) == [b"\x03SELECT " + literal] * 33


def test_numpy_owned_binary_cell_keeps_the_request_limit_and_failure_position() -> None:
    from sqlaero._sqlaero import OperationalError
    value = np.full(33, b"a\x00b", dtype="S16")
    with pytest.raises(OperationalError) as caught:
        _sqlaero._bind_sql("SELECT %s", value, many=True, limit=30)
    assert "args[0]" in "\n".join(getattr(caught.value, "__notes__", ()))


@pytest.mark.parametrize(("dtype", "content"), [
    ("U2048", ""),
    (">U2048", "x" * 1024 + "\x00中🙂"),
    ("<U2048", "x" * 1024 + "\x00中🙂"),
])
def test_numpy_owned_unicode_cell_keeps_padding_endian_and_fragment_boundaries(
    dtype: str, content: str,
) -> None:
    value = np.full(33, content, dtype=dtype)
    expected = b"\x03SELECT " + _sqlaero._encode_scalar(content)
    assert _sqlaero._bind_sql("SELECT %s", value, many=True) == [expected] * 33


@pytest.mark.parametrize("dtype", ["U2048", ">U2048"])
def test_numpy_owned_unicode_scratch_keeps_each_cell_when_lengths_grow_and_shrink(dtype: str) -> None:
    pattern = ["", "x" * 1500, "中", "", "a\x00b", "🙂" * 1500, "'\\"]
    value = np.resize(np.array(pattern, dtype=dtype), (37, 4))
    expected = [
        b"\x03SELECT " + b",".join(_sqlaero._encode_scalar(str(cell)) for cell in row)
        for row in value
    ]
    assert _sqlaero._bind_sql("SELECT %s,%s,%s,%s", value) == expected


def test_numpy_owned_swapped_unicode_cell_keeps_the_canonical_surrogate_error() -> None:
    value = np.full(64, "ok", dtype=">U16")
    value[37] = "\ud800"
    with pytest.raises(UnicodeEncodeError) as caught:
        _sqlaero._bind_sql("SELECT %s", value, many=True)
    assert caught.value.object == "\ud800"
    assert (caught.value.start, caught.value.end) == (0, 1)
    assert "args[37]" in "\n".join(getattr(caught.value, "__notes__", ()))


def test_numpy_owned_unicode_cell_checks_the_minimum_limit_before_scalar_fallback() -> None:
    from sqlaero._sqlaero import OperationalError
    value = np.full(33, "\ud800", dtype="U16")
    with pytest.raises(OperationalError) as caught:
        _sqlaero._bind_sql("SELECT %s", value, many=True, limit=9)
    assert "args[0]" in "\n".join(getattr(caught.value, "__notes__", ()))


@pytest.mark.parametrize("dtype", ["S16", "U16"])
def test_numpy_fixed_width_future_bad_coordinate_is_reported_only_at_that_row(dtype: str) -> None:
    root = np.full(20, "a", dtype=dtype)
    invalid = np.lib.stride_tricks.as_strided(root, shape=(64,), strides=(root.itemsize,))
    with pytest.raises(BufferError, match="physical allocation") as caught:
        _sqlaero._bind_sql("SELECT %s", invalid, many=True)
    assert "args[20]" in "\n".join(getattr(caught.value, "__notes__", ()))


def test_numpy_cached_unicode_surrogate_keeps_the_original_exception_and_late_position() -> None:
    value = np.full(64, "ok", dtype="U2")
    value[37] = "\ud800"
    with pytest.raises(UnicodeEncodeError) as caught:
        _sqlaero._bind_sql("SELECT %s", value, many=True)
    assert caught.value.object == "\ud800"
    assert (caught.value.start, caught.value.end) == (0, 1)
    assert "args[37]" in "\n".join(getattr(caught.value, "__notes__", ()))


@pytest.mark.parametrize(("no_backslash", "literal"), [
    (False, b"'a\\'\\\\b'"),
    (True, b"'a''\\b'"),
])
def test_numpy_cached_unicode_uses_the_actual_backslash_mode(no_backslash: bool, literal: bytes) -> None:
    value = np.full(64, "a'\\b", dtype="U16")
    assert _sqlaero._bind_sql("SELECT %s", value, many=True, no_backslash=no_backslash) == [b"\x03SELECT " + literal] * 64


def test_numpy_cached_unicode_connection_codec_error_stays_at_the_referenced_cell() -> None:
    value = np.full(64, "ascii", dtype="U16")
    value[37] = "中"
    with pytest.raises(UnicodeEncodeError) as caught:
        _sqlaero._bind_sql("SELECT %s", value, many=True, charset="latin1")
    assert caught.value.object == "中"
    assert (caught.value.start, caught.value.end) == (0, 1)
    assert "args[37]" in "\n".join(getattr(caught.value, "__notes__", ()))


@pytest.mark.parametrize("layout", ["vector", "reverse", "matrix", "transpose", "fortran"])
def test_numpy_object_owning_and_view_exports_keep_each_public_conversion(layout: str) -> None:
    calls: list[int] = []
    class ObservedInt(int):
        def __int__(self) -> int:
            converted = int.__int__(self) + 1
            calls.append(converted)
            return converted
    shape = (64,) if layout in ("vector", "reverse") else (8, 8)
    original = np.empty(shape, dtype=object, order="F" if layout == "fortran" else "C")
    for index in range(64):
        original.flat[index] = ObservedInt(index)
    assert calls == []
    value = original
    numbers = np.arange(64, dtype=np.int64).reshape(shape)
    if layout == "reverse":
        value = original[::-1]
        numbers = numbers[::-1]
    elif layout == "transpose":
        value = original.T
        numbers = numbers.T
    assert bool(value.flags.owndata) is (layout not in ("reverse", "transpose"))
    if value.ndim == 1:
        expected = [b"\x03SELECT " + str(int(cell) + 1).encode("ascii") for cell in numbers]
        actual = _sqlaero._bind_sql("SELECT %s", value, many=True)
    else:
        sql = "SELECT " + ",".join(["%s"] * value.shape[1])
        expected = [b"\x03SELECT " + b",".join(str(int(cell) + 1).encode("ascii") for cell in row) for row in numbers]
        actual = _sqlaero._bind_sql(sql, value)
    assert actual == expected
    assert calls == [int(cell) + 1 for cell in numbers.flat]
