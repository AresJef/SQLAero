"""A Pandas wrapper preserves the native child's structure, not just its bytes."""

# pyright: reportPrivateUsage=false
import pytest
from typing import Any
from sqlaero import Cursor, DictCursor, SSCursor, SSDictCursor, _sqlaero, json_dumps
from sqlaero.errors import ProgrammingError
from test_public_connection import live, requires_mysql

pd=pytest.importorskip("pandas")
pa=pytest.importorskip("pyarrow")


def compound(kind: str, chunked: bool) -> Any:
    struct="struct" in kind
    dtype=pa.struct([("id",pa.int64()),("name",pa.string())]) if struct else pa.list_(pa.int64())
    values=[{"id":7,"name":"ab"},{"id":8,"name":"cd"}] if struct else [[7,8],[9,10]]
    if kind.startswith("dictionary"):
        chunks=[pa.DictionaryArray.from_arrays(pa.array([0],type=pa.int8()),pa.array([value],type=dtype)) for value in values]
        return pa.chunked_array(chunks) if chunked else pa.DictionaryArray.from_arrays(pa.array([0,1],type=pa.int8()),pa.array(values,type=dtype))
    array=pa.array(values,type=dtype)
    return pa.chunked_array([array[:0],array[:1],array[1:]]) if chunked else array


def wrap(raw: Any, wrapper: str) -> Any:
    value=pd.arrays.ArrowExtensionArray(raw)
    return value if wrapper=="array" else pd.Series(value) if wrapper=="series" else pd.Index(value)


@pytest.mark.parametrize("wrapper",["array","series","index"])
@pytest.mark.parametrize("kind",["list","struct","dictionary-list","dictionary-struct"])
@pytest.mark.parametrize("chunked",[False,True])
@pytest.mark.parametrize(("many","itemize"),[(False,True),(False,False),(True,True),(True,False)])
def test_pandas_native_compound_rows_follow_the_same_many_and_itemize_contract(wrapper: str,kind: str,chunked: bool,many: bool,itemize: bool) -> None:
    source=wrap(compound(kind,chunked),wrapper)
    first,second=(("7","'ab'"),("8","'cd'")) if "struct" in kind else (("7","8"),("9","10"))
    fragments=("("+",".join(first)+")","("+",".join(second)+")")
    expected: object=[first,second] if many and itemize else list(fragments) if many else fragments if itemize else "("+",".join(fragments)+")"
    assert _sqlaero._escape_projection(source,many=many,itemize=itemize)==expected
    assert json_dumps(source)==(b'[{"id":7,"name":"ab"},{"id":8,"name":"cd"}]' if "struct" in kind else b"[[7,8],[9,10]]")


@pytest.mark.parametrize("wrapper",["array","series","index"])
def test_selected_native_list_row_does_not_apply_many_again_to_its_children(wrapper: str) -> None:
    source=wrap(pa.array([[[7,8]],[[9,10]]]),wrapper)
    assert _sqlaero._escape_projection(source,many=True)==[("(7,8)",),("(9,10)",)]
    assert json_dumps(source)==b"[[[7,8]],[[9,10]]]"


@pytest.mark.parametrize("wrapper",["array","series","index"])
def test_null_parent_and_zero_fields_keep_distinct_actual_row_widths(wrapper: str) -> None:
    source=wrap(pa.array([{},None,{}],type=pa.struct([])),wrapper)
    assert _sqlaero._escape_projection(source,many=True)==[(),"NULL",()]
    assert json_dumps(source)==b"[{},null,{}]"


@pytest.mark.parametrize("many",[False,True])
def test_a_struct_cell_inside_a_dataframe_stays_one_column(many: bool) -> None:
    source=pd.DataFrame({"record":pd.arrays.ArrowExtensionArray(compound("struct",False))})
    assert _sqlaero._escape_projection(source,many=many)==[("(7,'ab')",),("(8,'cd')",)]
    assert json_dumps(source)==b'[{"record":{"id":7,"name":"ab"}},{"record":{"id":8,"name":"cd"}}]'


@requires_mysql
@pytest.mark.parametrize("label",["84","97"])
@pytest.mark.parametrize("cursor",[Cursor,DictCursor,SSCursor,SSDictCursor])
@pytest.mark.parametrize("prepared",[False,True])
@pytest.mark.parametrize("wrapper",["array","series","index"])
@pytest.mark.parametrize("kind",["list","struct"])
def test_native_compound_pandas_rows_reach_both_protocols_as_actual_fields(label: str,cursor: type[Cursor] | type[DictCursor] | type[SSCursor] | type[SSDictCursor],prepared: bool,wrapper: str,kind: str) -> None:
    source=wrap(compound(kind,True),wrapper)
    with live(label) as connection:
        with connection.cursor() as setup:
            setup.execute("CREATE TEMPORARY TABLE pandas_native_fields (a BIGINT,b VARCHAR(16))")
        with connection.cursor(cursor,prepared=prepared) as current:
            assert current.execute("INSERT INTO pandas_native_fields VALUES (%s,%s)",source,many=True)==2
        with connection.cursor() as check:
            check.execute("SELECT a,b FROM pandas_native_fields ORDER BY a")
            assert check.fetchall()==(((7,"ab"),(8,"cd")) if kind=="struct" else ((7,"8"),(9,"10")))


@requires_mysql
@pytest.mark.parametrize("label",["84","97"])
@pytest.mark.parametrize("cursor",[Cursor,DictCursor,SSCursor,SSDictCursor])
@pytest.mark.parametrize("prepared",[False,True])
@pytest.mark.parametrize("wrapper",["array","series","index"])
def test_later_native_row_width_failure_preserves_only_the_actually_confirmed_prefix(label: str,cursor: type[Cursor] | type[DictCursor] | type[SSCursor] | type[SSDictCursor],prepared: bool,wrapper: str) -> None:
    source=wrap(pa.array([[7,8],None,[]],type=pa.list_(pa.int64())),wrapper)
    with live(label) as connection:
        with connection.cursor() as setup:
            setup.execute("CREATE TEMPORARY TABLE pandas_native_prefix (a BIGINT,b BIGINT)")
        with connection.cursor(cursor,prepared=prepared) as current:
            with pytest.raises(ProgrammingError) as caught:
                current.execute("INSERT INTO pandas_native_prefix SELECT %s,%s",source,many=True)
            assert caught.value.is_server_error is False
            assert caught.value.code==65542
            assert "args[1]" in str(caught.value) and "not sent" in str(caught.value)
            assert current.row_count==1 and current.batch_row_count is None
            notes="\n".join(caught.value.__notes__)
            assert "confirmed " in notes and "source rows 0..1" in notes
            with pytest.raises(ProgrammingError):current.nextset()
        with connection.cursor() as check:
            check.execute("SELECT a,b FROM pandas_native_prefix")
            assert check.fetchall()==((7,8),)
            check.execute("SELECT 42")
            assert check.fetchone()==(42,)
