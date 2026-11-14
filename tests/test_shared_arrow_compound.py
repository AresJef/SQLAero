"""Native Arrow cells expose structure once; targets choose row/JSON layout."""

import json
import struct
import pytest
from sqlaero import _sqlaero, json_dumps

pa = pytest.importorskip("pyarrow")


@pytest.mark.parametrize("dtype", [pa.list_(pa.int64()), pa.large_list(pa.int64()), pa.list_(pa.int64(), 2)])
@pytest.mark.parametrize("chunked", [False, True])
def test_list_cells_preserve_structure_and_parent_null(dtype, chunked: bool) -> None:
    value = pa.array([[7, None], None, [8, 9]], type=dtype)
    if chunked:
        value = pa.chunked_array([value.slice(0, 1), value.slice(1, 0), value.slice(1)])
    assert json.loads(json_dumps(value)) == [[7, None], None, [8, 9]]
    assert _sqlaero._escape_projection(value) == ("(7,NULL)", "NULL", "(8,9)")
    assert _sqlaero._escape_projection(value, many=True) == [("7", "NULL"), "NULL", ("8", "9")]


def test_nested_list_shape_does_not_become_another_root_batch() -> None:
    value = pa.array([[[7, 8], [9]], [[]]], type=pa.list_(pa.list_(pa.int64())))
    assert json_dumps(value) == b"[[[7,8],[9]],[[]]]"
    assert _sqlaero._escape_projection(value, many=True) == [("(7,8)", "(9)"), ("()",)]


def test_each_dictionary_chunk_uses_its_own_nested_labels() -> None:
    def dictionary(labels, keys):
        return pa.DictionaryArray.from_arrays(pa.array(keys, type=pa.int8()), pa.array(labels, type=pa.list_(pa.int64())))
    value = pa.chunked_array([dictionary([[7], [8, 9]], [1, None, 0]), dictionary([[10], []], [0, 1])])
    assert json_dumps(value) == b"[[8,9],null,[7],[10],[]]"
    assert _sqlaero._escape_projection(value, many=True) == [("8", "9"), "NULL", ("7",), ("10",), ()]


def test_struct_series_is_one_dimensional_rows_only_when_many_true() -> None:
    value = pa.array([{"id": 7, "name": "ab"}, {"id": 8, "name": "cd"}], type=pa.struct([("id", pa.int64()), ("name", pa.string())]))
    assert _sqlaero._escape_projection(value) == ("(7,'ab')", "(8,'cd')")
    assert _sqlaero._escape_projection(value, many=True) == [("7", "'ab'"), ("8", "'cd'")]


@pytest.mark.parametrize("kind", ["list", "struct"])
def test_missing_compound_parent_never_reads_invalid_utf8_payload(kind: str) -> None:
    child = pa.Array.from_buffers(pa.string(), 1, [None, pa.py_buffer(struct.pack("=ii", 0, 1)), pa.py_buffer(b"\xff")])
    value = pa.ListArray.from_arrays(pa.array([0, 1], type=pa.int32()), child, mask=pa.array([True])) if kind == "list" else pa.StructArray.from_arrays([child], names=["ignored"], mask=pa.array([True]))
    assert json_dumps(value) == b"[null]"
    assert _sqlaero._escape_projection(value, many=True) == ["NULL"]
    assert _sqlaero._bind_sql("SELECT %s", value, many=True) == [b"\x03SELECT NULL"]


def test_null_parent_does_not_make_an_unsupported_binary_schema_supported() -> None:
    value = pa.array([None], type=pa.struct([("data", pa.binary())]))
    with pytest.raises(TypeError, match="binary"):
        json_dumps(value)


def test_zero_field_records_are_objects_not_nulls() -> None:
    value = pa.array([{}, None, {}], type=pa.struct([]))
    assert json_dumps(value) == b"[{},null,{}]"
    assert _sqlaero._escape_projection(value, many=True) == [(), "NULL", ()]
