"""Closed MySQL conversion targets for sqlfunc."""

from ._sqlaero import JSON as JSON
from ._sqlaero import _sqltype  # pyright: ignore[reportPrivateUsage]

BINARY = _sqltype.BINARY
CHAR = _sqltype.CHAR
DATE = _sqltype.DATE
DATETIME = _sqltype.DATETIME
DECIMAL = _sqltype.DECIMAL
DOUBLE = _sqltype.DOUBLE
FLOAT = _sqltype.FLOAT
GEOMETRYCOLLECTION = _sqltype.GEOMETRYCOLLECTION
GEOMCOLLECTION = GEOMETRYCOLLECTION
LINESTRING = _sqltype.LINESTRING
MULTILINESTRING = _sqltype.MULTILINESTRING
MULTIPOINT = _sqltype.MULTIPOINT
MULTIPOLYGON = _sqltype.MULTIPOLYGON
NCHAR = _sqltype.NCHAR
POINT = _sqltype.POINT
POLYGON = _sqltype.POLYGON
REAL = _sqltype.REAL
SIGNED = _sqltype.SIGNED
TIME = _sqltype.TIME
UNSIGNED = _sqltype.UNSIGNED
YEAR = _sqltype.YEAR

__all__: list[str] = [
    'BINARY', 'CHAR', 'DATE', 'DATETIME', 'DECIMAL', 'DOUBLE', 'FLOAT',
    'GEOMCOLLECTION', 'GEOMETRYCOLLECTION', 'JSON', 'LINESTRING', 'MULTILINESTRING',
    'MULTIPOINT', 'MULTIPOLYGON', 'NCHAR', 'POINT', 'POLYGON', 'REAL', 'SIGNED',
    'TIME', 'UNSIGNED', 'YEAR',
]
