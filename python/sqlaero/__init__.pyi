"""Public native synchronous MySQL entry points."""

from ._sqlaero import (
    connect as connect,
    Connection as Connection,
    Cursor as Cursor,
    SSL as SSL,
    Compression as Compression,
    TLSInfo as TLSInfo,
    TLSCertificate as TLSCertificate,
    ColumnInfo as ColumnInfo,
    Columns as Columns,
)

__all__: list[str]
