"""Compare the complete spatial catalogue and core vector functions with MySQL."""

from collections.abc import Callable
from inspect import signature

import pytest
from sqlaero import Column, Connection, sqlfunc
from sqlaero.errors import SQLAeroError
from sqlaero.typing import EscapeInput

from test_public_connection import connection as connection, requires_mysql


POINT = "POINT(1 1)"
LINE = "LINESTRING(0 0,2 2,4 2)"
OTHER_LINE = "LINESTRING(0 1,2 3,4 3)"
RING = "LINESTRING(0 0,4 0,4 4,0 4,0 0)"
POLYGON = "POLYGON((0 0,4 0,4 4,0 4,0 0),(1 1,1 2,2 2,2 1,1 1))"
OTHER_POLYGON = "POLYGON((3 3,7 3,7 7,3 7,3 3))"
MULTI_POINT = "MULTIPOINT((0 0),(2 2))"
MULTI_LINE = "MULTILINESTRING((0 0,2 2),(3 3,4 4))"
MULTI_POLY = "MULTIPOLYGON(((0 0,4 0,4 4,0 4,0 0)))"
COLLECTION = "GEOMETRYCOLLECTION(POINT(1 1))"
GEO_JSON = '{"type":"Point","coordinates":[12,35]}'


class Operand:
    """A nested typed argument with an independent SQL control."""

    value: object
    native: str

    def __init__(self, value: object, native: str) -> None:
        self.value = value
        self.native = native


def spatial_catalogue() -> dict[str, tuple[tuple[Operand, ...], bool]]:
    def literal(value: int | float | str) -> Operand:
        return Operand(value, f"'{value}'" if isinstance(value, str) else str(value))

    def wkt(value: str, srid: int = 0) -> Operand:
        return Operand(sqlfunc.ST_GeomFromText(value, srid), f"ST_GeomFromText('{value}',{srid})")

    cases: dict[str, tuple[tuple[Operand, ...], bool]] = {}

    def add(names: str, *arguments: Operand, geometry: bool = False) -> None:
        for name in names.split():
            assert name not in cases
            cases[name] = arguments, geometry

    point = wkt(POINT)
    outside = wkt("POINT(6 4)")
    geodetic = wkt("POINT(35 12)", 4326)
    line = wkt(LINE)
    other_line = wkt(OTHER_LINE)
    ring = wkt(RING)
    poly = wkt(POLYGON)
    other_poly = wkt(OTHER_POLYGON)
    multi_point = wkt(MULTI_POINT)
    multi_line = wkt(MULTI_LINE)
    multi_poly = wkt(MULTI_POLY)
    collection = wkt(COLLECTION)

    add("Point", literal(1), literal(2), geometry=True)
    add("GeometryCollection", point, poly, geometry=True)
    add("LineString MultiPoint", point, outside, geometry=True)
    add("Polygon", ring, geometry=True)
    add("MultiLineString", line, other_line, geometry=True)
    add("MultiPolygon", poly, geometry=True)
    add("MBRContains MBRCovers ST_Contains", poly, point)
    add("MBRCoveredBy MBRWithin ST_Within", point, poly)
    add("MBRDisjoint MBREquals MBRIntersects MBROverlaps MBRTouches", poly, other_poly)
    add("ST_Crosses", line, other_line)
    add("ST_Disjoint ST_Equals ST_Intersects ST_Overlaps ST_Touches", poly, other_poly)
    add("ST_Area ST_Dimension ST_GeometryType ST_IsEmpty ST_IsSimple ST_IsValid", poly)
    add("ST_AsBinary ST_AsGeoJSON ST_AsText", point)
    add("ST_Buffer", point, literal(0.25), geometry=True)
    add("ST_Buffer_Strategy", literal("point_circle"), literal(8))
    add("ST_Centroid ST_ConvexHull ST_Envelope ST_ExteriorRing ST_Validate", poly, geometry=True)
    add("ST_Difference ST_Intersection ST_SymDifference ST_Union", poly, other_poly, geometry=True)
    add("ST_Distance ST_Distance_Sphere", point, outside)
    add("ST_EndPoint ST_StartPoint", line, geometry=True)
    add("ST_FrechetDistance ST_HausdorffDistance", line, other_line)
    add("ST_GeoHash", literal(12), literal(35), literal(8))
    add("ST_GeomFromGeoJSON", literal(GEO_JSON), literal(1), literal(4326), geometry=True)
    add("ST_GeometryN", collection, literal(1), geometry=True)
    add("ST_InteriorRingN", poly, literal(1), geometry=True)
    add("ST_IsClosed ST_Length ST_NumPoints", line)
    add("ST_LatFromGeoHash ST_LongFromGeoHash", literal("snu38h2z"))
    add("ST_Latitude ST_Longitude", geodetic)
    add("ST_LineInterpolatePoint ST_LineInterpolatePoints", line, literal(0.5), geometry=True)
    add("ST_MakeEnvelope", point, outside, geometry=True)
    add("ST_NumGeometries", multi_point)
    add("ST_NumInteriorRings", poly)
    add("ST_PointAtDistance", line, literal(1), geometry=True)
    add("ST_PointFromGeoHash", literal("snu38h2z"), literal(4326), geometry=True)
    add("ST_PointN", line, literal(2), geometry=True)
    add("ST_Simplify", line, literal(0.1), geometry=True)
    add("ST_SRID", geodetic)
    add("ST_SwapXY", point, geometry=True)
    add("ST_Transform", geodetic, literal(3857), geometry=True)
    add("ST_X ST_Y", point)

    for prefix, text, geom in (
        ("GeomColl", COLLECTION, collection), ("Geom", POINT, point), ("Line", LINE, line),
        ("MLine", MULTI_LINE, multi_line), ("MPoint", MULTI_POINT, multi_point),
        ("MPoly", MULTI_POLY, multi_poly), ("Point", POINT, point), ("Poly", POLYGON, poly),
    ):
        add(f"ST_{prefix}FromText", literal(text), literal(0), geometry=True)
        add(f"ST_{prefix}FromWKB", Operand(sqlfunc.ST_AsBinary(geom.value), f"ST_AsBinary({geom.native})"),
            literal(0), geometry=True)
    return cases


@requires_mysql
def test_spatial_catalogue_against_mysql(connection: Connection) -> None:
    cases = spatial_catalogue()
    assert len(cases) == 91
    with connection.cursor() as cursor:
        for name, (arguments, geometry) in cases.items():
            constructor: Callable[..., EscapeInput] = getattr(sqlfunc, name)
            expr = constructor(*(argument.value for argument in arguments))
            name_sql = f"{name}({','.join(argument.native for argument in arguments)})"
            assert name in sqlfunc.__all__ and type(expr).__doc__
            assert type(expr).__base__ is sqlfunc.ABS.__base__
            assert str(connection.escape(expr)).startswith(f"{name}(")
            signature(constructor).bind(*(argument.value for argument in arguments))
            query = f"SELECT ST_AsText(%s), ST_AsText({name_sql})" if geometry else f"SELECT %s,{name_sql}"
            cursor.execute(query, expr)
            row = cursor.fetchone()
            assert row is not None and row[0] is not None and row[0] == row[1], (name, row)


@requires_mysql
def test_spatial_collection_with_distinct_and_over(connection: Connection) -> None:
    with connection.cursor() as cursor:
        cursor.execute("CREATE TEMPORARY TABLE sqlaero_geo (id INT, point POINT)")
        cursor.execute("INSERT INTO sqlaero_geo VALUES (1,Point(1,1)), (2,Point(1,1)), (3,Point(2,2))")
        collect = sqlfunc.ST_Collect(Column("point"), distinct=True)
        cursor.execute("SELECT ST_NumGeometries(%s) FROM sqlaero_geo", collect)
        assert cursor.fetchone() == (2,)
        cursor.execute(
            "SELECT ST_NumGeometries(%s OVER (ORDER BY id ROWS UNBOUNDED PRECEDING)) "
            "FROM sqlaero_geo ORDER BY id",
            collect,
        )
        assert cursor.fetchall() == ((1,), (1,), (2,))


@requires_mysql
def test_vector_basics_and_version_support(connection: Connection) -> None:
    value = sqlfunc.STRING_TO_VECTOR("[1.05, -17.8, 32]")
    dimension = sqlfunc.VECTOR_DIM(value)
    text = sqlfunc.VECTOR_TO_STRING(value)
    expressions = value, dimension, text
    assert all(type(expr).__base__ is sqlfunc.ABS.__base__ for expr in expressions)
    with connection.cursor() as cursor:
        cursor.execute("SELECT VERSION()")
        server = cursor.fetchone()
        assert server is not None and isinstance(server[0], str)
        if server[0].startswith("8.4"):
            for expr in (value, sqlfunc.VECTOR_DIM(b""), sqlfunc.VECTOR_TO_STRING(b"")):
                with pytest.raises(SQLAeroError) as caught:
                    cursor.execute("SELECT %s", expr)
                assert caught.value.is_server_error
            return
        cursor.execute("SELECT HEX(%s),%s,%s", expressions)
        assert cursor.fetchone() == ("6666863F66668EC100000042", 3, "[1.05000e+00,-1.78000e+01,3.20000e+01]")
        with pytest.raises(SQLAeroError) as caught:
            cursor.execute("SELECT %s", sqlfunc.STRING_TO_VECTOR("[abc]"))
        assert caught.value.is_server_error
