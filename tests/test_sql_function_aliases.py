"""The 40 SQL function aliases retain distinct classes and share canonical rules."""

from collections.abc import Callable
from inspect import signature

import pytest
from sqlaero import Column, Connection, sqlfunc
from sqlaero.errors import SQLAeroError
from sqlaero.typing import EscapeInput

from test_public_connection import connection as connection, requires_mysql


ALIASES: dict[str, str] = {
    "CEILING": "CEIL",
    "CHARACTER_LENGTH": "CHAR_LENGTH",
    "CURDATE": "CURRENT_DATE",
    "CURTIME": "CURRENT_TIME",
    "DAYOFMONTH": "DAY",
    "FROM_VECTOR": "VECTOR_TO_STRING",
    "GeomCollection": "GeometryCollection",
    "LCASE": "LOWER",
    "LOCALTIME": "NOW",
    "LOCALTIMESTAMP": "NOW",
    "MID": "SUBSTRING",
    "OCTET_LENGTH": "LENGTH",
    "POWER": "POW",
    "SCHEMA": "DATABASE",
    "SESSION_USER": "USER",
    "ST_AsWKB": "ST_AsBinary",
    "ST_AsWKT": "ST_AsText",
    "ST_GeomCollFromTxt": "ST_GeomCollFromText",
    "ST_GeometryCollectionFromText": "ST_GeomCollFromText",
    "ST_GeometryCollectionFromWKB": "ST_GeomCollFromWKB",
    "ST_GeometryFromText": "ST_GeomFromText",
    "ST_GeometryFromWKB": "ST_GeomFromWKB",
    "ST_LineStringFromText": "ST_LineFromText",
    "ST_LineStringFromWKB": "ST_LineFromWKB",
    "ST_MultiLineStringFromText": "ST_MLineFromText",
    "ST_MultiLineStringFromWKB": "ST_MLineFromWKB",
    "ST_MultiPointFromText": "ST_MPointFromText",
    "ST_MultiPointFromWKB": "ST_MPointFromWKB",
    "ST_MultiPolygonFromText": "ST_MPolyFromText",
    "ST_MultiPolygonFromWKB": "ST_MPolyFromWKB",
    "ST_NumInteriorRing": "ST_NumInteriorRings",
    "ST_PolygonFromText": "ST_PolyFromText",
    "ST_PolygonFromWKB": "ST_PolyFromWKB",
    "STD": "STDDEV_POP",
    "STDDEV": "STDDEV_POP",
    "SUBSTR": "SUBSTRING",
    "SYSTEM_USER": "USER",
    "TO_VECTOR": "STRING_TO_VECTOR",
    "UCASE": "UPPER",
    "VARIANCE": "VAR_POP",
}


def test_aliases_use_the_same_constructor_contract() -> None:
    assert len(ALIASES) == 40
    for alias, source in ALIASES.items():
        public: type[object] = getattr(sqlfunc, alias)
        canonical: type[object] = getattr(sqlfunc, source)
        assert alias in sqlfunc.__all__ and public.__name__ == alias
        assert public is not canonical and public.__module__ == "sqlaero.sqlfunc"
        assert public.__doc__ and public.__base__ is canonical.__base__
        assert signature(public) == signature(canonical)
    with pytest.raises(ValueError, match="LOCALTIME fsp"):
        sqlfunc.LOCALTIME(9)
    with pytest.raises(TypeError, match="ST_GeometryFromText"):
        sqlfunc.ST_GeometryFromText("POINT(1 2)", options="axis-order=lat-long")


@requires_mysql
def test_every_alias_emits_and_runs_the_canonical_operation(connection: Connection) -> None:
    point = sqlfunc.Point(1, 2)
    polygon = "POLYGON((0 0,4 0,4 4,0 4,0 0))"
    arguments: dict[str, tuple[object, ...]] = {
        "CEIL": (1.25,), "CHAR_LENGTH": ("条目",), "CURRENT_DATE": (), "CURRENT_TIME": (3,),
        "DAY": ("2026-11-27",), "VECTOR_TO_STRING": (sqlfunc.STRING_TO_VECTOR("[1,2]"),),
        "GeometryCollection": (), "LOWER": ("AbC",), "NOW": (0,), "SUBSTRING": ("abcdef", 2, 3),
        "LENGTH": ("条目",), "POW": (2, 3), "DATABASE": (), "USER": (),
        "ST_AsBinary": (point,), "ST_AsText": (point,),
        "ST_NumInteriorRings": (sqlfunc.ST_GeomFromText(polygon),),
        "STDDEV_POP": (Column("value"),), "STRING_TO_VECTOR": ("[1,2]",), "UPPER": ("AbC",),
        "VAR_POP": (Column("value"),),
    }
    for name, wkt in (
        ("GeomColl", "GEOMETRYCOLLECTION(POINT(1 2))"), ("Geom", "POINT(1 2)"),
        ("Line", "LINESTRING(0 0, 1 2)"), ("MLine", "MULTILINESTRING((0 0,1 2))"),
        ("MPoint", "MULTIPOINT((1 2),(3 4))"), ("MPoly", "MULTIPOLYGON(((0 0,4 0,4 4,0 4,0 0)))"),
        ("Poly", polygon),
    ):
        arguments[f"ST_{name}FromText"] = (wkt, 0)
        arguments[f"ST_{name}FromWKB"] = (sqlfunc.ST_AsBinary(sqlfunc.ST_GeomFromText(wkt)), 0)
    assert set(arguments) == set(ALIASES.values())
    with connection.cursor() as cursor:
        cursor.execute("SELECT VERSION()")
        server = cursor.fetchone()
        assert server is not None and isinstance(server[0], str)
        for alias, source in ALIASES.items():
            public: Callable[..., EscapeInput] = getattr(sqlfunc, alias)
            canonical: Callable[..., EscapeInput] = getattr(sqlfunc, source)
            expression = public(*arguments[source])
            control = canonical(*arguments[source])
            assert connection.escape(expression) == connection.escape(control)
            if server[0].startswith("8.4") and source in {"STRING_TO_VECTOR", "VECTOR_TO_STRING"}:
                with pytest.raises(SQLAeroError) as caught:
                    cursor.execute("SELECT %s", expression)
                assert caught.value.is_server_error
                continue
            if source in {"VAR_POP", "STDDEV_POP"}:
                query = "SELECT %s,%s FROM (SELECT 1 AS value UNION ALL SELECT 3) v"
            elif source == "GeometryCollection" or source.endswith(("FromText", "FromWKB")):
                query = "SELECT ST_AsText(%s), ST_AsText(%s)"
            elif source == "STRING_TO_VECTOR":
                query = "SELECT HEX(%s), HEX(%s)"
            else:
                query = "SELECT %s,%s"
            cursor.execute(query, (expression, control))
            row = cursor.fetchone()
            assert row is not None and row[0] == row[1], (alias, row)
