import logging

logging.getLogger(__name__).addHandler(logging.NullHandler())

from sqlaero import aio, constants, errors, sqlfunc, sqlintvl
from sqlaero._ssl import SSL
from sqlaero._auth import AuthPlugin
from sqlaero.retry import RetryOnErrno, RetryOnError, retry_on_errno, retry_on_error
from sqlaero._optionfile import OptionFile
from sqlaero.charset import Charset, Charsets, all_charsets
from sqlaero.protocol import MysqlPacket, FieldDescriptorPacket
from sqlaero.transcode import escape, ObjStr, CustomEscapeType, BIT, JSON
from sqlaero.connection import (
    Cursor,
    DictCursor,
    DfCursor,
    SSCursor,
    SSDictCursor,
    SSDfCursor,
    BaseConnection,
    Connection,
)
from sqlaero.aio.pool import Pool, PoolConnection, PoolSyncConnection
from sqlaero._connect import connect, create_pool

__all__ = [
    # Module
    "aio",
    "constants",
    "errors",
    "sqlfunc",
    "sqlintvl",
    # Class
    "AuthPlugin",
    "OptionFile",
    "SSL",
    "Charset",
    "Charsets",
    "MysqlPacket",
    "FieldDescriptorPacket",
    "Cursor",
    "DictCursor",
    "DfCursor",
    "SSCursor",
    "SSDictCursor",
    "SSDfCursor",
    "BaseConnection",
    "Connection",
    "Pool",
    "PoolConnection",
    "PoolSyncConnection",
    # Type
    "ObjStr",
    "CustomEscapeType",
    "BIT",
    "JSON",
    # Function
    "all_charsets",
    "escape",
    "connect",
    "create_pool",
    # Retru
    "retry_on_errno",
    "retry_on_error",
    "RetryOnErrno",
    "RetryOnError",
]
