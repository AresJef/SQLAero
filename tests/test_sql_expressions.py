"""Public construction, preview, and execution of native SQL expressions."""

from datetime import date

import pytest
from sqlaero import Connection, JSON, json_dumps, sqlfunc, sqlintv
from sqlaero.errors import DataError, NotSupportedError

from test_prepared_cursor import statement_counts
from test_public_connection import connection as connection, requires_mysql


def test_expression_construction_checks_static_roles() -> None:
    assert sqlfunc.ABS.__module__ == "sqlaero.sqlfunc"
    assert sqlintv.DAY.__module__ == "sqlaero.sqlintv"
    interval_base = sqlintv.DAY.__base__
    assert interval_base is not None and interval_base.__base__ is sqlfunc.ABS.__base__
    assert not hasattr(sqlfunc, "SQLFunction")
    assert not hasattr(sqlintv, "SQLInterval")
    with pytest.raises(TypeError):
        sqlfunc.ABS()  # type: ignore[call-arg]
    with pytest.raises(ValueError, match="EXTRACT.*unit"):
        sqlfunc.EXTRACT("YEAR FROM 1", date(2026, 11, 27))
    with pytest.raises(TypeError, match="DATE_ADD.*interval"):
        sqlfunc.DATE_ADD(date(2026, 11, 27), "INTERVAL 2 DAY")  # type: ignore[arg-type]
    with pytest.raises(TypeError, match="ABS.*scalar"):
        sqlfunc.ABS((1, 2))
    with pytest.raises(TypeError, match="ABS.*scalar"):
        sqlfunc.ABS({"x": 1})


@requires_mysql
def test_expression_depth_is_bounded_by_the_existing_traversal(connection: Connection) -> None:
    expression: object = -7
    for _ in range(65):
        expression = sqlfunc.ABS(expression)
    assert isinstance(expression, sqlfunc.ABS)
    with pytest.raises(DataError, match="64 active"):
        connection.escape(expression)


@requires_mysql
def test_public_expression_escape_preview_and_execution(connection: Connection) -> None:
    day = date(2026, 11, 27)
    absolute = sqlfunc.ABS(-7)
    year = sqlfunc.EXTRACT("year", day)
    interval = sqlintv.DAY(sqlfunc.ABS(-2))
    shifted = sqlfunc.DATE_ADD(day, interval)
    assert connection.escape(absolute) == "ABS(-7)"
    assert connection.escape(sqlintv.DAY(2)) == "INTERVAL 2 DAY"
    assert connection.escape(interval) == "INTERVAL ABS(-2) DAY"
    assert connection.escape(JSON({"x": 7})) == "'{\\\"x\\\":7}'"
    assert json_dumps({"x": 7}) == b'{"x":7}'

    with connection.cursor() as cursor:
        args = (absolute, year, shifted, "ABS(-7)", "O'Brien")
        assert cursor.mogrify("SELECT %s,%s,%s,%s,%s", args) == (
            "SELECT ABS(-7),EXTRACT(YEAR FROM '2026-11-27'),"
            "DATE_ADD('2026-11-27',INTERVAL ABS(-2) DAY),'ABS(-7)','O\\'Brien'"
        )
        cursor.execute("SELECT %s,%s,%s,%s,%s", args)
        assert cursor.fetchone() == (7, 2026, "2026-11-29", "ABS(-7)", "O'Brien")
        cursor.execute("SELECT %s", (sqlfunc.DATE_ADD(sqlfunc.DATE(day), sqlintv.DAY(2)),))
        assert cursor.fetchone() == (date(2026, 11, 29),)


@requires_mysql
def test_expression_is_reusable_in_both_string_modes(connection: Connection) -> None:
    original = "A'B\\C"
    expression = sqlfunc.EXTRACT("YEAR", date(2026, 11, 27))
    with connection.cursor() as cursor:
        for mode in ("", "NO_BACKSLASH_ESCAPES"):
            cursor.execute("SET SESSION sql_mode=%s", (mode,))
            cursor.execute("SELECT %s, %s", (expression, original))
            assert cursor.fetchone() == (2026, original)


@requires_mysql
def test_prepared_previews_expression_but_rejects_native_parameter(connection: Connection) -> None:
    with connection.cursor(prepared=True) as cursor:
        absolute = sqlfunc.ABS(-7)
        before = statement_counts(connection)
        assert cursor.mogrify("SELECT %s", (absolute,)) == "SELECT ABS(-7)"
        assert statement_counts(connection) == before
        with pytest.raises(NotSupportedError):
            cursor.execute("SELECT %s", (absolute,))
        assert statement_counts(connection) == before
        cursor.execute("SELECT ABS(%s)", (-7,))
        assert cursor.fetchone() == (7,)
