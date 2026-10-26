"""SQLAero: synchronous MySQL commands and result conversion in a native Rust core."""

from ._sqlaero import ColumnInfo, Columns, Connection, Cursor, SSL, connect

__all__: list[str] = ['connect', 'Connection', 'Cursor', 'SSL', 'ColumnInfo', 'Columns']
