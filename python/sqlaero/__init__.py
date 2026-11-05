"""SQLAero: synchronous MySQL commands and result conversion in a native Rust core."""

from ._sqlaero import ColumnInfo, Columns, Compression, Connection, Cursor, DictCursor, SSCursor, SSDictCursor, SSL, TLSInfo, TLSCertificate, connect

__all__: list[str] = ['connect', 'Connection', 'Cursor', 'DictCursor', 'SSCursor', 'SSDictCursor', 'SSL', 'Compression', 'ColumnInfo', 'Columns', 'TLSInfo', 'TLSCertificate']
