"""Identifiers are explicit operands of expressions that accept column references."""

from decimal import Decimal

import pytest
from sqlaero import Column, Connection, sqlfunc

from test_public_connection import connection as connection, requires_mysql


def test_column_checks_components_and_sum_option() -> None:
    with pytest.raises(ValueError, match="nonempty"):
        Column("")
    with pytest.raises(ValueError, match="NUL"):
        Column("pri\0ce")
    with pytest.raises(ValueError, match="table"):
        Column("price", database="shop")
    with pytest.raises(TypeError, match="distinct"):
        sqlfunc.SUM(Column("price"), distinct=1)  # type: ignore[arg-type]


@requires_mysql
def test_column_segments_are_quoted_and_original_strings_remain_data(
    connection: Connection,
) -> None:
    assert connection.escape(sqlfunc.SUM("price")) == "SUM('price')"
    assert connection.escape(sqlfunc.SUM(Column("a.b"))) == "SUM(`a.b`)"
    assert connection.escape(sqlfunc.SUM(Column("b", table="a", database="shop"))) == (
        "SUM(`shop`.`a`.`b`)"
    )
    assert connection.escape(sqlfunc.SUM(Column("a`b"))) == "SUM(`a``b`)"
    with connection.cursor() as cursor:
        query = "WITH amounts AS (SELECT 3 AS price UNION ALL SELECT 4 UNION ALL SELECT 4) SELECT %s FROM amounts"
        cursor.execute(query, sqlfunc.SUM(Column("price")))
        assert cursor.fetchone() == (Decimal("11"),)
        cursor.execute(query, sqlfunc.SUM(Column("price"), distinct=True))
        assert cursor.fetchone() == (Decimal("7"),)
