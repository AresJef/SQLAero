"""Arrow input values and shapes through the production native binding path."""

# pyright: reportPrivateUsage=false

from __future__ import annotations

from collections.abc import Callable
from concurrent.futures import ThreadPoolExecutor
import ctypes
from decimal import Decimal
from datetime import datetime, timedelta, tzinfo
import gc
import math
import struct
import sys
from types import FrameType
from typing import Any, cast
import weakref

import pytest
from _native_signal import NativeAlarm

from sqlaero import _sqlaero

pa = pytest.importorskip("pyarrow")


@pytest.mark.parametrize("native_levels", [62, 63])
def test_mixed_python_and_arrow_nesting_shares_one_logical_limit(native_levels: int) -> None:
    data: Any = 7
    dtype = pa.int64()
    for _ in range(native_levels):
        data = [data]
        dtype = pa.list_(dtype)
    value = [pa.array([data], type=dtype)]
    if native_levels == 62:
        assert _sqlaero._encode_value(value).to_bytes() == b"(" * 64 + b"7" + b")" * 64
    else:
        with pytest.raises(_sqlaero.DataError) as caught:
            _sqlaero._encode_value(value)
        assert caught.value.code == 65543


@pytest.mark.parametrize(
    ("value", "expected"),
    [
        pytest.param(pa.array([1, None, -3], type=pa.int64()), b"(1,NULL,-3)", id="int64-null"),
        pytest.param(pa.array([0, 2**64 - 1], type=pa.uint64()), b"(0,18446744073709551615)", id="uint64-full"),
        pytest.param(pa.array([True, None, False]), b"(1,NULL,0)", id="bit-packed-bool"),
        pytest.param(pa.array([None, None], type=pa.null()), b"(NULL,NULL)", id="null"),
        pytest.param(pa.array(["a", None, "中"]), "('a',NULL,'中')".encode(), id="utf8"),
        pytest.param(pa.array([], type=pa.int32()), b"()", id="empty-value-fragment"),
    ],
)
def test_arrow_array_values_use_shared_native_writer(value: object, expected: bytes) -> None:
    assert _sqlaero._encode_value(value).to_bytes() == expected


@pytest.mark.parametrize("raises_property", [False, True], ids=["pretends-array", "raises"])
def test_arrow_recognition_does_not_execute_a_disguised_class_property(raises_property: bool) -> None:
    calls: list[str] = []

    class Disguised:
        @property
        def __class__(self) -> type[object]:  # pyright: ignore[reportIncompatibleMethodOverride]
            calls.append("__class__")
            if raises_property:
                raise AssertionError("unapproved __class__ property must never execute")
            return pa.Array

    value = Disguised()
    with pytest.raises(TypeError):
        _sqlaero._encode_value(value)
    assert calls == []
    with pytest.raises(TypeError):
        _sqlaero._bind_sql("SELECT %s", [value])
    assert calls == []


@pytest.mark.parametrize("many", [False, True])
@pytest.mark.parametrize("itemize", [False, True])
def test_arrow_vector_has_explicit_row_and_cell_roles(many: bool, itemize: bool) -> None:
    value = pa.array([1, None, 3], type=pa.int64())
    if many:
        sql = "SELECT %s"
        expected = [b"\x03SELECT 1", b"\x03SELECT NULL", b"\x03SELECT 3"]
    elif itemize:
        sql = "SELECT %s,%s,%s"
        expected = [b"\x03SELECT 1,NULL,3"]
    else:
        sql = "SELECT 1 IN %s"
        expected = [b"\x03SELECT 1 IN (1,NULL,3)"]
    assert _sqlaero._bind_sql(sql, value, many=many, itemize=itemize) == expected


def test_arrow_slices_and_chunks_preserve_offsets_and_validity() -> None:
    base = pa.array([False, True, None, False, True, None, True, False, True])
    sliced = base.slice(1, 7)
    chunked = pa.chunked_array([sliced.slice(0, 2), sliced.slice(2, 0), sliced.slice(2)])
    assert _sqlaero._encode_value(sliced).to_bytes() == b"(1,NULL,0,1,NULL,1,0)"
    assert _sqlaero._encode_value(chunked).to_bytes() == b"(1,NULL,0,1,NULL,1,0)"


def test_arrow_boolean_slice_crosses_both_data_and_validity_byte_boundaries() -> None:
    values = [None if i % 7 == 0 else i % 3 == 0 for i in range(64)]
    value = pa.array(values).slice(5, 40)
    expected = "(" + ",".join("NULL" if item is None else str(int(item)) for item in values[5:45]) + ")"
    assert _sqlaero._encode_value(value).to_bytes() == expected.encode()


@pytest.mark.parametrize("many", [False, True])
@pytest.mark.parametrize("itemize", [False, True])
@pytest.mark.parametrize("kind", ["recordbatch", "table"])
def test_arrow_tables_are_natural_row_sources(kind: str, many: bool, itemize: bool) -> None:
    columns = [pa.array([1, 2]), pa.array(["a", "b"])]
    if kind == "recordbatch":
        value = pa.record_batch(columns, names=["duplicate", "duplicate"])
    else:
        value = pa.Table.from_arrays(columns, names=["duplicate", "duplicate"])
    sql = "SELECT %s,%s" if itemize else "SELECT %s"
    expected = (
        [b"\x03SELECT 1,'a'", b"\x03SELECT 2,'b'"]
        if itemize
        else [b"\x03SELECT (1,'a')", b"\x03SELECT (2,'b')"]
    )
    assert _sqlaero._bind_sql(sql, value, many=many, itemize=itemize) == expected


def test_arrow_table_columns_do_not_need_matching_chunk_boundaries() -> None:
    value = pa.table(
        {
            "x": pa.chunked_array([[1], [2, 3]]),
            "y": pa.chunked_array([["a", "b"], ["c"]]),
        }
    )
    assert _sqlaero._bind_sql("SELECT %s,%s", value) == [
        b"\x03SELECT 1,'a'", b"\x03SELECT 2,'b'", b"\x03SELECT 3,'c'"
    ]


@pytest.mark.parametrize("kind", ["recordbatch", "table"])
def test_arrow_sliced_table_applies_parent_and_child_offsets(kind: str) -> None:
    columns = [pa.array([1, 2, 3, 4]), pa.array([False, True, None, False])]
    value = (
        pa.record_batch(columns, names=["x", "y"])
        if kind == "recordbatch"
        else pa.Table.from_arrays(columns, names=["x", "y"])
    ).slice(1, 2)
    assert _sqlaero._bind_sql("SELECT %s,%s", value) == [b"\x03SELECT 2,1", b"\x03SELECT 3,NULL"]


def test_arrow_sliced_list_uses_its_own_offset_window() -> None:
    value = pa.array([[0], [1, None, 2], None, [], [3]], type=pa.list_(pa.int64())).slice(1, 3)
    assert _sqlaero._encode_value(value).to_bytes() == b"((1,NULL,2),NULL,())"


def test_arrow_large_list_uses_checked_64_bit_offsets() -> None:
    value = pa.array([[0], [1, None, 2], None, []], type=pa.large_list(pa.int64())).slice(1)
    assert _sqlaero._encode_value(value).to_bytes() == b"((1,NULL,2),NULL,())"


def test_arrow_dictionary_uses_each_chunks_labels() -> None:
    first = pa.DictionaryArray.from_arrays(pa.array([0, 1, None], type=pa.int8()), pa.array(["a", "b"]))
    second = pa.DictionaryArray.from_arrays(pa.array([0, 1], type=pa.int8()), pa.array(["b", "a"]))
    value = pa.chunked_array([first, second])
    assert _sqlaero._encode_value(value).to_bytes() == b"('a','b',NULL,'b','a')"


@pytest.mark.parametrize("dtype", [pa.string(), pa.large_string(), pa.string_view()])
@pytest.mark.parametrize("no_backslash", [False, True])
def test_arrow_repeated_dictionary_strings_keep_actual_encoding_and_null_labels(dtype: object, no_backslash: bool) -> None:
    # The non-ASCII category is valid UTF-8 but cannot be encoded as ASCII. Its
    # unused presence must never trigger eager label conversion or validation.
    labels = pa.array(["a'\\", None, "unused🙂"], type=dtype)
    value = pa.DictionaryArray.from_arrays(pa.array([0, 0, None, 1, 0, 1] * 8, type=pa.int16()), labels)
    literal = b"'a''\\'" if no_backslash else b"'a\\'\\\\'"
    expected = b"(" + b",".join([literal, literal, b"NULL", b"NULL", literal, b"NULL"] * 8) + b")"
    assert _sqlaero._bind_sql("SELECT %s", (value,), charset="ascii", no_backslash=no_backslash) == [b"\x03SELECT " + expected]


def test_arrow_repeated_dictionary_labels_are_independent_across_columns_and_chunks() -> None:
    chunks: list[Any] = []
    for labels in (["a'", "b\\"], ["b\\", "a'"]):
        chunks.append(pa.DictionaryArray.from_arrays(pa.array([0, 0, 1, None, 1] * 8), pa.array(labels)))
    left = pa.chunked_array(chunks)
    right = pa.chunked_array(list(reversed(chunks)))
    value = pa.table({"left": left, "right": right})
    left_values = ["a'", "a'", "b\\", None, "b\\"] * 8 + ["b\\", "b\\", "a'", None, "a'"] * 8
    right_values = left_values[40:] + left_values[:40]
    expected = _sqlaero._bind_sql("SELECT %s,%s", list(zip(left_values, right_values, strict=True)), many=True)
    assert _sqlaero._bind_sql("SELECT %s,%s", value) == expected


@pytest.mark.parametrize("kind", ["many-labels", "large-label"])
def test_arrow_dictionary_cache_bounds_keep_the_original_writer(kind: str) -> None:
    labels = [f"label{i}" for i in range(65)] if kind == "many-labels" else ["x" * (20 * 1024)]
    codes = [i % len(labels) for i in range(130 if kind == "many-labels" else 32)]
    value = pa.DictionaryArray.from_arrays(pa.array(codes, type=pa.int16()), pa.array(labels))
    expected = _sqlaero._encode_value([labels[code] for code in codes]).to_bytes()
    assert _sqlaero._encode_value(value).to_bytes() == expected


def test_arrow_repeated_dictionary_label_still_checks_the_output_limit() -> None:
    value = pa.DictionaryArray.from_arrays(pa.array([0] * 32, type=pa.int8()), pa.array(["abcd"]))
    with pytest.raises(_sqlaero.OperationalError):
        _sqlaero._bind_sql("SELECT %s", (value,), limit=len(b"\x03SELECT ('abcd','abc"))


def test_arrow_dictionary_reuse_keeps_later_original_encoding_errors_located() -> None:
    value = pa.DictionaryArray.from_arrays(pa.array([0] * 32 + [1]), pa.array(["ok", "🙂"]))
    with pytest.raises(UnicodeEncodeError) as caught:
        _sqlaero._bind_sql("SELECT %s", value, many=True, charset="ascii")
    assert caught.value.object == "🙂" and (caught.value.start, caught.value.end) == (0, 1)
    assert "args[32]" in "\n".join(getattr(caught.value, "__notes__", []))


def test_arrow_dictionary_columns_outside_the_small_cache_keep_the_same_values() -> None:
    columns = [
        pa.DictionaryArray.from_arrays(pa.array([0, 0, None] * 11), pa.array([f"c{column}'"]))
        for column in range(10)
    ]
    value = pa.Table.from_arrays(columns, names=[f"c{column}" for column in range(10)])
    sql = "SELECT " + ",".join(["%s"] * 10)
    expected = _sqlaero._bind_sql(sql, ([[f"c{column}'" for column in range(10)]] * 2 + [[None] * 10]) * 11, many=True)
    assert _sqlaero._bind_sql(sql, value) == expected


@pytest.mark.parametrize("length", [1, 100, 1000])
@pytest.mark.parametrize("high_cardinality", [False, True], ids=["low", "high"])
def test_arrow_small_and_large_dictionary_sources_keep_their_exact_labels(length: int, high_cardinality: bool) -> None:
    labels = [f"label{i}'" for i in range(length if high_cardinality else 3)]
    codes = [i % len(labels) for i in range(length)]
    value = pa.DictionaryArray.from_arrays(pa.array(codes), pa.array(labels))
    assert _sqlaero._encode_value(value).to_bytes() == _sqlaero._encode_value([labels[code] for code in codes]).to_bytes()


@pytest.mark.parametrize(
    "dtype", [pa.int8(), pa.int16(), pa.int32(), pa.int64(), pa.uint8(), pa.uint16(), pa.uint32(), pa.uint64()]
)
@pytest.mark.parametrize("length", [3, 40])
def test_arrow_dictionary_all_key_widths_resolve_actual_labels(dtype: object, length: int) -> None:
    codes = ([1, None, 0] * ((length + 2) // 3))[:length]
    keys = pa.array([None] + codes, type=dtype)
    value = pa.DictionaryArray.from_arrays(keys, pa.array(["a", "b"])).slice(1, length)
    tokens = [b"'b'", b"NULL", b"'a'"]
    expected = b"(" + b",".join(tokens[index % 3] for index in range(length)) + b")"
    assert _sqlaero._encode_value(value).to_bytes() == expected


def test_arrow_dictionary_chunks_with_shared_values_keep_their_own_keys() -> None:
    labels = pa.array(["left", "right"])
    first = pa.DictionaryArray.from_arrays(pa.array([0, 1, None, 0] * 10, type=pa.int16()), labels)
    second = pa.DictionaryArray.from_arrays(pa.array([1, 0, None, 1] * 10, type=pa.int16()), labels)
    # Both exported chunks remain long enough to prepare their own readers.
    value = pa.chunked_array([first, second]).slice(1, 78)
    tokens = ([b"'left'", b"'right'", b"NULL", b"'left'"] * 10
              + [b"'right'", b"'left'", b"NULL", b"'right'"] * 10)
    expected = b"(" + b",".join(tokens[1:79]) + b")"
    assert _sqlaero._encode_value(value).to_bytes() == expected


def test_arrow_dictionary_nested_labels_keep_their_logical_value_and_validity() -> None:
    labels = pa.array([[1, None], None, []], type=pa.list_(pa.int64()))
    value = pa.DictionaryArray.from_arrays(pa.array([2, 0, None, 1], type=pa.int16()), labels)
    assert _sqlaero._bind_sql("SELECT %s", value, many=True) == [
        b"\x03SELECT ()", b"\x03SELECT (1,NULL)", b"\x03SELECT NULL", b"\x03SELECT NULL"
    ]


@pytest.mark.parametrize("kind", ["list", "struct"])
def test_arrow_null_parent_never_decodes_its_invalid_utf8_child(kind: str) -> None:
    child = pa.Array.from_buffers(
        pa.string(), 1, [None, pa.py_buffer(struct.pack("=ii", 0, 1)), pa.py_buffer(b"\xff")]
    )
    value = (
        pa.ListArray.from_arrays(pa.array([0, 1], type=pa.int32()), child, mask=pa.array([True]))
        if kind == "list"
        else pa.StructArray.from_arrays([child], names=["ignored"], mask=pa.array([True]))
    )
    assert _sqlaero._encode_value(value).to_bytes() == b"(NULL)"


def test_arrow_nested_cells_keep_one_parameter_and_schema_order() -> None:
    lists = pa.array([[1, None], None, []], type=pa.list_(pa.int64()))
    structs = pa.array(
        [{"last": 2, "first": 1}, None],
        type=pa.struct([("first", pa.int64()), ("last", pa.int64())]),
    )
    assert _sqlaero._bind_sql("SELECT %s", lists, many=True) == [
        b"\x03SELECT (1,NULL)", b"\x03SELECT NULL", b"\x03SELECT ()"
    ]
    assert _sqlaero._bind_sql("SELECT %s", structs, many=True) == [
        b"\x03SELECT (1,2)", b"\x03SELECT NULL"
    ]


@pytest.mark.parametrize("constructor", [pa.decimal32, pa.decimal64, pa.decimal128, pa.decimal256])
def test_arrow_negative_decimal_scale_is_an_exact_value(constructor: Callable[[int, int], object]) -> None:
    dtype = constructor(8, -2)
    value = pa.array([Decimal("12300"), None, Decimal("-4500")], type=dtype)
    assert _sqlaero._encode_value(value).to_bytes() == b"(12300,NULL,-4500)"


@pytest.mark.parametrize(
    "dtype", [pa.int8(), pa.int16(), pa.int32(), pa.int64(), pa.uint8(), pa.uint16(), pa.uint32(), pa.uint64()]
)
def test_arrow_all_integer_widths_preserve_their_logical_values(dtype: object) -> None:
    value = pa.array([0, None, 17], type=dtype)
    assert _sqlaero._encode_value(value).to_bytes() == b"(0,NULL,17)"


@pytest.mark.parametrize("length", [1, 100, 1000])
@pytest.mark.parametrize("table", [False, True], ids=["vector", "table"])
def test_arrow_short_and_long_int64_sources_keep_nulls_and_exact_extremes(length: int, table: bool) -> None:
    values = [None if index % 7 == 6 else (-(2**63) if index % 2 else 2**63 - 1) for index in range(length)]
    array = pa.array(values, type=pa.int64())
    if table:
        value = pa.table({"left": array, "right": array})
        rows = [b"NULL,NULL" if item is None else f"{item},{item}".encode() for item in values]
        expected = [b"\x03SELECT " + row for row in rows]
        assert _sqlaero._bind_sql("SELECT %s,%s", value) == expected
    else:
        expected = b"(" + b",".join(b"NULL" if item is None else str(item).encode() for item in values) + b")"
        assert _sqlaero._encode_value(array).to_bytes() == expected


def test_arrow_long_int64_table_reprepares_typed_views_at_each_actual_chunk() -> None:
    left = list(range(100))
    right = [None if index % 9 == 8 else -index for index in left]
    value = pa.table({
        "left": pa.chunked_array([left[:40], left[40:]], type=pa.int64()),
        "right": pa.chunked_array([right[:50], right[50:]], type=pa.int64()),
    })
    # The union of column boundaries produces 40/10/50-row exported chunks;
    # the middle short chunk must retain the ordinary per-cell writer.
    expected = [b"\x03SELECT " + str(a).encode() + b"," + (b"NULL" if b is None else str(b).encode())
                for a, b in zip(left, right, strict=True)]
    assert _sqlaero._bind_sql("SELECT %s,%s", value) == expected
    assert _sqlaero._bind_sql("SELECT %s,%s", value.slice(5, 80)) == expected[5:85]


def test_arrow_nullable_struct_parent_keeps_its_value_and_recordbatch_nulls() -> None:
    logical = [None if index % 7 == 6 else {"left": index, "right": -index} for index in range(40)]
    value = pa.array(logical, type=pa.struct([("left", pa.int64()), ("right", pa.int64())]))
    cells = [b"NULL" if item is None else f"({item['left']},{item['right']})".encode() for item in logical]
    assert _sqlaero._encode_value(value).to_bytes() == b"(" + b",".join(cells) + b")"
    rows = pa.RecordBatch.from_struct_array(value)
    expected = [b"\x03SELECT NULL,NULL" if item is None else f"\x03SELECT {item['left']},{item['right']}".encode()
                for item in logical]
    assert _sqlaero._bind_sql("SELECT %s,%s", rows) == expected


def test_arrow_long_mixed_table_does_not_encode_a_future_bad_string_early() -> None:
    value = pa.table({"left": list(range(70)), "right": ["ok"] * 64 + ["🙂"] + ["ok"] * 5})
    with pytest.raises(UnicodeEncodeError) as caught:
        _sqlaero._bind_sql("SELECT %s,%s", value, charset="ascii")
    assert caught.value.object == "🙂"
    assert "args[64][1]" in "\n".join(getattr(caught.value, "__notes__", []))


@pytest.mark.parametrize("kind", ["int64", "float32", "boolean"])
def test_arrow_long_typed_writer_retains_the_current_output_limit(kind: str) -> None:
    if kind == "float32":
        value = pa.array([0.5] * 40, type=pa.float32())
    elif kind == "boolean":
        value = pa.array([True] * 40, type=pa.bool_())
    else:
        value = pa.array([2**63 - 1] * 40, type=pa.int64())
    with pytest.raises(_sqlaero.OperationalError):
        _sqlaero._bind_sql("SELECT %s", (value,), limit=40)


@pytest.mark.parametrize("failure", [False, True], ids=["success", "limit-failure"])
@pytest.mark.parametrize("kind", ["int64", "float32", "boolean", "dictionary"])
def test_arrow_owned_typed_views_release_the_real_shared_buffer_on_every_exit(failure: bool, kind: str) -> None:
    np = pytest.importorskip("numpy")
    if kind == "dictionary":
        storage = np.zeros(40, dtype=np.int16)
        value = pa.DictionaryArray.from_arrays(pa.array(storage), pa.array(["label"]))
        expected = b"(" + b",".join([b"'label'"] * 40) + b")"
    elif kind == "float32":
        storage = np.full(40, 0.5, dtype=np.float32)
        value = pa.array(storage)
        expected = b"(" + b",".join([b"0.5e0"] * 40) + b")"
    elif kind == "boolean":
        # Bit-packed Arrow storage has a real external owner even though
        # converting NumPy bool_ elements would normally copy them.
        storage = np.full(5, 0x55, dtype=np.uint8)
        value = pa.Array.from_buffers(pa.bool_(), 40, [None, pa.py_buffer(storage)])
        expected = b"(" + b",".join([b"1", b"0"] * 20) + b")"
    else:
        storage = np.arange(40, dtype=np.int64)
        value = pa.array(storage)
        expected = b"(" + b",".join(str(index).encode() for index in range(40)) + b")"
    reference = weakref.ref(storage)
    del storage
    gc.collect()
    assert reference() is not None
    if failure:
        with pytest.raises(_sqlaero.OperationalError):
            _sqlaero._bind_sql("SELECT %s", (value,), limit=20)
    else:
        assert _sqlaero._encode_value(value).to_bytes() == expected
    del value
    gc.collect()
    assert reference() is None


@pytest.mark.parametrize("dtype", [pa.float32(), pa.float64()])
def test_arrow_floats_preserve_source_value_negative_zero_and_nonfinite_policy(dtype: object) -> None:
    value = pa.array([0.1, -0.0, float("nan"), float("inf"), -float("inf")], type=dtype)
    parts = _sqlaero._encode_value(value).to_bytes()[1:-1].split(b",")
    # as_py is only the test oracle for this one source value, never an encoder path.
    assert float(parts[0]) == value[0].as_py()
    assert float(parts[1]) == 0.0 and math.copysign(1.0, float(parts[1])) < 0
    assert parts[2:] == [b"NULL", b"NULL", b"NULL"]


@pytest.mark.parametrize("length", [1, 31, 32, 100, 1000])
def test_arrow_float32_prepared_vectors_keep_source_precision_and_sliced_nulls(length: int) -> None:
    pattern: list[float | None] = [0.1, -0.0, float("nan"), float("inf"), -float("inf"), None,
                                   1.5, -1.5, 2.0**-149, (2.0 - 2.0**-23) * 2.0**127]
    values = [pattern[index % len(pattern)] for index in range(length)]
    source = pa.array([None] * 9 + values, type=pa.float32()).slice(9, length)
    parts = _sqlaero._encode_value(source).to_bytes()[1:-1].split(b",")
    assert len(parts) == length
    for token, original in zip(parts, values, strict=True):
        if original is None or not math.isfinite(original):
            assert token == b"NULL"
        else:
            # The independent IEEE binary32 round-trip defines the source
            # value; no SQLAero writer generates this test's expectation.
            expected = struct.unpack("=f", struct.pack("=f", original))[0]
            actual = float(token)
            assert actual == expected
            if expected == 0.0:
                assert math.copysign(1.0, actual) == math.copysign(1.0, expected)


@pytest.mark.parametrize("length", [1, 31, 32, 100, 1000])
@pytest.mark.parametrize("offset", [1, 9])
def test_arrow_prepared_boolean_vectors_keep_bit_and_validity_offsets(length: int, offset: int) -> None:
    values = [None if index % 7 == 6 else index % 3 != 1 for index in range(length)]
    source = pa.array([False] * offset + values, type=pa.bool_()).slice(offset, length)
    expected = b"(" + b",".join(b"NULL" if value is None else b"1" if value else b"0" for value in values) + b")"
    assert _sqlaero._encode_value(source).to_bytes() == expected


@pytest.mark.parametrize("kind", ["float32", "boolean"])
def test_arrow_primitive_views_reprepare_across_empty_long_and_short_chunks(kind: str) -> None:
    values: list[float | bool | None]
    if kind == "float32":
        values = [None if index % 7 == 6 else index * 0.125 for index in range(100)]
        dtype = pa.float32()
    else:
        values = [None if index % 7 == 6 else bool(index % 2) for index in range(100)]
        dtype = pa.bool_()
    chunks: list[Any] = [pa.array(values[:40], type=dtype), pa.array([], type=dtype),
                        pa.array(values[40:50], type=dtype), pa.array([], type=dtype), pa.array(values[50:], type=dtype)]
    source = pa.chunked_array(chunks).slice(5, 80)
    parts = _sqlaero._encode_value(source).to_bytes()[1:-1].split(b",")
    for token, value in zip(parts, values[5:85], strict=True):
        if value is None:
            assert token == b"NULL"
        elif kind == "float32":
            assert float(token) == value
        else:
            assert token == (b"1" if value else b"0")


@pytest.mark.parametrize("dtype", [pa.string(), pa.large_string(), pa.string_view()])
def test_arrow_text_storage_kinds_share_connection_escaping(dtype: object) -> None:
    value = pa.array(["a'\\中", None, ""], type=dtype)
    assert _sqlaero._encode_value(value).to_bytes() == "('a\\'\\\\中',NULL,'')".encode()
    assert _sqlaero._encode_value(value, no_backslash=True).to_bytes() == "('a''\\中',NULL,'')".encode()


@pytest.mark.parametrize("dtype", [pa.binary(), pa.large_binary(), pa.binary_view(), pa.binary(3)])
def test_arrow_binary_storage_preserves_trailing_zero_bytes(dtype: object) -> None:
    value = pa.array([b"A\x00\x00", None], type=dtype)
    assert _sqlaero._encode_value(value).to_bytes() == b"(CAST(X'410000' AS BINARY),NULL)"


def test_arrow_zero_width_fixed_binary_still_has_values_and_nulls() -> None:
    value = pa.array([b"", None, b""], type=pa.binary(0))
    assert _sqlaero._encode_value(value).to_bytes() == b"(CAST(X'' AS BINARY),NULL,CAST(X'' AS BINARY))"


@pytest.mark.parametrize("dtype", [pa.string(), pa.large_string(), pa.string_view()])
def test_arrow_long_text_crosses_native_chunks_on_complete_codepoints(dtype: object) -> None:
    source = "a" * (256 * 1024 - 1) + "中" + "b" * (256 * 1024 - 1) + "🙂"
    assert _sqlaero._encode_value(pa.array([source], type=dtype)).to_bytes() == ("('" + source + "')").encode()


@pytest.mark.parametrize("dtype", [pa.binary(), pa.large_binary(), pa.binary_view()])
def test_arrow_long_binary_uses_native_chunks_without_losing_bytes(dtype: object) -> None:
    source = bytes(range(256)) * 1025
    expected = b"(CAST(X'" + source.hex().upper().encode() + b"' AS BINARY))"
    assert _sqlaero._encode_value(pa.array([source], type=dtype)).to_bytes() == expected


@pytest.mark.parametrize("prefix", ["a", "中"])
def test_arrow_encoding_error_retains_full_source_and_character_position(prefix: str) -> None:
    source = prefix * (256 * 1024 + 1) + "🙂"
    with pytest.raises(UnicodeEncodeError) as caught:
        _sqlaero._bind_sql("SELECT %s", pa.array([source]), charset="gbk")
    assert caught.value.object == source
    assert (caught.value.start, caught.value.end) == (len(source) - 1, len(source))


@pytest.mark.parametrize("binary", [False, True], ids=["utf8", "binary"])
def test_arrow_long_value_keeps_the_original_operation_deadline(binary: bool) -> None:
    source = pa.array([b"x" * (16 * 1024 * 1024)]) if binary else pa.array(["'" * (16 * 1024 * 1024)])
    with pytest.raises(_sqlaero.OperationalTimeoutError):
        _sqlaero._bind_sql("SELECT %s", source, timeout_ns=5_000_000)


@pytest.mark.parametrize("binary", [False, True], ids=["utf8", "binary"])
def test_arrow_long_value_preserves_the_actual_signal_exception(binary: bool) -> None:
    class InterruptedArrow(Exception):
        pass

    original = InterruptedArrow("original Arrow signal")
    source = pa.array([b"x" * (16 * 1024 * 1024)]) if binary else pa.array(["'" * (16 * 1024 * 1024)])
    interrupted = False

    def interrupt(signum: int, frame: FrameType | None) -> None:
        nonlocal interrupted
        # Ignore delivery while the platform sender or approved tzinfo callback
        # is being armed; the periodic alarm must reach the native caller.
        if interrupted or frame is None or frame.f_code is not test_arrow_long_value_preserves_the_actual_signal_exception.__code__:
            return
        interrupted = True
        alarm.cancel()
        raise original

    class Arm(tzinfo):
        def utcoffset(self, value: datetime | None) -> timedelta:
            alarm.start(0.0001, 0.0001, wait_ready=True)
            return timedelta(0)

    alarm = NativeAlarm(interrupt)
    try:
        with pytest.raises(InterruptedArrow) as caught:
            _sqlaero._bind_sql("SELECT %s,%s", [datetime(2024, 1, 1, tzinfo=Arm()), source])
        assert caught.value is original
    finally:
        alarm.close()


@pytest.mark.parametrize(
    ("dtype", "ticks", "expected"),
    [
        (pa.date32(), -1, b"('1969-12-31',NULL)"),
        (pa.date64(), 86_400_000, b"('1970-01-02',NULL)"),
        (pa.timestamp("s"), -1, b"('1969-12-31 23:59:59',NULL)"),
        (pa.timestamp("ms"), -1, b"('1969-12-31 23:59:59.999',NULL)"),
        (pa.timestamp("us"), -1, b"('1969-12-31 23:59:59.999999',NULL)"),
        (pa.timestamp("ns", tz="Asia/Shanghai"), -1, b"('1969-12-31 23:59:59.999999999',NULL)"),
        (pa.duration("ns"), -1, b"('-00:00:00.000000001',NULL)"),
        (pa.time32("s"), 1, b"('00:00:01',NULL)"),
        (pa.time32("ms"), 1, b"('00:00:00.001',NULL)"),
        (pa.time64("us"), 1, b"('00:00:00.000001',NULL)"),
        (pa.time64("ns"), 1, b"('00:00:00.000000001',NULL)"),
    ],
)
def test_arrow_temporal_units_keep_source_precision_and_validity(dtype: object, ticks: int, expected: bytes) -> None:
    assert _sqlaero._encode_value(pa.array([ticks, None], type=dtype)).to_bytes() == expected


def test_arrow_valid_minimum_duration_is_not_numpy_nat() -> None:
    value = pa.array([-(2**63), None], type=pa.duration("ms"))
    assert _sqlaero._encode_value(value).to_bytes() == b"('-2562047788015:12:55.808',NULL)"


@pytest.mark.parametrize(
    "unit,expected",
    [("s", b"('-00:00:01')"), ("ms", b"('-00:00:00.001')"),
     ("us", b"('-00:00:00.000001')"), ("ns", b"('-00:00:00.000000001')")],
)
def test_arrow_all_duration_units_use_exact_signed_ticks(unit: str, expected: bytes) -> None:
    assert _sqlaero._encode_value(pa.array([-1], type=pa.duration(unit))).to_bytes() == expected


@pytest.mark.parametrize("dtype,ticks", [(pa.date64(), 1), (pa.time32("s"), -1), (pa.time64("ns"), 86_400_000_000_000)])
def test_arrow_invalid_date_or_clock_storage_is_not_silently_changed(dtype: object, ticks: int) -> None:
    with pytest.raises(_sqlaero.DataError):
        _sqlaero._encode_value(pa.array([ticks], type=dtype))


def test_arrow_sliced_fixed_size_list_keeps_child_offset() -> None:
    values = pa.array([[1, 2], [3, 4], None, [5, 6]], type=pa.list_(pa.int64(), 2)).slice(1)
    assert _sqlaero._encode_value(values).to_bytes() == b"((3,4),NULL,(5,6))"


def test_arrow_invalid_utf8_from_public_buffer_constructor_fails_before_native_text() -> None:
    value = pa.Array.from_buffers(
        pa.string(), 1, [None, pa.py_buffer(struct.pack("=ii", 0, 1)), pa.py_buffer(b"\xff")]
    )
    with pytest.raises(TypeError, match="UTF-8"):
        _sqlaero._encode_value(value)


@pytest.mark.parametrize("long_message", [False, True], ids=["escaped-bytes", "bounded-512"])
def test_arrow_real_stream_failure_keeps_status_bounded_detail_and_one_release(
    monkeypatch: pytest.MonkeyPatch, long_message: bool
) -> None:
    """A public Pandas export bridges the actual capsule on both GIL ABIs."""
    pd = pytest.importorskip("pandas", reason="CStream failure observation uses the public Pandas Arrow export")

    class Stream(ctypes.Structure):
        _fields_ = [(name, ctypes.c_void_p) for name in ("get_schema", "get_next", "get_last_error", "release", "private_data")]

    source = pa.chunked_array([[7]], type=pa.int64())
    capsule = source.__arrow_c_stream__()
    get_pointer = ctypes.pythonapi.PyCapsule_GetPointer
    get_pointer.argtypes = [ctypes.py_object, ctypes.c_char_p]
    get_pointer.restype = ctypes.c_void_p
    stream = cast(Any, ctypes.cast(get_pointer(capsule, b"arrow_array_stream"), ctypes.POINTER(Stream)))
    expected_header = bytes(stream.contents)
    original_next = stream.contents.get_next
    original_error = stream.contents.get_last_error
    original_release = stream.contents.release
    release_type = ctypes.PYFUNCTYPE(None, ctypes.c_void_p)
    release_original = release_type(original_release)
    payload = b"x" * 600 if long_message else b"producer \xff detail"
    detail = ctypes.create_string_buffer(payload)
    failures: list[str] = []
    released: list[int] = []
    error_reads: list[int] = []

    def next_array(_stream: int, _array: int) -> int:
        # Returning a genuine C status leaves the initialized out-array empty;
        # no producer buffer address or schema is fabricated.
        return 73

    def last_error(_stream: int) -> int:
        error_reads.append(1)
        return ctypes.addressof(detail)

    def release(raw_pointer: int) -> None:
        live = cast(Any, ctypes.cast(raw_pointer, ctypes.POINTER(Stream)))
        live.contents.get_next = original_next
        live.contents.get_last_error = original_error
        live.contents.release = original_release
        if bytes(live.contents) != expected_header:
            failures.append("stream header changed before original release")
        released.append(1)
        release_original(raw_pointer)

    callbacks = (
        ctypes.PYFUNCTYPE(ctypes.c_int, ctypes.c_void_p, ctypes.c_void_p)(next_array),
        ctypes.PYFUNCTYPE(ctypes.c_void_p, ctypes.c_void_p)(last_error),
        release_type(release),
    )
    for name, callback in zip(("get_next", "get_last_error", "release"), callbacks, strict=True):
        setattr(stream.contents, name, ctypes.cast(callback, ctypes.c_void_p).value)

    class Export:
        def __arrow_c_stream__(self) -> object:
            return capsule

    def export(_source: object, _type: object = None) -> object:
        return Export()

    value = pd.array([7], dtype=pd.ArrowDtype(pa.int64()))
    monkeypatch.setattr(pd.arrays.ArrowExtensionArray, "__arrow_array__", export)
    with pytest.raises(TypeError) as caught:
        _sqlaero._encode_value(value)
    message = str(caught.value)
    expected_detail = payload[:512].decode("ascii", "backslashreplace")
    assert "Arrow stream get_next failed with status 73:" in message
    assert message.endswith(expected_detail + (" [truncated]" if long_message else ""))
    assert len(message) < 650
    assert error_reads == [1] and released == [1] and failures == []
    assert not stream.contents.release, "the capsule must be consumed rather than released twice"


def test_arrow_invalid_intermediate_offset_is_checked_at_actual_value() -> None:
    value = pa.Array.from_buffers(
        pa.string(), 2, [None, pa.py_buffer(struct.pack("=iii", 0, 100, 2)), pa.py_buffer(b"ab")]
    )
    with pytest.raises(TypeError, match="offset"):
        _sqlaero._encode_value(value)


@pytest.mark.parametrize(
    "view,buffers",
    [
        (struct.pack("=IIII", 20, int.from_bytes(b"aaaa", "little"), 5, 0), [b"a" * 20]),
        (struct.pack("=I", 1) + b"\xff" + b"\0" * 11, []),
    ],
    ids=["buffer-index", "inline-utf8"],
)
def test_arrow_invalid_string_view_is_checked_before_native_access(view: bytes, buffers: list[bytes]) -> None:
    value = pa.Array.from_buffers(
        pa.string_view(), 1, [None, pa.py_buffer(view), *(pa.py_buffer(buffer) for buffer in buffers)]
    )
    with pytest.raises(TypeError):
        _sqlaero._encode_value(value)


@pytest.mark.parametrize("key", [-1, 2])
@pytest.mark.parametrize("prefix", [0, 39], ids=["cold", "prepared-reader"])
def test_arrow_dictionary_rejects_invalid_actual_key(key: int, prefix: int) -> None:
    keys = pa.array([0] * prefix + [key], type=pa.int8())
    value = pa.DictionaryArray.from_arrays(keys, pa.array(["a", "b"]), safe=False)
    with pytest.raises(TypeError, match="[Dd]ictionary"):
        _sqlaero._encode_value(value)
    if prefix:
        with pytest.raises(TypeError, match="[Dd]ictionary") as caught:
            _sqlaero._bind_sql("SELECT %s", value, many=True)
        assert "args[39]" in "\n".join(getattr(caught.value, "__notes__", ()))


@pytest.mark.parametrize("many", [False, True])
@pytest.mark.parametrize("itemize", [False, True])
def test_empty_arrow_root_uses_zero_positions(many: bool, itemize: bool) -> None:
    value = pa.array([], type=pa.int64())
    assert _sqlaero._bind_sql("SELECT 1", value, many=many, itemize=itemize) == [b"\x03SELECT 1"]
    with pytest.raises(_sqlaero.ProgrammingError):
        _sqlaero._bind_sql("SELECT %s", value, many=many, itemize=itemize)


def test_arrow_zero_width_table_preserves_its_nonzero_row_count() -> None:
    rows = pa.array([{}, {}], type=pa.struct([]))
    value = pa.RecordBatch.from_struct_array(rows)
    assert _sqlaero._bind_sql("SELECT 1", value) == [b"\x03SELECT 1", b"\x03SELECT 1"]


@pytest.mark.parametrize("kind", ["chunked", "batch", "table", "zero-batch", "zero-table"])
@pytest.mark.parametrize("many", [False, True])
@pytest.mark.parametrize("itemize", [False, True])
def test_arrow_empty_and_zero_column_shapes_keep_every_logical_row(kind: str, many: bool, itemize: bool) -> None:
    if kind == "chunked":
        value = pa.chunked_array([], type=pa.int64())
    elif kind in ("batch", "table"):
        batch = pa.record_batch([pa.array([], type=pa.int64())], names=["x"])
        value = batch if kind == "batch" else pa.Table.from_batches([batch])
    else:
        batch = pa.RecordBatch.from_struct_array(pa.array([{}, {}, {}], type=pa.struct([])))
        value = batch if kind == "zero-batch" else pa.Table.from_batches([batch])
    count = 3 if kind.startswith("zero-") else 1
    assert _sqlaero._bind_sql("SELECT 1", value, many=many, itemize=itemize) == [b"\x03SELECT 1"] * count
    with pytest.raises(_sqlaero.ProgrammingError):
        _sqlaero._bind_sql("SELECT %s", value, many=many, itemize=itemize)


@pytest.mark.parametrize(
    "value",
    [
        pytest.param(pa.array([], type=pa.float16()), id="empty-half"),
        pytest.param(pa.array([None], type=pa.map_(pa.string(), pa.int64())), id="null-map"),
        pytest.param(pa.array([], type=pa.list_view(pa.int64())), id="empty-list-view"),
        pytest.param(pa.array([b"\0" * 16], type=pa.uuid()), id="uuid-extension"),
        pytest.param(
            pa.DictionaryArray.from_arrays(
                pa.array([0], type=pa.int8()), pa.array([b"\0" * 16], type=pa.uuid())
            ),
            id="dictionary-extension-label",
        ),
    ],
)
def test_unsupported_arrow_schema_is_not_hidden_by_empty_or_null_data(value: object) -> None:
    with pytest.raises(TypeError):
        _sqlaero._bind_sql("SELECT 1", value)


@pytest.mark.parametrize("kind", ["chunked", "cached-dictionary", "typed-int64", "typed-float32", "typed-boolean"])
def test_arrow_repeated_shared_immutable_input_has_independent_stream_owners(kind: str) -> None:
    # Each call exports and consumes its own capsule. Sharing the Python source
    # cannot make another call observe an already consumed or released capsule.
    if kind == "cached-dictionary":
        value = pa.DictionaryArray.from_arrays(pa.array([0] * 40, type=pa.int8()), pa.array(["label"]))
        expected = b"(" + b",".join([b"'label'"] * 40) + b")"
    elif kind == "typed-int64":
        value = pa.array(list(range(40)), type=pa.int64())
        expected = b"(" + b",".join(str(index).encode() for index in range(40)) + b")"
    elif kind == "typed-float32":
        value = pa.array([0.5] * 40, type=pa.float32())
        expected = b"(" + b",".join([b"0.5e0"] * 40) + b")"
    elif kind == "typed-boolean":
        value = pa.array([True, None, False, True] * 10, type=pa.bool_())
        expected = b"(" + b",".join([b"1", b"NULL", b"0", b"1"] * 10) + b")"
    else:
        value = pa.chunked_array([[1, None], [], [-3, 4]])
        expected = b"(1,NULL,-3,4)"

    def encode_repeatedly(_: int) -> bytes:
        result = b""
        for _ in range(25):
            result = _sqlaero._encode_value(value).to_bytes()
            assert result == expected
        return result

    with ThreadPoolExecutor(max_workers=4) as executor:
        assert list(executor.map(encode_repeatedly, range(8))) == [expected] * 8


@pytest.mark.parametrize(
    "kind,expected",
    [
        ("direct", b"(10,20,30,40,50,60)"),
        ("slice", b"(30,40,50)"),
        ("reverse", b"(60,50,40,30,20,10)"),
        ("gap", b"(20,40,60)"),
        ("positive-physical", b"(20,30,40,50,60)"),
        ("negative-physical", b"(20,10)"),
        ("zero-stride", b"(20,20,20)"),
    ],
)
def test_arrow_to_numpy_retains_the_real_physical_buffer_owner(kind: str, expected: bytes) -> None:
    np = pytest.importorskip("numpy")
    source = pa.array([10, 20, 30, 40, 50, 60], type=pa.int64())
    value = source.to_numpy()
    if kind == "slice":
        value = source.slice(2, 3).to_numpy()
    elif kind == "reverse":
        value = value[::-1]
    elif kind == "gap":
        value = value[1::2]
    elif kind in ("positive-physical", "negative-physical", "zero-stride"):
        value = source.slice(1, 2).to_numpy()
        shape, stride = {"positive-physical": (5, 8), "negative-physical": (2, -8), "zero-stride": (3, 0)}[kind]
        # These proposed windows are wholly inside the actual 48-byte Arrow
        # Buffer, including values outside this slice's 16 logical bytes.
        value = np.lib.stride_tricks.as_strided(value, shape=(shape,), strides=(stride,))
    assert _sqlaero._encode_value(value).to_bytes() == expected


@pytest.mark.parametrize("kind", ["end", "begin", "sliced-end", "huge-stride"])
def test_arrow_backed_numpy_rejects_out_of_buffer_spans_before_reading(kind: str) -> None:
    np = pytest.importorskip("numpy")
    source = pa.array([10, 20, 30, 40, 50, 60], type=pa.int64())
    value = source.slice(1, 2).to_numpy() if kind == "sliced-end" else source.to_numpy()
    shape, stride = {"end": (7, 8), "begin": (2, -8), "sliced-end": (6, 8), "huge-stride": (2, sys.maxsize)}[kind]
    # as_strided changes metadata only. Never index or stringify this invalid
    # view: the production allocation preflight must reject it before reading.
    value = np.lib.stride_tricks.as_strided(value, shape=(shape,), strides=(stride,))
    with pytest.raises(BufferError):
        _sqlaero._encode_value(value)


def test_empty_arrow_to_numpy_does_not_need_a_fabricated_allocation_interval() -> None:
    value = pa.array([], type=pa.int64()).to_numpy()
    assert _sqlaero._encode_value(value).to_bytes() == b"()"


def test_arrow_owner_bridge_does_not_expand_direct_arrow_dtype_support() -> None:
    source = pa.array([1.5], type=pa.float16())
    value = source.to_numpy()
    # The input is a supported NumPy float16 ndarray. Arrow is only the physical
    # owner; its direct half-float input contract remains explicitly excluded.
    assert _sqlaero._encode_value(value).to_bytes() == b"(1.5e0)"
    with pytest.raises(TypeError):
        _sqlaero._encode_value(source)


def test_arrow_backed_numpy_keeps_and_then_releases_its_actual_array_owner() -> None:
    np = pytest.importorskip("numpy")
    source = pa.array([10, 20, 30], type=pa.int64())
    value = source.to_numpy()
    owner = value
    while type(owner) is np.ndarray:
        owner = owner.base
    reference = weakref.ref(owner)
    del owner, source
    gc.collect()
    assert reference() is not None
    assert _sqlaero._encode_value(value).to_bytes() == b"(10,20,30)"
    del value
    gc.collect()
    assert reference() is None
