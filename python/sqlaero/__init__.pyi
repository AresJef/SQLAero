"""Public native synchronous MySQL entry points."""

from ._sqlaero import (
    connect as connect,
    Connection as Connection,
    Cursor as Cursor,
    SSL as SSL,
    ColumnInfo as ColumnInfo,
    Columns as Columns,
)

__all__: list[str]
