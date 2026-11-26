"""The closed conversion target catalog and its public SQL behavior."""

from collections.abc import Callable
from datetime import date, datetime, timedelta
from decimal import Decimal

import pytest
from sqlaero import Column, Connection, CustomEscape, JSON, sqlfunc, sqltype
from sqlaero.typing import EscapeInput

from test_public_connection import connection as connection, requires_mysql


NONSPATIAL = (
    ("SIGNED", (), "12", "SIGNED", 12),
    ("UNSIGNED", (True,), "12", "UNSIGNED INTEGER", 12),
    ("DECIMAL", (10, 2), "12.3", "DECIMAL(10,2)", Decimal("12.30")),
    ("FLOAT", (24,), "1.5", "FLOAT(24)", 1.5),
    ("DOUBLE", (), "1.5", "DOUBLE", 1.5),
    ("REAL", (), "1.5", "REAL", 1.5),
    ("CHAR", (3,), "abcde", "CHAR(3)", "abc"),
    ("NCHAR", (2,), "abcde", "NCHAR(2)", "ab"),
    ("BINARY", (3,), "A", "BINARY(3)", b"A\0\0"),
    ("DATE", (), "2026-11-27", "DATE", date(2026, 11, 27)),
    ("TIME", (2,), "12:34:56.12", "TIME(2)", timedelta(hours=12, minutes=34, seconds=56, microseconds=120000)),
    ("DATETIME", (2,), "2026-11-27 12:34:56.12", "DATETIME(2)", datetime(2026, 11, 27, 12, 34, 56, 120000)),
    ("YEAR", (), "2026", "YEAR", 2026),
)

SPATIAL = (
    ("POINT", "POINT(1 2)"),
    ("LINESTRING", "LINESTRING(0 0,1 1)"),
    ("POLYGON", "POLYGON((0 0,0 1,1 0,0 0))"),
    ("MULTIPOINT", "MULTIPOINT(0 0,1 1)"),
    ("MULTILINESTRING", "MULTILINESTRING((0 0,1 1))"),
    ("MULTIPOLYGON", "MULTIPOLYGON(((0 0,0 1,1 0,0 0)))"),
    ("GEOMETRYCOLLECTION", "GEOMETRYCOLLECTION(POINT(1 2))"),
)


def test_type_descriptor_identity_and_invalid_roles() -> None:
    assert sqltype.JSON is JSON
    assert sqltype.DATE is not sqlfunc.DATE
    assert len(set(sqltype.__all__) - {"GEOMCOLLECTION"}) == 21
    with pytest.raises(ValueError, match="precision"):
        sqltype.DECIMAL(0, 0)
    with pytest.raises(ValueError, match="scale"):
        sqltype.DECIMAL(2, 3)
    with pytest.raises(ValueError, match="fractional"):
        sqltype.TIME(7)
    with pytest.raises(TypeError):
        sqltype.BINARY(True)
    with pytest.raises(TypeError):
        sqlfunc.CAST("abc", JSON({}))  # type: ignore[arg-type]
    calls: list[int] = []
    wrapper = CustomEscape(1, lambda value: calls.append(value) or "SIGNED")
    with pytest.raises(TypeError, match="target"):
        sqlfunc.CAST("12", wrapper)  # type: ignore[arg-type]
    assert not calls


@requires_mysql
@pytest.mark.parametrize(("name", "parameters", "data", "spelling", "expected"), NONSPATIAL)
def test_conversion_targets(
    connection: Connection, name: str, parameters: tuple[object, ...], data: str,
    spelling: str, expected: object,
) -> None:
    constructor = getattr(sqltype, name)
    target = constructor(*parameters) if parameters else constructor
    cast = sqlfunc.CAST(data, target)
    convert = sqlfunc.CONVERT(data, target)
    assert connection.escape(cast) == f"CAST('{data}' AS {spelling})"
    assert connection.escape(convert) == f"CONVERT('{data}',{spelling})"
    with connection.cursor() as cursor:
        cursor.execute("SELECT %s,%s", (cast, convert))
        row = cursor.fetchone()
        assert row is not None
        assert row == (expected, expected)
        if isinstance(expected, Decimal):
            assert str(row[0]) == str(expected) and str(row[1]) == str(expected)


@requires_mysql
@pytest.mark.parametrize(("name", "wkt"), SPATIAL)
def test_spatial_targets_preserve_a_geometry_of_the_same_kind(
    connection: Connection, name: str, wkt: str,
) -> None:
    target = getattr(sqltype, name)
    cast = sqlfunc.CAST(Column("g"), target)
    convert = sqlfunc.CONVERT(Column("g"), target)
    assert connection.escape(cast) == f"CAST(`g` AS {name})"
    assert connection.escape(convert) == f"CONVERT(`g`,{name})"
    with connection.cursor() as cursor:
        cursor.execute(
            "WITH geometry_data AS (SELECT ST_GeomFromText(%s) AS g) "
            "SELECT ST_Equals(g,%s),ST_Equals(g,%s) FROM geometry_data",
            (wkt, cast, convert),
        )
        assert cursor.fetchone() == (1, 1)


@requires_mysql
def test_json_type_identity_lazy_value_and_charset_conversion(connection: Connection) -> None:
    document = {"amount": 3}
    expression = sqlfunc.CAST(JSON(document), sqltype.JSON)
    transcode: Callable[[], EscapeInput] = lambda: sqlfunc.CONVERT("é", using="latin1")
    assert connection.escape(transcode()) == "CONVERT('é' USING latin1)"
    assert connection.escape(sqlfunc.CAST("é", sqltype.CHAR(charset="latin1"))) == (
        "CAST('é' AS CHAR CHARACTER SET latin1)"
    )
    with connection.cursor() as cursor:
        cursor.decode_json = True
        for amount in (3, 8):
            document["amount"] = amount
            cursor.execute("SELECT %s,%s", (expression, transcode()))
            assert cursor.fetchone() == ({"amount": amount}, "é")


@requires_mysql
def test_utc_cast_is_restricted_to_datetime(connection: Connection) -> None:
    with pytest.raises(ValueError, match="DATETIME"):
        sqlfunc.CAST(Column("ts"), sqltype.DATE, at_utc=True)
    with connection.cursor() as cursor:
        cursor.execute("SET time_zone='+02:00'")
        cursor.execute("CREATE TEMPORARY TABLE sqlaero_utc_cast (ts TIMESTAMP(2))")
        cursor.execute("INSERT INTO sqlaero_utc_cast VALUES ('2026-11-27 12:30:00.25')")
        cursor.execute(
            "SELECT %s FROM sqlaero_utc_cast",
            sqlfunc.CAST(Column("ts"), sqltype.DATETIME(2), at_utc=True),
        )
        actual = cursor.fetchone()
        cursor.execute("SELECT CAST(ts AT TIME ZONE '+00:00' AS DATETIME(2)) FROM sqlaero_utc_cast")
        assert actual == cursor.fetchone()
        assert actual is not None and isinstance(actual[0], datetime)
        assert actual[0].replace(microsecond=0) == datetime(2026, 11, 27, 10, 30)
