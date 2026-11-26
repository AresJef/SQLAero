"""Closed MySQL conversion targets for sqlfunc."""

from typing import TypeAlias, final
from ._sqlaero import JSON as JSON

class _SQLType: ...

_Target: TypeAlias = _SQLType | type[_SQLType] | type[JSON]
__all__: list[str]

@final
class BINARY(_SQLType):
    """Use BINARY with an optional byte length."""
    def __init__(self, length: int | None = None) -> None: ...

@final
class CHAR(_SQLType):
    """Use CHAR with an optional length and MySQL charset."""
    def __init__(self, length: int | None = None, *, charset: str | None = None) -> None: ...

@final
class DATE(_SQLType):
    """Use MySQL DATE as the conversion target."""

@final
class DATETIME(_SQLType):
    """Use DATETIME with optional fractional-second precision."""
    def __init__(self, fsp: int | None = None) -> None: ...

@final
class DECIMAL(_SQLType):
    """Use DECIMAL with optional precision and scale."""
    def __init__(self, precision: int | None = None, scale: int | None = None) -> None: ...

@final
class DOUBLE(_SQLType):
    """Use MySQL DOUBLE as the conversion target."""

@final
class FLOAT(_SQLType):
    """Use FLOAT with an optional binary precision."""
    def __init__(self, precision: int | None = None) -> None: ...

@final
class GEOMETRYCOLLECTION(_SQLType):
    """Use MySQL GEOMETRYCOLLECTION as the geometry conversion target."""

GEOMCOLLECTION = GEOMETRYCOLLECTION

@final
class LINESTRING(_SQLType):
    """Use MySQL LINESTRING as the geometry conversion target."""

@final
class MULTILINESTRING(_SQLType):
    """Use MySQL MULTILINESTRING as the geometry conversion target."""

@final
class MULTIPOINT(_SQLType):
    """Use MySQL MULTIPOINT as the geometry conversion target."""

@final
class MULTIPOLYGON(_SQLType):
    """Use MySQL MULTIPOLYGON as the geometry conversion target."""

@final
class NCHAR(_SQLType):
    """Use NCHAR with an optional character length."""
    def __init__(self, length: int | None = None) -> None: ...

@final
class POINT(_SQLType):
    """Use MySQL POINT as the geometry conversion target."""

@final
class POLYGON(_SQLType):
    """Use MySQL POLYGON as the geometry conversion target."""

@final
class REAL(_SQLType):
    """Use MySQL REAL, whose precision follows REAL_AS_FLOAT mode."""

@final
class SIGNED(_SQLType):
    """Use SIGNED with an optional INTEGER suffix."""
    def __init__(self, integer: bool = False) -> None: ...

@final
class TIME(_SQLType):
    """Use TIME with optional fractional-second precision."""
    def __init__(self, fsp: int | None = None) -> None: ...

@final
class UNSIGNED(_SQLType):
    """Use UNSIGNED with an optional INTEGER suffix."""
    def __init__(self, integer: bool = False) -> None: ...

@final
class YEAR(_SQLType):
    """Use MySQL YEAR as the conversion target."""
