"""Scalar and axis facts preserve root layout independently of target policy."""

import pytest
from sqlaero import _sqlaero, json_dumps
from sqlaero.errors import SQLAeroError
from sqlaero import Cursor, DictCursor, SSCursor, SSDictCursor
from test_public_connection import live, requires_mysql

np = pytest.importorskip("numpy")


@pytest.mark.parametrize("many", [False, True])
@pytest.mark.parametrize("itemize", [False, True])
@pytest.mark.parametrize(("value", "sql", "json"), [
    (np.array(7, dtype="i8"), "7", b"7"),
    (np.array(0.1, dtype="f4"), "0.10000000149011612e0", b"0.10000000149011612"),
    (np.array(True), "1", b"true"),
    (np.array("ab", dtype="U2"), "'ab'", b'"ab"'),
    (np.array(None, dtype=object), "NULL", b"null"),
    (np.array("2026-11-12T08:00:00.123456789", dtype="M8[ns]"),
     "'2026-11-12 08:00:00.123456'", b'"2026-11-12T08:00:00.123456789"'),
])
def test_scalar_zero_dimensional_values_are_one_parameter_and_one_document(
    value: object, sql: str, json: bytes, many: bool, itemize: bool,
) -> None:
    assert _sqlaero._escape_projection(value, many=many, itemize=itemize) == sql
    assert _sqlaero._bind_sql("SELECT %s", value, many=many, itemize=itemize) == [b"\x03SELECT " + sql.encode()]
    assert json_dumps(value) == json


@pytest.mark.parametrize("many", [False, True])
@pytest.mark.parametrize("itemize", [False, True])
def test_zero_dimensional_object_does_not_replan_a_root_container(many: bool, itemize: bool) -> None:
    value = np.empty((), dtype=object)
    value[()] = [7, 8]
    assert _sqlaero._escape_projection(value, many=many, itemize=itemize) == "(7,8)"
    assert _sqlaero._bind_sql("SELECT %s", value, many=many, itemize=itemize) == [b"\x03SELECT (7,8)"]
    assert json_dumps(value) == b"[7,8]"
    with pytest.raises(SQLAeroError) as caught:
        _sqlaero._bind_sql("SELECT %s,%s", value, many=many, itemize=itemize)
    assert getattr(caught.value, "code", None) == 65542


def test_zero_dimensional_object_identity_is_transparent_to_actual_output_depth() -> None:
    source = np.empty((), dtype=object)
    value: object = 7
    for _ in range(64):
        value = [value]
    source[()] = value
    assert json_dumps(source) == b"[" * 64 + b"7" + b"]" * 64
    assert _sqlaero._encode_value(source).to_bytes() == b"(" * 64 + b"7" + b")" * 64
    source[()] = source
    with pytest.raises(ValueError, match="cycle"):
        json_dumps(source)
    with pytest.raises(SQLAeroError, match="cycle"):
        _sqlaero._encode_value(source)


@pytest.mark.parametrize("many", [False, True])
@pytest.mark.parametrize("itemize", [False, True])
def test_zero_dimensional_prepared_input_preserves_one_position(many: bool, itemize: bool) -> None:
    assert _sqlaero._encode_native_rows(np.array(7), many=many, itemize=itemize) == [b"7"]
    source = np.empty((), dtype=object)
    source[()] = [7, 8]
    with pytest.raises(SQLAeroError, match="scalar"):
        _sqlaero._encode_native_rows(source, many=many, itemize=itemize)


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("cursor", [Cursor, DictCursor, SSCursor, SSDictCursor])
@pytest.mark.parametrize("prepared", [False, True])
def test_actual_zero_dimensional_inputs_write_exactly_one_row_each_time(label: str, cursor, prepared: bool) -> None:
    with live(label) as connection:
        with connection.cursor(cursor, prepared=prepared) as current:
            current.execute("CREATE TEMPORARY TABLE scalar_zero_dim (id INT AUTO_INCREMENT PRIMARY KEY, value BIGINT)")
            for many in (False, True):
                for itemize in (False, True):
                    current.execute("INSERT INTO scalar_zero_dim(value) VALUES (%s)", np.array(7), many=many, itemize=itemize)
                    current.execute("INSERT INTO scalar_zero_dim(value) VALUES (%s)", np.array(None, dtype=object), many=many, itemize=itemize)
        with connection.cursor() as check:
            check.execute("SELECT value FROM scalar_zero_dim ORDER BY id")
            assert check.fetchall() == ((7,), (None,)) * 4
