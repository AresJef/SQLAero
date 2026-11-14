"""Explicit array layout and failed-prefix examples from the approved table."""

# pyright: reportPrivateUsage=false
from typing import Any


import pathlib

import pytest

from sqlaero import Cursor, DictCursor, SSCursor, SSDictCursor, _sqlaero, json_dumps
from sqlaero.errors import DataError
from test_public_connection import live, requires_mysql

np = pytest.importorskip("numpy")

def box(value: Any):
    source = np.empty((), dtype=object)
    source[()] = value
    return source


@pytest.mark.parametrize("many",[False,True])
@pytest.mark.parametrize("itemize",[False,True])
@pytest.mark.parametrize("kind",["empty","mapping","record"])
def test_zero_dimensional_object_keeps_its_one_value_boundary_for_every_payload(kind: str,many: bool,itemize: bool) -> None:
    record=np.array([(7,"ab")],dtype=[("id","i8"),("name","U2")])[0]
    cases: dict[str, tuple[object, str, bytes]] = {
        "empty":([],"()",b"[]"),
        "mapping":({"id":7,"name":"ab"},"(7,'ab')",b'{"id":7,"name":"ab"}'),
        "record":(record,"(7,'ab')",b'{"id":7,"name":"ab"}'),
    }
    value,sql,document=cases[kind]
    source=box(value)
    assert _sqlaero._escape_projection(source,many=many,itemize=itemize)==sql
    assert json_dumps(source)==document


@pytest.mark.parametrize(("many","itemize","expected"),[
    (False,True,("()","()")),(False,False,"((),())"),
    (True,True,[(),()]),(True,False,["()","()"]),
])
def test_two_zero_field_records_preserve_two_real_elements(many: bool,itemize: bool,expected: Any) -> None:
    source=np.empty(2,dtype=[])
    assert _sqlaero._escape_projection(source,many=many,itemize=itemize)==expected
    assert json_dumps(source)==b"[{},{}]"


@pytest.mark.parametrize("kind",["record-series","recarray","masked-record","masked"])
@pytest.mark.parametrize(("many","itemize","expected"),[
    (False,True,()),(False,False,"()"),(True,True,[()]),(True,False,[()]),
])
def test_empty_native_sequence_keeps_one_zero_parameter_call(kind: str,many: bool,itemize: bool,expected: object) -> None:
    source=np.empty(0,dtype=[("id","i8"),("name","U2")])
    if kind=="recarray":source=source.view(np.recarray)
    elif kind=="masked-record":source=np.ma.array(source,mask=False)
    elif kind=="masked":source=np.ma.array([],dtype="i8")
    assert _sqlaero._escape_projection(source,many=many,itemize=itemize)==expected
    assert json_dumps(source)==b"[]"


def test_unstructured_void_is_never_a_record_or_implicit_binary_value() -> None:
    source=np.void(b"ab")
    for convert in (_sqlaero._escape_projection,json_dumps):
        with pytest.raises(TypeError):convert(source)


@pytest.mark.parametrize(("many","itemize","expected"),[
    (False,True,("(NULL,NULL)",)),(False,False,"((NULL,NULL))"),
    (True,True,[("NULL","NULL")]),(True,False,["(NULL,NULL)"]),
])
def test_one_fully_masked_record_keeps_its_outer_axis_and_field_positions(many: bool,itemize: bool,expected: object) -> None:
    source=np.ma.array(np.array([(7,"ab")],dtype=[("id","i8"),("name","U2")]),mask=True)
    assert _sqlaero._escape_projection(source,many=many,itemize=itemize)==expected
    assert json_dumps(source)==b'[{"id":null,"name":null}]'


@pytest.mark.parametrize("itemize",[False,True])
def test_masked_zero_dimensional_record_many_preserves_the_two_missing_fields(itemize: bool) -> None:
    source=np.ma.array(np.array((7,"ab"),dtype=[("id","i8"),("name","U2")]),mask=True)
    assert _sqlaero._escape_projection(source,many=True,itemize=itemize)==["NULL","NULL"]
    assert json_dumps(source,allow_nan=False)==b'{"id":null,"name":null}'


@pytest.mark.parametrize(("many","itemize"),[(False,True),(False,False),(True,True),(True,False)])
def test_record_array_names_change_only_json_keys_not_root_sql_positions(many: bool,itemize: bool) -> None:
    first=np.array([(7,"ab"),(8,"cd")],dtype=[("id","i8"),("name","U2")]).view(np.recarray)
    renamed=np.array([(7,"ab"),(8,"cd")],dtype=[("code","i8"),("text","U2")]).view(np.recarray)
    expected={
        (False,True):("(7,'ab')","(8,'cd')"),
        (False,False):"((7,'ab'),(8,'cd'))",
        (True,True):[("7","'ab'"),("8","'cd'")],
        (True,False):["(7,'ab')","(8,'cd')"],
    }[many,itemize]
    for source in (first,renamed):
        assert _sqlaero._escape_projection(source,many=many,itemize=itemize)==expected
    assert json_dumps(first)==b'[{"id":7,"name":"ab"},{"id":8,"name":"cd"}]'
    assert json_dumps(renamed)==b'[{"code":7,"text":"ab"},{"code":8,"text":"cd"}]'


@pytest.mark.parametrize(("source", "sql", "document"), [
    pytest.param(np.array([7,8],dtype="i8"), ("7","8"), b"[7,8]", id="041-01"),
    pytest.param(np.array([[7,8],[9,10]]), [("7","8"),("9","10")], b"[[7,8],[9,10]]", id="041-02"),
    pytest.param(np.array(7), "7", b"7", id="041-03"),
    pytest.param(np.array(2.5), "2.5e0", b"2.5", id="041-04"),
    pytest.param(np.array("ab"), "'ab'", b'"ab"', id="041-05"),
    pytest.param(np.array(None,dtype=object), "NULL", b"null", id="041-06"),
    pytest.param(box([7,8]), "(7,8)", b"[7,8]", id="041-07"),
    pytest.param(box({"id":7,"name":"ab"}), "(7,'ab')", b'{"id":7,"name":"ab"}', id="041-08"),
    pytest.param(np.array([],dtype="i8"), (), b"[]", id="041-10"),
    pytest.param(np.empty((0,2),dtype="i8"), [()], b"[]", id="041-11"),
    pytest.param(np.empty((2,0),dtype="i8"), [(),()], b"[[],[]]", id="041-12"),
    pytest.param(np.array([7,"ab"],dtype=object), ("7","'ab'"), b'[7,"ab"]', id="041-13"),
    pytest.param(np.array([{"id":7}],dtype=object), ("(7)",), b'[{"id":7}]', id="041-14"),
    pytest.param(np.array(["a","中"],dtype="U1"), ("'a'","'中'"), '["a","中"]'.encode(), id="041-15"),
    pytest.param(np.array([7,8])[::-1], ("8","7"), b"[8,7]", id="041-18"),
    pytest.param(np.array([7,8],dtype=">i8"), ("7","8"), b"[7,8]", id="041-19"),
    pytest.param(np.array([np.timedelta64(-900,"ns"),np.datetime64(-1,"ns")],dtype=object),
                 ("'00:00:00'","'1969-12-31 23:59:59.999999'"),
                 b'["-PT0.0000009S","1969-12-31T23:59:59.999999999"]', id="041-31"),
])
def test_numpy_array_table_exact_root_target_exits(source: Any, sql: Any, document: bytes) -> None:
    assert _sqlaero._escape_projection(source) == sql
    assert json_dumps(source) == document


@pytest.mark.parametrize("source", [np.array([[[7]]]),np.ma.array([[[7]]],mask=False)], ids=["041-09","114-13"])
def test_numpy_array_table_higher_axes_are_json_only(source: Any) -> None:
    assert json_dumps(source) == b"[[[7]]]"
    with pytest.raises(ValueError):
        _sqlaero._escape_projection(source)


@pytest.mark.parametrize("empty", [False,True], ids=["041-16","041-17"])
def test_numpy_array_table_binary_schema_is_checked_even_without_rows(empty: bool) -> None:
    source = np.array([],dtype="S2") if empty else np.array([b"ab"],dtype="S2")
    assert _sqlaero._escape_projection(source) == (() if empty else ("CAST(X'6162' AS BINARY)",))
    with pytest.raises(TypeError):
        json_dumps(source)


@pytest.mark.parametrize(("shape","values","sql","document"), [
    ((2,),[7,8],("7","8"),b"[7,8]"),
    ((2,2),[[7,8],[9,10]],[("7","8"),("9","10")],b"[[7,8],[9,10]]"),
    ((),7,"7",b"7"),
    ((1,1,1),[[[7]]],None,b"[[[7]]]"),
], ids=["042-01","042-03","042-04","042-05"])
def test_memmap_table_actual_axes_share_the_same_target_policy(tmp_path: pathlib.Path,shape: tuple[int, ...],values: Any,sql: Any,document: bytes) -> None:
    source = np.memmap(tmp_path/"values.bin",dtype="i8",mode="w+",shape=shape)
    source[...] = values
    source.flush()
    assert json_dumps(source) == document
    if sql is None:
        with pytest.raises(ValueError):
            _sqlaero._escape_projection(source)
    else:
        assert _sqlaero._escape_projection(source) == sql
    if shape == (2,):
        assert _sqlaero._escape_projection(source[::-1]) == ("8","7")
        assert json_dumps(source[::-1]) == b"[8,7]"


@pytest.mark.parametrize(("dtype","values","sql","document"), [
    ("f8",[float("nan"),-0.0],("NULL","-0.0e0"),b"[null,-0.0]"),
    ("M8[ns]",[-1,-(1<<63)],("'1969-12-31 23:59:59.999999'","NULL"),
     b'["1969-12-31T23:59:59.999999999",null]'),
])
def test_memmap_table_does_not_change_nonfinite_or_temporal_leaf_policy(tmp_path: pathlib.Path,dtype: Any,values: Any,sql: Any,document: bytes) -> None:
    source = np.memmap(tmp_path/"typed.bin",dtype=dtype,mode="w+",shape=(2,))
    source[:] = values
    assert _sqlaero._escape_projection(source) == sql
    assert json_dumps(source) == document


@pytest.mark.parametrize("kind",["M8","m8"])
@pytest.mark.parametrize("unit",["ps","fs","as","generic","0ns"])
def test_temporal_array_table_missing_precedes_but_never_grants_finite_unit_admission(kind: str,unit: str) -> None:
    dtype=np.dtype(kind if unit=="generic" else f"{kind}[{unit}]")
    missing=np.array([-(1<<63)],dtype="i8").view(dtype)
    assert _sqlaero._escape_projection(missing)==("NULL",)
    assert _sqlaero._escape_projection(missing,many=True)==["NULL"]
    assert json_dumps(missing,allow_nan=False)==b"[null]"
    finite=np.array([1],dtype="i8").view(dtype)
    with pytest.raises((DataError,TypeError,ValueError)):
        _sqlaero._escape_projection(finite)
    with pytest.raises((TypeError,ValueError)):
        json_dumps(finite)


@requires_mysql
@pytest.mark.parametrize("label",["84","97"])
@pytest.mark.parametrize("cursor",[Cursor,DictCursor,SSCursor,SSDictCursor])
@pytest.mark.parametrize("prepared",[False,True])
@pytest.mark.parametrize("kind",["M8","m8"])
def test_late_invalid_temporal_multiplier_preserves_the_actual_confirmed_request(label: str,cursor: type[Cursor] | type[DictCursor] | type[SSCursor] | type[SSDictCursor],prepared: bool,kind: str) -> None:
    source=np.array([-(1<<63),1],dtype="i8").view(np.dtype(kind+"[0ns]"))
    with pytest.raises(TypeError,match="multiplier") as local:
        json_dumps(source)
    assert "[1]" in "\n".join(local.value.__notes__)
    with live(label) as connection:
        with connection.cursor() as setup:
            setup.execute("CREATE TEMPORARY TABLE nat_boundary (value VARCHAR(64) NULL)")
        with connection.cursor(cursor,prepared=prepared) as current:
            # SELECT form deliberately prevents VALUES fusion: the first NULL
            # request is confirmed before the second source row is inspected.
            with pytest.raises(TypeError,match="multiplier") as caught:
                current.execute("INSERT INTO nat_boundary SELECT %s",source,many=True)
            assert current.row_count==current.rowcount==1
            assert current.batch_row_count is None
            notes="\n".join(caught.value.__notes__)
            assert "confirmed " in notes and "not sent" in notes
            with pytest.raises(type(caught.value),match="multiplier"):
                current.nextset()
        with connection.cursor() as observer:
            observer.execute("SELECT value FROM nat_boundary")
            assert observer.fetchall()==((None,),)
            observer.execute("SELECT 42")
            assert observer.fetchone()==(42,)
