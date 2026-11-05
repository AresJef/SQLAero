"""Public native synchronous MySQL entry points."""

from ._sqlaero import (
    connect as connect,
    Connection as Connection,
    Cursor as Cursor,
    DictCursor as DictCursor,
    SSCursor as SSCursor,
    SSDictCursor as SSDictCursor,
    SSL as SSL,
    Compression as Compression,
    TLSInfo as TLSInfo,
    TLSCertificate as TLSCertificate,
    ColumnInfo as ColumnInfo,
    Columns as Columns,
)

__all__: list[str]
