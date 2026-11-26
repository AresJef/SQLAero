"""Public native synchronous MySQL entry points."""

from . import sqlfunc as sqlfunc, sqlintv as sqlintv, sqltype as sqltype
from ._sqlaero import (
    JSON as JSON,
    json_dumps as json_dumps,
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
    Column as Column,
    CustomEscape as CustomEscape,
    ColumnInfo as ColumnInfo,
    Columns as Columns,
)

__all__: list[str]
