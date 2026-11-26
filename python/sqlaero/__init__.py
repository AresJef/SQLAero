"""SQLAero: synchronous MySQL commands and result conversion in a native Rust core."""

from ._sqlaero import ColumnInfo, Columns, Compression, Connection, Cursor, DictCursor, SSCursor, SSDictCursor, SSL, TLSInfo, TLSCertificate, connect
from ._sqlaero import Column, JSON, json_dumps
from . import sqlfunc, sqlintv

__all__: list[str] = ['connect', 'Connection', 'Cursor', 'DictCursor', 'SSCursor', 'SSDictCursor', 'SSL', 'Compression', 'Column', 'ColumnInfo', 'Columns', 'TLSInfo', 'TLSCertificate', 'JSON', 'json_dumps', 'sqlfunc', 'sqlintv']
