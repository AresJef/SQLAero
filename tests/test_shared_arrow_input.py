"""Shared Arrow facts reach native targets without Python scalar conversion."""

# pyright: reportPrivateUsage=false


from typing import Any
from collections.abc import Callable

import json
import math
import struct
from datetime import date
from decimal import Decimal

import pytest

from sqlaero import _sqlaero, json_dumps

pa = pytest.importorskip("pyarrow")


@pytest.mark.parametrize("kind", [pa.int8, pa.int16, pa.int32, pa.int64, pa.uint8, pa.uint16, pa.uint32, pa.uint64])
@pytest.mark.parametrize("chunked", [False, True])
def test_native_integer_values_and_masks(kind: Callable[[], Any], chunked: bool) -> None:
    value = pa.array([7, None, 8], type=kind())
    if chunked:
        value = pa.chunked_array([value.slice(0, 1), value.slice(1, 0), value.slice(1)])
    assert json_dumps(value) == b"[7,null,8]"
    assert _sqlaero._escape_projection(value) == ("7", "NULL", "8")


@pytest.mark.parametrize("kind", [pa.float32, pa.float64])
def test_native_float_bits_and_nonfinite_policy(kind: Callable[[], Any]) -> None:
    value = pa.array([-0.0, 0.5, None, float("nan"), float("inf")], type=kind())
    result = json.loads(json_dumps(value))
    assert math.copysign(1.0, result[0]) == -1.0
    assert result[1:] == [0.5, None, None, None]
    with pytest.raises(ValueError, match="nonfinite"):
        json_dumps(value, allow_nan=False)


@pytest.mark.parametrize("kind", [pa.string, pa.large_string, pa.string_view])
@pytest.mark.parametrize("length", [0, 7, 40000])
def test_native_utf8_complete_values_and_sliced_offsets(kind: Callable[[], Any], length: int) -> None:
    text = "ab\0\n\"\\\U0001f642" * length
    value = pa.array(["ignored", text, None, "tail"], type=kind()).slice(1, 2)
    assert json.loads(json_dumps(value)) == [text, None]


@pytest.mark.parametrize("kind", [pa.binary, pa.large_binary, pa.binary_view, lambda: pa.binary(2)])
def test_binary_schema_is_rejected_even_when_no_actual_value(kind: Callable[[], Any]) -> None:
    for value in (pa.array([], type=kind()), pa.array([None], type=kind())):
        with pytest.raises(TypeError, match="binary"):
            json_dumps(value)


def test_boolean_native_offsets_and_null_data_bits() -> None:
    value = pa.array([False, True, None, False, True]).slice(1, 3)
    assert json_dumps(value) == b"[true,null,false]"


def test_default_arrow_string_pandas_column_uses_shared_reader() -> None:
    pd = pytest.importorskip("pandas")
    value = pd.DataFrame({"id": [7, 8], "name": ["ab", "cd"]})
    assert json_dumps(value) == b'[{"id":7,"name":"ab"},{"id":8,"name":"cd"}]'


@pytest.mark.parametrize("kind",[pa.int8,pa.int16,pa.int32,pa.int64,pa.uint8,pa.uint16,pa.uint32,pa.uint64])
def test_each_integer_table_extreme_is_an_actual_value_not_a_missing_sentinel(kind: Callable[[],Any]) -> None:
    dtype=kind()
    bits=dtype.bit_width
    lower=-(1<<(bits-1)) if pa.types.is_signed_integer(dtype) else 0
    upper=(1<<(bits-1))-1 if pa.types.is_signed_integer(dtype) else (1<<bits)-1
    values=[7,8,lower,upper,None]
    source=pa.array(values,type=dtype)
    assert _sqlaero._escape_projection(source)==("7","8",str(lower),str(upper),"NULL")
    assert json_dumps(source)==json.dumps(values,separators=(",",":" )).encode()
    assert _sqlaero._escape_projection(source[:0])==()
    assert json_dumps(source[:0])==b"[]"


@pytest.mark.parametrize("kind",[pa.float32,pa.float64])
def test_float_table_preserves_the_actual_binary_value_and_strict_missing_distinction(kind: Callable[[],Any]) -> None:
    source=pa.array([2.5,0.1,-0.0,None],type=kind())
    text="0.10000000149011612" if kind is pa.float32 else "0.1"
    assert _sqlaero._escape_projection(source)==("2.5e0",text+"e0","-0.0e0","NULL")
    assert json_dumps(source)==("[2.5,"+text+",-0.0,null]").encode()
    assert _sqlaero._escape_projection(source[:0])==()
    assert json_dumps(source[:0])==b"[]"
    assert json_dumps(pa.array([None],type=kind()),allow_nan=False)==b"[null]"
    for value in (float("nan"),float("inf"),float("-inf")):
        nonfinite=pa.array([value],type=kind())
        assert _sqlaero._escape_projection(nonfinite)==("NULL",)
        assert json_dumps(nonfinite)==b"[null]"
        with pytest.raises(ValueError,match="nonfinite"):json_dumps(nonfinite,allow_nan=False)


@pytest.mark.parametrize("kind",[pa.string,pa.large_string,pa.string_view])
def test_text_table_uses_logical_unicode_including_an_actual_empty_value(kind: Callable[[],Any]) -> None:
    source=pa.array(["ab","中文","a\0b","",None],type=kind())
    assert _sqlaero._escape_projection(source)==("'ab'","'中文'","'a\\0b'","''","NULL")
    assert json_dumps(source)=='["ab","中文","a\\u0000b","",null]'.encode()
    assert _sqlaero._escape_projection(source[:0])==()
    assert json_dumps(source[:0])==b"[]"


@pytest.mark.parametrize("kind",[pa.binary,pa.large_binary,pa.binary_view,lambda:pa.binary(2)])
def test_binary_table_keeps_sql_bytes_and_never_grants_json_schema_admission(kind: Callable[[],Any]) -> None:
    source=pa.array([b"ab",None],type=kind())
    assert _sqlaero._escape_projection(source)==("CAST(X'6162' AS BINARY)","NULL")
    assert _sqlaero._escape_projection(source[:0])==()
    for value in (source,source[:0],source[1:]):
        with pytest.raises(TypeError,match="binary"):json_dumps(value)


@pytest.mark.parametrize("kind",[pa.binary,pa.large_binary,pa.binary_view,lambda:pa.binary(0)])
def test_zero_byte_binary_value_is_a_real_sql_parameter(kind: Callable[[],Any]) -> None:
    source=pa.array([b""],type=kind())
    assert _sqlaero._escape_projection(source)==("CAST(X'' AS BINARY)",)
    with pytest.raises(TypeError,match="binary"):json_dumps(source)


def test_boolean_and_null_table_have_distinct_logical_types() -> None:
    for source,sql,document in (
        (pa.nulls(2),("NULL","NULL"),b"[null,null]"),
        (pa.array([True,False,None]),("1","0","NULL"),b"[true,false,null]"),
    ):
        assert _sqlaero._escape_projection(source)==sql
        assert json_dumps(source,allow_nan=False)==document
        assert _sqlaero._escape_projection(source[:0])==()
        assert json_dumps(source[:0])==b"[]"


@pytest.mark.parametrize("kind",[pa.date32,pa.date64])
def test_date_table_is_a_calendar_date_with_no_implicit_midnight(kind: Callable[[],Any]) -> None:
    source=pa.array([date(2026,11,12),date(1,1,1),None],type=kind())
    assert _sqlaero._escape_projection(source)==("'2026-11-12'","'0001-01-01'","NULL")
    assert json_dumps(source)==b'["2026-11-12","0001-01-01",null]'
    assert json_dumps(source[:0])==b"[]"


@pytest.mark.parametrize(("dtype","ticks","sql","document"),[
    (pa.time32("s"),29350,"'08:09:10'",b'["08:09:10"]'),
    (pa.time32("ms"),29350120,"'08:09:10.120000'",b'["08:09:10.120000"]'),
    (pa.time32("s"),86400,"'24:00:00'",b'["24:00:00"]'),
    (pa.time32("s"),90000,"'25:00:00'",b'["25:00:00"]'),
    (pa.time32("s"),-3600,"'-01:00:00'",b'["-01:00:00"]'),
    (pa.time32("s"),-1,"'-00:00:01'",b'["-00:00:01"]'),
    (pa.time32("ms"),-1,"'-00:00:00.001000'",b'["-00:00:00.001000"]'),
    (pa.time32("s"),3020400,"'839:00:00'",b'["839:00:00"]'),
    (pa.time64("us"),29350120000,"'08:09:10.120000'",b'["08:09:10.120000"]'),
    (pa.time64("ns"),29350123456789,"'08:09:10.123456'",b'["08:09:10.123456789"]'),
    (pa.time64("ns"),900,"'00:00:00'",b'["00:00:00.000000900"]'),
    (pa.time64("ns"),-1,"'00:00:00'",b'["-00:00:00.000000001"]'),
    (pa.time64("us"),-1,"'-00:00:00.000001'",b'["-00:00:00.000001"]'),
    (pa.time64("ns"),-1001,"'-00:00:00.000001'",b'["-00:00:00.000001001"]'),
    (pa.time64("ns"),86400000000000,"'24:00:00'",b'["24:00:00"]'),
    (pa.time64("ns"),90000000000001,"'25:00:00'",b'["25:00:00.000000001"]'),
    (pa.time64("ns"),-90000000000001,"'-25:00:00'",b'["-25:00:00.000000001"]'),
    (pa.time64("ns"),86399999999999,"'23:59:59.999999'",b'["23:59:59.999999999"]'),
    (pa.timestamp("ns"),-(1<<63),"'1677-09-21 00:12:43.145224'",b'["1677-09-21T00:12:43.145224192"]'),
    (pa.timestamp("ns"),-1,"'1969-12-31 23:59:59.999999'",b'["1969-12-31T23:59:59.999999999"]'),
    (pa.duration("s"),90,"'00:01:30'",b'["PT90S"]'),
    (pa.duration("ns"),900,"'00:00:00'",b'["PT0.0000009S"]'),
    (pa.duration("ns"),-1,"'00:00:00'",b'["-PT0.000000001S"]'),
    (pa.duration("ns"),-1001,"'-00:00:00.000001'",b'["-PT0.000001001S"]'),
    (pa.duration("ns"),-(1<<63),"'-2562047:47:16.854775'",b'["-P106751DT85636.854775808S"]'),
])
def test_temporal_table_compares_literal_outputs_at_both_exits(dtype: Any,ticks: int,sql: str,document: bytes) -> None:
    raw=pa.py_buffer(struct.pack("=i" if dtype.bit_width==32 else "=q",ticks))
    source=pa.Array.from_buffers(dtype,1,[None,raw])
    assert _sqlaero._escape_projection(source)==(sql,)
    assert json_dumps(source)==document
    missing=pa.array([None],type=dtype)
    assert _sqlaero._escape_projection(missing)==("NULL",)
    assert json_dumps(missing,allow_nan=False)==b"[null]"
    assert _sqlaero._escape_projection(source[:0])==()
    assert json_dumps(source[:0])==b"[]"


@pytest.mark.parametrize(("factory","precision"),[(pa.decimal32,9),(pa.decimal64,18),(pa.decimal128,38),(pa.decimal256,76)])
@pytest.mark.parametrize(("scale","value","sql","document"),[
    (3,"12.500","12.500",b'["12.500",null]'),
    (2,"0.00","0.00",b'["0.00",null]'),
    (-2,"12300","12300",b'["1.23E+4",null]'),
])
def test_decimal_table_retains_exact_scale_and_target_specific_spelling(factory: Callable[...,Any],precision: int,scale: int,value: str,sql: str,document: bytes) -> None:
    source=pa.array([Decimal(value),None],type=factory(precision,scale))
    assert _sqlaero._escape_projection(source)==(sql,"NULL")
    assert json_dumps(source)==document
    assert _sqlaero._escape_projection(source[:0])==()
    assert json_dumps(source[:0])==b"[]"


@pytest.mark.parametrize("kind",[pa.date32,pa.date64])
@pytest.mark.parametrize(("days","text"),[(-719893,"-0001-01-01"),(3663382,"12000-01-01")])
def test_date_table_extended_years_keep_the_full_integer_year(kind: Callable[[],Any],days: int,text: str) -> None:
    dtype=kind()
    ticks=days if dtype.bit_width==32 else days*86_400_000
    source=pa.Array.from_buffers(dtype,1,[None,pa.py_buffer(struct.pack("=i" if dtype.bit_width==32 else "=q",ticks))])
    assert _sqlaero._escape_projection(source)==("'"+text+"'",)
    assert json_dumps(source)==('["'+text+'"]').encode()


@pytest.mark.parametrize("ticks",[-1,1])
def test_date64_nonintegral_day_is_rejected_by_both_targets(ticks: int) -> None:
    source=pa.Array.from_buffers(pa.date64(),1,[None,pa.py_buffer(struct.pack("=q",ticks))])
    for convert in (_sqlaero._escape_projection,json_dumps):
        with pytest.raises(_sqlaero.DataError,match="integral day"):convert(source)


@pytest.mark.parametrize("kind",["seconds","nanoseconds","aware"])
def test_timestamp_table_preserves_original_epoch_and_target_precision(kind: str) -> None:
    import calendar
    seconds=calendar.timegm((2026,11,12,8,9,10))
    if kind=="seconds":
        source=pa.array([seconds],type=pa.timestamp("s"))
        sql,document="'2026-11-12 08:09:10'",b'["2026-11-12T08:09:10"]'
    elif kind=="nanoseconds":
        source=pa.array([seconds*1_000_000_000+123456789],type=pa.timestamp("ns"))
        sql,document="'2026-11-12 08:09:10.123456'",b'["2026-11-12T08:09:10.123456789"]'
    else:
        source=pa.array([(seconds-8*3600)*1_000_000_000+123456789],type=pa.timestamp("ns","+08:00"))
        sql,document="'2026-11-12 00:09:10.123456'",b'["2026-11-12T08:09:10.123456789+08:00"]'
    assert _sqlaero._escape_projection(source)==(sql,)
    assert json_dumps(source)==document


@pytest.mark.parametrize("kind",["table","batch"])
def test_real_index_named_column_is_not_hidden_by_unrelated_pandas_metadata(kind: str) -> None:
    factory=pa.table if kind=="table" else pa.record_batch
    metadata={b"pandas":b'{"index_columns":["__index_level_0__"],"columns":[]}'}
    source=factory({"__index_level_0__":[7,8]}).replace_schema_metadata(metadata)
    assert _sqlaero._escape_projection(source)==[("7",),("8",)]
    assert json_dumps(source)==b'[{"__index_level_0__":7},{"__index_level_0__":8}]'
    source=factory({"id":[7]}).replace_schema_metadata(metadata)
    assert _sqlaero._escape_projection(source)==[("7",)]
    assert json_dumps(source)==b'[{"id":7}]'


def test_missing_struct_parent_is_not_an_existing_record_with_missing_fields() -> None:
    source=pa.array([None,{"id":None,"name":None}],type=pa.struct([("id",pa.int64()),("name",pa.string())]))
    assert _sqlaero._escape_projection(source)==("NULL","(NULL,NULL)")
    assert _sqlaero._escape_projection(source,many=True)==["NULL",("NULL","NULL")]
    assert json_dumps(source)==b'[null,{"id":null,"name":null}]'


@pytest.mark.parametrize("kind",["list","large-list","fixed-zero"])
def test_actual_empty_list_and_missing_parent_have_distinct_row_widths(kind: str) -> None:
    dtype=pa.list_(pa.int64()) if kind=="list" else pa.large_list(pa.int64()) if kind=="large-list" else pa.list_(pa.int64(),0)
    source=pa.array([[],None,[]],type=dtype)
    assert _sqlaero._escape_projection(source)==("()","NULL","()")
    assert _sqlaero._escape_projection(source,many=True)==[(),"NULL",()]
    assert json_dumps(source)==b"[[],null,[]]"


def test_dictionary_only_visits_actual_labels_and_preserves_each_kind_of_missing() -> None:
    labels=pa.Array.from_buffers(pa.string(),2,[None,pa.py_buffer(struct.pack("=iii",0,2,3)),pa.py_buffer(b"ab\xff")])
    selected=pa.DictionaryArray.from_arrays(pa.array([0,None,0],type=pa.int8()),labels)
    assert _sqlaero._escape_projection(selected)==("'ab'","NULL","'ab'")
    assert json_dumps(selected)==b'["ab",null,"ab"]'
    missing=pa.DictionaryArray.from_arrays(pa.array([0,None],type=pa.int8()),pa.array([None],type=pa.string()))
    assert _sqlaero._escape_projection(missing)==("NULL","NULL")
    assert json_dumps(missing,allow_nan=False)==b"[null,null]"
    nonfinite=pa.DictionaryArray.from_arrays(pa.array([0],type=pa.int8()),pa.array([float("nan")]))
    assert _sqlaero._escape_projection(nonfinite)==("NULL",)
    assert json_dumps(nonfinite)==b"[null]"
    with pytest.raises(ValueError,match="nonfinite"):json_dumps(nonfinite,allow_nan=False)


@pytest.mark.parametrize("key",[-1,2])
def test_dictionary_invalid_code_is_rejected_at_both_target_exits(key: int) -> None:
    source=pa.DictionaryArray.from_arrays(pa.array([0,key],type=pa.int8()),pa.array(["ab","cd"]),safe=False)
    for convert in (_sqlaero._escape_projection,json_dumps):
        with pytest.raises(TypeError,match="[Dd]ictionary"):convert(source)


@pytest.mark.parametrize("middle_missing",[False,True])
def test_actual_empty_chunk_is_distinct_from_one_missing_value(middle_missing: bool) -> None:
    middle=pa.array([None],type=pa.int64()) if middle_missing else pa.array([],type=pa.int64())
    source=pa.chunked_array([pa.array([7]),middle,pa.array([8,9])])
    assert _sqlaero._escape_projection(source)==(("7","NULL","8","9") if middle_missing else ("7","8","9"))
    assert json_dumps(source)==(b"[7,null,8,9]" if middle_missing else b"[7,8,9]")
    empty=pa.chunked_array([],type=pa.int64())
    assert _sqlaero._escape_projection(empty)==()
    assert json_dumps(empty)==b"[]"


@pytest.mark.parametrize("kind",["batch","table"])
def test_actual_two_dimensional_arrow_table_keeps_rows_and_column_names(kind: str) -> None:
    factory=pa.record_batch if kind=="batch" else pa.table
    source=factory({"id":[7,8],"name":["ab","cd"]})
    assert _sqlaero._escape_projection(source)==[("7","'ab'"),("8","'cd'")]
    assert json_dumps(source)==b'[{"id":7,"name":"ab"},{"id":8,"name":"cd"}]'
    assert _sqlaero._escape_projection(source.slice(0,0))==[()]
    assert json_dumps(source.slice(0,0))==b"[]"


@pytest.mark.parametrize("kind",["half","map","list-view","extension","dictionary-extension"])
def test_unknown_arrow_schema_never_hides_in_empty_or_missing_values(kind: str) -> None:
    if kind=="half":source=pa.array([],type=pa.float16())
    elif kind=="map":source=pa.array([None],type=pa.map_(pa.string(),pa.int64()))
    elif kind=="list-view":source=pa.array([],type=pa.list_view(pa.int64()))
    elif kind=="extension":source=pa.array([b"\0"*16],type=pa.uuid())
    else:source=pa.DictionaryArray.from_arrays(pa.array([0],type=pa.int8()),pa.array([b"\0"*16],type=pa.uuid()))
    for convert in (_sqlaero._escape_projection,json_dumps):
        with pytest.raises(TypeError):convert(source)


@pytest.mark.parametrize(("dtype","json_allowed"),[
    *((kind(),True) for kind in (pa.null,pa.bool_,pa.int8,pa.int16,pa.int32,pa.int64,pa.uint8,pa.uint16,pa.uint32,pa.uint64,pa.float32,pa.float64,pa.string,pa.large_string,pa.string_view,pa.date32,pa.date64)),
    *((kind(),False) for kind in (pa.binary,pa.large_binary,pa.binary_view)),
    (pa.binary(2),False),
    *((pa.time32(unit),True) for unit in ("s","ms")),
    *((pa.time64(unit),True) for unit in ("us","ns")),
    *((pa.timestamp(unit),True) for unit in ("s","ms","us","ns")),
    *((pa.duration(unit),True) for unit in ("s","ms","us","ns")),
    *((factory(8,2),True) for factory in (pa.decimal32,pa.decimal64,pa.decimal128,pa.decimal256)),
    (pa.list_(pa.int64()),True),(pa.large_list(pa.int64()),True),(pa.list_(pa.int64(),2),True),
    (pa.struct([("id",pa.int64()),("name",pa.string())]),True),
    (pa.dictionary(pa.int8(),pa.string()),True),
])
def test_each_declared_arrow_family_checks_schema_without_manufacturing_values(dtype: Any,json_allowed: bool) -> None:
    empty=pa.array([],type=dtype)
    missing=pa.array([None],type=dtype)
    assert _sqlaero._escape_projection(empty)==()
    assert _sqlaero._escape_projection(empty,many=True)==[()]
    assert _sqlaero._escape_projection(missing)==("NULL",)
    if json_allowed:
        assert json_dumps(empty)==b"[]"
        assert json_dumps(missing,allow_nan=False)==b"[null]"
    else:
        for value in (empty,missing):
            with pytest.raises(TypeError,match="binary"):json_dumps(value)
