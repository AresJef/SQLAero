"""Root many selects rows once; each selected value keeps its own structure."""

from collections.abc import Callable

import pytest
from sqlaero import Cursor, DictCursor, SSCursor, SSDictCursor
from sqlaero import _sqlaero, json_dumps
from sqlaero.errors import SQLAeroError
from test_public_connection import live, requires_mysql

np = pytest.importorskip("numpy")


def one_record(kind: str) -> object:
    array = np.array([(7, "ab")], dtype=[("id", "i8"), ("name", "U2")])
    if kind == "void":
        return array[0]
    if kind == "record":
        return array.view(np.recarray)[0]
    if kind == "masked":
        return np.ma.array(array.reshape(()), mask=False)
    return array.reshape(())


@pytest.mark.parametrize("kind", ["void", "record", "zero", "masked"])
@pytest.mark.parametrize(("many", "itemize", "expected"), [
    (False, True, ("7", "'ab'")),
    (False, False, "(7,'ab')"),
    (True, True, ["7", "'ab'"]),
    (True, False, ["7", "'ab'"]),
])
def test_root_record_fields_use_the_same_outer_many_rule(
    kind: str, many: bool, itemize: bool, expected: object,
) -> None:
    value = one_record(kind)
    assert _sqlaero._escape_projection(value, many=many, itemize=itemize) == expected
    assert json_dumps(value) == b'{"id":7,"name":"ab"}'


def pairs(kind: str) -> object:
    if kind in ("record", "masked_record"):
        value = np.array([(7, 8), (9, 10)], dtype=[("a", "i8"), ("b", "i8")])
    elif kind in ("object", "masked_object"):
        value = np.empty(2, dtype=object)
        value[:] = [[7, 8], [9, 10]]
    else:
        raise AssertionError(kind)
    return np.ma.array(value, mask=False) if kind.startswith("masked") else value


@pytest.mark.parametrize("kind", ["record", "masked_record", "object", "masked_object"])
@pytest.mark.parametrize(("many", "itemize", "expected"), [
    (False, True, ("(7,8)", "(9,10)")),
    (False, False, "((7,8),(9,10))"),
    (True, True, [("7", "8"), ("9", "10")]),
    (True, False, ["(7,8)", "(9,10)"]),
])
def test_record_and_object_series_select_one_parameter_row_per_element(
    kind: str, many: bool, itemize: bool, expected: object,
) -> None:
    assert _sqlaero._escape_projection(pairs(kind), many=many, itemize=itemize) == expected


@pytest.mark.parametrize("factory", [
    lambda: np.empty(0, dtype="i8"),
    lambda: np.empty(0, dtype=object),
    lambda: np.zeros((), dtype=[]),
])
@pytest.mark.parametrize(("many", "itemize", "expected"), [
    (False, True, ()), (False, False, "()"),
    (True, True, [()]), (True, False, [()]),
])
def test_empty_root_sequence_has_one_zero_parameter_batch_row_only_with_many(
    factory: Callable[[], object], many: bool, itemize: bool, expected: object,
) -> None:
    assert _sqlaero._escape_projection(factory(), many=many, itemize=itemize) == expected


@pytest.mark.parametrize("many", [False, True])
@pytest.mark.parametrize(("itemize", "expected"), [(True, [(), ()]), (False, ["()", "()"])])
def test_actual_zero_width_table_rows_are_not_an_empty_root_batch(
    many: bool, itemize: bool, expected: object,
) -> None:
    assert _sqlaero._escape_projection(np.empty((2, 0), dtype="i8"), many=many, itemize=itemize) == expected


@pytest.mark.parametrize("itemize", [False, True])
def test_one_empty_record_is_one_real_row(itemize: bool) -> None:
    assert _sqlaero._escape_projection(np.empty(1, dtype=[]), many=True, itemize=itemize) == ([()] if itemize else ["()"])


@pytest.mark.parametrize("many", [False, True])
@pytest.mark.parametrize("itemize", [False, True])
def test_zero_dimensional_object_never_replans_its_container(many: bool, itemize: bool) -> None:
    value = np.empty((), dtype=object)
    value[()] = [7, 8]
    assert _sqlaero._escape_projection(value, many=many, itemize=itemize) == "(7,8)"


@pytest.mark.parametrize("itemize", [False, True])
def test_many_does_not_run_again_inside_a_selected_record_field(itemize: bool) -> None:
    value = np.zeros((), dtype=[("pair", "i8", (2,)), ("nested", [("x", "i8"), ("y", "i8")])])
    value["pair"] = [7, 8]
    value["nested"] = (9, 10)
    expected: object = [("7", "8"), ("9", "10")] if itemize else ["(7,8)", "(9,10)"]
    assert _sqlaero._escape_projection(value, many=True, itemize=itemize) == expected


@pytest.mark.parametrize("kind", ["object", "masked_object"])
def test_object_rows_use_actual_dict_values_and_detect_real_cycles(kind: str) -> None:
    value = np.empty(1, dtype=object)
    value[0] = {"a": 7, "b": 8}
    if kind.startswith("masked"):
        value = np.ma.array(value, mask=False)
    assert _sqlaero._escape_projection(value, many=True) == [("7", "8")]
    value[0] = value
    with pytest.raises(SQLAeroError, match="cycle"):
        _sqlaero._escape_projection(value, many=True)


@pytest.mark.parametrize("kind", ["record", "masked_record", "object", "masked_object"])
def test_prepared_uses_selected_actual_fields_not_one_composite_value(kind: str) -> None:
    assert _sqlaero._encode_native_rows(pairs(kind), many=True) == [b"78", b"910"]
    assert _sqlaero._bind_sql("SELECT %s,%s", pairs(kind), many=True) == [b"\x03SELECT 7,8", b"\x03SELECT 9,10"]


@pytest.mark.parametrize("itemize", [False, True])
def test_two_axes_inside_one_selected_row_do_not_start_a_second_batch(itemize: bool) -> None:
    value = np.zeros((), dtype=[("grid", "i8", (2, 2))])
    value["grid"] = [[7, 8], [9, 10]]
    expected: object = [("(7,8)", "(9,10)")] if itemize else ["((7,8),(9,10))"]
    assert _sqlaero._escape_projection(value, many=True, itemize=itemize) == expected


@pytest.mark.parametrize("structured", [False, True])
@pytest.mark.parametrize("itemize", [False, True])
def test_selected_rows_count_actual_native_and_python_container_depth(
    structured: bool, itemize: bool,
) -> None:
    allowed = 62 if structured else 63
    value = np.empty(1, dtype=[("value", object)] if structured else object)
    payload: object = 7
    for _ in range(allowed):
        payload = [payload]
    if structured:
        value["value"][0] = payload
    else:
        value[0] = payload
    _sqlaero._escape_projection(value, many=True, itemize=itemize)
    if structured:
        value["value"][0] = [payload]
    else:
        value[0] = [payload]
    with pytest.raises(SQLAeroError, match="64 active container levels") as caught:
        _sqlaero._escape_projection(value, many=True, itemize=itemize)
    assert caught.value.code == 65543


@pytest.mark.parametrize("kind", ["record", "masked_record", "object", "masked_object"])
def test_concurrent_stable_sources_keep_independent_selected_row_owners(kind: str) -> None:
    from concurrent.futures import ThreadPoolExecutor

    value = pairs(kind)

    def read(_index: int) -> list[bytes]:
        return _sqlaero._encode_native_rows(value, many=True)

    with ThreadPoolExecutor(max_workers=4) as executor:
        assert list(executor.map(read, range(64))) == [[b"78", b"910"]] * 64


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("cursor", [Cursor, DictCursor, SSCursor, SSDictCursor])
@pytest.mark.parametrize("prepared", [False, True])
def test_actual_root_rows_and_zero_width_rows_execute_their_exact_work(
    label: str, cursor: type, prepared: bool,
) -> None:
    with live(label) as connection:
        with connection.cursor(cursor, prepared=prepared) as current:
            current.execute("CREATE TEMPORARY TABLE selected_rows (a BIGINT, b BIGINT)")
            for kind in ("record", "masked_record", "object", "masked_object"):
                assert current.execute("INSERT INTO selected_rows VALUES (%s,%s)", pairs(kind), many=True) == 2
            current.execute("CREATE TEMPORARY TABLE selected_defaults (id INT AUTO_INCREMENT PRIMARY KEY, a INT DEFAULT 7)")
            assert current.execute("INSERT INTO selected_defaults VALUES ()", np.empty(0, dtype="i8"), many=True) == 1
            assert current.execute("INSERT INTO selected_defaults VALUES ()", np.empty((2, 0), dtype="i8")) == 2
        with connection.cursor() as check:
            check.execute("SELECT a,b FROM selected_rows ORDER BY a,b")
            assert check.fetchall() == ((7, 8),) * 4 + ((9, 10),) * 4
            check.execute("SELECT a FROM selected_defaults ORDER BY id")
            assert check.fetchall() == ((7,),) * 3
