"""Full, length-aware Arrow names join native values by schema position."""

import json
import pytest
from sqlaero import _sqlaero, json_dumps

pa = pytest.importorskip("pyarrow")


def record(names=("a\0x", "a\0y"), *, missing=False):
    return pa.StructArray.from_arrays([pa.array([7]), pa.array([8])], names=list(names), mask=pa.array([missing]))


@pytest.mark.parametrize("kind", ["struct", "chunked", "table", "batch", "nested_table", "list", "large_list", "fixed_list", "dictionary", "pandas_series", "pandas_frame"])
def test_complete_nul_names_are_distinct_and_keep_native_field_positions(kind: str) -> None:
    inner = record()
    row = {"a\0x": 7, "a\0y": 8}
    if kind == "struct":
        value, expected = inner, [row]
    elif kind == "chunked":
        value, expected = pa.chunked_array([inner.slice(0, 0), inner, inner]), [row, row]
    elif kind in ("table", "batch"):
        constructor = pa.Table.from_arrays if kind == "table" else pa.RecordBatch.from_arrays
        value, expected = constructor([pa.array([7]), pa.array([8])], names=["a\0x", "a\0y"]), [row]
    elif kind == "nested_table":
        value, expected = pa.table({"record": inner}), [{"record": row}]
    elif kind in ("list", "large_list", "fixed_list"):
        dtype = pa.list_(inner.type) if kind == "list" else pa.large_list(inner.type) if kind == "large_list" else pa.list_(inner.type, 1)
        value, expected = pa.array([[row]], type=dtype), [[row]]
    elif kind == "dictionary":
        value, expected = pa.DictionaryArray.from_arrays(pa.array([0, None, 0], type=pa.int8()), inner), [row, None, row]
    else:
        pd = pytest.importorskip("pandas")
        series = pd.Series(pd.array(inner, dtype=pd.ArrowDtype(inner.type)))
        value, expected = (series, [row]) if kind == "pandas_series" else (pd.DataFrame({"record": series}), [{"record": row}])
    result = json_dumps(value)
    assert json.loads(result) == expected
    assert b'"a\\u0000x":7' in result and b'"a\\u0000y":8' in result


@pytest.mark.parametrize("names", [("a", "a"), ("a\0x", "a\0x")])
def test_actual_duplicate_complete_names_fail_only_for_actual_objects(names) -> None:
    inner = record(names)
    with pytest.raises(ValueError, match="collid"):
        json_dumps(inner)
    assert json_dumps(inner.slice(0, 0)) == b"[]"
    assert json_dumps(record(names, missing=True)) == b"[null]"
    assert _sqlaero._escape_projection(inner) == ("(7,8)",)


@pytest.mark.parametrize("kind", ["table", "batch"])
def test_table_names_check_zero_rows_and_zero_columns_independently(kind: str) -> None:
    constructor = pa.Table.from_arrays if kind == "table" else pa.RecordBatch.from_arrays
    table = constructor([pa.array([7]), pa.array([8])], names=["same", "same"])
    with pytest.raises(ValueError, match="collid"):
        json_dumps(table)
    assert json_dumps(table.slice(0, 0)) == b"[]"
    empty = pa.RecordBatch.from_struct_array(pa.array([{}, {}], type=pa.struct([])))
    assert json_dumps(empty if kind == "batch" else pa.Table.from_batches([empty])) == b"[{},{}]"


def test_nested_field_schemas_keep_their_own_names_and_dictionary_identity() -> None:
    left = pa.struct([("same\0left", pa.int64())])
    right = pa.struct([("same\0right", pa.int64())])
    value = pa.array([{"left": {"same\0left": 7}, "right": {"same\0right": 8}}], type=pa.struct([("left", left), ("right", right)]))
    assert json.loads(json_dumps(value)) == [{"left": {"same\0left": 7}, "right": {"same\0right": 8}}]


@pytest.mark.parametrize("length", [1, 20000])
def test_full_unicode_name_is_not_truncated_or_normalized(length: int) -> None:
    name = ("e\u0301\0\U0001f642" * length) + "tail"
    value = pa.table({name: [7]})
    assert json.loads(json_dumps(value)) == [{name: 7}]


def test_sql_positional_read_does_not_enter_python_name_source() -> None:
    # Public provider construction accepts the full bytes; only its name
    # snapshot raises. The C Data prefix is valid, and positional SQL need not
    # decode this name. Source construction is separately recorded.
    value = pa.StructArray.from_arrays([pa.array([7])], names=[b"a\0\xff"])
    assert _sqlaero._escape_projection(value) == ("(7)",)
    with pytest.raises(UnicodeDecodeError) as caught:
        json_dumps(value)
    assert "schema field name at position 0" in "\n".join(getattr(caught.value, "__notes__", ()))


def test_parent_null_never_requests_unneeded_complete_name_snapshot() -> None:
    value = pa.StructArray.from_arrays([pa.array([7])], names=[b"a\0\xff"], mask=pa.array([True]))
    assert json_dumps(value) == b"[null]"


def test_provider_table_export_can_reject_name_before_driver_adaptation() -> None:
    value = pa.Table.from_arrays([pa.array([7])], names=[b"a\0\xff"])
    with pytest.raises(UnicodeDecodeError):
        value.__arrow_c_stream__()
    with pytest.raises(UnicodeDecodeError):
        _sqlaero._escape_projection(value)
