"""Dictionary rows reuse result keys without changing native read ownership."""

from typing import SupportsFloat, TypeAlias, cast

import pytest
import sqlaero
from sqlaero.errors import DataError

from test_public_connection import connection as connection, requires_mysql


CursorClass: TypeAlias = type[sqlaero.Cursor] | type[sqlaero.DictCursor]
CursorFactory: TypeAlias = None | type[tuple[object, ...]] | type[dict[str, object]] | CursorClass


@requires_mysql
def test_builtin_format_selection_and_no_user_constructor(connection: sqlaero.Connection) -> None:
    dict_cursor: type[sqlaero.DictCursor] = getattr(sqlaero, "DictCursor")
    selectors: tuple[tuple[CursorFactory, CursorClass], ...] = (
        (None, sqlaero.Cursor), (tuple, sqlaero.Cursor),
        (sqlaero.Cursor, sqlaero.Cursor), (dict, dict_cursor), (dict_cursor, dict_cursor),
    )
    for selector, expected in selectors:
        with connection.cursor(selector) as cur:
            assert type(cur) is expected
            assert cur.execute("SELECT 1 AS value") == 1
            assert cur.fetchone() == ({"value": 1} if expected is dict_cursor else (1,))
    invalid_selectors: tuple[object, ...] = ({}, (), "dict", object, lambda: None)
    for selector in invalid_selectors:
        with pytest.raises(TypeError, match="cursor"):
            # These deliberately invalid selectors must reach runtime validation.
            connection.cursor(cast(CursorFactory, selector))


@requires_mysql
def test_dictionary_keys_are_shared_but_each_final_row_is_independent(connection: sqlaero.Connection) -> None:
    with connection.cursor(dict) as cur:
        cur.execute("SELECT 10 AS id, 20 AS id UNION ALL SELECT 11, 21")
        assert cur.columns.names == ("id", "id")
        keys = cur.columns.keys
        assert keys == ("id.1", "id.2")
        rows = cur.fetchall()
        assert type(rows) is tuple
        assert rows == ({"id.1": 10, "id.2": 20}, {"id.1": 11, "id.2": 21})
        assert rows[0] is not rows[1]
        assert all(key is keys[index] for row in rows for index, key in enumerate(row))
        rows[0]["id.1"] = -1
        rows[0]["extra"] = "owned"
        assert rows[1] == {"id.1": 11, "id.2": 21}
        assert cur.fetchall() == ()
        assert cur.fetchone() is None
        cur.scroll(0, mode="absolute")
        assert next(cur) == {"id.1": 10, "id.2": 20}
        assert next(cur) == {"id.1": 11, "id.2": 21}
        with pytest.raises(StopIteration):
            next(cur)


@requires_mysql
def test_dictionary_transaction_and_multiple_results(connection: sqlaero.Connection) -> None:
    with connection.transaction(dict) as cur:
        assert type(cur) is getattr(sqlaero, "DictCursor")
        cur.execute("SELECT 1 AS first; SELECT 2 AS second")
        first_columns = cur.columns
        assert cur.fetchall() == ({"first": 1},)
        assert cur.nextset() is True
        assert cur.fetchone() == {"second": 2}
        assert first_columns.keys == ("first",)
    assert cur.closed is True
    assert connection.closed is False


@requires_mysql
def test_dictionary_wide_rows_and_empty_results(connection: sqlaero.Connection) -> None:
    with connection.cursor(dict) as cur:
        cur.execute("SELECT 1 AS a, NULL AS b, 'x' AS c, 1.5 AS d, X'00FF' AS e")
        row = cast(dict[str, object], cur.fetchone())
        assert row["a"] == 1 and row["b"] is None and row["c"] == "x"
        assert float(cast(SupportsFloat, row["d"])) == 1.5 and row["e"] == b"\x00\xff"
        cur.execute("SELECT 1 AS missing WHERE FALSE")
        assert cur.fetchmany(0) == ()
        assert cur.fetchone() is None
        assert cur.columns.keys == ("missing",)


@requires_mysql
def test_dictionary_decode_failure_keeps_atomic_position(connection: sqlaero.Connection) -> None:
    with connection.cursor(dict) as cur:
        cur.execute("SET SESSION sql_mode=''")
        cur.execute("SELECT CAST('0000-00-00' AS DATE) AS value")
        for _ in range(2):
            with pytest.raises(DataError):
                cur.fetchone()
        cur.scroll(0, mode="absolute")
        assert cur.row_count == 1
        assert cur.columns.keys == ("value",)
