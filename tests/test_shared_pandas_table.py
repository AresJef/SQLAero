"""Literal Pandas table examples and equal-value policies at both target exits."""

# pyright: reportPrivateUsage=false
from collections.abc import Callable
from decimal import Decimal
from typing import Any, cast

import pytest
from sqlaero import _sqlaero, json_dumps

pd=pytest.importorskip("pandas")
np=pytest.importorskip("numpy")


@pytest.mark.parametrize(("value","sql","document"),[
    pytest.param(pd.NA,"NULL",b"null",id="045-01"),
    pytest.param(pd.NaT,"NULL",b"null",id="046-01"),
    pytest.param(pd.Timestamp("2026-11-12 08:09:10"),"'2026-11-12 08:09:10'",b'"2026-11-12T08:09:10"',id="047-01"),
    pytest.param(pd.Timestamp("2026-11-12 08:09:10.120"),"'2026-11-12 08:09:10.120000'",b'"2026-11-12T08:09:10.120000"',id="047-02"),
    pytest.param(pd.Timestamp("2026-11-12 08:09:10.123456789"),"'2026-11-12 08:09:10.123456'",b'"2026-11-12T08:09:10.123456789"',id="047-03"),
    pytest.param(pd.Timestamp("2026-11-12 08:09:10.123456789+08:00"),"'2026-11-12 00:09:10.123456'",b'"2026-11-12T08:09:10.123456789+08:00"',id="047-04"),
    pytest.param(pd.Timestamp(16725225600,unit="s").as_unit("s"),"'2500-01-01 00:00:00'",b'"2500-01-01T00:00:00"',id="047-05"),
    pytest.param(pd.Timedelta(90,unit="s"),"'00:01:30'",b'"PT90S"',id="048-01"),
    pytest.param(pd.Timedelta(120,unit="ms"),"'00:00:00.120000'",b'"PT0.12S"',id="048-02"),
    pytest.param(pd.Timedelta(900,unit="ns"),"'00:00:00'",b'"PT0.0000009S"',id="048-03"),
    pytest.param(pd.Timedelta(-1,unit="ns"),"'00:00:00'",b'"-PT0.000000001S"',id="048-04"),
    pytest.param(pd.Timedelta(days=1,seconds=90),"'24:01:30'",b'"P1DT90S"',id="048-05"),
    pytest.param(pd.Series([7,8],index=["u","v"],name="id"),("7","8"),b"[7,8]",id="049-01"),
    pytest.param(pd.Series([7,8],index=["u","u"]),("7","8"),b"[7,8]",id="049-02"),
    pytest.param(pd.Series([],dtype="int64"),(),b"[]",id="049-03"),
    pytest.param(pd.Series([7,"ab",None],dtype=object),("7","'ab'","NULL"),b'[7,"ab",null]',id="049-04"),
    pytest.param(pd.DataFrame({"id":[7,8]})["id"],("7","8"),b"[7,8]",id="049-05"),
    pytest.param(pd.DataFrame({"id":[7],"name":["ab"]}).iloc[0],("7","'ab'"),b'[7,"ab"]',id="049-06"),
    pytest.param(pd.Index(["u","v"],name="labels"),("'u'","'v'"),b'["u","v"]',id="050-01"),
    pytest.param(pd.Index([7,8]),("7","8"),b"[7,8]",id="050-02"),
    pytest.param(pd.Index([],dtype="int64"),(),b"[]",id="050-03"),
    pytest.param(pd.Index([7,None,7],dtype=object),("7","NULL","7"),b"[7,null,7]",id="050-04"),
    pytest.param(pd.RangeIndex(3),("0","1","2"),b"[0,1,2]",id="051-01"),
    pytest.param(pd.RangeIndex(5,0,-2),("5","3","1"),b"[5,3,1]",id="051-02"),
    pytest.param(pd.RangeIndex(0),(),b"[]",id="051-03"),
    pytest.param(pd.date_range("2026-11-12",periods=2),("'2026-11-12 00:00:00'","'2026-11-13 00:00:00'"),b'["2026-11-12T00:00:00","2026-11-13T00:00:00"]',id="052-01"),
    pytest.param(pd.DatetimeIndex(["2026-11-12 08:00:00+08:00"]),("'2026-11-12 00:00:00'",),b'["2026-11-12T08:00:00+08:00"]',id="052-02"),
    pytest.param(pd.DatetimeIndex(["2026-11-12 08:09:10.123456789"]),("'2026-11-12 08:09:10.123456'",),b'["2026-11-12T08:09:10.123456789"]',id="052-03"),
    pytest.param(pd.DatetimeIndex([pd.NaT]),("NULL",),b"[null]",id="052-04"),
    pytest.param(pd.CategoricalIndex(["ab",None,"ab"]),("'ab'","NULL","'ab'"),b'["ab",null,"ab"]',id="054-01"),
    pytest.param(pd.CategoricalIndex([7,None,8]),("7","NULL","8"),b"[7,null,8]",id="054-03"),
    pytest.param(pd.MultiIndex.from_tuples([(7,"ab"),(8,"cd")]),("(7,'ab')","(8,'cd')"),b'[[7,"ab"],[8,"cd"]]',id="055-01"),
    pytest.param(pd.DataFrame({"id":[7,8],"name":["ab","cd"]}),[("7","'ab'"),("8","'cd'")],b'[{"id":7,"name":"ab"},{"id":8,"name":"cd"}]',id="056-01"),
    pytest.param(pd.DataFrame([[7]],columns=[1]),[("7",)],b'[{"1":7}]',id="056-05"),
    pytest.param(pd.DataFrame([],columns=["id","id"]),[()],b"[]",id="056-07"),
    pytest.param(pd.DataFrame(index=[0,1]),[(),()],b"[{},{}]",id="056-08"),
    pytest.param(pd.DataFrame({"at":[pd.Timestamp("2026-11-12 08:09:10.123456789")],"lag":[pd.Timedelta(-900,unit="ns")]}),
                 [("'2026-11-12 08:09:10.123456'","'00:00:00'")],b'[{"at":"2026-11-12T08:09:10.123456789","lag":"-PT0.0000009S"}]',id="056-12"),
    pytest.param(pd.array([7,None],dtype="Int64"),("7","NULL"),b"[7,null]",id="058-01"),
    pytest.param(pd.array([2.5,None],dtype="Float64"),("2.5e0","NULL"),b"[2.5,null]",id="059-01"),
    pytest.param(pd.array([0.1],dtype="Float32"),("0.10000000149011612e0",),b"[0.10000000149011612]",id="059-04"),
    pytest.param(pd.array([True,False,None],dtype="boolean"),("1","0","NULL"),b"[true,false,null]",id="060-01"),
    pytest.param(pd.array(["ab",None],dtype="string[python]"),("'ab'","NULL"),b'["ab",null]',id="061-01"),
    pytest.param(pd.date_range("2026-11-12",periods=2).array,("'2026-11-12 00:00:00'","'2026-11-13 00:00:00'"),b'["2026-11-12T00:00:00","2026-11-13T00:00:00"]',id="064-01"),
    pytest.param(pd.array(["2026-11-12 08:09:10.123456789",None],dtype="datetime64[ns]"),("'2026-11-12 08:09:10.123456'","NULL"),b'["2026-11-12T08:09:10.123456789",null]',id="064-02"),
    pytest.param(pd.to_timedelta([1,2],unit="s").array,("'00:00:01'","'00:00:02'"),b'["PT1S","PT2S"]',id="065-01"),
    pytest.param(pd.to_timedelta([900,None],unit="ns").array,("'00:00:00'","NULL"),b'["PT0.0000009S",null]',id="065-02"),
    pytest.param(pd.to_timedelta([-1001],unit="ns").array,("'-00:00:00.000001'",),b'["-PT0.000001001S"]',id="065-03"),
    pytest.param(pd.arrays.NumpyExtensionArray(np.array([7,8])),("7","8"),b"[7,8]",id="066-01"),
    pytest.param(pd.arrays.NumpyExtensionArray(np.array([7,"ab",None],dtype=object)),("7","'ab'","NULL"),b'[7,"ab",null]',id="066-02"),
    pytest.param(pd.arrays.NumpyExtensionArray(np.array([None,None],dtype=object)),("NULL","NULL"),b"[null,null]",id="066-06"),
])
def test_pandas_table_exact_target_policy_pairs(value: object,sql: object,document: bytes) -> None:
    assert _sqlaero._escape_projection(value)==sql
    assert json_dumps(value)==document


@pytest.mark.parametrize("base",[pd.Timestamp,pd.Timedelta],ids=["047","048"])
def test_pandas_temporal_subclass_source_is_recorded_before_claiming_qualified_storage(base: Any,record_property: Callable[[str,object],None]) -> None:
    child=type("StoredTemporal",(base,),{})
    value=child("2026-11-12 08:09:10.123456789") if base is pd.Timestamp else child(-1,unit="ns")
    if type(value) is not child:
        record_property("verification_boundary","provider-construction-only")
        record_property("provider_versions","pandas="+pd.__version__)
        record_property("conditional_subclass_unavailable","normal provider subclass constructor returns "+type(value).__qualname__)
        return
    expected_sql="'2026-11-12 08:09:10.123456'" if base is pd.Timestamp else "'00:00:00'"
    expected_json=b'"2026-11-12T08:09:10.123456789"' if base is pd.Timestamp else b'"-PT0.000000001S"'
    assert _sqlaero._escape_projection(value)==expected_sql
    assert json_dumps(value)==expected_json


def array_case(kind: str) -> tuple[Any, object, bytes]:
    if kind=="integer":return pd.array([7,None],dtype="Int64"),("7","NULL"),b"[7,null]"
    if kind=="float":return pd.array([0.5,None],dtype="Float64"),("0.5e0","NULL"),b"[0.5,null]"
    if kind=="boolean":return pd.array([True,None,False],dtype="boolean"),("1","NULL","0"),b"[true,null,false]"
    if kind=="string":return pd.array(["ab",None],dtype="string[python]"),("'ab'","NULL"),b'["ab",null]'
    if kind=="categorical":return pd.Categorical(["ab",None]),("'ab'","NULL"),b'["ab",null]'
    if kind=="datetime":return pd.date_range("2026-11-12",periods=2).array,("'2026-11-12 00:00:00'","'2026-11-13 00:00:00'"),b'["2026-11-12T00:00:00","2026-11-13T00:00:00"]'
    if kind=="timedelta":return pd.to_timedelta([1,2],unit="s").array,("'00:00:01'","'00:00:02'"),b'["PT1S","PT2S"]'
    if kind=="numpy":return pd.arrays.NumpyExtensionArray(np.array([7,8])),("7","8"),b"[7,8]"
    pa=pytest.importorskip("pyarrow")
    if kind=="arrow-string":return pd.array(["ab",None],dtype="string[pyarrow]"),("'ab'","NULL"),b'["ab",null]'
    assert kind=="arrow"
    return pd.arrays.ArrowExtensionArray(pa.array([7,None],type=pa.int64())),("7","NULL"),b"[7,null]"


@pytest.mark.parametrize("kind",["integer","float","boolean","string","categorical","datetime","timedelta","numpy","arrow-string","arrow"])
def test_every_approved_array_family_real_subclass_keeps_actual_storage(kind: str) -> None:
    value,sql,document=array_case(kind)
    base=cast(type[Any],type(value))
    child=type("StoredArray",(base,),{"__slots__":()})
    value.__class__=child
    calls: list[str]=[]
    def forbidden(self: object,name: str) -> Any:
        calls.append(name)
        raise AssertionError("replacement array attribute must not execute")
    child.__getattribute__=forbidden
    assert _sqlaero._escape_projection(value)==sql
    assert json_dumps(value)==document
    assert calls==[]


@pytest.mark.parametrize("kind",["integer","float","boolean","string","categorical","datetime","timedelta","numpy","arrow-string","arrow"])
@pytest.mark.parametrize("missing",[False,True],ids=["empty","all-null"])
def test_every_approved_array_family_distinguishes_empty_from_actual_missing_positions(kind: str,missing: bool) -> None:
    value,_,_=array_case(kind)
    if missing and kind=="numpy":
        value=pd.arrays.NumpyExtensionArray(np.array([None,None],dtype=object))
    else:
        value=pd.array([None,None],dtype=value.dtype) if missing else value[:0]
    assert _sqlaero._escape_projection(value)==(("NULL","NULL") if missing else ())
    assert json_dumps(value,allow_nan=False)==(b"[null,null]" if missing else b"[]")
    assert _sqlaero._escape_projection(value,many=True)==(["NULL","NULL"] if missing else [()])


@pytest.mark.parametrize("dtype",["Int8","Int16","Int32","Int64","UInt8","UInt16","UInt32","UInt64"])
def test_nullable_integer_table_minimum_and_maximum_are_values_not_missing(dtype: str) -> None:
    native=np.dtype(dtype.lower())
    limits=np.iinfo(native)
    source=pd.array([int(limits.min),None,int(limits.max)],dtype=dtype)
    assert _sqlaero._escape_projection(source)==(str(limits.min),"NULL",str(limits.max))
    assert json_dumps(source)==("["+str(limits.min)+",null,"+str(limits.max)+"]").encode()


@pytest.mark.parametrize("kind",["decimal","zero","int","string-chunks","binary","struct"])
def test_pandas_arrow_storage_preserves_actual_target_value_and_shape(kind: str) -> None:
    pa=pytest.importorskip("pyarrow")
    if kind=="decimal":
        value=pd.arrays.ArrowExtensionArray(pa.array([Decimal("1.20")],type=pa.decimal128(5,2)))
        sql,document=("1.20",),b'["1.20"]'
    elif kind=="zero":
        value=pd.arrays.ArrowExtensionArray(pa.array([Decimal("0.00")],type=pa.decimal128(5,2)))
        sql,document=("0.00",),b'["0.00"]'
    elif kind=="int":
        value=pd.arrays.ArrowExtensionArray(pa.array([7,None],type=pa.int64()))
        sql,document=("7","NULL"),b"[7,null]"
    elif kind=="string-chunks":
        chunks=pa.chunked_array([pa.array(["ab"]),pa.array([],type=pa.string()),pa.array([None,"cd"],type=pa.string())])
        value=pd.arrays.ArrowStringArray(chunks)
        assert value._pa_array.num_chunks>=2
        sql,document=("'ab'","NULL","'cd'"),b'["ab",null,"cd"]'
    elif kind=="binary":
        value=pd.arrays.ArrowExtensionArray(pa.array([b"ab"],type=pa.binary()))
        assert _sqlaero._escape_projection(value)==("CAST(X'6162' AS BINARY)",)
        with pytest.raises(TypeError):json_dumps(value)
        with pytest.raises(TypeError):json_dumps(value[:0])
        return
    else:
        value=pd.arrays.ArrowExtensionArray(pa.array([{"id":7,"name":"ab"},{"id":8,"name":"cd"}]))
        assert _sqlaero._escape_projection(value,many=True)==[("7","'ab'"),("8","'cd'")]
        sql,document=("(7,'ab')","(8,'cd')"),b'[{"id":7,"name":"ab"},{"id":8,"name":"cd"}]'
    assert _sqlaero._escape_projection(value)==sql
    assert json_dumps(value)==document


@pytest.mark.parametrize("kind",["string","numpy"])
def test_supported_pandas_storage_never_repairs_surrogates_or_admits_unknown_payload(kind: str) -> None:
    source=pd.array(["a\ud800"],dtype="string[python]") if kind=="string" else pd.arrays.NumpyExtensionArray(np.array([object()],dtype=object))
    for convert in (_sqlaero._escape_projection,json_dumps):
        with pytest.raises(UnicodeEncodeError if kind=="string" else TypeError):convert(source)
