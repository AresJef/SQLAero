"""Verify supported aggregate and window function bodies on both MySQL servers."""

from datetime import datetime
from inspect import signature

import pytest
from sqlaero import Column, Connection, CustomEscape, sqlfunc
from sqlaero.typing import EscapeInput

from test_public_connection import connection as connection, requires_mysql


@requires_mysql
def test_aggregate_functions_match_native(connection: Connection) -> None:
    cases: tuple[tuple[EscapeInput, str], ...] = (
        (sqlfunc.ANY_VALUE(Column("x")), "ANY_VALUE(x)"),
        (sqlfunc.AVG(Column("x"), distinct=True), "AVG(DISTINCT x)"),
        (sqlfunc.BIT_AND(Column("x")), "BIT_AND(x)"),
        (sqlfunc.BIT_OR(Column("x")), "BIT_OR(x)"),
        (sqlfunc.BIT_XOR(Column("x")), "BIT_XOR(x)"),
        (sqlfunc.COUNT(), "COUNT(*)"),
        (sqlfunc.GROUP_CONCAT(Column("x"), order_by=[(Column("id"), "ASC")]), "GROUP_CONCAT(x ORDER BY id)"),
        (sqlfunc.GROUPING(Column("x")), "GROUPING(x)"),
        (sqlfunc.JSON_ARRAYAGG(Column("id")), "JSON_ARRAYAGG(id)"),
        (sqlfunc.JSON_OBJECTAGG(Column("label"), Column("x")), "JSON_OBJECTAGG(label,x)"),
        (sqlfunc.MAX(Column("x"), distinct=True), "MAX(DISTINCT x)"),
        (sqlfunc.MIN(Column("x"), distinct=True), "MIN(DISTINCT x)"),
        (sqlfunc.STDDEV_POP(Column("x")), "STDDEV_POP(x)"),
        (sqlfunc.STDDEV_SAMP(Column("x")), "STDDEV_SAMP(x)"),
        (sqlfunc.SUM(Column("x"), distinct=True), "SUM(DISTINCT x)"),
        (sqlfunc.VAR_POP(Column("x")), "VAR_POP(x)"),
        (sqlfunc.VAR_SAMP(Column("x")), "VAR_SAMP(x)"),
    )
    assert len({type(expr).__name__ for expr, _ in cases}) == 17
    with connection.cursor() as cursor:
        cursor.execute("CREATE TEMPORARY TABLE sqlaero_agg (id INT, x INT, label CHAR(1))")
        cursor.execute("INSERT INTO sqlaero_agg VALUES (1,2,'a'), (2,2,'b'), (3,5,'c')")
        for expr, native in cases:
            name = type(expr).__name__
            assert type(expr).__base__ is sqlfunc.ABS.__base__
            assert name in sqlfunc.__all__ and type(expr).__doc__
            assert str(connection.escape(expr)).startswith(f"{name}(")
            suffix = " GROUP BY x WITH ROLLUP" if name == "GROUPING" else ""
            cursor.execute(f"SELECT %s,{native} FROM sqlaero_agg{suffix}", expr)
            rows = cursor.fetchall()
            assert rows and all(row[0] == row[1] for row in rows)


@requires_mysql
def test_count_group_concat_and_column_order(connection: Connection) -> None:
    with connection.cursor() as cursor:
        cursor.execute(
            "CREATE TEMPORARY TABLE sqlaero_orders (order_no VARCHAR(9), paid_at DATETIME, "
            "amount INT, `paid.at` INT)"
        )
        cursor.executemany(
            "INSERT INTO sqlaero_orders VALUES (%s,%s,%s,%s)",
            [
                ("A17", datetime(2026, 11, 27, 10), 3, 1),
                ("B04", datetime(2026, 11, 27, 9), 3, 2),
                ("C09", datetime(2026, 11, 27, 11), None, 3),
            ],
        )
        expr = sqlfunc.GROUP_CONCAT(
            Column("order_no"), order_by=[(Column("paid_at"), "ASC")], separator=", ",
        )
        assert str(connection.escape(expr)) == "GROUP_CONCAT(`order_no` ORDER BY `paid_at` ASC SEPARATOR ', ')"
        cursor.execute("SELECT %s,%s,%s,%s,%s FROM sqlaero_orders", (
            expr, sqlfunc.COUNT(), sqlfunc.COUNT("*"), sqlfunc.COUNT(Column("amount")),
            sqlfunc.COUNT(Column("order_no"), Column("amount"), distinct=True),
        ))
        assert cursor.fetchone() == ("B04, A17, C09", 3, 3, 2, 2)
        expr = sqlfunc.GROUP_CONCAT(
            Column("order_no"), distinct=True,
            order_by=[(Column("amount"), "desc"), (Column("paid.at", table="sqlaero_orders"), "DESC")],
            separator="' ",
        )
        cursor.execute("SET sql_mode='NO_BACKSLASH_ESCAPES'")
        assert str(connection.escape(expr)) == (
            "GROUP_CONCAT(DISTINCT `order_no` ORDER BY `amount` DESC,"
            "`sqlaero_orders`.`paid.at` DESC SEPARATOR ''' ')"
        )
        cursor.execute("SELECT %s FROM sqlaero_orders", expr)
        assert cursor.fetchone() == ("B04' A17' C09",)


def test_group_and_window_static_errors() -> None:
    with pytest.raises(TypeError, match="COUNT"):
        sqlfunc.COUNT(1, 2)
    with pytest.raises(TypeError, match="COUNT"):
        sqlfunc.COUNT(distinct=True)
    with pytest.raises(ValueError, match="direction"):
        sqlfunc.GROUP_CONCAT(Column("order_no"), order_by=[(Column("paid_at"), "sideways")])
    with pytest.raises(TypeError, match="Column"):
        sqlfunc.GROUP_CONCAT(Column("order_no"), order_by=[("paid_at", "DESC")])  # type: ignore[arg-type]
    seen: list[int] = []
    raw = CustomEscape(1, lambda value: seen.append(value) or "'text'")
    with pytest.raises(TypeError):
        sqlfunc.GROUP_CONCAT(Column("order_no"), separator=raw)  # type: ignore[arg-type]
    assert not seen
    assert signature(sqlfunc.GROUP_CONCAT).parameters.keys() == {
        "value", "values", "distinct", "order_by", "separator",
    }
    for constructor in (sqlfunc.LAG, sqlfunc.LEAD):
        with pytest.raises(ValueError, match="offset"):
            constructor(1, -1)
        with pytest.raises(TypeError, match="offset"):
            constructor(1, default=0)
        with pytest.raises(TypeError, match="offset"):
            constructor(1, None)  # type: ignore[arg-type]
    with pytest.raises(ValueError, match="position"):
        sqlfunc.NTH_VALUE(1, 0)
    with pytest.raises(ValueError, match="buckets"):
        sqlfunc.NTILE(0)
    with pytest.raises(ValueError, match="null_treatment"):
        sqlfunc.FIRST_VALUE(1, null_treatment="IGNORE NULLS")


@requires_mysql
def test_window_functions_match_native(connection: Connection) -> None:
    cases: tuple[tuple[EscapeInput, str], ...] = (
        (sqlfunc.CUME_DIST(), "CUME_DIST()"),
        (sqlfunc.DENSE_RANK(), "DENSE_RANK()"),
        (sqlfunc.FIRST_VALUE(Column("n"), null_treatment="RESPECT NULLS"), "FIRST_VALUE(n) RESPECT NULLS"),
        (sqlfunc.LAG(Column("n"), 1, -1), "LAG(n,1,-1)"),
        (sqlfunc.LAST_VALUE(Column("n")), "LAST_VALUE(n)"),
        (sqlfunc.LEAD(Column("n"), 0, None), "LEAD(n,0,NULL)"),
        (sqlfunc.NTH_VALUE(Column("n"), 2, from_position="FIRST"), "NTH_VALUE(n,2) FROM FIRST"),
        (sqlfunc.NTILE(2), "NTILE(2)"),
        (sqlfunc.PERCENT_RANK(), "PERCENT_RANK()"),
        (sqlfunc.RANK(), "RANK()"),
        (sqlfunc.ROW_NUMBER(), "ROW_NUMBER()"),
    )
    assert len({type(expr).__name__ for expr, _ in cases}) == 11
    with connection.cursor() as cursor:
        cursor.execute("CREATE TEMPORARY TABLE sqlaero_window (id INT, n INT)")
        cursor.execute("INSERT INTO sqlaero_window VALUES (1,2), (2,NULL), (3,5)")
        for expr, native in cases:
            name = type(expr).__name__
            assert type(expr).__base__ is sqlfunc.ABS.__base__
            assert name in sqlfunc.__all__ and type(expr).__doc__
            assert str(connection.escape(expr)).startswith(f"{name}(")
            cursor.execute(
                f"SELECT %s OVER ordered, {native} OVER ordered FROM sqlaero_window "
                "WINDOW ordered AS (ORDER BY id) ORDER BY id",
                expr,
            )
            rows = cursor.fetchall()
            assert len(rows) == 3 and all(row[0] == row[1] for row in rows)
